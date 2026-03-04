#![no_std]
#![no_main]

extern crate alloc;

use core::ptr::addr_of_mut;
use swiftcore::{BootInfo, MemoryRegion, MemoryType};
use uefi::prelude::*;
use uefi::proto::console::gop::GraphicsOutput;
use uefi::proto::loaded_image::LoadedImage;
use uefi::proto::media::file::{File, FileAttribute, FileMode, FileInfo, FileType};
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::table::boot::{AllocateType, MemoryType as UefiMemType};

#[global_allocator]
static ALLOCATOR: uefi::allocator::Allocator = uefi::allocator::Allocator;

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
};

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

/// `\System\kernel.elf` を読み込み、PT_LOAD セグメントを物理アドレスに展開してエントリアドレスを返す
unsafe fn load_kernel(bt: &BootServices, image_handle: Handle) -> Option<u64> {
    // ブートローダーが存在するデバイス（ESP）を取得
    let loaded_image = bt
        .open_protocol_exclusive::<LoadedImage>(image_handle)
        .ok()?;
    let device_handle = loaded_image.device()?;
    let mut sfs = bt
        .open_protocol_exclusive::<SimpleFileSystem>(device_handle)
        .ok()?;
    let mut root = sfs.open_volume().ok()?;

    // カーネル ELF を開く
    let kernel_path = cstr16!(r"\System\kernel.elf");
    let file_handle = root
        .open(kernel_path, FileMode::Read, FileAttribute::empty())
        .ok()?;
    let mut file = match file_handle.into_type().ok()? {
        FileType::Regular(f) => f,
        _ => return None,
    };

    // ファイルサイズを取得して一時バッファに読み込む
    let mut info_buf = [0u8; 512];
    let info = file.get_info::<FileInfo>(&mut info_buf).ok()?;
    let file_size = info.file_size() as usize;
    let pages = (file_size + 0xFFF) / 0x1000;
    let buf_phys = bt
        .allocate_pages(AllocateType::AnyPages, UefiMemType::LOADER_DATA, pages)
        .ok()?;
    let buf = core::slice::from_raw_parts_mut(buf_phys as *mut u8, file_size);
    file.read(buf).ok()?;

    // ELF マジック / クラス / アーキテクチャを検証
    let hdr = &*(buf.as_ptr() as *const Elf64Header);
    if &hdr.e_ident[0..4] != b"\x7fELF" || hdr.e_ident[4] != 2 || hdr.e_machine != 0x3E {
        return None;
    }

    // PT_LOAD セグメントを物理アドレスに展開
    for i in 0..hdr.e_phnum as usize {
        let phdr_offset = hdr.e_phoff as usize + i * hdr.e_phentsize as usize;
        let phdr = &*(buf.as_ptr().add(phdr_offset) as *const Elf64Phdr);
        if phdr.p_type != PT_LOAD || phdr.p_memsz == 0 {
            continue;
        }
        let seg_pages = (phdr.p_memsz as usize + 0xFFF) / 0x1000;
        bt.allocate_pages(
            AllocateType::Address(phdr.p_paddr),
            UefiMemType::LOADER_DATA,
            seg_pages,
        )
        .ok()?;

        let dst = core::slice::from_raw_parts_mut(phdr.p_paddr as *mut u8, phdr.p_memsz as usize);
        let src = &buf[phdr.p_offset as usize..phdr.p_offset as usize + phdr.p_filesz as usize];
        dst[..phdr.p_filesz as usize].copy_from_slice(src);
        // BSS ゼロ埋め
        dst[phdr.p_filesz as usize..].fill(0);
    }

    Some(hdr.e_entry)
}

/// UEFI エントリーポイント
#[entry]
unsafe fn main(image_handle: Handle, mut system_table: SystemTable<Boot>) -> Status {
    if uefi::helpers::init(&mut system_table).is_err() {
        return Status::UNSUPPORTED;
    }

    let _ = system_table.stdout().clear();
    let _ = system_table
        .stdout()
        .output_string(cstr16!("SwiftCore bootloader\n"));

    // フレームバッファ情報を取得
    let (fb_addr, fb_size, screen_w, screen_h, stride) = {
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
        (
            fb.as_mut_ptr() as u64,
            fb.size(),
            mode_info.resolution().0,
            mode_info.resolution().1,
            mode_info.stride(),
        )
    };

    // カーネル ELF をロード (Boot Services が有効な間に行う)
    let kernel_entry_addr =
        match unsafe { load_kernel(system_table.boot_services(), image_handle) } {
            Some(addr) => addr,
            None => {
                let _ = system_table
                    .stdout()
                    .output_string(cstr16!("Failed to load kernel.elf\n"));
                return Status::NOT_FOUND;
            }
        };

    // Boot Services を終了してメモリマップを取得
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
        BOOT_INFO.memory_map_entry_size = core::mem::size_of::<MemoryRegion>();
        // kernel_heap_addr はカーネル自身が entry.rs 内で設定する
        BOOT_INFO.kernel_heap_addr = 0;
    }

    // カーネルへジャンプ (System V AMD64 ABI)
    let kernel_entry: unsafe extern "sysv64" fn(*mut BootInfo) -> ! =
        core::mem::transmute(kernel_entry_addr);
    unsafe { kernel_entry(addr_of_mut!(BOOT_INFO)) }
}

