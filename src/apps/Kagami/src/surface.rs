/// Wayland Surface
#[derive(Debug, Clone)]
pub struct Surface {
    pub object_id: u32,
    pub client_id: u32,

    // バッファ情報
    pub buffer_data: Option<Vec<u8>>,
    pub buffer_width: u32,
    pub buffer_height: u32,
    pub buffer_stride: u32,

    // ジオメトリ
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,

    // ダメージ領域
    pub damage: DamageRect,

    // 画面に表示されているか
    pub visible: bool,
    pub z_index: u32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DamageRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Surface {
    pub fn new(object_id: u32, client_id: u32) -> Self {
        Surface {
            object_id,
            client_id,
            buffer_data: None,
            buffer_width: 0,
            buffer_height: 0,
            buffer_stride: 0,
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            damage: DamageRect::default(),
            visible: true,
            z_index: 0,
        }
    }

    /// バッファ参照を解除
    pub fn detach_buffer(&mut self) {
        self.buffer_data = None;
        self.buffer_width = 0;
        self.buffer_height = 0;
        self.buffer_stride = 0;
        self.visible = false;
    }

    /// バッファをアタッチ
    pub fn attach_buffer(&mut self, data: Vec<u8>, width: u32, height: u32, stride: u32) {
        self.buffer_data = Some(data);
        self.buffer_width = width;
        self.buffer_height = height;
        self.buffer_stride = stride;
        self.visible = true;
    }

    /// ダメージ領域を設定
    pub fn set_damage(&mut self, x: i32, y: i32, width: i32, height: i32) {
        self.damage = DamageRect { x, y, width, height };
    }

    /// 描画要求を初期状態へ戻す
    pub fn clear_damage(&mut self) {
        self.damage = DamageRect::default();
    }

    /// ジオメトリを確定
    pub fn commit(&mut self) {
        if self.buffer_width > 0 && self.buffer_height > 0 {
            self.width = self.buffer_width;
            self.height = self.buffer_height;
        }
    }

    /// 指定座標がこのサーフェス内にあるか
    pub fn contains_point(&self, px: i32, py: i32) -> bool {
        px >= self.x
            && px < self.x + self.width as i32
            && py >= self.y
            && py < self.y + self.height as i32
    }

    /// 矩形領域が交差しているか
    pub fn intersects(&self, x: i32, y: i32, width: i32, height: i32) -> bool {
        let right = x + width;
        let bottom = y + height;
        let self_right = self.x + self.width as i32;
        let self_bottom = self.y + self.height as i32;

        self.x < right && x < self_right && self.y < bottom && y < self_bottom
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_surface_creation() {
        let surface = Surface::new(1, 1);
        assert_eq!(surface.object_id, 1);
        assert_eq!(surface.client_id, 1);
    }

    #[test]
    fn test_surface_attach_buffer() {
        let mut surface = Surface::new(1, 1);
        let buffer = vec![0xFF; 100];
        surface.attach_buffer(buffer, 10, 10, 40);
        assert_eq!(surface.buffer_width, 10);
        assert_eq!(surface.buffer_height, 10);
    }

    #[test]
    fn test_surface_contains_point() {
        let mut surface = Surface::new(1, 1);
        surface.x = 10;
        surface.y = 20;
        surface.width = 100;
        surface.height = 100;

        assert!(surface.contains_point(50, 50));
        assert!(!surface.contains_point(5, 50));
        assert!(!surface.contains_point(150, 50));
    }

    #[test]
    fn test_surface_intersects() {
        let mut surface = Surface::new(1, 1);
        surface.x = 10;
        surface.y = 10;
        surface.width = 100;
        surface.height = 100;

        assert!(surface.intersects(50, 50, 20, 20));
        assert!(!surface.intersects(200, 200, 10, 10));
    }
}
