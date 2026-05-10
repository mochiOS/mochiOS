#![no_std]
#![no_main]

extern crate alloc;

use swiftlib::io;

fn parse_number(s: &str) -> Option<usize> {
    let mut result = 0usize;
    for &b in s.as_bytes() {
        if b >= b'0' && b <= b'9' {
            result = result * 10 + (b - b'0') as usize;
        } else {
            return None;
        }
    }
    Some(result)
}

#[no_mangle]
pub extern "C" fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 3 {
        io::print("Usage: cut -f<field> <text>\n");
        return 1;
    }
    
    let field_arg = unsafe {
        let arg_ptr = *argv.offset(1);
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
    
    if !field_arg.starts_with("-f") {
        io::print("cut: usage: -f<number>\n");
        return 1;
    }
    
    let field_num = match parse_number(&field_arg[2..]) {
        Some(n) if n > 0 => n - 1,
        _ => {
            io::print("cut: invalid field number\n");
            return 1;
        }
    };
    
    let text = unsafe {
        let arg_ptr = *argv.offset(2);
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
    
    // スペース区切りでフィールド分割
    let fields: alloc::vec::Vec<&str> = text.split(' ').collect();
    
    if field_num < fields.len() {
        io::print(fields[field_num]);
        io::print("\n");
    }
    
    0
}
