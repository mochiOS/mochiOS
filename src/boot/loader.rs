#![no_std]
#![no_main]

extern crate alloc;

mod console;

use core::ptr::addr_of_mut;
use core::sync::atomic::Ordering;
use core::time::Duration;
use mochios::{BootInfo, MemoryRegion, MemoryType, SmpHandoff, MAX_CPU_IDS};
use uefi::prelude::*;
use uefi::proto::console::gop::GraphicsOutput;
use uefi::proto::loaded_image::LoadedImage;
use uefi::proto::media::file::{File, FileAttribute, FileInfo, FileMode, FileType, RegularFile};
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::proto::pi::mp::MpServices;
use uefi::table::boot::{
    AllocateType, MemoryType as UefiMemType, OpenProtocolAttributes, OpenProtocolParams,
};

macro_rules! sprint {
    ($($arg:tt)*) => {{
        $crate::console::_serial_print(format_args!($($arg)*));
    }};
}

macro_rules! vga_print {
    ($($arg:tt)*) => {{
        let _ = core::fmt::write(
            &mut *console::CONSOLE.lock(),
            format_args!($($arg)*),
        );
    }};
}

macro_rules! println {
    () => {{
        vga_print!("\n");
        sprint!("\n");
    }};
    ($($arg:tt)*) => {{
        vga_print!("[mBoot] {}\n", format_args!($($arg)*));
        sprint!("[mBoot] {}\n", format_args!($($arg)*));
    }};
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

/// ELF64 セクションヘッダ
#[repr(C)]
struct Elf64Shdr {
    sh_name: u32,
    sh_type: u32,
    sh_flags: u64,
    sh_addr: u64,
    sh_offset: u64,
    sh_size: u64,
    sh_link: u32,
    sh_info: u32,
    sh_addralign: u64,
    sh_entsize: u64,
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
const PAGE_SIZE: u64 = 0x1000;

#[inline]
fn align_up(value: u64, align: u64) -> u64 {
    (value + align - 1) & !(align - 1)
}

unsafe fn find_elf_symbol_in_file(
    file: &mut RegularFile,
    hdr_buf: &[u8],
    symbol_name: &str,
) -> Option<u64> {
    if hdr_buf.len() < core::mem::size_of::<Elf64Header>() {
        return None;
    }
    let eh = &*(hdr_buf.as_ptr() as *const Elf64Header);
    if &eh.e_ident[0..4] != b"\x7fELF" || eh.e_ident[4] != 2 || eh.e_machine != 0x3E {
        return None;
    }
    let shoff = eh.e_shoff as usize;
    let shentsz = eh.e_shentsize as usize;
    let shnum = eh.e_shnum as usize;
    if shoff == 0 || shentsz == 0 || shnum == 0 {
        return None;
    }

    let mut sh_buf = [0u8; 64];
    for si in 0..shnum {
        let sh_off = shoff + si * shentsz;
        if sh_off + core::mem::size_of::<Elf64Shdr>() > hdr_buf.len() {
            let pos = sh_off as u64;
            if file.set_position(pos).is_err() {
                return None;
            }
            if file
                .read(&mut sh_buf[..core::mem::size_of::<Elf64Shdr>()])
                .ok()?
                != core::mem::size_of::<Elf64Shdr>()
            {
                return None;
            }
            let sh = &*(sh_buf.as_ptr() as *const Elf64Shdr);
            if sh.sh_type != 2 && sh.sh_type != 11 {
                continue;
            }
            let symtab_size = usize::try_from(sh.sh_size).ok()?;
            let symtab_entsize = usize::try_from(sh.sh_entsize).ok()?;
            if symtab_entsize < 24 || symtab_size == 0 {
                continue;
            }
            let sh_link = sh.sh_link as usize;
            if sh_link >= shnum {
                continue;
            }

            let link_off = shoff + sh_link * shentsz;
            if file.set_position(link_off as u64).is_err() {
                return None;
            }
            if file
                .read(&mut sh_buf[..core::mem::size_of::<Elf64Shdr>()])
                .ok()?
                != core::mem::size_of::<Elf64Shdr>()
            {
                return None;
            }
            let str_sh = &*(sh_buf.as_ptr() as *const Elf64Shdr);
            let strtab_size = usize::try_from(str_sh.sh_size).ok()?;
            let strtab_offset = usize::try_from(str_sh.sh_offset).ok()?;
            let symtab_offset = usize::try_from(sh.sh_offset).ok()?;

            let mut symtab = alloc::vec![0u8; symtab_size];
            let mut strtab = alloc::vec![0u8; strtab_size];
            file.set_position(symtab_offset as u64).ok()?;
            if file.read(&mut symtab).ok()? != symtab_size {
                return None;
            }
            file.set_position(strtab_offset as u64).ok()?;
            if file.read(&mut strtab).ok()? != strtab_size {
                return None;
            }
            let nsyms = symtab_size / symtab_entsize;
            for i_sym in 0..nsyms {
                let sym_off = i_sym * symtab_entsize;
                if sym_off + 24 > symtab.len() {
                    break;
                }
                let st_name = u32::from_le_bytes([
                    symtab[sym_off],
                    symtab[sym_off + 1],
                    symtab[sym_off + 2],
                    symtab[sym_off + 3],
                ]) as usize;
                if st_name >= strtab.len() {
                    continue;
                }
                let st_value = u64::from_le_bytes([
                    symtab[sym_off + 8],
                    symtab[sym_off + 9],
                    symtab[sym_off + 10],
                    symtab[sym_off + 11],
                    symtab[sym_off + 12],
                    symtab[sym_off + 13],
                    symtab[sym_off + 14],
                    symtab[sym_off + 15],
                ]);
                let mut end = st_name;
                while end < strtab.len() && strtab[end] != 0 {
                    end += 1;
                }
                let Ok(name_str) = core::str::from_utf8(&strtab[st_name..end]) else {
                    continue;
                };
                if name_str == symbol_name {
                    return Some(st_value);
                }
            }
        }
    }
    None
}

unsafe fn read_kernel_meta_secondary_entry(root: &mut impl File) -> Option<u64> {
    let meta_path = cstr16!(r"\system\kernel.meta");
    let fh = root
        .open(meta_path, FileMode::Read, FileAttribute::empty())
        .ok()?;
    let mut file = match fh.into_type().ok()? {
        FileType::Regular(f) => f,
        _ => return None,
    };
    let mut buf = alloc::vec![0u8; 256];
    let read = file.read(&mut buf).ok()?;
    buf.truncate(read);
    let text = core::str::from_utf8(&buf).ok()?;
    for line in text.lines() {
        let Some(value) = line.strip_prefix("secondary_cpu_entry=0x") else {
            continue;
        };
        if let Ok(addr) = u64::from_str_radix(value.trim(), 16) {
            return Some(addr);
        }
    }
    None
}

unsafe fn populate_cpu_info(bt: &BootServices) {
    let handle = match bt.get_handle_for_protocol::<MpServices>() {
        Ok(handle) => handle,
        Err(_) => {
            println!("MpServices unavailable; assuming single CPU");
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
            println!("MpServices open failed: {:?}", e.status());
            return;
        }
    };

    let counts = match mp.get_number_of_processors() {
        Ok(counts) => counts,
        Err(e) => {
            println!("MpServices count failed: {:?}", e.status());
            return;
        }
    };

    BOOT_INFO.cpu_total = counts.total;
    BOOT_INFO.cpu_enabled = counts.enabled;
    BOOT_INFO.cpu_apic_id_count = 0;
    let cpu_total = BOOT_INFO.cpu_total;
    let cpu_enabled = BOOT_INFO.cpu_enabled;
    println!("CPU topology: total={} enabled={}", cpu_total, cpu_enabled);

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
    let bsp_apic_id = BOOT_INFO.bsp_apic_id;
    let cpu_apic_id_count = BOOT_INFO.cpu_apic_id_count;
    println!(
        "BSP APIC ID={} APIC list count={}",
        bsp_apic_id, cpu_apic_id_count
    );
}

unsafe fn launch_secondary_cpus(
    bt: &BootServices,
    boot_info_ptr: *mut BootInfo,
    secondary_entry: u64,
) {
    if secondary_entry == 0 {
        println!("secondary_cpu_entry unavailable; skipping AP startup");
        return;
    }
    if BOOT_INFO.cpu_enabled <= 1 {
        let cpu_enabled = BOOT_INFO.cpu_enabled;
        println!(
            "single CPU detected (enabled={}); skipping AP startup",
            cpu_enabled
        );
        return;
    }

    let handle = match bt.get_handle_for_protocol::<MpServices>() {
        Ok(handle) => handle,
        Err(_) => {
            println!("MpServices unavailable; secondary CPUs remain offline");
            return;
        }
    };

    let mp = match bt.open_protocol_exclusive::<MpServices>(handle) {
        Ok(mp) => mp,
        Err(e) => {
            println!("MpServices open failed for AP start: {:?}", e.status());
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
    let cpu_total = BOOT_INFO.cpu_total;
    let cpu_enabled = BOOT_INFO.cpu_enabled;
    println!(
        "Starting APs: total={} enabled={} secondary_entry={:#x} boot_info_ptr={:#x} handoff={:#x}",
        cpu_total, cpu_enabled, secondary_entry, boot_info_ptr as u64, handoff as u64
    );

    let handoff_ptr = handoff as *mut core::ffi::c_void;
    match mp.startup_all_aps(
        false,
        ap_bootstrap,
        handoff_ptr,
        None,
        Some(Duration::from_millis(10)),
    ) {
        Ok(()) => println!("AP startup completed"),
        Err(e) => {
            if e.status() == Status::TIMEOUT {
                println!("AP startup timed out; continuing boot");
            } else {
                println!("AP startup failed: {:?}", e.status());
            }
        }
    }
}

extern "efiapi" fn ap_bootstrap(arg: *mut core::ffi::c_void) {
    println!("AP bootstrap entered: arg={:#x}", arg as u64);
    let handoff = unsafe { &*(arg as *const SmpHandoff) };
    loop {
        if handoff.ready.load(Ordering::Acquire) != 0 {
            break;
        }
        core::hint::spin_loop();
    }
    println!(
        "AP bootstrap released: boot_info_ptr={:#x} secondary_entry={:#x} kernel_cr3={:#x}",
        handoff.boot_info_ptr.load(Ordering::Acquire),
        handoff.kernel_secondary_entry.load(Ordering::Acquire),
        handoff.kernel_cr3.load(Ordering::Acquire)
    );

    let boot_info_ptr = handoff.boot_info_ptr.load(Ordering::Acquire) as *const BootInfo;
    let secondary_entry = handoff.kernel_secondary_entry.load(Ordering::Acquire);
    let kernel_cr3 = handoff.kernel_cr3.load(Ordering::Acquire);
    println!(
        "AP handoff: boot_info_ptr={:#x} secondary_entry={:#x} kernel_cr3={:#x}",
        boot_info_ptr as u64, secondary_entry, kernel_cr3
    );
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
    println!("AP page table switched");

    let entry: unsafe extern "sysv64" fn(*const BootInfo) -> ! =
        unsafe { core::mem::transmute(secondary_entry) };
    println!("AP jumping to secondary_cpu_entry={:#x}", secondary_entry);
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
            println!("initfs loaded at {:#x} ({} bytes)", addr, size);
            return (addr, size);
        }
    }
    println!("[WARN] initfs.img not found, initfs will be empty");
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
    println!("{} size: {} bytes, reading...", label, size);
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
        println!("[WARN] {}: read {} / {} bytes", label, read_total, size);
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
        Err(e) => println!("LoadedImage open failed: {:?}", e.status()),
        Ok(loaded_image) => match loaded_image.device() {
            None => println!("LoadedImage.device() = None"),
            Some(dev) => {
                drop(loaded_image);
                tick_booting_gif();
                if let Some(entry) = try_load_from(bt, image_handle, dev, kernel_path) {
                    return Some(entry);
                }
                println!("try_load_from (device handle) failed");
            }
        },
    }

    // フォールバック: 全 SimpleFileSystem ハンドルをスキャンして kernel.elf を探す
    match bt.find_handles::<SimpleFileSystem>() {
        Err(e) => {
            println!("find_handles failed: {:?}", e.status());
            return None;
        }
        Ok(sfs_handles) => {
            println!("SFS handle count: {}", sfs_handles.len());
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
            println!("SFS open_protocol failed: {:?}", e.status());
            return None;
        }
    };
    let mut root = match sfs.open_volume() {
        Ok(r) => r,
        Err(e) => {
            println!("open_volume failed: {:?}", e.status());
            return None;
        }
    };

    let secondary_meta = read_kernel_meta_secondary_entry(&mut root);

    // カーネル ELF を開く
    let file_handle = match root.open(kernel_path, FileMode::Read, FileAttribute::empty()) {
        Ok(f) => f,
        Err(e) => {
            println!("file open failed: {:?}", e.status());
            return None;
        }
    };
    let mut file = match file_handle.into_type().ok()? {
        FileType::Regular(f) => f,
        _ => {
            println!("not a regular file");
            return None;
        }
    };

    // ファイルサイズを取得する
    let mut info_buf = [0u8; 512];
    let info = match file.get_info::<FileInfo>(&mut info_buf) {
        Ok(i) => i,
        Err(e) => {
            println!("get_info failed: {:?}", e.status());
            return None;
        }
    };
    let file_size = info.file_size() as usize;
    println!("kernel.elf size: {} bytes", file_size);

    // ELF ヘッダとプログラムヘッダを小さなスタックバッファで先読みし、
    // カーネルのロードアドレス範囲を確定してからページを先に確保する。
    // (フルバッファを AnyPages で先に確保すると 0x200000 に配置される場合があり、
    //  その後の Address 指定確保が NOT_FOUND で失敗するため順序を逆にする)
    const HDR_READ: usize = 16384; // 16 KiB: ELF ヘッダ + プログラムヘッダテーブルを包含
    let mut hdr_buf = [0u8; HDR_READ];
    let hdr_n = match file.read(&mut hdr_buf) {
        Ok(n) => n,
        Err(e) => {
            println!("header read failed: {:?}", e.status());
            return None;
        }
    };

    // ELF マジック / クラス / アーキテクチャを検証
    let hdr = &*(hdr_buf.as_ptr() as *const Elf64Header);
    if &hdr.e_ident[0..4] != b"\x7fELF" || hdr.e_ident[4] != 2 || hdr.e_machine != 0x3E {
        println!(
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
        println!("no PT_LOAD segments");
        return None;
    }

    let kernel_bytes = load_max - load_min;
    let kernel_pages = align_up(kernel_bytes, PAGE_SIZE) / PAGE_SIZE;
    let chosen_base = {
        let mmap = match bt.memory_map(UefiMemType::LOADER_DATA) {
            Ok(mmap) => mmap,
            Err(e) => {
                println!("memory_map failed: {:?}", e.status());
                return None;
            }
        };
        let mut selected = None;
        for desc in mmap.entries() {
            if desc.ty != UefiMemType::CONVENTIONAL {
                continue;
            }
            let region_start = align_up(desc.phys_start, PAGE_SIZE);
            let region_end = desc.phys_start + desc.page_count * PAGE_SIZE;
            if region_end <= region_start {
                continue;
            }
            if region_end - region_start < kernel_pages * PAGE_SIZE {
                continue;
            }
            selected = Some(region_start);
            break;
        }
        match selected {
            Some(base) => base,
            None => {
                println!(
                    "no conventional region large enough for kernel: need {} bytes",
                    kernel_bytes
                );
                return None;
            }
        }
    };
    let load_delta = chosen_base as i128 - load_min as i128;
    let load_end = chosen_base + kernel_bytes;
    println!(
        "kernel range {:#x}..{:#x} -> {:#x}..{:#x} ({} pages)",
        load_min, load_max, chosen_base, load_end, kernel_pages
    );
    match bt.allocate_pages(
        AllocateType::Address(chosen_base),
        UefiMemType::LOADER_DATA,
        kernel_pages as usize,
    ) {
        Ok(_) => {}
        Err(e) => {
            println!("allocate_pages kernel failed: {:?}", e.status());
            if let Ok(mmap) = bt.memory_map(UefiMemType::LOADER_DATA) {
                println!("memory map around {:#x}:", chosen_base);
                for desc in mmap.entries() {
                    let end = desc.phys_start + desc.page_count * PAGE_SIZE;
                    if end > chosen_base.saturating_sub(0x200000)
                        && desc.phys_start < chosen_base + 0x200000
                    {
                        println!(
                            "  [{:#010x}..{:#010x}] type={:?}",
                            desc.phys_start, end, desc.ty
                        );
                    }
                }
            }
            return None;
        }
    }
    core::ptr::write_bytes(chosen_base as *mut u8, 0, kernel_bytes as usize);

    if let Err(e) = file.set_position(0) {
        println!("set_position failed: {:?}", e.status());
        return None;
    }
    let hdr = &*(hdr_buf.as_ptr() as *const Elf64Header);

    // 各 PT_LOAD セグメントのデータをコピー
    for i in 0..hdr.e_phnum as usize {
        let phdr_offset = hdr.e_phoff as usize + i * hdr.e_phentsize as usize;
        if phdr_offset + size_of::<Elf64Phdr>() > hdr_n {
            println!("phdr OOB: offset={:#x}", phdr_offset);
            return None;
        }
        let phdr = &*(hdr_buf.as_ptr().add(phdr_offset) as *const Elf64Phdr);
        if phdr.p_type != PT_LOAD || phdr.p_filesz == 0 {
            continue;
        }
        if phdr.p_filesz > phdr.p_memsz {
            println!("segment filesz>memsz: idx={}", i);
            return None;
        }
        let dst_end = match phdr.p_paddr.checked_add(phdr.p_memsz) {
            Some(v) => v,
            None => {
                println!("segment paddr overflow: idx={}", i);
                return None;
            }
        };
        let dst_start = match (phdr.p_paddr as i128)
            .checked_add(load_delta)
            .and_then(|addr| u64::try_from(addr).ok())
        {
            Some(v) => v,
            None => {
                println!("segment base relocation overflow: idx={}", i);
                return None;
            }
        };
        let dst_end = match (dst_end as i128)
            .checked_add(load_delta)
            .and_then(|addr| u64::try_from(addr).ok())
        {
            Some(v) => v,
            None => {
                println!("segment end relocation overflow: idx={}", i);
                return None;
            }
        };
        if dst_start < chosen_base || dst_end > load_end {
            println!("segment outside load range: idx={}", i);
            return None;
        }
        if let Err(e) = file.set_position(phdr.p_offset) {
            println!("segment seek failed: idx={} status={:?}", i, e.status());
            return None;
        }
        let dst = core::slice::from_raw_parts_mut(dst_start as *mut u8, phdr.p_filesz as usize);
        let mut read_total = 0usize;
        while read_total < dst.len() {
            tick_booting_gif();
            let read_end = core::cmp::min(read_total + READ_CHUNK_BYTES, dst.len());
            let chunk = &mut dst[read_total..read_end];
            match file.read(chunk) {
                Ok(0) => break,
                Ok(n) => read_total += n,
                Err(e) => {
                    println!("segment read failed: idx={} status={:?}", i, e.status());
                    return None;
                }
            }
        }
        if read_total != dst.len() {
            println!(
                "segment read short: idx={} read={} expected={}",
                i,
                read_total,
                dst.len()
            );
            return None;
        }
    }

    // PT_DYNAMIC から RELA 再配置テーブルを探して R_X86_64_RELATIVE を適用する
    let mut rela_addr = 0u64;
    let mut rela_size = 0usize;
    let mut rela_ent = size_of::<Elf64Rela>();
    for i in 0..hdr.e_phnum as usize {
        let phdr_offset = hdr.e_phoff as usize + i * hdr.e_phentsize as usize;
        let phdr = &*(hdr_buf.as_ptr().add(phdr_offset) as *const Elf64Phdr);
        if phdr.p_type != PT_DYNAMIC {
            continue;
        }
        let dyn_count = phdr.p_memsz as usize / size_of::<Elf64Dyn>();
        let dyn_ptr = (phdr.p_paddr as i128 + load_delta)
            .try_into()
            .ok()
            .map(|addr: u64| addr as *const Elf64Dyn)?;
        for j in 0..dyn_count {
            let entry = &*dyn_ptr.add(j);
            match entry.d_tag {
                DT_NULL => break,
                DT_RELA => rela_addr = (entry.d_val as i128 + load_delta).try_into().ok()?,
                DT_RELASZ => rela_size = entry.d_val as usize,
                DT_RELAENT => rela_ent = entry.d_val as usize,
                _ => {}
            }
        }
        break;
    }
    if rela_addr != 0 && rela_size > 0 && rela_ent > 0 {
        let rela_count = rela_size / rela_ent;
        println!("applying {} RELA relocations", rela_count);
        for i in 0..rela_count {
            let rela = &*((rela_addr as usize + i * rela_ent) as *const Elf64Rela);
            if (rela.r_info & 0xFFFF_FFFF) as u32 == R_X86_64_RELATIVE {
                let target_addr: u64 = match (rela.r_offset as i128 + load_delta).try_into() {
                    Ok(addr) => addr,
                    Err(_) => {
                        println!(
                            "RELA target relocation overflow: idx={} offset={:#x} addend={:#x} delta={:#x}",
                            i,
                            rela.r_offset,
                            rela.r_addend,
                            load_delta
                        );
                        return None;
                    }
                };
                let value: u64 = match (rela.r_addend as i128 + load_delta).try_into() {
                    Ok(addr) => addr,
                    Err(_) => {
                        println!(
                            "RELA value relocation overflow: idx={} offset={:#x} addend={:#x} delta={:#x}",
                            i,
                            rela.r_offset,
                            rela.r_addend,
                            load_delta
                        );
                        return None;
                    }
                };
                unsafe { *(target_addr as *mut u64) = value };
            }
        }
    }

    let entry = (hdr.e_entry as i128 + load_delta).try_into().ok()?;
    let secondary_entry = if let Some(meta_addr) = secondary_meta {
        match (meta_addr as i128 + load_delta).try_into().ok() {
            Some(addr) => addr,
            None => {
                println!("kernel.meta secondary_cpu_entry out of range; falling back to symtab");
                0
            }
        }
    } else {
        println!("kernel.meta missing or unreadable; falling back to symtab");
        match find_elf_symbol_in_file(&mut file, &hdr_buf, "secondary_cpu_entry") {
            Some(ptr) => match (ptr as i128 + load_delta).try_into().ok() {
                Some(addr) => addr,
                None => {
                    println!("secondary_cpu_entry relocation out of range");
                    0
                }
            },
            None => {
                println!("secondary_cpu_entry symbol not found");
                0
            }
        }
    };
    Some((entry, secondary_entry))
}

/// UEFI エントリーポイント
#[entry]
unsafe fn main(image_handle: Handle, mut system_table: SystemTable<Boot>) -> Status {
    if uefi::helpers::init(&mut system_table).is_err() {
        return Status::UNSUPPORTED;
    }

    // GOP フレームバッファを最初に取得してコンソールを初期化
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
        console::CONSOLE.lock().init(fb_ptr, w, h, st);
        (fb_ptr, fb_ptr as u64, fb_sz, w, h, st)
    };

    *console::SERIAL.lock() = Some(console::SerialConsole::new(0x3F8));

    if let Some(serial) = console::SERIAL.lock().as_mut() {
        serial.init();
    }

    println!("mochiOS bootloader");
    println!("Framebuffer: {}x{} stride={}", screen_w, screen_h, stride);

    // カーネルをロード
    let kernel_entry_addrs = {
        let bt = system_table.boot_services();
        unsafe { load_kernel(bt, image_handle) }
    };
    let kernel_entry_addrs = match kernel_entry_addrs {
        Some(addrs) => addrs,
        None => {
            println!("Failed to load kernel.elf");
            return Status::NOT_FOUND;
        }
    };

    // initfsをESPから読み込む
    let (initfs_addr, initfs_size) = {
        let bt = system_table.boot_services();
        unsafe { load_initfs(bt, image_handle) }
    };

    // rootfsは起動後にFS層がマウントして利用するため、
    // ブートローダーではプリロードしない
    let (rootfs_addr, rootfs_size) = (0u64, 0usize);

    {
        let bt = system_table.boot_services();
        unsafe {
            populate_cpu_info(bt);
            launch_secondary_cpus(bt, addr_of_mut!(BOOT_INFO), kernel_entry_addrs.1);
        }
    }

    // Boot servicesを終了してメモリマップを取得
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
