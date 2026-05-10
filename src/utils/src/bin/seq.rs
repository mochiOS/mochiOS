#![no_std]
#![no_main]

use swiftlib::io;

fn parse_number(s: &str) -> Option<i64> {
    let mut result = 0i64;
    let mut negative = false;
    let bytes = s.as_bytes();
    let mut i = 0;
    
    if !bytes.is_empty() && bytes[0] == b'-' {
        negative = true;
        i = 1;
    }
    
    while i < bytes.len() {
        if bytes[i] >= b'0' && bytes[i] <= b'9' {
            result = result * 10 + (bytes[i] - b'0') as i64;
        } else {
            return None;
        }
        i += 1;
    }
    
    Some(if negative { -result } else { result })
}

fn print_number(n: i64) {
    if n < 0 {
        io::print("-");
        print_number(-n);
        return;
    }
    
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
    if argc < 2 {
        io::print("Usage: seq <end> or seq <start> <end>\n");
        return 1;
    }
    
    let (start, end) = if argc == 2 {
        let end_str = unsafe {
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
        (1, parse_number(end_str).unwrap_or(1))
    } else {
        let (start_str, end_str) = unsafe {
            let start_ptr = *argv.offset(1);
            let end_ptr = *argv.offset(2);
            
            let mut start_len = 0;
            while !start_ptr.is_null() && *start_ptr.offset(start_len) != 0 {
                start_len += 1;
            }
            
            let mut end_len = 0;
            while !end_ptr.is_null() && *end_ptr.offset(end_len) != 0 {
                end_len += 1;
            }
            
            let s = core::str::from_utf8(core::slice::from_raw_parts(start_ptr, start_len as usize));
            let e = core::str::from_utf8(core::slice::from_raw_parts(end_ptr, end_len as usize));
            
            match (s, e) {
                (Ok(a), Ok(b)) => (a, b),
                _ => return 1,
            }
        };
        
        (parse_number(start_str).unwrap_or(1), parse_number(end_str).unwrap_or(1))
    };
    
    if start <= end {
        for i in start..=end {
            print_number(i);
            io::print("\n");
        }
    } else {
        for i in (end..=start).rev() {
            print_number(i);
            io::print("\n");
        }
    }
    
    0
}
