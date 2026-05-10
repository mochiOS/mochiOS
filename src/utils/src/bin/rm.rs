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

fn unlink(path: &str) -> i64 {
    let mut buf = [0u8; 512];
    let bytes = path.as_bytes();
    let len = bytes.len().min(511);
    buf[..len].copy_from_slice(&bytes[..len]);
    syscall1(SyscallNumber::Unlink as u64, buf.as_ptr() as u64) as i64
}

fn rmdir(path: &str) -> i64 {
    let mut buf = [0u8; 512];
    let bytes = path.as_bytes();
    let len = bytes.len().min(511);
    buf[..len].copy_from_slice(&bytes[..len]);
    syscall1(SyscallNumber::Rmdir as u64, buf.as_ptr() as u64) as i64
}

#[no_mangle]
pub extern "C" fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc <= 1 {
        io::print("Usage: rm [-r] <file>...\n");
        return 1;
    }
    
    let mut is_recursive = false;
    let mut start_idx = 1;
    
    // -r オプションチェック
    if argc > 1 {
        unsafe {
            let arg_ptr = *argv.offset(1);
            if !arg_ptr.is_null() {
                let mut len = 0;
                while *arg_ptr.offset(len) != 0 && len < 10 {
                    len += 1;
                }
                if let Ok(s) = core::str::from_utf8(core::slice::from_raw_parts(arg_ptr, len as usize)) {
                    if s == "-r" {
                        is_recursive = true;
                        start_idx = 2;
                    }
                }
            }
        }
    }
    
    let mut ret = 0;
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
        
        let mut result = unlink(path);
        if result < 0 && is_recursive {
            // unlink が失敗したらディレクトリかもしれないので rmdir を試す
            result = rmdir(path);
        }
        
        if result < 0 {
            io::print("rm: cannot remove '");
            io::print(path);
            io::print("'\n");
            ret = 1;
        }
    }
    
    ret
}
