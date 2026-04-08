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

fn open(path: &str, flags: u64) -> i64 {
    let mut buf = [0u8; 512];
    let bytes = path.as_bytes();
    let len = bytes.len().min(511);
    buf[..len].copy_from_slice(&bytes[..len]);
    syscall2(SyscallNumber::Open as u64, buf.as_ptr() as u64, flags) as i64
}

fn read(fd: u64, buf: &mut [u8]) -> i64 {
    syscall3(
        SyscallNumber::Read as u64,
        fd,
        buf.as_mut_ptr() as u64,
        buf.len() as u64,
    ) as i64
}

fn write(fd: u64, buf: &[u8]) -> i64 {
    syscall3(
        SyscallNumber::Write as u64,
        fd,
        buf.as_ptr() as u64,
        buf.len() as u64,
    ) as i64
}

fn close(fd: u64) {
    syscall1(SyscallNumber::Close as u64, fd);
}

const O_CREAT: u64 = 0x40;
const O_WRONLY: u64 = 0x01;
const O_TRUNC: u64 = 0x200;

#[no_mangle]
pub extern "C" fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 3 {
        io::print("Usage: cp <source> <dest>\n");
        return 1;
    }
    
    let (src_path, dest_path) = unsafe {
        let src_ptr = *argv.offset(1);
        let dest_ptr = *argv.offset(2);
        
        let mut src_len = 0;
        while !src_ptr.is_null() && *src_ptr.offset(src_len) != 0 {
            src_len += 1;
        }
        
        let mut dest_len = 0;
        while !dest_ptr.is_null() && *dest_ptr.offset(dest_len) != 0 {
            dest_len += 1;
        }
        
        let src = core::str::from_utf8(core::slice::from_raw_parts(src_ptr, src_len as usize));
        let dest = core::str::from_utf8(core::slice::from_raw_parts(dest_ptr, dest_len as usize));
        
        match (src, dest) {
            (Ok(s), Ok(d)) => (s, d),
            _ => {
                io::print("cp: invalid path\n");
                return 1;
            }
        }
    };
    
    let src_fd = open(src_path, 0);
    if src_fd < 0 {
        io::print("cp: cannot open source: ");
        io::print(src_path);
        io::print("\n");
        return 1;
    }
    
    let dest_fd = open(dest_path, O_CREAT | O_WRONLY | O_TRUNC);
    if dest_fd < 0 {
        io::print("cp: cannot create destination: ");
        io::print(dest_path);
        io::print("\n");
        close(src_fd as u64);
        return 1;
    }
    
    let mut buf = [0u8; 4096];
    loop {
        let n = read(src_fd as u64, &mut buf);
        if n <= 0 {
            break;
        }
        
        let written = write(dest_fd as u64, &buf[..n as usize]);
        if written != n {
            io::print("cp: write error\n");
            close(src_fd as u64);
            close(dest_fd as u64);
            return 1;
        }
    }
    
    close(src_fd as u64);
    close(dest_fd as u64);
    0
}
