#![no_std]

extern crate alloc;
#[cfg(test)]
extern crate std;

use core::fmt::{self, Write};

pub use mochi_user_runtime as runtime;
pub use mochi_user_syscall as syscall;

pub mod io {
    use super::syscall::SysResult;

    pub const STDIN: u64 = 0;
    pub const STDOUT: u64 = 1;
    pub const STDERR: u64 = 2;

    pub fn write(fd: u64, bytes: &[u8]) -> SysResult<()> {
        super::runtime::write_all(fd, bytes)
    }

    pub fn stdout(bytes: &[u8]) -> SysResult<()> {
        write(STDOUT, bytes)
    }

    pub fn stderr(bytes: &[u8]) -> SysResult<()> {
        write(STDERR, bytes)
    }
}

struct FmtWriter(u64);

impl Write for FmtWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        io::write(self.0, s.as_bytes()).map_err(|_| fmt::Error)
    }
}

pub fn write_fmt(fd: u64, args: fmt::Arguments<'_>) -> syscall::SysResult<()> {
    let mut writer = FmtWriter(fd);
    writer
        .write_fmt(args)
        .map_err(|_| syscall::SysError::from_raw(syscall::EINVAL as i64))
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {{
        let _ = $crate::write_fmt($crate::io::STDOUT, format_args!($($arg)*));
    }};
}

#[macro_export]
macro_rules! println {
    () => {{
        let _ = $crate::write_fmt($crate::io::STDOUT, format_args!("\n"));
    }};
    ($($arg:tt)*) => {{
        let _ = $crate::write_fmt($crate::io::STDOUT, format_args!("{}\n", format_args!($($arg)*)));
    }};
}

pub mod thread {
    pub fn yield_now() {
        super::runtime::yield_now();
    }
}

pub mod process {
    pub fn exit(code: u64) -> ! {
        super::runtime::process_exit(code)
    }
}

pub mod time {
    use super::syscall::{self, SysResult};

    pub fn ticks() -> SysResult<u64> {
        syscall::call0(syscall::SyscallNumber::GetTicks)
    }
}

pub mod memory {
    use super::syscall::{self, SysResult};

    pub fn mmap(addr: u64, len: u64, prot: u64, flags: u64, fd: u64) -> SysResult<u64> {
        syscall::call5(
            syscall::SyscallNumber::MemoryMap,
            addr,
            len,
            prot,
            flags,
            fd,
        )
    }

    pub fn munmap(addr: u64, len: u64) -> SysResult<u64> {
        syscall::call2(syscall::SyscallNumber::MemoryUnmap, addr, len)
    }
}

pub mod file {
    use super::syscall::{self, SysResult};

    pub fn open(path_ptr: u64, flags: u64) -> SysResult<u64> {
        syscall::call2(syscall::SyscallNumber::FileOpen, path_ptr, flags)
    }

    pub fn openat(dirfd: i64, path_ptr: u64, flags: u64, mode: u64) -> SysResult<u64> {
        syscall::call4(
            syscall::SyscallNumber::FileOpenAt,
            dirfd as u64,
            path_ptr,
            flags,
            mode,
        )
    }

    pub fn close(fd: u64) -> SysResult<u64> {
        syscall::call1(syscall::SyscallNumber::FileClose, fd)
    }

    pub fn read(fd: u64, buf_ptr: u64, len: u64) -> SysResult<u64> {
        syscall::call3(syscall::SyscallNumber::FileRead, fd, buf_ptr, len)
    }

    pub fn write(fd: u64, buf_ptr: u64, len: u64) -> SysResult<u64> {
        syscall::call3(syscall::SyscallNumber::FileWrite, fd, buf_ptr, len)
    }

    pub fn seek(fd: u64, offset: i64, whence: u64) -> SysResult<u64> {
        syscall::call3(syscall::SyscallNumber::FileSeek, fd, offset as u64, whence)
    }
}

pub mod event {
    use super::syscall::{self, SysResult};

    pub fn create(flags: u64) -> SysResult<u64> {
        syscall::call2(syscall::SyscallNumber::EventCreate, flags, 0)
    }

    pub fn wait(event_id: u64, timeout_ms: u64) -> SysResult<u64> {
        syscall::call3(syscall::SyscallNumber::EventWait, event_id, timeout_ms, 0)
    }

    pub fn signal(event_id: u64) -> SysResult<u64> {
        syscall::call3(syscall::SyscallNumber::EventSignal, event_id, 0, 0)
    }

    pub fn poll(events_ptr: u64, count: u64, timeout_ms: u64) -> SysResult<u64> {
        syscall::call3(
            syscall::SyscallNumber::EventPoll,
            events_ptr,
            count,
            timeout_ms,
        )
    }
}

pub mod capability {
    use super::syscall::{self, SysResult};

    pub fn query(ptr: u64, len: u64) -> SysResult<u64> {
        syscall::call2(syscall::SyscallNumber::CapQuery, ptr, len)
    }
}

pub mod env {
    use super::syscall::{ENOSYS, SysError, SysResult};

    pub fn args() -> SysResult<&'static [&'static [u8]]> {
        Err(SysError::from_raw(ENOSYS as i64))
    }
}
