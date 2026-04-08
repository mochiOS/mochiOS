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
pub extern "C" fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc <= 1 {
        io::print("Usage: wc <file>...\n");
        return 1;
    }
    
    for i in 1..argc {
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
            io::print("wc: ");
            io::print(path);
            io::print(": cannot open\n");
            continue;
        }
        
        let fd_u = fd as u64;
        let mut buf = [0u8; 2048];
        let mut lines = 0u64;
        let mut words = 0u64;
        let mut bytes = 0u64;
        let mut in_word = false;
        
        loop {
            let n = read(fd_u, &mut buf);
            if n <= 0 {
                break;
            }
            
            bytes += n as u64;
            
            for &byte in &buf[..n as usize] {
                if byte == b'\n' {
                    lines += 1;
                }
                
                let is_whitespace = byte == b' ' || byte == b'\t' || byte == b'\n' || byte == b'\r';
                if is_whitespace {
                    if in_word {
                        words += 1;
                        in_word = false;
                    }
                } else {
                    in_word = true;
                }
            }
        }
        
        if in_word {
            words += 1;
        }
        
        close(fd_u);
        
        // 出力
        io::print(" ");
        print_number(lines);
        io::print(" ");
        print_number(words);
        io::print(" ");
        print_number(bytes);
        io::print(" ");
        io::print(path);
        io::print("\n");
    }
    
    0
}
