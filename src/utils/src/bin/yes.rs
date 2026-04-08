#![no_std]
#![no_main]

use swiftlib::io;

#[no_mangle]
pub extern "C" fn main(argc: i32, argv: *const *const u8) -> i32 {
    let text = if argc > 1 {
        unsafe {
            let arg_ptr = *argv.offset(1);
            if !arg_ptr.is_null() {
                let mut len = 0;
                while *arg_ptr.offset(len) != 0 {
                    len += 1;
                }
                match core::str::from_utf8(core::slice::from_raw_parts(arg_ptr, len as usize)) {
                    Ok(s) => s,
                    Err(_) => "y",
                }
            } else {
                "y"
            }
        }
    } else {
        "y"
    };
    
    loop {
        io::print(text);
        io::print("\n");
    }
}
