#![no_std]
#![no_main]

extern crate alloc;

mod vga_console;

use core::ptr::addr_of_mut;
use core::sync::atomic::Ordering;
use core::time::Duration;
use mochios::{BootInfo, MemoryRegion, MemoryType, SmpHandoff, MAX_CPU_IDS};
use uefi::prelude::*;
use uefi::proto::console::gop::GraphicsOutput;
use uefi::proto::loaded_image::LoadedImage;
use uefi::proto::media::file::{File, FileAttribute, FileInfo, FileMode, FileType};
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::proto::pi::mp::MpServices;
use uefi::table::boot::{
    AllocateType, MemoryType as UefiMemType, OpenProtocolAttributes, OpenProtocolParams,
};

/// VGA フレームバッファへ書き出す print マクロ
macro_rules! vga_print {
    ($($arg:tt)*) => {{
        let _ = core::fmt::write(&mut *vga_console::CONSOLE.lock(), format_args!($($arg)*));
    }};
}

macro_rules! vga_println {
    () => { vga_print!("\n") };
    ($($arg:tt)*) => { vga_print!("{}\n", format_args!($($arg)*)) };
}

static mut BOOT_INFO: BootInfo = BootInfo {
    physical_memory_offset: 0,
    framebuffer_addr: 0,
    framebuffer_size: 0,
    screen_width: 0,
    screen_height: 0,
    stride: 0,
    memory_map_addr: 0,
    memory_map_len: 0,
    memory_map_entry_size: 0,
    kernel_heap_addr: 0,
    initfs_addr: 0,
    initfs_size: 0,
    rootfs_addr: 0,
    rootfs_size: 0,
    cpu_total: 1,
    cpu_enabled: 1,
    bsp_apic_id: 0,
    cpu_apic_ids: [0; MAX_CPU_IDS],
    cpu_apic_id_count: 0,
    smp_handoff_addr: 0,
    smp_handoff_size: 0,
};

static mut SMP_HANDOFF: SmpHandoff = SmpHandoff::new();

static mut MEMORY_MAP: [MemoryRegion; 256] = [MemoryRegion {
    start: 0,
    len: 0,
    region_type: MemoryType::Reserved,
}; 256];

/// ELF64 ファイルヘッダ
#[repr(C)]
struct Elf64Header {
    e_ident: [u8; 16],
    e_type: u16,
    e_machine: u16,
    e_version: u32,
    e_entry: u64,
    e_phoff: u64,
    e_shoff: u64,
    e_flags: u32,
    e_ehsize: u16,
    e_phentsize: u16,
    e_phnum: u16,
    e_shentsize: u16,
    e_shnum: u16,
    e_shstrndx: u16,
}

/// ELF64 プログラムヘッダ
#[repr(C)]
struct Elf64Phdr {
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_paddr: u64,
    p_filesz: u64,
    p_memsz: u64,
    p_align: u64,
}

const PT_LOAD: u32 = 1;
const PT_DYNAMIC: u32 = 2;

/// ELF64 動的セクションエントリ
#[repr(C)]
struct Elf64Dyn {
    d_tag: i64,
    d_val: u64,
}

/// ELF64 RELA 再配置エントリ
#[repr(C)]
struct Elf64Rela {
    r_offset: u64,
    r_info: u64,
    r_addend: i64,
}

const R_X86_64_RELATIVE: u32 = 8;
const DT_NULL: i64 = 0;
const DT_RELA: i64 = 7;
const DT_RELASZ: i64 = 8;
const DT_RELAENT: i64 = 9;
const READ_CHUNK_BYTES: usize = 64 * 1024;

#[inline]
fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let raw = bytes.get(offset..offset + 4)?;
    Some(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
}

#[inline]
fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    let raw = bytes.get(offset..offset + 8)?;
    Some(u64::from_le_bytes([
        raw[0], raw[1], raw[2], raw[3], raw[4], raw[5], raw[6], raw[7],
    ]))
}

fn find_elf_symbol(elf: &[u8], symbol_name: &str) -> Option<u64> {
    if elf.len() < core::mem::size_of::<Elf64Header>() {
        return None;
    }
    let eh = unsafe { &*(elf.as_ptr() as *const Elf64Header) };
    if &eh.e_ident[0..4] != b"\x7fELF" || eh.e_ident[4] != 2 || eh.e_machine != 0x3E {
        return None;
    }
    let shoff = eh.e_shoff as usize;
    let shentsz = eh.e_shentsize as usize;
    let shnum = eh.e_shnum as usize;
    if shoff == 0 || shentsz == 0 || shnum == 0 {
        return None;
    }

    for si in 0..shnum {
        let sh_off = shoff + si * shentsz;
        let sh_type = read_u32(elf, sh_off + 4)?;
        if sh_type != 2 && sh_type != 11 {
            continue;
        }
        let symtab_offset = usize::try_from(read_u64(elf, sh_off + 24)?).ok()?;
        let symtab_size = usize::try_from(read_u64(elf, sh_off + 32)?).ok()?;
        let sh_link = read_u32(elf, sh_off + 40)? as usize;
        let symtab_entsize = usize::try_from(read_u64(elf, sh_off + 56)?).ok()?;
        if symtab_entsize < 24 || symtab_size == 0 || sh_link >= shnum {
            continue;
        }
        let link_sh_off = shoff + sh_link * shentsz;
        let strtab_offset = usize::try_from(read_u64(elf, link_sh_off + 24)?).ok()?;
        let strtab_size = usize::try_from(read_u64(elf, link_sh_off + 32)?).ok()?;
        let nsyms = symtab_size / symtab_entsize;
        for i_sym in 0..nsyms {
            let sym_off = symtab_offset + i_sym * symtab_entsize;
            let st_name = read_u32(elf, sym_off)? as usize;
            let st_value = read_u64(elf, sym_off + 8)?;
            if st_name >= strtab_size {
                continue;
            }
            let name_off = strtab_offset + st_name;
            if name_off >= elf.len() {
                continue;
            }
            let mut end = name_off;
            while end < elf.len() && elf[end] != 0 {
                end += 1;
            }
            let Ok(name_str) = core::str::from_utf8(&elf[name_off..end]) else {
                continue;
            };
            if name_str == symbol_name {
                return Some(st_value);
            }
        }
    }
    None
}

unsafe fn populate_cpu_info(bt: &BootServices) {
    let handle = match bt.get_handle_for_protocol::<MpServices>() {
        Ok(handle) => handle,
        Err(_) => {
            vga_println!("MpServices unavailable; assuming single CPU");
            BOOT_INFO.cpu_total = 1;
            BOOT_INFO.cpu_enabled = 1;
            BOOT_INFO.bsp_apic_id = 0;
            BOOT_INFO.cpu_apic_id_count = 0;
            return;
        }
    };

    let mp = match bt.open_protocol_exclusive::<MpServices>(handle) {
        Ok(mp) => mp,
        Err(e) => {
            vga_println!("MpServices open failed: {:?}", e.status());
            return;
        }
    };

    let counts = match mp.get_number_of_processors() {
        Ok(counts) => counts,
        Err(e) => {
            vga_println!("MpServices count failed: {:?}", e.status());
            return;
        }
    };

    BOOT_INFO.cpu_total = counts.total;
    BOOT_INFO.cpu_enabled = counts.enabled;
    BOOT_INFO.cpu_apic_id_count = 0;

    let total = core::cmp::min(counts.total, MAX_CPU_IDS);
    for index in 0..total {
        if let Ok(info) = mp.get_processor_info(index) {
            if index < MAX_CPU_IDS {
                BOOT_INFO.cpu_apic_ids[index] = info.processor_id as u32;
                BOOT_INFO.cpu_apic_id_count = BOOT_INFO.cpu_apic_id_count.saturating_add(1);
            }
            if info.is_bsp() {
                BOOT_INFO.bsp_apic_id = info.processor_id as u32;
            }
        }
    }
}

unsafe fn launch_secondary_cpus(
    bt: &BootServices,
    boot_info_ptr: *mut BootInfo,
    secondary_entry: u64,
) {
    if BOOT_INFO.cpu_enabled <= 1 {
        return;
    }

    let handle = match bt.get_handle_for_protocol::<MpServices>() {
        Ok(handle) => handle,
        Err(_) => {
            vga_println!("MpServices unavailable; secondary CPUs remain offline");
            return;
        }
    };

    let mp = match bt.open_protocol_exclusive::<MpServices>(handle) {
        Ok(mp) => mp,
        Err(e) => {
            vga_println!("MpServices open failed for AP start: {:?}", e.status());
            return;
        }
    };

    let handoff = addr_of_mut!(SMP_HANDOFF);
    unsafe {
        (*handoff)
            .boot_info_ptr
            .store(boot_info_ptr as u64, Ordering::Release);
        (*handoff)
            .kernel_secondary_entry
            .store(secondary_entry, Ordering::Release);
        (*handoff).kernel_cr3.store(0, Ordering::Release);
        (*handoff).ready.store(0, Ordering::Release);
        (*handoff).ap_count.store(0, Ordering::Release);
    }

    let handoff_ptr = handoff as *mut core::ffi::c_void;
    match mp.startup_all_aps(
        false,
        ap_bootstrap,
        handoff_ptr,
        None,
        Some(Duration::from_millis(10)),
    ) {
        Ok(()) => vga_println!("AP startup completed"),
        Err(e) => {
            if e.status() == Status::TIMEOUT {
                vga_println!("AP startup timed out; continuing boot");
            } else {
                vga_println!("AP startup failed: {:?}", e.status());
            }
        }
    }
}

extern "efiapi" fn ap_bootstrap(arg: *mut core::ffi::c_void) {
    let handoff = unsafe { &*(arg as *const SmpHandoff) };
    loop {
        if handoff.ready.load(Ordering::Acquire) != 0 {
            break;
        }
        core::hint::spin_loop();
    }

    let boot_info_ptr = handoff.boot_info_ptr.load(Ordering::Acquire) as *const BootInfo;
    let secondary_entry = handoff.kernel_secondary_entry.load(Ordering::Acquire);
    let kernel_cr3 = handoff.kernel_cr3.load(Ordering::Acquire);
    if boot_info_ptr.is_null() || secondary_entry == 0 {
        loop {
            x86_64::instructions::hlt();
        }
    }

    if kernel_cr3 == 0 {
        loop {
            x86_64::instructions::hlt();
        }
    }

    mochios::mem::paging::switch_page_table(kernel_cr3);

    let entry: unsafe extern "sysv64" fn(*const BootInfo) -> ! =
        unsafe { core::mem::transmute(secondary_entry) };
    unsafe { entry(boot_info_ptr) }
}

#[inline]
fn tick_booting_gif() {}

/// `\system\initfs.img` を読み込んで物理アドレスとサイズを返す
unsafe fn load_initfs(bt: &BootServices, image_handle: Handle) -> (u64, usize) {
    let initfs_path = cstr16!(r"\system\initfs.img");

    // LoadedImage デバイスを優先
    let handles: alloc::vec::Vec<Handle> =
        if let Ok(li) = bt.open_protocol_exclusive::<LoadedImage>(image_handle) {
            if let Some(dev) = li.device() {
                drop(li);
                alloc::vec![dev]
            } else {
                bt.find_handles::<SimpleFileSystem>().unwrap_or_default()
            }
        } else {
            bt.find_handles::<SimpleFileSystem>().unwrap_or_default()
        };

    for handle in handles {
        tick_booting_gif();
        if let Some((addr, size)) = try_load_raw(bt, image_handle, handle, initfs_path, "initfs") {
            vga_println!("initfs loaded at {:#x} ({} bytes)", addr, size);
            return (addr, size);
        }
    }
    vga_println!("[WARN] initfs.img not found, initfs will be empty");
    (0, 0)
}

/// 指定ハンドルから任意ファイルをページ単位でロードし (物理アドレス, サイズ) を返す
unsafe fn try_load_raw(
    bt: &BootServices,
    agent: Handle,
    handle: Handle,
    path: &uefi::CStr16,
    label: &str,
) -> Option<(u64, usize)> {
    let mut sfs = bt
        .open_protocol::<SimpleFileSystem>(
            OpenProtocolParams {
                handle,
                agent,
                controller: None,
            },
            OpenProtocolAttributes::GetProtocol,
        )
        .ok()?;
    let mut root = sfs.open_volume().ok()?;
    let fh = root
        .open(path, FileMode::Read, FileAttribute::empty())
        .ok()?;
    let mut file = match fh.into_type().ok()? {
        FileType::Regular(f) => f,
        _ => return None,
    };
    let mut info_buf = [0u8; 512];
    let info = file.get_info::<FileInfo>(&mut info_buf).ok()?;
    let size = info.file_size() as usize;
    if size == 0 {
        return None;
    }
    vga_println!("{} size: {} bytes, reading...", label, size);
    let pages = (size + 0xFFF) / 0x1000;
    let addr = bt
        .allocate_pages(AllocateType::AnyPages, UefiMemType::LOADER_DATA, pages)
        .ok()?;
    let buf = core::slice::from_raw_parts_mut(addr as *mut u8, size);
    // 大きなファイルは UEFI Read() の上限があるためチャンク単位で読む
    let mut read_total = 0usize;
    while read_total < size {
        tick_booting_gif();
        let read_end = core::cmp::min(read_total + READ_CHUNK_BYTES, size);
        let chunk = &mut buf[read_total..read_end];
        match file.read(chunk) {
            Ok(0) => break, // EOF
            Ok(n) => read_total += n,
            Err(_) => return None,
        }
    }
    if read_total != size {
        vga_println!("[WARN] {}: read {} / {} bytes", label, read_total, size);
        return None;
    }
    tick_booting_gif();
    Some((addr, size))
}

/// `\system\kernel.elf` を読み込み、PT_LOAD セグメントを物理アドレスに展開してエントリアドレスを返す
unsafe fn load_kernel(bt: &BootServices, image_handle: Handle) -> Option<(u64, u64)> {
    let kernel_path = cstr16!(r"\system\kernel.elf");

    // LoadedImage からブートローダー自身のデバイスハンドルを取得して優先的に試みる
    match bt.open_protocol_exclusive::<LoadedImage>(image_handle) {
        Err(e) => vga_println!("LoadedImage open failed: {:?}", e.status()),
        Ok(loaded_image) => match loaded_image.device() {
            None => vga_println!("LoadedImage.device() = None"),
            Some(dev) => {
                drop(loaded_image);
                tick_booting_gif();
                if let Some(entry) = try_load_from(bt, image_handle, dev, kernel_path) {
                    return Some(entry);
                }
                vga_println!("try_load_from (device handle) failed");
            }
        },
    }

    // フォールバック: 全 SimpleFileSystem ハンドルをスキャンして kernel.elf を探す
    match bt.find_handles::<SimpleFileSystem>() {
        Err(e) => {
            vga_println!("find_handles failed: {:?}", e.status());
            return None;
        }
        Ok(sfs_handles) => {
            vga_println!("SFS handle count: {}", sfs_handles.len());
            for handle in sfs_handles {
                tick_booting_gif();
                if let Some(entry) = try_load_from(bt, image_handle, handle, kernel_path) {
                    return Some(entry);
                }
            }
        }
    }

    None
}

/// 指定 SFS ハンドルから kernel.elf のロードを試みる
unsafe fn try_load_from(
    bt: &BootServices,
    agent: Handle,
    handle: Handle,
    kernel_path: &uefi::CStr16,
) -> Option<(u64, u64)> {
    // GetProtocol で非排他的に開く（ファームウェアが既に開いていても失敗しない）
    let mut sfs = match bt.open_protocol::<SimpleFileSystem>(
        OpenProtocolParams {
            handle,
            agent,
            controller: None,
        },
        OpenProtocolAttributes::GetProtocol,
    ) {
        Ok(s) => s,
        Err(e) => {
            vga_println!("SFS open_protocol failed: {:?}", e.status());
            return None;
        }
    };
    let mut root = match sfs.open_volume() {
        Ok(r) => r,
        Err(e) => {
            vga_println!("open_volume failed: {:?}", e.status());
            return None;
        }
    };

    // カーネル ELF を開く
    let file_handle = match root.open(kernel_path, FileMode::Read, FileAttribute::empty()) {
        Ok(f) => f,
        Err(e) => {
            vga_println!("file open failed: {:?}", e.status());
            return None;
        }
    };
    let mut file = match file_handle.into_type().ok()? {
        FileType::Regular(f) => f,
        _ => {
            vga_println!("not a regular file");
            return None;
        }
    };

    // ファイルサイズを取得する
    let mut info_buf = [0u8; 512];
    let info = match file.get_info::<FileInfo>(&mut info_buf) {
        Ok(i) => i,
        Err(e) => {
            vga_println!("get_info failed: {:?}", e.status());
            return None;
        }
    };
    let file_size = info.file_size() as usize;
    vga_println!("kernel.elf size: {} bytes", file_size);

    // ELF ヘッダとプログラムヘッダを小さなスタックバッファで先読みし、
    // カーネルのロードアドレス範囲を確定してからページを先に確保する。
    // (フルバッファを AnyPages で先に確保すると 0x200000 に配置される場合があり、
    //  その後の Address 指定確保が NOT_FOUND で失敗するため順序を逆にする)
    const HDR_READ: usize = 16384; // 16 KiB: ELF ヘッダ + プログラムヘッダテーブルを包含
    let mut hdr_buf = [0u8; HDR_READ];
    let hdr_n = match file.read(&mut hdr_buf) {
        Ok(n) => n,
        Err(e) => {
            vga_println!("header read failed: {:?}", e.status());
            return None;
        }
    };

    // ELF マジック / クラス / アーキテクチャを検証
    let hdr = &*(hdr_buf.as_ptr() as *const Elf64Header);
    if &hdr.e_ident[0..4] != b"\x7fELF" || hdr.e_ident[4] != 2 || hdr.e_machine != 0x3E {
        vga_println!(
            "ELF check failed: ident={:?} machine={:#x}",
            &hdr.e_ident[0..4],
            hdr.e_machine
        );
        return None;
    }

    // PT_LOAD セグメント全体の物理アドレス範囲を計算する
    // (セグメントは隣接・重複することがあるため、個別確保は不可)
    let mut load_min = u64::MAX;
    let mut load_max = 0u64;
    for i in 0..hdr.e_phnum as usize {
        let phdr_offset = hdr.e_phoff as usize + i * hdr.e_phentsize as usize;
        if phdr_offset + size_of::<Elf64Phdr>() > hdr_n {
            break;
        }
        let phdr = &*(hdr_buf.as_ptr().add(phdr_offset) as *const Elf64Phdr);
        if phdr.p_type != PT_LOAD || phdr.p_memsz == 0 {
            continue;
        }
        load_min = load_min.min(phdr.p_paddr & !0xFFF);
        load_max = load_max.max((phdr.p_paddr + phdr.p_memsz + 0xFFF) & !0xFFF);
    }
    if load_min == u64::MAX {
        vga_println!("no PT_LOAD segments");
        return None;
    }

    // カーネルページをフルバッファより先に確保することで、
    // 後の AnyPages 確保が同アドレスに重ならないようにする
    let kernel_pages = ((load_max - load_min) as usize) / 0x1000;
    vga_println!(
        "kernel range {:#x}..{:#x} ({} pages)",
        load_min,
        load_max,
        kernel_pages
    );
    match bt.allocate_pages(
        AllocateType::Address(load_min),
        UefiMemType::LOADER_DATA,
        kernel_pages,
    ) {
        Ok(_) => {}
        Err(e) => {
            vga_println!("allocate_pages kernel failed: {:?}", e.status());
            // 診断: load_min 付近のメモリマップエントリを表示する
            if let Ok(mmap) = bt.memory_map(UefiMemType::LOADER_DATA) {
                vga_println!("memory map around {:#x}:", load_min);
                for desc in mmap.entries() {
                    let end = desc.phys_start + desc.page_count * 0x1000;
                    if end > load_min.saturating_sub(0x200000)
                        && desc.phys_start < load_min + 0x200000
                    {
                        vga_println!(
                            "  [{:#010x}..{:#010x}] type={:?}",
                            desc.phys_start,
                            end,
                            desc.ty
                        );
                    }
                }
            }
            return None;
        }
    }
    // 全体をゼロクリア（BSS を含む）
    core::ptr::write_bytes(load_min as *mut u8, 0, (load_max - load_min) as usize);

    // ファイルを先頭に巻き戻してフルバッファに再読み込みする。
    // カーネルページが確保済みなので AnyPages は別アドレスに配置される。
    if let Err(e) = file.set_position(0) {
        vga_println!("set_position failed: {:?}", e.status());
        return None;
    }
    let pages = (file_size + 0xFFF) / 0x1000;
    let buf_phys = match bt.allocate_pages(AllocateType::AnyPages, UefiMemType::LOADER_DATA, pages)
    {
        Ok(p) => p,
        Err(e) => {
            vga_println!("allocate_pages (buf) failed: {:?}", e.status());
            return None;
        }
    };
    let buf = core::slice::from_raw_parts_mut(buf_phys as *mut u8, file_size);
    let mut read_total = 0usize;
    while read_total < file_size {
        tick_booting_gif();
        let read_end = core::cmp::min(read_total + READ_CHUNK_BYTES, file_size);
        let chunk = &mut buf[read_total..read_end];
        match file.read(chunk) {
            Ok(0) => break,
            Ok(n) => read_total += n,
            Err(e) => {
                vga_println!("file read failed: {:?}", e.status());
                return None;
            }
        }
    }
    vga_println!("read {} / {} bytes", read_total, file_size);
    if read_total != file_size {
        vga_println!(
            "[WARN] kernel.elf: read {} / {} bytes",
            read_total,
            file_size
        );
        return None;
    }
    tick_booting_gif();

    // 以降のコピー・再配置処理は buf を参照するため、hdr を buf から再取得する
    let hdr = &*(buf.as_ptr() as *const Elf64Header);

    // 各 PT_LOAD セグメントのデータをコピー
    for i in 0..hdr.e_phnum as usize {
        let phdr_offset = hdr.e_phoff as usize + i * hdr.e_phentsize as usize;
        if phdr_offset + size_of::<Elf64Phdr>() > buf.len() {
            vga_println!("phdr OOB: offset={:#x}", phdr_offset);
            return None;
        }
        let phdr = &*(buf.as_ptr().add(phdr_offset) as *const Elf64Phdr);
        if phdr.p_type != PT_LOAD || phdr.p_filesz == 0 {
            continue;
        }
        if phdr.p_filesz > phdr.p_memsz {
            vga_println!("segment filesz>memsz: idx={}", i);
            return None;
        }
        let dst_end = match phdr.p_paddr.checked_add(phdr.p_memsz) {
            Some(v) => v,
            None => {
                vga_println!("segment paddr overflow: idx={}", i);
                return None;
            }
        };
        if phdr.p_paddr < load_min || dst_end > load_max {
            vga_println!("segment outside load range: idx={}", i);
            return None;
        }
        let src_start = phdr.p_offset as usize;
        let src_end = match src_start.checked_add(phdr.p_filesz as usize) {
            Some(v) => v,
            None => {
                vga_println!("segment offset overflow: idx={}", i);
                return None;
            }
        };
        if src_end > buf.len() {
            vga_println!("segment exceeds file: idx={} end={:#x}", i, src_end);
            return None;
        }
        let dst = core::slice::from_raw_parts_mut(phdr.p_paddr as *mut u8, phdr.p_filesz as usize);
        let src = &buf[src_start..src_end];
        dst.copy_from_slice(src);
    }

    // PT_DYNAMIC から RELA 再配置テーブルを探して R_X86_64_RELATIVE を適用する
    // ロードアドレス == リンクアドレス (0x4000000) なので load_base = 0
    let mut rela_addr = 0u64;
    let mut rela_size = 0usize;
    let mut rela_ent = size_of::<Elf64Rela>();
    for i in 0..hdr.e_phnum as usize {
        let phdr_offset = hdr.e_phoff as usize + i * hdr.e_phentsize as usize;
        let phdr = &*(buf.as_ptr().add(phdr_offset) as *const Elf64Phdr);
        if phdr.p_type != PT_DYNAMIC {
            continue;
        }
        let dyn_count = phdr.p_memsz as usize / size_of::<Elf64Dyn>();
        let dyn_ptr = phdr.p_paddr as *const Elf64Dyn;
        for j in 0..dyn_count {
            let entry = &*dyn_ptr.add(j);
            match entry.d_tag {
                DT_NULL => break,
                DT_RELA => rela_addr = entry.d_val,
                DT_RELASZ => rela_size = entry.d_val as usize,
                DT_RELAENT => rela_ent = entry.d_val as usize,
                _ => {}
            }
        }
        break;
    }
    if rela_addr != 0 && rela_size > 0 && rela_ent > 0 {
        let rela_count = rela_size / rela_ent;
        vga_println!("applying {} RELA relocations", rela_count);
        for i in 0..rela_count {
            let rela = &*((rela_addr as usize + i * rela_ent) as *const Elf64Rela);
            if (rela.r_info & 0xFFFF_FFFF) as u32 == R_X86_64_RELATIVE {
                let target = rela.r_offset as *mut u64;
                *target = rela.r_addend as u64; // load_base = 0
            }
        }
    }

    let secondary_entry = find_elf_symbol(buf, "secondary_cpu_entry")?;
    Some((hdr.e_entry, secondary_entry))
}

/// UEFI エントリーポイント
#[entry]
unsafe fn main(image_handle: Handle, mut system_table: SystemTable<Boot>) -> Status {
    if uefi::helpers::init(&mut system_table).is_err() {
        return Status::UNSUPPORTED;
    }

    // ── GOP フレームバッファを最初に取得してコンソールを初期化 ──────────────
    let (_fb_ptr, fb_addr, fb_size, screen_w, screen_h, stride) = {
        let gop_handle = match system_table
            .boot_services()
            .get_handle_for_protocol::<GraphicsOutput>()
        {
            Ok(h) => h,
            Err(_) => return Status::UNSUPPORTED,
        };
        let mut gop = match system_table
            .boot_services()
            .open_protocol_exclusive::<GraphicsOutput>(gop_handle)
        {
            Ok(g) => g,
            Err(_) => return Status::UNSUPPORTED,
        };
        let mode_info = gop.current_mode_info();
        let mut fb = gop.frame_buffer();
        let fb_ptr = fb.as_mut_ptr() as *mut u32;
        let fb_sz = fb.size();
        let (w, h) = mode_info.resolution();
        let st = mode_info.stride();
        vga_console::CONSOLE.lock().init(fb_ptr, w, h, st);
        (fb_ptr, fb_ptr as u64, fb_sz, w, h, st)
    };

    vga_println!("mochiOS bootloader");
    vga_println!("Framebuffer: {}x{} stride={}", screen_w, screen_h, stride);
    // booting.gif disabled; proceed without animation

    // カーネルをロード (boot_services の借用をスコープで切る)
    let kernel_entry_addrs = {
        let bt = system_table.boot_services();
        unsafe { load_kernel(bt, image_handle) }
    };
    let kernel_entry_addrs = match kernel_entry_addrs {
        Some(addrs) => addrs,
        None => {
            vga_println!("Failed to load kernel.elf");
            return Status::NOT_FOUND;
        }
    };

    // initfs を ESP から読み込む
    let (initfs_addr, initfs_size) = {
        let bt = system_table.boot_services();
        unsafe { load_initfs(bt, image_handle) }
    };

    // rootfs は起動後にFS層がマウントして利用するため、
    // ブートローダーではプリロードしない（起動時間短縮）
    let (rootfs_addr, rootfs_size) = (0u64, 0usize);

    {
        let bt = system_table.boot_services();
        unsafe {
            populate_cpu_info(bt);
            launch_secondary_cpus(bt, addr_of_mut!(BOOT_INFO), kernel_entry_addrs.1);
        }
    }

    // Boot services を終了してメモリマップを取得
    let (_system_table, memory_map_iter) =
        unsafe { system_table.exit_boot_services(UefiMemType::LOADER_DATA) };

    let map_count;
    unsafe {
        let mut count = 0usize;
        for (i, desc) in memory_map_iter.entries().enumerate() {
            if i >= 256 {
                break;
            }
            MEMORY_MAP[i] = MemoryRegion {
                start: desc.phys_start,
                len: desc.page_count * 4096,
                region_type: match desc.ty {
                    UefiMemType::CONVENTIONAL => MemoryType::Usable,
                    UefiMemType::ACPI_RECLAIM => MemoryType::AcpiReclaimable,
                    UefiMemType::ACPI_NON_VOLATILE => MemoryType::AcpiNvs,
                    UefiMemType::UNUSABLE => MemoryType::BadMemory,
                    UefiMemType::LOADER_CODE | UefiMemType::LOADER_DATA => {
                        MemoryType::BootloaderReclaimable
                    }
                    _ => MemoryType::Reserved,
                },
            };
            count += 1;
        }
        map_count = count;
    }

    #[allow(static_mut_refs)]
    unsafe {
        BOOT_INFO.physical_memory_offset = 0;
        BOOT_INFO.framebuffer_addr = fb_addr;
        BOOT_INFO.framebuffer_size = fb_size;
        BOOT_INFO.screen_width = screen_w;
        BOOT_INFO.screen_height = screen_h;
        BOOT_INFO.stride = stride;
        BOOT_INFO.memory_map_addr = MEMORY_MAP.as_ptr() as u64;
        BOOT_INFO.memory_map_len = map_count;
        BOOT_INFO.memory_map_entry_size = size_of::<MemoryRegion>();
        // kernel_heap_addr はカーネル自身が entry.rs 内で設定する
        BOOT_INFO.kernel_heap_addr = 0;
        BOOT_INFO.initfs_addr = initfs_addr;
        BOOT_INFO.initfs_size = initfs_size;
        BOOT_INFO.rootfs_addr = rootfs_addr;
        BOOT_INFO.rootfs_size = rootfs_size;
        BOOT_INFO.smp_handoff_addr = addr_of_mut!(SMP_HANDOFF) as u64;
        BOOT_INFO.smp_handoff_size = core::mem::size_of::<SmpHandoff>();
    }

    // カーネルへジャンプ (system V AMD64 ABI)
    let kernel_entry: unsafe extern "sysv64" fn(*mut BootInfo) -> ! =
        core::mem::transmute(kernel_entry_addrs.0);
    unsafe { kernel_entry(addr_of_mut!(BOOT_INFO)) }
}
