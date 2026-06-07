mod char;
mod keyboard;

use char::{Font, Terminal};
use keyboard::Ps2Keyboard;
use mochi_syscall::{time, vga};

#[repr(C)]
struct WinSize {
    ws_row: u16,
    ws_col: u16,
    ws_xpixel: u16,
    ws_ypixel: u16,
}

fn main() {
    println!("[SHELL] Service Started.");

    let info = match vga::get_info() {
        Some(i) => i,
        None => {
            println!("[SHELL] Failed to get framebuffer info");
            loop {
                time::sleep_ms(1000);
            }
        }
    };
    let fb_ptr = match vga::map_framebuffer() {
        Some(p) => p,
        None => {
            println!("[SHELL] Failed to map framebuffer");
            loop {
                time::sleep_ms(1000);
            }
        }
    };
    println!(
        "[SHELL] fb info: width={} height={} stride={} fb_ptr={:p}",
        info.width, info.height, info.stride, fb_ptr
    );

    let font = match Font::load() {
        Some(f) => f,
        None => {
            println!("[SHELL] Failed to load font");
            loop {
                time::sleep_ms(1000);
            }
        }
    };
    
    let mut term = Terminal::new(fb_ptr, info, font);
    let mut kbd = Ps2Keyboard::new();
    let (cols, rows) = term.size_chars();
    let ws = WinSize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    unsafe {
        let _ = mochi_syscall::posix_stubs::ioctl(
            0,
            mochi_syscall::posix_stubs::TIOCSWINSZ,
            (&ws as *const WinSize) as u64,
        );
    }

    term.clear_screen(); // clear_screen 内で flush 済み
    term.fg = 0x00FF_FF00; // 黄色
    term.write_str("mochiOS Shell\n");
    term.fg = 0x00FF_FFFF;
    term.prompt();
    term.flush();
    println!("[SHELL] Ready. Input is on the QEMU VGA window.");

    loop {
        let ch = kbd.read_blocking();
        match ch {
            b'\n' | b'\r' => {
                term.handle_line();
                term.prompt();
                term.flush();
            }
            0x08 | 0x7F => { // Backspace / Delete
                if term.input_len > 0 {
                    term.input_len -= 1;
                    term.erase_previous_cell();
                    term.flush();
                }
            }
            0x20..=0x7E => {
                if term.input_len < term.input_buf.len() - 1 {
                    term.input_buf[term.input_len] = ch;
                    term.input_len += 1;
                    term.write_byte(ch);
                    term.flush();
                }
            }
            _ => {}
        }
    }
}
