use swiftlib::mouse as swift_mouse;

const MOUSE_SPEED_DIVISOR: i32 = 3;

pub struct MouseInputState {
    acc_x: i32,
    acc_y: i32,
}

impl MouseInputState {
    pub fn new() -> Self {
        Self { acc_x: 0, acc_y: 0 }
    }

    pub fn consume_packet(&mut self, packet: swift_mouse::MousePacket) -> Option<(i32, i32)> {
        self.acc_x += packet.dx as i32;
        self.acc_y += packet.dy as i32;

        let step_x = self.acc_x / MOUSE_SPEED_DIVISOR;
        let step_y = self.acc_y / MOUSE_SPEED_DIVISOR;

        self.acc_x -= step_x * MOUSE_SPEED_DIVISOR;
        self.acc_y -= step_y * MOUSE_SPEED_DIVISOR;

        if step_x == 0 && step_y == 0 {
            None
        } else {
            Some((step_x, step_y))
        }
    }
}
