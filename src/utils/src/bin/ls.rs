#![no_std]
#![no_main]

use swiftlib::{io, sys::SyscallNumber};

// ANSI カラーコード
const COLOR_RESET: &str = "\x1b[0m";
const COLOR_BLUE: &str = "\x1b[34m";    // ディレクトリ
const COLOR_GREEN: &str = "\x1b[32m";   // 実行可能ファイル
const COLOR_CYAN: &str = "\x1b[36m";    // シンボリックリンク

// Low-level syscalls
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

fn stat(path: &str) -> Result<(u16, u64), i64> {
    let mut buf = [0u8; 512];
    let bytes = path.as_bytes();
    let len = bytes.len().min(511);
    buf[..len].copy_from_slice(&bytes[..len]);
    
    let result = syscall1(SyscallNumber::Stat as u64, buf.as_ptr() as u64);
    
    // stat は mode を下位 16 ビット、size を上位 48 ビットに詰めて返す
    // または負の値でエラー
    if (result as i64) < 0 {
        return Err(result as i64);
    }
    
    let mode = (result & 0xFFFF) as u16;
    let size = (result >> 16) as u64;
    Ok((mode, size))
}

fn readdir(fd: u64, buf: &mut [u8]) -> u64 {
    syscall3(
        SyscallNumber::Readdir as u64,
        fd,
        buf.as_mut_ptr() as u64,
        buf.len() as u64,
    )
}

fn close(fd: u64) -> u64 {
    syscall1(SyscallNumber::Close as u64, fd)
}

fn is_directory(mode: u16) -> bool {
    (mode & 0xF000) == 0x4000
}

fn is_executable(mode: u16) -> bool {
    // 実行権限ビット (user/group/other のいずれか)
    (mode & 0o111) != 0
}

fn is_symlink(mode: u16) -> bool {
    (mode & 0xF000) == 0xA000
}

fn get_color_for_file(path: &str) -> &'static str {
    match stat(path) {
        Ok((mode, _)) => {
            if is_directory(mode) {
                COLOR_BLUE
            } else if is_symlink(mode) {
                COLOR_CYAN
            } else if is_executable(mode) {
                COLOR_GREEN
            } else {
                COLOR_RESET
            }
        }
        Err(_) => COLOR_RESET,
    }
}

#[no_mangle]
pub extern "C" fn main(_argc: i32, _argv: *const *const u8) -> i32 {
    let path = if _argc > 1 {
        unsafe {
            let arg_ptr = *_argv.offset(1);
            if arg_ptr.is_null() {
                "."
            } else {
                let mut len = 0;
                while *arg_ptr.offset(len) != 0 {
                    len += 1;
                }
                match core::str::from_utf8(core::slice::from_raw_parts(arg_ptr, len as usize)) {
                    Ok(s) => s,
                    Err(_) => ".",
                }
            }
        }
    } else {
        "."
    };

    let fd = open(path);
    if fd < 0 {
        io::print("ls: cannot open directory: ");
        io::print(path);
        io::print("\n");
        return 1;
    }

    let fd_u = fd as u64;
    let mut buf = [0u8; 2048];
    
    // パスの末尾に / を追加するかチェック
    let needs_slash = !path.ends_with('/');
    
    loop {
        let n = readdir(fd_u, &mut buf);
        if n == 0 || n > 0xFFFF_FFFF_0000_0000 {
            break;
        }
        
        // Parse null-terminated strings
        let data = &buf[..n as usize];
        let mut pos = 0;
        while pos < data.len() {
            if data[pos] == 0 {
                break;
            }
            let start = pos;
            while pos < data.len() && data[pos] != 0 {
                pos += 1;
            }
            if pos > start {
                if let Ok(name) = core::str::from_utf8(&data[start..pos]) {
                    // フルパスを構築して stat
                    let mut full_path_buf = [0u8; 256];

                    // path をコピー
                    let path_bytes = path.as_bytes();
                    let path_copy_len = path_bytes.len().min(200);
                    full_path_buf[..path_copy_len].copy_from_slice(&path_bytes[..path_copy_len]);
                    let mut full_len = path_copy_len;
                    
                    // / を追加（必要なら）
                    if needs_slash && full_len < 255 {
                        full_path_buf[full_len] = b'/';
                        full_len += 1;
                    }
                    
                    // name を追加
                    let name_bytes = name.as_bytes();
                    let name_copy_len = name_bytes.len().min(255 - full_len);
                    full_path_buf[full_len..full_len + name_copy_len].copy_from_slice(&name_bytes[..name_copy_len]);
                    full_len += name_copy_len;
                    
                    if let Ok(full_path) = core::str::from_utf8(&full_path_buf[..full_len]) {
                        let color = get_color_for_file(full_path);
                        io::print(color);
                        io::print(name);
                        io::print(COLOR_RESET);
                        io::print("\n");
                    } else {
                        io::print(name);
                        io::print("\n");
                    }
                }
            }
            pos += 1; // skip null terminator
        }
    }
    
    close(fd_u);
    0
}
