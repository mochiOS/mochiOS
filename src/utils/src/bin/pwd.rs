#![no_std]
#![no_main]

use swiftlib::{io, sys::SyscallNumber};

fn syscall2(num: u64, arg1: u64, arg2: u64) -> u64 {
    let result: u64;
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") num => result,
            in("rdi") arg1,
            in("rsi") arg2,
            lateout("rcx") _,
            lateout("r11") _,
        );
    }
    result
}

#[no_mangle]
pub extern "C" fn main(_argc: i32, _argv: *const *const u8) -> i32 {
    let mut buf = [0u8; 512];
    let ret = syscall2(
        SyscallNumber::Getcwd as u64,
        buf.as_mut_ptr() as u64,
        buf.len() as u64,
    );
    
    if ret == 0 || ret > 0xFFFF_FFFF_0000_0000 {
        io::print("pwd: error\n");
        return 1;
    }
    
    let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    if let Ok(path) = core::str::from_utf8(&buf[..len]) {
        io::print(path);
        io::print("\n");
        0
    } else {
        io::print("pwd: invalid path\n");
        1
    }
}
