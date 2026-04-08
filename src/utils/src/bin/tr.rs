#![no_std]
#![no_main]

use swiftlib::io;

#[no_mangle]
pub extern "C" fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        io::print("Usage: tr <string1> <string2>\n");
        return 1;
    }
    
    let (from_str, to_str) = if argc >= 3 {
        unsafe {
            let from_ptr = *argv.offset(1);
            let to_ptr = *argv.offset(2);
            
            let mut from_len = 0;
            while !from_ptr.is_null() && *from_ptr.offset(from_len) != 0 {
                from_len += 1;
            }
            
            let mut to_len = 0;
            while !to_ptr.is_null() && *to_ptr.offset(to_len) != 0 {
                to_len += 1;
            }
            
            let f = core::str::from_utf8(core::slice::from_raw_parts(from_ptr, from_len as usize));
            let t = core::str::from_utf8(core::slice::from_raw_parts(to_ptr, to_len as usize));
            
            match (f, t) {
                (Ok(a), Ok(b)) => (a, b),
                _ => return 1,
            }
        }
    } else {
        return 1;
    };
    
    if argc >= 4 {
        let text = unsafe {
            let arg_ptr = *argv.offset(3);
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
        
        // 文字変換
        for ch in text.chars() {
            if let Some(pos) = from_str.chars().position(|c| c == ch) {
                if let Some(replacement) = to_str.chars().nth(pos) {
                    let mut buf = [0u8; 4];
                    let s = replacement.encode_utf8(&mut buf);
                    io::print(s);
                } else {
                    let mut buf = [0u8; 4];
                    let s = ch.encode_utf8(&mut buf);
                    io::print(s);
                }
            } else {
                let mut buf = [0u8; 4];
                let s = ch.encode_utf8(&mut buf);
                io::print(s);
            }
        }
        io::print("\n");
    }
    
    0
}
