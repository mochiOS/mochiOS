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
    if argc < 3 {
        io::print("Usage: cmp <file1> <file2>\n");
        return 1;
    }
    
    let (path1, path2) = unsafe {
        let ptr1 = *argv.offset(1);
        let ptr2 = *argv.offset(2);
        
        let mut len1 = 0;
        while !ptr1.is_null() && *ptr1.offset(len1) != 0 {
            len1 += 1;
        }
        
        let mut len2 = 0;
        while !ptr2.is_null() && *ptr2.offset(len2) != 0 {
            len2 += 1;
        }
        
        let s1 = core::str::from_utf8(core::slice::from_raw_parts(ptr1, len1 as usize));
        let s2 = core::str::from_utf8(core::slice::from_raw_parts(ptr2, len2 as usize));
        
        match (s1, s2) {
            (Ok(a), Ok(b)) => (a, b),
            _ => return 1,
        }
    };
    
    let fd1 = open(path1);
    if fd1 < 0 {
        io::print("cmp: cannot open: ");
        io::print(path1);
        io::print("\n");
        return 1;
    }
    
    let fd2 = open(path2);
    if fd2 < 0 {
        io::print("cmp: cannot open: ");
        io::print(path2);
        io::print("\n");
        close(fd1 as u64);
        return 1;
    }
    
    let mut buf1 = [0u8; 2048];
    let mut buf2 = [0u8; 2048];
    let mut byte_pos = 1u64;
    
    loop {
        let n1 = read(fd1 as u64, &mut buf1);
        let n2 = read(fd2 as u64, &mut buf2);
        
        if n1 != n2 {
            io::print(path1);
            io::print(" ");
            io::print(path2);
            io::print(" differ: size\n");
            close(fd1 as u64);
            close(fd2 as u64);
            return 1;
        }
        
        if n1 <= 0 {
            break;
        }
        
        for i in 0..(n1 as usize) {
            if buf1[i] != buf2[i] {
                io::print(path1);
                io::print(" ");
                io::print(path2);
                io::print(" differ: byte ");
                
                let mut num_buf = [0u8; 20];
                let mut idx = num_buf.len();
                let mut n = byte_pos + i as u64;
                while n > 0 && idx > 0 {
                    idx -= 1;
                    num_buf[idx] = b'0' + (n % 10) as u8;
                    n /= 10;
                }
                if let Ok(s) = core::str::from_utf8(&num_buf[idx..]) {
                    io::print(s);
                }
                io::print("\n");
                
                close(fd1 as u64);
                close(fd2 as u64);
                return 1;
            }
        }
        
        byte_pos += n1 as u64;
    }
    
    close(fd1 as u64);
    close(fd2 as u64);
    0
}
