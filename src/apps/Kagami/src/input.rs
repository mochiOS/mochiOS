use swiftlib::mouse;

use crate::mouse::MouseInputState;

pub struct InputState {
    esc_armed: bool,
    mouse: MouseInputState,
}

impl InputState {
    pub fn new() -> Self {
        Self {
            esc_armed: false,
            mouse: MouseInputState::new(),
        }
    }

    pub fn should_exit(&mut self, scancode: u8) -> bool {
        match scancode {
            0x01 => {
                self.esc_armed = true;
                false
            }
            0x81 => self.esc_armed,
            _ => false,
        }
    }

    pub fn consume_mouse(&mut self, packet: mouse::MousePacket) -> Option<(i32, i32)> {
        self.mouse.consume_packet(packet)
    }
}
