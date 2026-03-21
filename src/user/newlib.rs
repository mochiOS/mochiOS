//! Newlib サポート用のシステムコールグルーコード

use super::sys::{syscall1, syscall2, syscall3, SyscallNumber};

#[no_mangle]
pub extern "C" fn _write(fd: i32, buf: *const u8, len: usize) -> isize {
    syscall3(SyscallNumber::Write as u64, fd as u64, buf as u64, len as u64) as isize
}

#[no_mangle]
pub extern "C" fn write(fd: i32, buf: *const u8, len: usize) -> isize {
    _write(fd, buf, len)
}

#[no_mangle]
pub extern "C" fn _read(fd: i32, buf: *mut u8, len: usize) -> isize {
    syscall3(SyscallNumber::Read as u64, fd as u64, buf as u64, len as u64) as isize
}

#[no_mangle]
pub extern "C" fn read(fd: i32, buf: *mut u8, len: usize) -> isize {
    _read(fd, buf, len)
}

#[no_mangle]
pub extern "C" fn _close(fd: i32) -> i32 {
    syscall1(SyscallNumber::Close as u64, fd as u64) as i32
}

#[no_mangle]
pub extern "C" fn close(fd: i32) -> i32 {
    _close(fd)
}

#[no_mangle]
pub extern "C" fn _lseek(fd: i32, offset: isize, whence: i32) -> isize {
    syscall3(SyscallNumber::Lseek as u64, fd as u64, offset as u64, whence as u64) as isize
}

#[no_mangle]
pub extern "C" fn lseek(fd: i32, offset: isize, whence: i32) -> isize {
    _lseek(fd, offset, whence)
}

#[no_mangle]
pub extern "C" fn _exit(code: i32) -> ! {
    syscall1(SyscallNumber::Exit as u64, code as u64);
    loop {}
}

// exit は libc にあるので定義しなくてよいかも？でも _exit を呼ぶはず。
// ただし crt0 から呼ばれるのは _exit だったりする。

#[no_mangle]
pub extern "C" fn _fstat(fd: i32, stat: *mut u8) -> i32 {
    syscall2(SyscallNumber::Fstat as u64, fd as u64, stat as u64) as i32
}

#[no_mangle]
pub extern "C" fn fstat(fd: i32, stat: *mut u8) -> i32 {
    _fstat(fd, stat)
}

#[no_mangle]
pub extern "C" fn _isatty(fd: i32) -> i32 {
    // 簡易実装: 標準入出力(0,1,2)はTTYとみなす
    if fd >= 0 && fd <= 2 {
        1
    } else {
        0
    }
}

#[no_mangle]
pub extern "C" fn isatty(fd: i32) -> i32 {
    _isatty(fd)
}

#[no_mangle]
pub extern "C" fn _sbrk(incr: isize) -> *mut u8 {
    // brk は mmap/MMIO マッピングでも更新されるため、ユーザー側で末端を
    // キャッシュすると整合性が壊れてヒープ破壊につながる。
    // 毎回 brk(0) で現在値を取得してから更新する。
    let cur = syscall1(SyscallNumber::Brk as u64, 0);
    if cur == 0 || cur > 0xffff_ffff_ffff_f000 {
        return -1_isize as *mut u8;
    }
    let old_heap_end = cur;

    // 安全側に倒して縮小は未サポートにする（MMIO 併用時の破壊回避）。
    if incr < 0 {
        return -1_isize as *mut u8;
    }
    if incr == 0 {
        return old_heap_end as *mut u8;
    }

    let incr_u64 = incr as u64;
    let new_heap_end = match old_heap_end.checked_add(incr_u64) {
        Some(v) => v,
        None => return -1_isize as *mut u8,
    };
    let ret = syscall1(SyscallNumber::Brk as u64, new_heap_end);
    if ret == new_heap_end {
        old_heap_end as *mut u8
    } else {
        -1_isize as *mut u8
    }
}

#[no_mangle]
pub extern "C" fn sbrk(incr: isize) -> *mut u8 {
    _sbrk(incr)
}

#[no_mangle]
pub extern "C" fn _getpid() -> i32 {
    syscall1(SyscallNumber::GetPid as u64, 0) as i32
}

#[no_mangle]
pub extern "C" fn getpid() -> i32 {
    _getpid()
}

#[no_mangle]
pub extern "C" fn _kill(_pid: i32, _sig: i32) -> i32 {
    // 未実装
    -1
}

#[no_mangle]
pub extern "C" fn kill(pid: i32, sig: i32) -> i32 {
    _kill(pid, sig)
}
