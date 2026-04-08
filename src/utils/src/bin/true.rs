#![no_std]
#![no_main]

use swiftlib as _;

#[no_mangle]
pub extern "C" fn main(_argc: i32, _argv: *const *const u8) -> i32 {
    0
}
