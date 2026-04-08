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

fn open(path: &str, flags: u64) -> i64 {
    let mut buf = [0u8; 512];
    let bytes = path.as_bytes();
    let len = bytes.len().min(511);
    buf[..len].copy_from_slice(&bytes[..len]);
    syscall2(SyscallNumber::Open as u64, buf.as_ptr() as u64, flags) as i64
}

const O_CREAT: u64 = 0x40;
const O_RDWR: u64 = 0x02;

#[no_mangle]
pub extern "C" fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc <= 1 {
        io::print("Usage: touch <file>...\n");
        return 1;
    }
    
    let mut ret = 0;
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
        
        let fd = open(path, O_CREAT | O_RDWR);
        if fd < 0 {
            io::print("touch: cannot touch '");
            io::print(path);
            io::print("'\n");
            ret = 1;
        } else {
            // すぐに閉じる
            syscall2(SyscallNumber::Close as u64, fd as u64, 0);
        }
    }
    
    ret
}
