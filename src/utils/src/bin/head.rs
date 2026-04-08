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

fn syscall3(num: u64, arg1: u64, arg2: u64, arg3: u64) -> u64 {
    let result: u64;
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") num => result,
            in("rdi") arg1,
            in("rsi") arg2,
            in("rdx") arg3,
            lateout("rcx") _,
            lateout("r11") _,
        );
    }
    result
}

fn open(path: &str) -> i64 {
    let mut buf = [0u8; 512];
    let bytes = path.as_bytes();
    let len = bytes.len().min(511);
    buf[..len].copy_from_slice(&bytes[..len]);
    syscall2(SyscallNumber::Open as u64, buf.as_ptr() as u64, 0) as i64
}

fn read(fd: u64, buf: &mut [u8]) -> i64 {
    syscall3(
        SyscallNumber::Read as u64,
        fd,
        buf.as_mut_ptr() as u64,
        buf.len() as u64,
    ) as i64
}

fn close(fd: u64) {
    syscall1(SyscallNumber::Close as u64, fd);
}

#[no_mangle]
pub extern "C" fn main(argc: i32, argv: *const *const u8) -> i32 {
    let lines = 10;
    let start_idx = 1;
    
    if argc <= start_idx {
        io::print("Usage: head <file>...\n");
        return 1;
    }
    
    for i in start_idx..argc {
        let path = unsafe {
            let arg_ptr = *argv.offset(i as isize);
            if arg_ptr.is_null() {
                continue;
            }
            let mut len = 0;
            while *arg_ptr.offset(len) != 0 {
                len += 1;
            }
            match core::str::from_utf8(core::slice::from_raw_parts(arg_ptr, len as usize)) {
                Ok(s) => s,
                Err(_) => continue,
            }
        };
        
        let fd = open(path);
        if fd < 0 {
            io::print("head: ");
            io::print(path);
            io::print(": cannot open\n");
            continue;
        }
        
        let fd_u = fd as u64;
        let mut buf = [0u8; 2048];
        let mut line_count = 0;
        
        loop {
            let n = read(fd_u, &mut buf);
            if n <= 0 {
                break;
            }
            
            for &byte in &buf[..n as usize] {
                if byte == b'\n' {
                    line_count += 1;
                    if line_count >= lines {
                        break;
                    }
                }
                let ch = [byte];
                if let Ok(s) = core::str::from_utf8(&ch) {
                    io::print(s);
                }
            }
            
            if line_count >= lines {
                break;
            }
        }
        
        close(fd_u);
    }
    
    0
}
