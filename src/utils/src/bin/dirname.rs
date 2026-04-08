#![no_std]
#![no_main]

use swiftlib::io;

#[no_mangle]
pub extern "C" fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        io::print("Usage: dirname <path>\n");
        return 1;
    }
    
    let path = unsafe {
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
    
    // 末尾の / を削除
    let path = path.trim_end_matches('/');
    
    // 最後の / まで取得
    if let Some(pos) = path.rfind('/') {
        if pos == 0 {
            io::print("/\n");
        } else {
            io::print(&path[..pos]);
            io::print("\n");
        }
    } else {
        io::print(".\n");
    }
    
    0
}
