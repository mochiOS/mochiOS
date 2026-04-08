#![no_std]
#![no_main]

use swiftlib::{io, sys::SyscallNumber};

fn syscall1(num: u64, arg1: u64) -> u64 {
    let result: u64;
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") num => result,
            in("rdi") arg1,
            lateout("rcx") _,
            lateout("r11") _,
        );
    }
    result
}

fn parse_number(s: &str) -> Option<u64> {
    let mut result = 0u64;
    for &b in s.as_bytes() {
        if b >= b'0' && b <= b'9' {
            result = result * 10 + (b - b'0') as u64;
        } else {
            return None;
        }
    }
    Some(result)
}

#[no_mangle]
pub extern "C" fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        io::print("Usage: sleep <seconds>\n");
        return 1;
    }
    
    let secs = unsafe {
        let arg_ptr = *argv.offset(1);
        if arg_ptr.is_null() {
            return 1;
        }
        let mut len = 0;
        while *arg_ptr.offset(len) != 0 {
            len += 1;
        }
        match core::str::from_utf8(core::slice::from_raw_parts(arg_ptr, len as usize)) {
            Ok(s) => s,
            Err(_) => return 1,
        }
    };
    
    if let Some(n) = parse_number(secs) {
        syscall1(SyscallNumber::Sleep as u64, n * 1000);
        0
    } else {
        io::print("sleep: invalid number\n");
        1
    }
}
