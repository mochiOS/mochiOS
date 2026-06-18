#![no_std]
#![no_main]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

use mochi_user_platform as platform;

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    main();
    platform::runtime::abort()
}

fn main() {
    let mut parts = Vec::new();
    parts.push(String::from("core.service"));
    parts.push(String::from("started"));

    platform::println!("{}: {}", parts[0], parts[1]);

    loop {
        platform::thread::yield_now();
    }
}
