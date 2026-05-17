use swiftlib::vga;

const BG_COLOR: u32 = 0x001E_1E2E;
const WINDOW_POS_X: i32 = 96;
const WINDOW_POS_Y: i32 = 96;
const WINDOW_STEP_X: i32 = 14;
const WINDOW_STEP_Y: i32 = 10;
const STATUS_BAR_HEIGHT: i32 = 28;
const TITLE_BAR_HEIGHT: usize = 18;
const WINDOW_CORNER_RADIUS: usize = 4;
const STATUS_BAR_COLOR: u32 = 0xFF1A_1A24;
const WINDOW_BORDER_COLOR: u32 = 0xFFB9_BDCB;
const TITLE_TOP_COLOR: u32 = 0xFFE9_EAF1;
const TITLE_BOTTOM_COLOR: u32 = 0xFFD7_DAE5;
const TITLE_SEPARATOR_COLOR: u32 = 0xFFB6_BAC8;
const TRAFFIC_RED: u32 = 0xFFFF_5F57;
const TRAFFIC_YELLOW: u32 = 0xFFFEB_C2E;
const TRAFFIC_GREEN: u32 = 0xFF28_C840;
const TRAFFIC_RING: u32 = 0xFF95_95A2;
const TRAFFIC_DIAMETER: isize = 8;
const TRAFFIC_GAP: isize = 8;
const TRAFFIC_OFFSET_X: isize = 7;
const TRAFFIC_OFFSET_Y: isize = 8;
const TRAFFIC_RING_WIDTH: isize = 1;
const SHADOW_NEAR_ALPHA: u32 = 56;
const SHADOW_FAR_ALPHA: u32 = 28;
const STATUS_DOCK_HIDDEN_OFFSET: i32 = 110;
const STATUS_DOCK_REVEAL_TRIGGER: i32 = 120;
const STATUS_DOCK_KEEP_ZONE: i32 = 220;
const STATUS_DOCK_ANIM_STEP: i32 = 10;
const STATUS_DOCK_AUTOHIDE: bool = false;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowLayer {
    Wallpaper,
    App,
    Status,
    System,
}

impl WindowLayer {
    fn order(self) -> i32 {
        match self {
            Self::Wallpaper => 0,
            Self::App => 1,
            Self::Status => 2,
            Self::System => 3,
        }
    }
}

include!(concat!(env!("OUT_DIR"), "/cursor_pixels.rs"));

struct CursorSprite {
    width: usize,
    height: usize,
    pixels: Vec<u32>,
}

impl CursorSprite {
    fn from_generated() -> Self {
        Self {
            width: CURSOR_WIDTH,
            height: CURSOR_HEIGHT,
            pixels: CURSOR_PIXELS.to_vec(),
        }
    }
}

pub struct WindowSurface {
    pub id: u32,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub layer: WindowLayer,
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u32>,
    shared: Option<SharedSurface>,
}

struct SharedSurface {
    virt_addr: u64,
    page_count: u64,
}

pub struct Renderer {
    fb_ptr: *mut u32,
    width: i32,
    height: i32,
    stride: i32,
    back_buffer: Vec<u32>,
    cursor_x: i32,
    cursor_y: i32,
    cursor_sprite: CursorSprite,
    windows: Vec<WindowSurface>,
    status_dock_slide_px: i32,
    status_dock_target_visible: bool,
}

impl Renderer {
    fn infer_overlay_layer(width: usize, height: usize) -> WindowLayer {
        if width <= 400 && height <= 140 {
            WindowLayer::Status
        } else {
            WindowLayer::App
        }
    }

    pub fn new(fb_ptr: *mut u32, info: vga::FbInfo) -> Self {
        Self {
            fb_ptr,
            width: info.width as i32,
            height: info.height as i32,
            stride: info.stride as i32,
            back_buffer: vec![0; (info.height * info.stride) as usize],
            cursor_x: (info.width / 2) as i32,
            cursor_y: (info.height / 2) as i32,
            cursor_sprite: CursorSprite::from_generated(),
            windows: Vec::new(),
            status_dock_slide_px: 0,
            status_dock_target_visible: true,
        }
    }

    fn status_window_y(&self, height: usize) -> i32 {
        (self.height - height as i32 - 14 + self.status_dock_slide_px).max(0)
    }

    fn update_status_dock_target(&mut self) {
        if !STATUS_DOCK_AUTOHIDE {
            self.status_dock_target_visible = true;
            return;
        }
        let dist_from_bottom = self.height.saturating_sub(self.cursor_y.saturating_add(1));
        let dist_from_top = self.cursor_y.max(0);
        let near_reveal_edge =
            dist_from_bottom <= STATUS_DOCK_REVEAL_TRIGGER || dist_from_top <= STATUS_DOCK_REVEAL_TRIGGER;
        let near_keep_edge =
            dist_from_bottom <= STATUS_DOCK_KEEP_ZONE || dist_from_top <= STATUS_DOCK_KEEP_ZONE;
        if self.status_dock_target_visible || self.status_dock_slide_px < STATUS_DOCK_HIDDEN_OFFSET {
            self.status_dock_target_visible = near_keep_edge;
        } else {
            self.status_dock_target_visible = near_reveal_edge;
        }
    }

    fn step_status_dock_animation(&mut self) -> bool {
        let target = if self.status_dock_target_visible {
            0
        } else {
            STATUS_DOCK_HIDDEN_OFFSET
        };
        if self.status_dock_slide_px == target {
            return false;
        }
        if self.status_dock_slide_px < target {
            self.status_dock_slide_px =
                (self.status_dock_slide_px + STATUS_DOCK_ANIM_STEP).min(target);
        } else {
            self.status_dock_slide_px =
                (self.status_dock_slide_px - STATUS_DOCK_ANIM_STEP).max(target);
        }
        let y = self.status_dock_slide_px;
        for win in &mut self.windows {
            if win.layer == WindowLayer::Status {
                win.y = (self.height - win.height as i32 - 14 + y).max(0);
            }
        }
        true
    }

    pub fn tick_animations(&mut self) -> bool {
        self.update_status_dock_target();
        if self.step_status_dock_animation() {
            self.render_full();
            true
        } else {
            false
        }
    }

    pub fn initialize(&mut self) {
        self.render_full();
    }

    pub fn create_window(
        &mut self,
        id: u32,
        layer: WindowLayer,
        width: usize,
        height: usize,
        pixels: Vec<u32>,
    ) {
        if self.windows.iter().any(|w| w.id == id) {
            self.update_window_pixels(id, width, height, pixels);
            return;
        }
        let z = self.next_z();
        let (x, y) = match layer {
            WindowLayer::Wallpaper => (0, 0),
            WindowLayer::Status => (
                ((self.width - width as i32) / 2).max(0),
                self.status_window_y(height),
            ),
            WindowLayer::System => (WINDOW_POS_X, WINDOW_POS_Y),
            WindowLayer::App => (
                WINDOW_POS_X + ((id as i32 - 1) * WINDOW_STEP_X),
                WINDOW_POS_Y + ((id as i32 - 1) * WINDOW_STEP_Y),
            ),
        };
        self.windows.push(WindowSurface {
            id,
            x,
            y,
            z,
            layer,
            width,
            height,
            pixels,
            shared: None,
        });
        self.sort_windows_by_z();
        self.render_full();
    }

    pub fn update_window_pixels(&mut self, id: u32, width: usize, height: usize, pixels: Vec<u32>) {
        let new_z = self.next_z();
        if let Some(win) = self.windows.iter_mut().find(|w| w.id == id) {
            win.width = width;
            win.height = height;
            win.pixels = pixels;
            win.shared = None;
            win.z = new_z;
            self.sort_windows_by_z();
            self.render_full();
            return;
        }
        self.create_window(id, Self::infer_overlay_layer(width, height), width, height, pixels);
    }

    pub fn update_window_chunk_pixels(
        &mut self,
        id: u32,
        width: usize,
        height: usize,
        chunk_x: usize,
        chunk_y: usize,
        chunk_w: usize,
        chunk_h: usize,
        pixels: &[u32],
    ) {
        if width == 0 || height == 0 {
            return;
        }
        if chunk_w == 0 || chunk_h == 0 {
            return;
        }
        if chunk_x >= width || chunk_y >= height {
            return;
        }
        if chunk_x.saturating_add(chunk_w) > width || chunk_y.saturating_add(chunk_h) > height {
            return;
        }
        if pixels.len() != chunk_w.saturating_mul(chunk_h) {
            return;
        }

        let is_last_chunk =
            chunk_x.saturating_add(chunk_w) == width && chunk_y.saturating_add(chunk_h) == height;
        let new_z = self.next_z();
        if let Some(win) = self.windows.iter_mut().find(|w| w.id == id) {
            if win.width != width || win.height != height {
                win.width = width;
                win.height = height;
                let fill = if matches!(win.layer, WindowLayer::Status | WindowLayer::Wallpaper) {
                    0x0000_0000
                } else {
                    0xFF30_3048
                };
                win.pixels = vec![fill; width * height];
                win.shared = None;
            }
            for row in 0..chunk_h {
                let src_start = row * chunk_w;
                let src_end = src_start + chunk_w;
                let dst_start = (chunk_y + row) * width + chunk_x;
                let dst_end = dst_start + chunk_w;
                win.pixels[dst_start..dst_end].copy_from_slice(&pixels[src_start..src_end]);
            }
            if matches!(win.layer, WindowLayer::Wallpaper | WindowLayer::Status) {
                if is_last_chunk {
                    self.render_full();
                }
            } else {
                win.z = new_z;
                self.sort_windows_by_z();
                self.render_full();
            }
            return;
        }

        let mut full = vec![0x0000_0000; width * height];
        for row in 0..chunk_h {
            let src_start = row * chunk_w;
            let src_end = src_start + chunk_w;
            let dst_start = (chunk_y + row) * width + chunk_x;
            let dst_end = dst_start + chunk_w;
            full[dst_start..dst_end].copy_from_slice(&pixels[src_start..src_end]);
        }
        self.create_window(id, Self::infer_overlay_layer(width, height), width, height, full);
    }

    pub fn attach_mapped_shared_surface(
        &mut self,
        id: u32,
        width: usize,
        height: usize,
        virt_addr: u64,
        mapped_total_bytes: u64,
    ) -> bool {
        if width == 0 || height == 0 || virt_addr == 0 {
            return false;
        }
        let needed_bytes = match width.checked_mul(height).and_then(|v| v.checked_mul(4)) {
            Some(v) => v as u64,
            None => return false,
        };
        let mapped_bytes = mapped_total_bytes;
        if mapped_bytes < needed_bytes {
            return false;
        }
        let page_count = mapped_bytes.div_ceil(4096);

        if let Some(win) = self.windows.iter_mut().find(|w| w.id == id) {
            win.width = width;
            win.height = height;
            if win.pixels.len() != width * height {
                let fill = if matches!(win.layer, WindowLayer::Status | WindowLayer::Wallpaper) {
                    0x0000_0000
                } else {
                    0xFF30_3048
                };
                win.pixels = vec![fill; width * height];
            }
            win.shared = Some(SharedSurface {
                virt_addr,
                page_count,
            });
            return true;
        }

        let pixels = vec![0x0000_0000; width * height];
        let z = self.next_z();
        let layer = Self::infer_overlay_layer(width, height);
        let (x, y) = match layer {
            WindowLayer::Status => (
                ((self.width - width as i32) / 2).max(0),
                self.status_window_y(height),
            ),
            _ => (
                WINDOW_POS_X + ((id as i32 - 1) * WINDOW_STEP_X),
                WINDOW_POS_Y + ((id as i32 - 1) * WINDOW_STEP_Y),
            ),
        };
        self.windows.push(WindowSurface {
            id,
            x,
            y,
            z,
            layer,
            width,
            height,
            pixels,
            shared: Some(SharedSurface {
                virt_addr,
                page_count,
            }),
        });
        self.sort_windows_by_z();
        true
    }

    pub fn present_shared_surface(&mut self, id: u32) {
        let new_z = self.next_z();
        let Some(win) = self.windows.iter_mut().find(|w| w.id == id) else {
            return;
        };
        let Some(shared) = win.shared.as_ref() else {
            return;
        };
        let total_pixels = win.width.saturating_mul(win.height);
        let mapped_pixels = (shared.page_count as usize).saturating_mul(4096) / 4;
        if mapped_pixels < total_pixels {
            return;
        }
        let src = unsafe { core::slice::from_raw_parts(shared.virt_addr as *const u32, total_pixels) };
        if win.pixels.len() != total_pixels {
            win.pixels.resize(total_pixels, 0xFF30_3048);
        }
        for (dst, s) in win.pixels.iter_mut().zip(src.iter()) {
            *dst = *s;
        }
        win.z = new_z;
        self.sort_windows_by_z();
        self.render_full();
    }

    pub fn layer_of_window(&self, id: u32) -> Option<WindowLayer> {
        self.windows.iter().find(|w| w.id == id).map(|w| w.layer)
    }

    pub fn top_layer(&self) -> Option<WindowLayer> {
        self.windows
            .iter()
            .max_by_key(|w| (w.layer.order(), w.z))
            .map(|w| w.layer)
    }

    pub fn cursor_pos(&self) -> (i32, i32) {
        (self.cursor_x, self.cursor_y)
    }

    pub fn hit_test_top_window(&self, x: i32, y: i32) -> Option<u32> {
        for w in self.windows.iter().rev() {
            let right = w.x + w.width as i32;
            let bottom = w.y + w.height as i32;
            if x >= w.x && y >= w.y && x < right && y < bottom {
                return Some(w.id);
            }
        }
        None
    }

    pub fn is_title_bar_hit(&self, id: u32, x: i32, y: i32) -> bool {
        let Some(w) = self.windows.iter().find(|w| w.id == id) else {
            return false;
        };
        if w.layer != WindowLayer::App {
            return false;
        }
        let right = w.x + w.width as i32;
        let title_bottom = w.y + TITLE_BAR_HEIGHT as i32;
        x >= w.x && y >= w.y && x < right && y < title_bottom
    }

    pub fn window_pos(&self, id: u32) -> Option<(i32, i32)> {
        self.windows.iter().find(|w| w.id == id).map(|w| (w.x, w.y))
    }

    pub fn bring_to_front(&mut self, id: u32) {
        let new_z = self.next_z();
        if let Some(w) = self.windows.iter_mut().find(|w| w.id == id) {
            w.z = new_z;
            self.sort_windows_by_z();
            self.render_full();
        }
    }

    pub fn move_window_to(&mut self, id: u32, x: i32, y: i32) {
        if let Some(w) = self.windows.iter_mut().find(|w| w.id == id) {
            let max_x = self.width.saturating_sub(w.width as i32);
            let mut min_y = 0;
            if w.layer == WindowLayer::App {
                min_y = STATUS_BAR_HEIGHT;
            }
            let max_y = self.height.saturating_sub(w.height as i32);
            w.x = clamp_i32(x, 0, max_x);
            w.y = clamp_i32(y, min_y, max_y.max(min_y));
            self.render_full();
        }
    }

    pub fn move_cursor_by(&mut self, dx: i32, dy: i32) {
        let next_x = clamp_i32(self.cursor_x + dx, 0, self.width - 1);
        let next_y = clamp_i32(self.cursor_y - dy, 0, self.height - 1);
        if next_x == self.cursor_x && next_y == self.cursor_y {
            return;
        }
        self.cursor_x = next_x;
        self.cursor_y = next_y;
        self.update_status_dock_target();
        self.step_status_dock_animation();
        self.render_full();
    }

    fn render_full(&mut self) {
        self.clear_back_buffer(BG_COLOR);
        self.draw_status_bar_base();
        self.draw_windows_to_back_buffer();
        self.draw_cursor_to_back_buffer(self.cursor_x, self.cursor_y);
        self.present_back_buffer();
    }

    fn clear_back_buffer(&mut self, color: u32) {
        let pixel = color | 0xFF00_0000;
        for p in &mut self.back_buffer {
            *p = pixel;
        }
    }

    fn draw_status_bar_base(&mut self) {
        for y in 0..STATUS_BAR_HEIGHT {
            if y >= self.height {
                break;
            }
            for x in 0..self.width {
                let idx = (y * self.stride + x) as usize;
                self.back_buffer[idx] = STATUS_BAR_COLOR;
            }
        }
    }

    fn draw_windows_to_back_buffer(&mut self) {
        for surface in &self.windows {
            if surface.layer == WindowLayer::App {
                draw_app_window_shadow(
                    &mut self.back_buffer,
                    self.width,
                    self.height,
                    self.stride,
                    surface,
                );
            }
            for sy in 0..surface.height {
                for sx in 0..surface.width {
                    let x = surface.x + sx as i32;
                    let y = surface.y + sy as i32;
                    if x < 0 || y < 0 || x >= self.width || y >= self.height {
                        continue;
                    }
                    // App Layer は Status Layer 領域へ描画できない（クリッピング）。
                    if surface.layer == WindowLayer::App && y < STATUS_BAR_HEIGHT {
                        continue;
                    }
                    if surface.layer == WindowLayer::App
                        && is_rounded_corner_pixel(sx, sy, surface.width, WINDOW_CORNER_RADIUS)
                    {
                        continue;
                    }
                    let bb_idx = (y * self.stride + x) as usize;
                    let mut src = surface.pixels[sy * surface.width + sx];
                    if surface.layer == WindowLayer::App {
                        if let Some(chrome) = app_chrome_pixel(sx, sy, surface.width, surface.height)
                        {
                            src = chrome;
                        }
                    }
                    self.back_buffer[bb_idx] = blend_argb(self.back_buffer[bb_idx], src);
                }
            }
        }
    }

    fn draw_cursor_to_back_buffer(&mut self, cx: i32, cy: i32) {
        for sy in 0..self.cursor_sprite.height {
            for sx in 0..self.cursor_sprite.width {
                let sprite_idx = sy * self.cursor_sprite.width + sx;
                let x = cx + sx as i32;
                let y = cy + sy as i32;
                if x < 0 || y < 0 || x >= self.width || y >= self.height {
                    continue;
                }
                let bb_idx = (y * self.stride + x) as usize;
                let dst = self.back_buffer[bb_idx];
                let src = self.cursor_sprite.pixels[sprite_idx];
                let blended = blend_argb(dst, src);
                self.back_buffer[bb_idx] = blended;
            }
        }
    }

    fn present_back_buffer(&mut self) {
        for (i, px) in self.back_buffer.iter().enumerate() {
            unsafe {
                self.fb_ptr.add(i).write_volatile(*px);
            }
        }
    }

    fn sort_windows_by_z(&mut self) {
        self.windows.sort_by_key(|w| (w.layer.order(), w.z));
    }

    fn next_z(&self) -> i32 {
        self.windows
            .iter()
            .map(|w| w.z)
            .max()
            .unwrap_or(0)
            .saturating_add(1)
    }
}

fn blend_argb(dst: u32, src: u32) -> u32 {
    let sa = (src >> 24) & 0xFF;
    if sa == 0 {
        return dst;
    }
    if sa == 0xFF {
        return src | 0xFF00_0000;
    }
    let inv = 255 - sa;
    let sr = (src >> 16) & 0xFF;
    let sg = (src >> 8) & 0xFF;
    let sb = src & 0xFF;
    let dr = (dst >> 16) & 0xFF;
    let dg = (dst >> 8) & 0xFF;
    let db = dst & 0xFF;
    let r = (sr * sa + dr * inv) / 255;
    let g = (sg * sa + dg * inv) / 255;
    let b = (sb * sa + db * inv) / 255;
    0xFF00_0000 | (r << 16) | (g << 8) | b
}

fn draw_app_window_shadow(
    back_buffer: &mut [u32],
    screen_w: i32,
    screen_h: i32,
    stride: i32,
    surface: &WindowSurface,
) {
    let right = surface.x + surface.width as i32;
    let bottom = surface.y + surface.height as i32;
    for y in (surface.y - 2)..=(bottom + 2) {
        if y < 0 || y >= screen_h {
            continue;
        }
        if y < STATUS_BAR_HEIGHT {
            continue;
        }
        for x in (surface.x - 2)..=(right + 2) {
            if x < 0 || x >= screen_w {
                continue;
            }
            let inside = x >= surface.x && x < right && y >= surface.y && y < bottom;
            if inside {
                continue;
            }
            let near_h = x >= surface.x - 1 && x <= right;
            let near_v = y >= surface.y - 1 && y <= bottom;
            let alpha = if near_h && near_v { SHADOW_NEAR_ALPHA } else { SHADOW_FAR_ALPHA };
            let bb_idx = (y * stride + x) as usize;
            back_buffer[bb_idx] = blend_argb(back_buffer[bb_idx], (alpha << 24) | 0x0000_0000);
        }
    }
}

fn app_chrome_pixel(sx: usize, sy: usize, width: usize, height: usize) -> Option<u32> {
    if sy == 0 || sx == 0 || sy + 1 == height || sx + 1 == width {
        return Some(WINDOW_BORDER_COLOR);
    }
    if sy < TITLE_BAR_HEIGHT {
        if sy + 1 == TITLE_BAR_HEIGHT {
            return Some(TITLE_SEPARATOR_COLOR);
        }
        let top = TITLE_TOP_COLOR;
        let bottom = TITLE_BOTTOM_COLOR;
        let t = sy as u32;
        let h = TITLE_BAR_HEIGHT as u32;
        let r = lerp_channel((top >> 16) & 0xFF, (bottom >> 16) & 0xFF, t, h);
        let g = lerp_channel((top >> 8) & 0xFF, (bottom >> 8) & 0xFF, t, h);
        let b = lerp_channel(top & 0xFF, bottom & 0xFF, t, h);
        if let Some(c) = traffic_light_pixel(sx, sy) {
            return Some(c);
        }
        return Some(0xFF00_0000 | (r << 16) | (g << 8) | b);
    }
    None
}

fn traffic_light_pixel(sx: usize, sy: usize) -> Option<u32> {
    let radius = TRAFFIC_DIAMETER.max(2) / 2;
    let step = TRAFFIC_DIAMETER + TRAFFIC_GAP;
    let cx0 = TRAFFIC_OFFSET_X + radius;
    let cy = TRAFFIC_OFFSET_Y;
    let ring_outer = radius + TRAFFIC_RING_WIDTH;
    let fill_r2 = radius * radius;
    let ring_r2 = ring_outer * ring_outer;
    let buttons = [
        (cx0, cy, TRAFFIC_RED),
        (cx0 + step, cy, TRAFFIC_YELLOW),
        (cx0 + step * 2, cy, TRAFFIC_GREEN),
    ];
    let px = sx as isize;
    let py = sy as isize;
    for (cx, cy, color) in buttons {
        let dx = px - cx;
        let dy = py - cy;
        let d2 = dx * dx + dy * dy;
        if d2 <= fill_r2 {
            return Some(color);
        }
        if d2 <= ring_r2 {
            return Some(TRAFFIC_RING);
        }
    }
    None
}

fn is_rounded_corner_pixel(sx: usize, sy: usize, width: usize, radius: usize) -> bool {
    if sy >= radius {
        return false;
    }
    let r = radius as isize;
    let y = sy as isize;
    if sx < radius {
        let x = sx as isize;
        let dx = r - x - 1;
        let dy = r - y - 1;
        return dx * dx + dy * dy >= r * r;
    }
    if sx + radius >= width {
        let x = (width - sx - 1) as isize;
        let dx = r - x - 1;
        let dy = r - y - 1;
        return dx * dx + dy * dy >= r * r;
    }
    false
}

fn lerp_channel(a: u32, b: u32, t: u32, max_t: u32) -> u32 {
    if max_t == 0 {
        return a;
    }
    (a * (max_t - t) + b * t) / max_t
}

fn clamp_i32(v: i32, min: i32, max: i32) -> i32 {
    if v < min {
        min
    } else if v > max {
        max
    } else {
        v
    }
}
