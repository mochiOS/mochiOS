use swiftlib::{keyboard, mouse, time};

#[rustfmt::skip]
const MAP_NORMAL: [u8; 128] = [
    0,    0x1B, b'1', b'2', b'3', b'4', b'5', b'6',
    b'7', b'8', b'9', b'0', b'-', b'=', 0x08, b'\t',
    b'q', b'w', b'e', b'r', b't', b'y', b'u', b'i',
    b'o', b'p', b'[', b']', b'\n', 0,   b'a', b's',
    b'd', b'f', b'g', b'h', b'j', b'k', b'l', b';',
    b'\'',b'`', 0,   b'\\',b'z', b'x', b'c', b'v',
    b'b', b'n', b'm', b',', b'.', b'/', 0,   b'*',
    0,    b' ', 0,    0,    0,    0,    0,    0,
    0,    0,    0,    0,    0,    0,    0,    b'7',
    b'8', b'9', b'-', b'4', b'5', b'6', b'+', b'1',
    b'2', b'3', b'0', b'.', 0,    0,    0,    0,
    0,    0,    0,    0,    0,    0,    0,    0,
    0,    0,    0,    0,    0,    0,    0,    0,
    0,    0,    0,    0,    0,    0,    0,    0,
    0,    0,    0,    0,    0,    0,    0,    0,
    0,    0,    0,    0,    0,    0,    0,    0,
];

#[rustfmt::skip]
const MAP_SHIFT: [u8; 128] = [
    0,    0x1B, b'!', b'@', b'#', b'$', b'%', b'^',
    b'&', b'*', b'(', b')', b'_', b'+', 0x08, b'\t',
    b'Q', b'W', b'E', b'R', b'T', b'Y', b'U', b'I',
    b'O', b'P', b'{', b'}', b'\n', 0,   b'A', b'S',
    b'D', b'F', b'G', b'H', b'J', b'K', b'L', b':',
    b'"', b'~', 0,   b'|', b'Z', b'X', b'C', b'V',
    b'B', b'N', b'M', b'<', b'>', b'?', 0,   b'*',
    0,    b' ', 0,    0,    0,    0,    0,    0,
    0,    0,    0,    0,    0,    0,    0,    b'7',
    b'8', b'9', b'-', b'4', b'5', b'6', b'+', b'1',
    b'2', b'3', b'0', b'.', 0,    0,    0,    0,
    0,    0,    0,    0,    0,    0,    0,    0,
    0,    0,    0,    0,    0,    0,    0,    0,
    0,    0,    0,    0,    0,    0,    0,    0,
    0,    0,    0,    0,    0,    0,    0,    0,
    0,    0,    0,    0,    0,    0,    0,    0,
];

const SC_LSHIFT: u8 = 0x2A;
const SC_RSHIFT: u8 = 0x36;
const SC_CAPSLOCK: u8 = 0x3A;
const SC_RELEASE: u8 = 0x80;

#[derive(Default)]
struct KeyboardDecoder {
    shift: bool,
    caps: bool,
}

impl KeyboardDecoder {
    fn decode_scancode(&mut self, scancode: u8) -> Option<u8> {
        if scancode & SC_RELEASE != 0 {
            let make = scancode & !SC_RELEASE;
            if make == SC_LSHIFT || make == SC_RSHIFT {
                self.shift = false;
            }
            return None;
        }

        match scancode {
            SC_LSHIFT | SC_RSHIFT => {
                self.shift = true;
                return None;
            }
            SC_CAPSLOCK => {
                self.caps = !self.caps;
                return None;
            }
            _ => {}
        }

        let idx = scancode as usize;
        if idx >= 128 {
            return None;
        }

        let use_shift = self.shift ^ (self.caps && MAP_NORMAL[idx].is_ascii_alphabetic());
        let ch = if use_shift { MAP_SHIFT[idx] } else { MAP_NORMAL[idx] };
        if ch == 0 {
            None
        } else {
            Some(ch)
        }
    }
}

fn log_key_event(ch: u8) {
    match ch {
        b'\n' => println!("[USB3.0][KBD] <ENTER>"),
        b'\t' => println!("[USB3.0][KBD] <TAB>"),
        0x08 => println!("[USB3.0][KBD] <BACKSPACE>"),
        b' '..=b'~' => println!("[USB3.0][KBD] '{}'", ch as char),
        _ => println!("[USB3.0][KBD] 0x{:02X}", ch),
    }
}

fn log_mouse_event(packet: mouse::MousePacket, last_buttons: &mut u8) {
    let moved = packet.dx != 0 || packet.dy != 0;
    let buttons_changed = packet.buttons != *last_buttons;
    if !moved && !buttons_changed {
        return;
    }

    let dy_screen = -(packet.dy as i16);
    println!(
        "[USB3.0][MOUSE] dx={:>4}, dy={:>4}, L={} R={} M={}",
        packet.dx as i16,
        dy_screen,
        packet.left() as u8,
        packet.right() as u8,
        packet.middle() as u8
    );
    *last_buttons = packet.buttons;
}

fn main() {
    println!("[USB3.0] driver started");
    println!("[USB3.0] input monitor mode enabled (keyboard tap + mouse packet)");

    let mut decoder = KeyboardDecoder::default();
    let mut last_buttons = 0u8;
    let mut warned_keyboard_err = false;
    let mut warned_mouse_err = false;

    loop {
        let mut handled_any = false;

        loop {
            match keyboard::read_scancode_tap() {
                Ok(Some(scancode)) => {
                    handled_any = true;
                    if let Some(ch) = decoder.decode_scancode(scancode) {
                        log_key_event(ch);
                    }
                }
                Ok(None) => break,
                Err(err) => {
                    if !warned_keyboard_err {
                        println!("[USB3.0] keyboard tap error: {:#x}", err);
                        warned_keyboard_err = true;
                    }
                    break;
                }
            }
        }

        loop {
            match mouse::read_packet() {
                Ok(Some(packet)) => {
                    handled_any = true;
                    log_mouse_event(packet, &mut last_buttons);
                }
                Ok(None) => break,
                Err(err) => {
                    if !warned_mouse_err {
                        println!("[USB3.0] mouse read error: {:#x}", err);
                        warned_mouse_err = true;
                    }
                    break;
                }
            }
        }

        if !handled_any {
            time::sleep_ms(2);
        }
    }
}
