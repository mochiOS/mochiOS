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
    const RECORD_SIZE: usize = 88;
    let mut buf = [0u8; 4096];
    let ret = syscall2(SyscallNumber::ListProcesses as u64, buf.as_mut_ptr() as u64, buf.len() as u64);
    if ret == u64::MAX { io::print("ps: error\n"); return 1; }
    let entries = ret as usize;
    for i in 0..entries {
        let off = i * RECORD_SIZE;
        let tid = u64::from_ne_bytes(match buf[off..off+8].try_into() { Ok(a)=>a, Err(_) => [0u8;8] });
        let pid = u64::from_ne_bytes(match buf[off+8..off+16].try_into() { Ok(a)=>a, Err(_) => [0u8;8] });
        let state = u64::from_ne_bytes(match buf[off+16..off+24].try_into() { Ok(a)=>a, Err(_) => [0u8;8] });
        let name_bytes = &buf[off+32..off+96];
        let mut name_len = 0usize;
        while name_len < name_bytes.len() && name_bytes[name_len] != 0 { name_len += 1; }
        let name = core::str::from_utf8(&name_bytes[..name_len]).unwrap_or("<invalid>");
        let state_str = match state {
            0 => "Ready",
            1 => "Running",
            2 => "Blocked",
            3 => "Sleeping",
            4 => "Terminated",
            _ => "Unknown",
        };
        print_number(pid);
        io::print(" ");
        print_number(tid);
        io::print(" ");
        io::print(state_str);
        io::print(" ");
        io::print(name);
        io::print("\n");
    }
    0
}
