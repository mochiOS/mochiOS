#![no_std]
#![no_main]

extern crate alloc;
use alloc::vec::Vec;
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

fn contains_pattern(line: &[u8], pattern: &[u8]) -> bool {
    if pattern.is_empty() {
        return true;
    }
    
    if line.len() < pattern.len() {
        return false;
    }
    
    for i in 0..=(line.len() - pattern.len()) {
        let mut matches = true;
        for j in 0..pattern.len() {
            if line[i + j] != pattern[j] {
                matches = false;
                break;
            }
        }
        if matches {
            return true;
        }
    }
    false
}

#[no_mangle]
pub extern "C" fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        io::print("Usage: grep <pattern> [file...]\n");
        return 1;
    }
    
    let pattern = unsafe {
        let arg_ptr = *argv.offset(1);
        if arg_ptr.is_null() {
            return 1;
        }
        let mut len = 0;
        while *arg_ptr.offset(len) != 0 {
            len += 1;
        }
        core::slice::from_raw_parts(arg_ptr, len as usize)
    };
    
    if argc < 3 {
        io::print("grep: no files specified\n");
        return 1;
    }
    
    for i in 2..argc {
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
            io::print("grep: cannot open: ");
            io::print(path);
            io::print("\n");
            continue;
        }
        
        let mut content = Vec::new();
        let mut buf = [0u8; 2048];
        
        loop {
            let n = read(fd as u64, &mut buf);
            if n <= 0 {
                break;
            }
            content.extend_from_slice(&buf[..n as usize]);
        }
        
        close(fd as u64);
        
        // 行ごとに検索
        let mut line_start = 0;
        for (idx, &byte) in content.iter().enumerate() {
            if byte == b'\n' {
                let line = &content[line_start..idx];
                if contains_pattern(line, pattern) {
                    if let Ok(s) = core::str::from_utf8(line) {
                        io::print(s);
                        io::print("\n");
                    }
                }
                line_start = idx + 1;
            }
        }
        
        // 最後の行（改行なし）
        if line_start < content.len() {
            let line = &content[line_start..];
            if contains_pattern(line, pattern) {
                if let Ok(s) = core::str::from_utf8(line) {
                    io::print(s);
                    io::print("\n");
                }
            }
        }
    }
    
    0
}
