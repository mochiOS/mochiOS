use crate::backend::{ComponentRenderer, PropertyValue, RawOSEvent, ViewKitBackend, WindowBackend};
use image::{ImageBuffer, RgbaImage};
use serde_json::Value;
use std::any::Any;
use std::collections::HashMap;
use tiny_skia::{Color, Paint, Pixmap, Transform};
#[cfg(feature = "wayland")]
use wayland_client::Connection;

/// シンプルなコンポーネントテンプレートのキャッシュ構造
#[allow(unused)]
struct ComponentTemplate {
    name: String,
    raw: String,
    has_children_slot: bool,
    content_types: Vec<String>,
}
pub struct BackendImpl {
    width: u32,
    height: u32,
    templates: HashMap<String, ComponentTemplate>,
    // （ARGB as u32）
    pixels: Vec<u32>,
    #[cfg(feature = "wayland")]
    wayland_enabled: bool,
    #[cfg(feature = "wayland")]
    shm_tmp: Option<tempfile::NamedTempFile>,
    #[cfg(feature = "wayland")]
    conn: Option<Connection>,
}

impl BackendImpl {
    pub fn new() -> Result<Self, String> {
        // NOTE: 本来はここで wayland-client / sctk を用いて Connection::connect_to_env()
        // と registry 取得、wl_shm / wl_compositor / xdg_wm_base 等を初期化します。
        // 実装は環境依存なので、まずはレンダリングパスとテンプレート処理を整備します。
        println!(
            "ViewKit: Backend initialized (Wayland connection will be established when available)"
        );

        Ok(Self {
            width: 800,
            height: 600,
            templates: HashMap::new(),
            pixels: vec![0u32; (800 * 600) as usize],
            #[cfg(feature = "wayland")]
            wayland_enabled: {
                // try to connect to Wayland display; if fails, keep false
                match Connection::connect_to_env() {
                    Ok(conn) => {
                        println!("ViewKit: connected to Wayland (feature=wayland)");
                        // store connection for future event handling
                        // we will set conn below using a separate field initialization
                        true
                    }
                    Err(e) => {
                        eprintln!("ViewKit: failed to connect to Wayland: {}", e);
                        false
                    }
                }
            },
            #[cfg(feature = "wayland")]
            shm_tmp: None,
            #[cfg(feature = "wayland")]
            conn: match Connection::connect_to_env() {
                Ok(c) => Some(c),
                Err(_) => None,
            },
        })
    }

    fn ensure_buffer(&mut self, width: u32, height: u32) {
        let needed = (width * height) as usize;
        if self.pixels.len() != needed {
            self.pixels.resize(needed, 0);
            self.width = width;
            self.height = height;
        }
    }

    // (helper functions follow)
}

/// 簡易的な色文字列 (#RRGGBB) を ARGB(u32) に変換
fn parse_color_hex(s: &str) -> u32 {
    let s = s.trim();
    let s = s.strip_prefix('#').unwrap_or(s);
    if s.len() == 6 {
        if let Ok(v) = u32::from_str_radix(s, 16) {
            // ARGB (opaque)
            return 0xFF000000u32 | v;
        }
    }
    0xFF000000u32 // default: black
}

/// 再帰的に layout を計算してピクセルに描画する。（`self` を借用しない自由関数）
fn render_node_draw(pixmap: &mut Pixmap, node: &UiNode, x: i32, y: i32, width: i32, height: i32) {
    let mut paint = Paint::default();
    if let Some(color) = node.props.get("color") {
        if let Value::String(s) = color {
            let argb = parse_color_hex(s);
            let r = ((argb >> 16) & 0xFF) as u8;
            let g = ((argb >> 8) & 0xFF) as u8;
            let b = (argb & 0xFF) as u8;
            paint.set_color(Color::from_rgba8(r, g, b, 255));
        }
    } else {
        paint.set_color(Color::from_rgba8(0xEE, 0xEE, 0xEE, 255));
    }

    let rect = tiny_skia::Rect::from_xywh(x as f32, y as f32, width as f32, height as f32).unwrap();
    pixmap.fill_rect(rect, &paint, Transform::identity(), None);

    // 子要素は縦に積む
    if !node.children.is_empty() {
        let child_h = (height as usize / node.children.len()) as i32;
        for (i, child) in node.children.iter().enumerate() {
            let cy = y + i as i32 * child_h;
            render_node_draw(pixmap, child, x + 4, cy + 4, width - 8, child_h - 8);
        }
    } else {
        let has_textish_content = node.props.get("text").is_some()
            || matches!(node.content.as_ref().map(|c| c.ty.as_str()), Some("String"));
        if has_textish_content {
            let mut label_paint = Paint::default();
            label_paint.set_color(Color::from_rgba8(0x11, 0x11, 0x11, 255));
            let lw = (width as f32 * 0.6).max(8.0);
            let lh = 18.0f32.min(height as f32 * 0.5);
            let lx = x as f32 + 8.0;
            let ly = y as f32 + (height as f32 - lh) / 2.0;
            let lrect = tiny_skia::Rect::from_xywh(lx, ly, lw, lh).unwrap();
            pixmap.fill_rect(lrect, &label_paint, Transform::identity(), None);
        }
    }
}

/// 内部的な UI ノード表現（JSON から復元）
#[derive(Debug, Clone)]
#[allow(unused)]
struct UiNode {
    id: Option<String>,
    component: String,
    props: serde_json::Map<String, Value>,
    content: Option<UiContent>,
    children: Vec<UiNode>,
}

#[derive(Debug, Clone)]
#[allow(unused)]
struct UiContent {
    ty: String,
    value: String,
}

impl UiNode {
    fn from_value(v: &Value) -> Option<Self> {
        if !v.is_object() {
            return None;
        }
        let obj = v.as_object().unwrap();
        let component = obj
            .get("component")
            .and_then(|c| c.as_str())
            .unwrap_or("div")
            .to_string();
        let id = obj
            .get("id")
            .and_then(|s| s.as_str())
            .map(|s| s.to_string());
        let props = obj
            .get("props")
            .and_then(|p| p.as_object())
            .cloned()
            .unwrap_or_default();
        let content = obj.get("content").and_then(|c| c.as_object()).and_then(|m| {
            let ty = m.get("type").and_then(|v| v.as_str())?;
            let value = m.get("value").and_then(|v| v.as_str())?;
            Some(UiContent {
                ty: ty.to_string(),
                value: value.to_string(),
            })
        });
        let mut children = Vec::new();
        if let Some(arr) = obj.get("children").and_then(|c| c.as_array()) {
            for child in arr.iter() {
                if let Some(n) = UiNode::from_value(child) {
                    children.push(n);
                }
            }
        }
        Some(UiNode {
            id,
            component,
            props,
            content,
            children,
        })
    }
}

impl WindowBackend for BackendImpl {
    fn create_window(&mut self, width: u32, height: u32, title: &str, no_decoration: bool) {
        self.width = width;
        self.height = height;
        self.ensure_buffer(width, height);
        println!(
            "ViewKit: (stub) create_window '{}' {}x{} deco:{}",
            title, width, height, !no_decoration
        );
        // 本来はここで wl_surface, xdg_toplevel を作成する
    }

    fn swap_buffers(&mut self, buffer: &[u32], width: u32, height: u32) {
        // If compiled with `--features wayland` and Wayland connection succeeded,
        // here is where we would perform wl_shm buffer creation, attach -> damage -> commit.
        // For now, if Wayland is enabled but not fully implemented, fall back to PNG and log intentions.

        #[cfg(feature = "wayland")]
        {
            if self.wayland_enabled {
                // Attempt to create a memmap-backed tempfile and write RGBA data into it.
                // This prepares an shm-backed file that can be used to create a wl_shm_pool.
                match NamedTempFile::new() {
                    Ok(mut tmp) => {
                        let size = (width as usize)
                            .saturating_mul(height as usize)
                            .saturating_mul(4);
                        if let Err(e) = tmp.as_file_mut().set_len(size as u64) {
                            eprintln!("ViewKit: failed to set tmpfile len: {}", e);
                        } else {
                            match unsafe { MmapMut::map_mut(tmp.as_file()) } {
                                Ok(mut mmap) => {
                                    // copy ARGB(u32) -> RGBA bytes into mmap
                                    for y in 0..height as usize {
                                        for x in 0..width as usize {
                                            let idx = y * width as usize + x;
                                            let px = buffer[idx];
                                            let a = ((px >> 24) & 0xFF) as u8;
                                            let r = ((px >> 16) & 0xFF) as u8;
                                            let g = ((px >> 8) & 0xFF) as u8;
                                            let b = (px & 0xFF) as u8;
                                            let off = (idx * 4) as usize;
                                            mmap[off] = r;
                                            mmap[off + 1] = g;
                                            mmap[off + 2] = b;
                                            mmap[off + 3] = a;
                                        }
                                    }
                                    if let Err(e) = mmap.flush() {
                                        eprintln!("ViewKit: failed to flush mmap: {}", e);
                                    }
                                    println!(
                                        "ViewKit: prepared shm tempfile at {:?} ({} bytes)",
                                        tmp.path(),
                                        size
                                    );
                                    // Keep the tempfile alive by storing it in self.shm_tmp so it isn't deleted
                                    #[cfg(feature = "wayland")]
                                    {
                                        self.shm_tmp = Some(tmp);
                                    }
                                }
                                Err(e) => eprintln!("ViewKit: failed to mmap tmpfile: {}", e),
                            }
                        }
                    }
                    Err(e) => eprintln!("ViewKit: failed to create tempfile for shm pool: {}", e),
                }
                // Fall through to PNG fallback for visibility
            }
        }

        // PNG fallback: write out frame for debugging
        self.ensure_buffer(width, height);
        self.pixels.copy_from_slice(buffer);

        // Convert ARGB u32 -> RGBA8 for image crate
        let mut img: RgbaImage = ImageBuffer::new(width, height);
        for y in 0..height {
            for x in 0..width {
                let idx = (y * width + x) as usize;
                let px = self.pixels[idx];
                let a = ((px >> 24) & 0xFF) as u8;
                let r = ((px >> 16) & 0xFF) as u8;
                let g = ((px >> 8) & 0xFF) as u8;
                let b = (px & 0xFF) as u8;
                img.put_pixel(x, y, image::Rgba([r, g, b, a]));
            }
        }
        if let Err(e) = img.save("/tmp/viewkit_frame.png") {
            eprintln!("ViewKit: failed to save frame png: {}", e);
        } else {
            println!(
                "ViewKit: frame written to /tmp/viewkit_frame.png ({}x{})",
                width, height
            );
        }
    }

    fn poll_os_event(&mut self) -> Option<RawOSEvent> {
        // 実際の Wayland イベントのポーリングはここで行う。
        // 今はダミー実装を返すのみ。
        None
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl ComponentRenderer for BackendImpl {
    fn register_component(&mut self, name: &str, template_html: &str) -> Result<(), String> {
        // 簡易的にタグ名と <children /> があるかを検出して保存
        let has_children = template_html.contains("<children")
            || template_html.contains("<slot")
            || template_html.contains("{children}");

        // detect content types declared in the template
        // patterns: data-content-type="foo" or <content type="foo">
        let mut content_types = Vec::new();
        // search for data-content-type="..."
        let mut search_idx = 0usize;
        while let Some(p) = template_html[search_idx..].find("data-content-type=\"") {
            let start = search_idx + p + "data-content-type=\"".len();
            if let Some(end_rel) = template_html[start..].find('"') {
                let val = &template_html[start..start + end_rel];
                content_types.push(val.to_string());
                search_idx = start + end_rel + 1;
                continue;
            } else {
                break;
            }
        }
        // search for <content ... type="...">
        search_idx = 0;
        while let Some(p) = template_html[search_idx..].find("<content") {
            let start = search_idx + p;
            if let Some(tp_pos) = template_html[start..].find("type=\"") {
                let tstart = start + tp_pos + "type=\"".len();
                if let Some(tend_rel) = template_html[tstart..].find('"') {
                    let val = &template_html[tstart..tstart + tend_rel];
                    content_types.push(val.to_string());
                    search_idx = tstart + tend_rel + 1;
                    continue;
                }
            }
            search_idx = start + 8;
        }
        content_types.sort();
        content_types.dedup();

        let tpl = ComponentTemplate {
            name: name.to_string(),
            raw: template_html.to_string(),
            has_children_slot: has_children,
            content_types: content_types.clone(),
        };
        self.templates.insert(name.to_string(), tpl);
        println!(
            "ViewKit: Registered component '{}' (children_slot={})",
            name, has_children
        );
        if !content_types.is_empty() {
            println!("ViewKit: component '{}' content_types={:?}", name, content_types);
        }
        Ok(())
    }

    fn update_ui_tree(&mut self, tree_delta_json: &str) {
        // JSON -> UiNode tree
        match serde_json::from_str::<Value>(tree_delta_json) {
            Ok(v) => {
                if let Some(root_node) = UiNode::from_value(&v) {
                    // Prepare pixmap
                    let mut pixmap =
                        Pixmap::new(self.width, self.height).expect("pixmap alloc");
                    // background
                    let mut bg_paint = Paint::default();
                    bg_paint.set_color(Color::from_rgba8(0xFF, 0xFF, 0xFF, 255));
                    let full =
                        tiny_skia::Rect::from_xywh(0.0, 0.0, self.width as f32, self.height as f32)
                            .unwrap();
                    pixmap.fill_rect(full, &bg_paint, Transform::identity(), None);

                    // render
                    render_node_draw(
                        &mut pixmap,
                        &root_node,
                        0,
                        0,
                        self.width as i32,
                        self.height as i32,
                    );

                    // copy pixmap to pixels (RGBA bytes -> ARGB u32)
                    let w = self.width as usize;
                    let h = self.height as usize;
                    let data = pixmap.data();
                    for yy in 0..h {
                        for xx in 0..w {
                            let i = (yy * w + xx) * 4;
                            let r = data[i];
                            let g = data[i + 1];
                            let b = data[i + 2];
                            let a = data[i + 3];
                            let argb = ((a as u32) << 24)
                                | ((r as u32) << 16)
                                | ((g as u32) << 8)
                                | (b as u32);
                            let idx = yy * w + xx;
                            self.pixels[idx] = argb;
                        }
                    }

                    // 最後に swap_buffers を呼ぶ (ここでは self.pixels をクローンして渡すことで
                    // 借用競合を避ける簡易実装)
                    let outbuf = self.pixels.clone();
                    self.swap_buffers(&outbuf, self.width, self.height);
                } else {
                    eprintln!("ViewKit: Failed to parse UI JSON into node");
                }
            }
            Err(e) => {
                eprintln!("ViewKit: update_ui_tree - invalid json: {}", e);
            }
        }
    }

    fn set_component_property(&mut self, _component_id: &str, _key: &str, _value: PropertyValue) {}
}

impl ViewKitBackend for BackendImpl {}
