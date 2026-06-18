#![no_std]

#[cfg(test)]
extern crate std;

use core::arch::asm;

pub use mnu_abi::{
    EACCES, EAGAIN, EBADF, EEXIST, EFAULT, EINVAL, EIO, EISDIR, EMFILE, ENODATA, ENOENT, ENOMEM,
    ENOSPC, ENOSYS, ENOTDIR, ENOTSUP, ENOTTY, ENXIO, EPERM, EPIPE, ERANGE, ESRCH, SUCCESS,
    SyscallNumber,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SysError {
    raw: i64,
}

pub type SysResult<T> = Result<T, SysError>;

impl SysError {
    pub const fn from_raw(raw: i64) -> Self {
        Self { raw }
    }

    pub const fn raw(self) -> i64 {
        self.raw
    }

    pub fn errno(self) -> Option<u64> {
        if self.raw < 0 {
            Some((-self.raw) as u64)
        } else {
            None
        }
    }
}

impl From<u64> for SysError {
    fn from(raw: u64) -> Self {
        Self { raw: raw as i64 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RawSyscallResult {
    raw: u64,
}

impl RawSyscallResult {
    pub const fn new(raw: u64) -> Self {
        Self { raw }
    }

    pub const fn raw(self) -> u64 {
        self.raw
    }

    pub fn into_result(self) -> SysResult<u64> {
        let signed = self.raw as i64;
        if signed < 0 {
            Err(SysError::from_raw(signed))
        } else {
            Ok(self.raw)
        }
    }
}

#[inline(always)]
#[allow(unsafe_op_in_unsafe_fn)]
pub unsafe fn syscall0(number: u64) -> u64 {
    let ret: u64;
    asm!(
    "syscall",
    inlateout("rax") number => ret,
    lateout("rcx") _,
    lateout("r11") _,
    options(nostack)
    );
    ret
}

#[inline(always)]
#[allow(unsafe_op_in_unsafe_fn)]
pub unsafe fn syscall1(number: u64, arg0: u64) -> u64 {
    let ret: u64;
    asm!(
    "syscall",
    inlateout("rax") number => ret,
    in("rdi") arg0,
    lateout("rcx") _,
    lateout("r11") _,
    options(nostack)
    );
    ret
}

#[inline(always)]
#[allow(unsafe_op_in_unsafe_fn)]
pub unsafe fn syscall2(number: u64, arg0: u64, arg1: u64) -> u64 {
    let ret: u64;
    asm!(
    "syscall",
    inlateout("rax") number => ret,
    in("rdi") arg0,
    in("rsi") arg1,
    lateout("rcx") _,
    lateout("r11") _,
    options(nostack)
    );
    ret
}

#[inline(always)]
#[allow(unsafe_op_in_unsafe_fn)]
pub unsafe fn syscall3(number: u64, arg0: u64, arg1: u64, arg2: u64) -> u64 {
    let ret: u64;
    asm!(
    "syscall",
    inlateout("rax") number => ret,
    in("rdi") arg0,
    in("rsi") arg1,
    in("rdx") arg2,
    lateout("rcx") _,
    lateout("r11") _,
    options(nostack)
    );
    ret
}

#[inline(always)]
#[allow(unsafe_op_in_unsafe_fn)]
pub unsafe fn syscall4(number: u64, arg0: u64, arg1: u64, arg2: u64, arg3: u64) -> u64 {
    let ret: u64;
    asm!(
    "syscall",
    inlateout("rax") number => ret,
    in("rdi") arg0,
    in("rsi") arg1,
    in("rdx") arg2,
    in("r10") arg3,
    lateout("rcx") _,
    lateout("r11") _,
    options(nostack)
    );
    ret
}

#[inline(always)]
#[allow(unsafe_op_in_unsafe_fn)]
pub unsafe fn syscall5(number: u64, arg0: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64) -> u64 {
    let ret: u64;
    asm!(
    "syscall",
    inlateout("rax") number => ret,
    in("rdi") arg0,
    in("rsi") arg1,
    in("rdx") arg2,
    in("r10") arg3,
    in("r8") arg4,
    lateout("rcx") _,
    lateout("r11") _,
    options(nostack)
    );
    ret
}

#[inline(always)]
#[allow(unsafe_op_in_unsafe_fn)]
pub unsafe fn syscall6(
    number: u64,
    arg0: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
) -> u64 {
    let ret: u64;
    asm!(
    "syscall",
    inlateout("rax") number => ret,
    in("rdi") arg0,
    in("rsi") arg1,
    in("rdx") arg2,
    in("r10") arg3,
    in("r8") arg4,
    in("r9") arg5,
    lateout("rcx") _,
    lateout("r11") _,
    options(nostack)
    );
    ret
}

#[inline(always)]
pub fn raw_syscall0(number: SyscallNumber) -> RawSyscallResult {
    RawSyscallResult::new(unsafe { syscall0(number as u64) })
}

#[inline(always)]
pub fn raw_syscall1(number: SyscallNumber, arg0: u64) -> RawSyscallResult {
    RawSyscallResult::new(unsafe { syscall1(number as u64, arg0) })
}

#[inline(always)]
pub fn raw_syscall2(number: SyscallNumber, arg0: u64, arg1: u64) -> RawSyscallResult {
    RawSyscallResult::new(unsafe { syscall2(number as u64, arg0, arg1) })
}

#[inline(always)]
pub fn raw_syscall3(number: SyscallNumber, arg0: u64, arg1: u64, arg2: u64) -> RawSyscallResult {
    RawSyscallResult::new(unsafe { syscall3(number as u64, arg0, arg1, arg2) })
}

#[inline(always)]
pub fn raw_syscall4(
    number: SyscallNumber,
    arg0: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
) -> RawSyscallResult {
    RawSyscallResult::new(unsafe { syscall4(number as u64, arg0, arg1, arg2, arg3) })
}

#[inline(always)]
pub fn raw_syscall5(
    number: SyscallNumber,
    arg0: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
) -> RawSyscallResult {
    RawSyscallResult::new(unsafe { syscall5(number as u64, arg0, arg1, arg2, arg3, arg4) })
}

#[inline(always)]
pub fn raw_syscall6(
    number: SyscallNumber,
    arg0: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
) -> RawSyscallResult {
    RawSyscallResult::new(unsafe { syscall6(number as u64, arg0, arg1, arg2, arg3, arg4, arg5) })
}

#[inline(always)]
pub fn call0(number: SyscallNumber) -> SysResult<u64> {
    raw_syscall0(number).into_result()
}

#[inline(always)]
pub fn call1(number: SyscallNumber, arg0: u64) -> SysResult<u64> {
    raw_syscall1(number, arg0).into_result()
}

#[inline(always)]
pub fn call2(number: SyscallNumber, arg0: u64, arg1: u64) -> SysResult<u64> {
    raw_syscall2(number, arg0, arg1).into_result()
}

#[inline(always)]
pub fn call3(number: SyscallNumber, arg0: u64, arg1: u64, arg2: u64) -> SysResult<u64> {
    raw_syscall3(number, arg0, arg1, arg2).into_result()
}

#[inline(always)]
pub fn call4(number: SyscallNumber, arg0: u64, arg1: u64, arg2: u64, arg3: u64) -> SysResult<u64> {
    raw_syscall4(number, arg0, arg1, arg2, arg3).into_result()
}

#[inline(always)]
pub fn call5(
    number: SyscallNumber,
    arg0: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
) -> SysResult<u64> {
    raw_syscall5(number, arg0, arg1, arg2, arg3, arg4).into_result()
}

#[inline(always)]
pub fn call6(
    number: SyscallNumber,
    arg0: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
) -> SysResult<u64> {
    raw_syscall6(number, arg0, arg1, arg2, arg3, arg4, arg5).into_result()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_error_detection() {
        assert!(SysError::from_raw(-22).errno() == Some(22));
        assert!(
            RawSyscallResult::new((-12i64) as u64)
                .into_result()
                .is_err()
        );
        assert!(RawSyscallResult::new(0).into_result().is_ok());
        assert!(RawSyscallResult::new(7).into_result().is_ok());
    }
}
