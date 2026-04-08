#![no_std]
#![no_main]

use swiftlib::{io, sys::SyscallNumber};

fn syscall0(num: u64) -> u64 {
    let result: u64;
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") num => result,
            lateout("rcx") _,
            lateout("r11") _,
        );
    }
    result
}

fn print_number(n: u64) {
    let mut buf = [0u8; 20];
    let mut i = buf.len();
    let mut num = n;
    
    if num == 0 {
        io::print("0");
        return;
    }
    
    while num > 0 && i > 0 {
        i -= 1;
        buf[i] = b'0' + (num % 10) as u8;
        num /= 10;
    }
    
    if let Ok(s) = core::str::from_utf8(&buf[i..]) {
        io::print(s);
    }
}

#[no_mangle]
pub extern "C" fn main(_argc: i32, _argv: *const *const u8) -> i32 {
    let ticks = syscall0(SyscallNumber::GetTicks as u64);
    print_number(ticks);
    io::print(" ticks\n");
    0
}
