pub mod fs {
    pub use mochi_syscall::fs::{close_via_fs, open_via_fs, readdir};
    pub use mochi_syscall::fs_consts::{FS_PATH_MAX, IPC_MAX_MSG_SIZE, S_IFDIR, S_IFMT, S_IFREG};

    pub fn read_file(path: &str, max_size: usize) -> Option<Vec<u8>> {
        match mochi_syscall::fs::read_file_via_fs(path, max_size) {
            Ok(Some(data)) => Some(data),
            _ => None,
        }
    }

    pub fn read_file_via_fs(path: &str, max_size: usize) -> Result<Option<Vec<u8>>, i64> {
        mochi_syscall::fs::read_file_via_fs(path, max_size)
    }
}

pub mod ipc {
    pub const MAX_MSG_SIZE: usize = mochi_syscall::ipc::MAX_MSG_SIZE;
    pub const MAP_HEADER_MAGIC: u32 = mochi_syscall::ipc::MAP_HEADER_MAGIC;

    pub fn recv(buf: &mut [u8]) -> (u64, u64) {
        mochi_syscall::ipc::ipc_recv(buf)
    }

    pub fn send(dest_thread_id: u64, data: &[u8]) -> i64 {
        mochi_syscall::ipc::ipc_send(dest_thread_id, data) as i64
    }

    pub fn ipc_recv(buf: &mut [u8]) -> (u64, u64) {
        recv(buf)
    }

    pub fn ipc_send(dest_thread_id: u64, data: &[u8]) -> i64 {
        send(dest_thread_id, data)
    }
}

pub mod keyboard {
    pub fn read_scancode() -> Option<u8> {
        mochi_syscall::keyboard::read_scancode()
    }

    pub fn read_scancode_tap() -> Option<u8> {
        mochi_syscall::keyboard::read_scancode_tap().ok().flatten()
    }
}

pub mod privileged {
    pub unsafe fn alloc_shared_pages(
        num_pages: u64,
        out_phys: Option<&mut [u64]>,
        map_start: u64,
    ) -> u64 {
        unsafe { mochi_syscall::privileged::alloc_shared_pages(num_pages, out_phys, map_start) }
    }

    pub unsafe fn ipc_send_pages(dest_thread_id: u64, phys_pages: &[u64], map_start: u64) -> u64 {
        unsafe { mochi_syscall::privileged::ipc_send_pages(dest_thread_id, phys_pages, map_start) }
    }

    pub unsafe fn unmap_pages(virt_addr: u64, page_count: u64, deallocate: bool) -> u64 {
        mochi_syscall::privileged::unmap_pages(virt_addr, page_count, deallocate)
    }
}

pub mod process {
    pub fn exec_app(bundle_path: &str) -> Result<u64, i64> {
        mochi_syscall::process::exec_app_via_process_service(bundle_path)
    }
}

pub mod task {
    pub fn find_process_by_name(name: &str) -> Option<u64> {
        mochi_syscall::task::find_process_by_name(name)
    }

    pub fn gettid() -> u64 {
        mochi_syscall::task::gettid()
    }

    pub fn yield_now() {
        mochi_syscall::task::yield_now()
    }
}

pub mod time {
    pub fn sleep_ms(ms: u64) {
        mochi_syscall::time::sleep_ms(ms)
    }
}

pub mod user_space {
    pub fn looks_like_user_mapping(addr: u64, size: usize) -> bool {
        mochi_syscall::user_space::looks_like_user_mapping(addr, size)
    }
}

pub mod vga {
    pub use mochi_syscall::vga::FbInfo;

    #[cfg(feature = "hosted-vga")]
    pub fn host_init_framebuffer(width: u32, height: u32) {
        mochi_syscall::vga::host_init_framebuffer(width, height);
    }

    pub fn get_info() -> Option<FbInfo> {
        mochi_syscall::vga::get_info()
    }

    pub fn map_framebuffer() -> Option<*mut u32> {
        mochi_syscall::vga::map_framebuffer()
    }
}
