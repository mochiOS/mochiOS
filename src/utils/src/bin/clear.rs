#![no_std]
#![no_main]

use swiftlib::io;

#[no_mangle]
pub extern "C" fn main(_argc: i32, _argv: *const *const u8) -> i32 {
    // ANSI エスケープコード: 画面クリア + カーソルをホームへ
    io::print("\x1b[2J\x1b[H");
    0
}
