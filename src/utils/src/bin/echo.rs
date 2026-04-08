#![no_std]
#![no_main]

use swiftlib::io;

#[no_mangle]
pub extern "C" fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc <= 1 {
        io::print("\n");
        return 0;
    }
    
    let mut first = true;
    for i in 1..argc {
        if !first {
            io::print(" ");
        }
        first = false;
        
        unsafe {
            let arg_ptr = *argv.offset(i as isize);
            if !arg_ptr.is_null() {
                let mut len = 0;
                while *arg_ptr.offset(len) != 0 {
                    len += 1;
                }
                if let Ok(s) = core::str::from_utf8(core::slice::from_raw_parts(arg_ptr, len as usize)) {
                    io::print(s);
                }
            }
        }
    }
    io::print("\n");
    0
}
