use std::collections::HashMap;

use fontdue::Font;
use tiny_skia::{Color, Paint, PathBuilder, Pixmap, Rect, Transform};
use ui_layout::{
    AlignItems, Display, FlexDirection, ItemStyle, JustifyContent, LayoutEngine, LayoutNode, Length,
    Style,
};

#[derive(Clone, Debug, Default)]
pub struct VComponent {
    template: String,
    width: Option<u32>,
    height: Option<u32>,
    label: Option<String>,
    children: Vec<VComponent>,
}

impl VComponent {
    pub fn from_str(template: &str) -> Self {
        Self {
            template: template.to_string(),
            ..Default::default()
        }
    }

    pub fn width(mut self, w: u32) -> Self {
        self.width = Some(w);
        self
    }

    pub fn height(mut self, h: u32) -> Self {
        self.height = Some(h);
        self
    }

    pub fn label(mut self, s: String) -> Self {
        self.label = Some(s);
        self
    }

    pub fn child(mut self, c: VComponent) -> Self {
        self.children.push(c);
        self
    }
}

pub fn render_component_to_pixmap(component: &VComponent, width: u32, height: u32) -> Vec<u32> {
    render_component_to_pixmap_with_asset_root(component, width, height, None)
}

pub fn render_component_to_pixmap_with_asset_root(
    component: &VComponent,
    width: u32,
    height: u32,
    asset_root: Option<&str>,
) -> Vec<u32> {
    let mut pixmap = Pixmap::new(width, height).expect("pixmap");
    pixmap.fill(Color::from_rgba8(0, 0, 0, 0));

    let ctx = RenderContext::new(asset_root);
    render_component_impl(&ctx, &mut pixmap, component, 0.0, 0.0, width as f32, height as f32);

    // Convert RGBA bytes -> ARGB u32 for Binder/Kagami
    let mut out = vec![0u32; (width as usize).saturating_mul(height as usize)];
    let data = pixmap.data();
    for i in 0..out.len() {
        let off = i * 4;
        let r = data[off] as u32;
        let g = data[off + 1] as u32;
        let b = data[off + 2] as u32;
        let a = data[off + 3] as u32;
        out[i] = (a << 24) | (r << 16) | (g << 8) | b;
    }
    out
}

struct RenderContext {
    asset_root: Option<String>,
    font: Option<Font>,
    image_cache: std::cell::RefCell<HashMap<String, image::RgbaImage>>,
}

impl RenderContext {
    fn new(asset_root: Option<&str>) -> Self {
        let font = std::fs::read("/system/fonts/NotoSansJP-Regular.ttf")
            .ok()
            .and_then(|data| Font::from_bytes(data, fontdue::FontSettings::default()).ok());
        Self {
            asset_root: asset_root.map(|s| s.to_string()),
            font,
            image_cache: std::cell::RefCell::new(HashMap::new()),
        }
    }

    fn resolve_asset_path(&self, src: &str) -> Option<String> {
        let src = src.trim();
        if src.is_empty() {
            return None;
        }
        if src.starts_with('/') {
            return Some(src.to_string());
        }
        if let Some(root) = &self.asset_root {
            return Some(format!("{}/{}", root.trim_end_matches('/'), src));
        }
        // best-effort fallback: interpret relative to root
        Some(format!("/{}", src))
    }
}

#[derive(Clone, Debug, Default)]
struct ComputedStyle {
    layout: Style,
    bg: Option<u32>,
    border_radius: f32,
    text_align_center: bool,
    padding: (f32, f32, f32, f32),
}

fn render_component_impl(
    ctx: &RenderContext,
    pixmap: &mut Pixmap,
    component: &VComponent,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) {
    let (css, html) = split_style_and_html(&component.template);
    let css = css
        .replace("CONTENT_W", &format!("{}px", component.width.unwrap_or(w as u32)))
        .replace("CONTENT_H", &format!("{}px", component.height.unwrap_or(h as u32)));

    let class_styles = parse_class_css_rules(&css);

    let root = parse_html_fragment_simple(&html);

    let render_tree = build_render_tree(
        ctx,
        &class_styles,
        &root,
        component,
    );

    layout_and_paint(ctx, pixmap, &render_tree, x, y, w, h);
}

#[derive(Clone, Debug)]
enum HtmlNode {
    Element(HtmlElement),
    Text(String),
}

#[derive(Clone, Debug, Default)]
struct HtmlElement {
    tag: String,
    attrs: HashMap<String, String>,
    children: Vec<HtmlNode>,
}

#[derive(Clone)]
enum RenderNodeKind {
    Element { tag: String, class: Option<String> },
    Text { text: String },
    Image { src: String },
}

#[derive(Clone)]
struct RenderNode {
    kind: RenderNodeKind,
    style: ComputedStyle,
    children: Vec<RenderNode>,
}

fn split_style_and_html(template: &str) -> (String, String) {
    let mut css = String::new();
    let mut html = template.to_string();
    if let Some(start) = template.find("<style") {
        if let Some(end) = template.find("</style>") {
            // best-effort: extract between first '>' and </style>
            let head = &template[start..end];
            if let Some(gt) = head.find('>') {
                css = head[gt + 1..].to_string();
            }
            html = template[end + "</style>".len()..].to_string();
        }
    }
    (css, html)
}

fn parse_html_fragment_simple(html: &str) -> HtmlNode {
    // Minimal HTML-ish parser for ViewKit templates:
    // - elements: <tag ...>, </tag>, <tag ... />
    // - attrs: key="value" / key='value' / key=value / key
    // - ignores comments/doctype
    // - produces a synthetic root <div> wrapping top-level nodes
    let mut root = HtmlElement {
        tag: "div".to_string(),
        ..Default::default()
    };
    let mut stack: Vec<HtmlElement> = Vec::new();
    stack.push(root);

    let mut text_buf = String::new();
    let mut i = 0usize;
    let bytes = html.as_bytes();
    while i < bytes.len() {
        if bytes[i] != b'<' {
            text_buf.push(bytes[i] as char);
            i += 1;
            continue;
        }

        if !text_buf.trim().is_empty() {
            let txt = text_buf.clone();
            if let Some(top) = stack.last_mut() {
                top.children.push(HtmlNode::Text(txt));
            }
        }
        text_buf.clear();

        let Some(gt_rel) = html[i..].find('>') else { break };
        let gt = i + gt_rel;
        let mut inner = html[i + 1..gt].trim().to_string();
        i = gt + 1;

        if inner.starts_with("!--") {
            continue;
        }
        if inner.starts_with('!') || inner.starts_with('?') {
            continue;
        }

        let is_close = inner.starts_with('/');
        if is_close {
            inner = inner[1..].trim().to_string();
            let close_tag = inner
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            if close_tag.is_empty() {
                continue;
            }
            if stack.len() > 1 {
                let popped = stack.pop().unwrap();
                let node = HtmlNode::Element(popped);
                if let Some(top) = stack.last_mut() {
                    top.children.push(node);
                }
            }
            continue;
        }

        let self_close = inner.ends_with('/');
        if self_close {
            inner.pop();
            inner = inner.trim().to_string();
        }

        let (tag, attrs) = parse_tag_and_attrs(&inner);
        if tag.is_empty() {
            continue;
        }
        let el = HtmlElement {
            tag,
            attrs,
            children: Vec::new(),
        };

        if self_close {
            if let Some(top) = stack.last_mut() {
                top.children.push(HtmlNode::Element(el));
            }
        } else {
            stack.push(el);
        }
    }

    if !text_buf.trim().is_empty() {
        if let Some(top) = stack.last_mut() {
            top.children.push(HtmlNode::Text(text_buf));
        }
    }

    while stack.len() > 1 {
        let popped = stack.pop().unwrap();
        if let Some(top) = stack.last_mut() {
            top.children.push(HtmlNode::Element(popped));
        }
    }
    root = stack.pop().unwrap();
    HtmlNode::Element(root)
}

fn parse_tag_and_attrs(inner: &str) -> (String, HashMap<String, String>) {
    let mut s = inner.trim();
    let mut tag = String::new();
    while let Some(ch) = s.chars().next() {
        if ch.is_whitespace() {
            break;
        }
        tag.push(ch);
        s = &s[ch.len_utf8()..];
    }
    let tag = tag.to_ascii_lowercase();
    let mut attrs = HashMap::new();
    let mut rest = s.trim();
    while !rest.is_empty() {
        // key
        let mut key = String::new();
        let mut j = 0usize;
        for (idx, ch) in rest.char_indices() {
            if ch.is_whitespace() || ch == '=' {
                j = idx;
                break;
            }
            key.push(ch);
            j = idx + ch.len_utf8();
        }
        if key.is_empty() {
            break;
        }
        rest = rest[j..].trim_start();
        let key_l = key.to_ascii_lowercase();
        if rest.starts_with('=') {
            rest = rest[1..].trim_start();
            if rest.starts_with('"') || rest.starts_with('\'') {
                let q = rest.chars().next().unwrap();
                rest = &rest[q.len_utf8()..];
                if let Some(end) = rest.find(q) {
                    let val = rest[..end].to_string();
                    attrs.insert(key_l, val);
                    rest = rest[end + q.len_utf8()..].trim_start();
                } else {
                    attrs.insert(key_l, rest.to_string());
                    break;
                }
            } else {
                let mut end = rest.len();
                for (idx, ch) in rest.char_indices() {
                    if ch.is_whitespace() {
                        end = idx;
                        break;
                    }
                }
                let val = rest[..end].to_string();
                attrs.insert(key_l, val);
                rest = rest[end..].trim_start();
            }
        } else {
            // boolean attr
            attrs.insert(key_l, String::new());
        }
    }
    (tag, attrs)
}

fn get_attr(el: &HtmlElement, name: &str) -> Option<String> {
    el.attrs.get(&name.to_ascii_lowercase()).cloned()
}

fn build_render_tree(
    ctx: &RenderContext,
    class_styles: &HashMap<String, HashMap<String, String>>,
    node: &HtmlNode,
    component: &VComponent,
) -> RenderNode {
    // Handle special tags
    if let HtmlNode::Element(el) = node {
        let tag_lower = el.tag.to_ascii_lowercase();
        if tag_lower == "children" {
            // expand component children
            let mut kids = Vec::new();
            for ch in &component.children {
                let (css, html) = split_style_and_html(&ch.template);
                let css = css
                    .replace("CONTENT_W", &format!("{}px", ch.width.unwrap_or(0)))
                    .replace("CONTENT_H", &format!("{}px", ch.height.unwrap_or(0)));
                let class_styles_child = parse_class_css_rules(&css);
                let root = parse_html_fragment_simple(&html);
                kids.push(build_render_tree(ctx, &class_styles_child, &root, ch));
            }
            return RenderNode {
                kind: RenderNodeKind::Element {
                    tag: "div".to_string(),
                    class: None,
                },
                style: ComputedStyle::default(),
                children: kids,
            };
        }
        if tag_lower == "content" {
            let ty = get_attr(el, "type").unwrap_or_default();
            if ty.eq_ignore_ascii_case("string") {
                return RenderNode {
                    kind: RenderNodeKind::Text {
                        text: component.label.clone().unwrap_or_default(),
                    },
                    style: ComputedStyle::default(),
                    children: vec![],
                };
            }
        }
        if tag_lower == "img" {
            let src = get_attr(el, "src").unwrap_or_default();
            if let Some(resolved) = ctx.resolve_asset_path(&src) {
                let style = compute_style_for_node(el, class_styles);
                return RenderNode {
                    kind: RenderNodeKind::Image { src: resolved },
                    style,
                    children: vec![],
                };
            }
        }
    }

    // Text nodes
    if let HtmlNode::Text(text) = node {
        let text = text.trim();
        if !text.is_empty() {
            return RenderNode {
                kind: RenderNodeKind::Text {
                    text: text.to_string(),
                },
                style: ComputedStyle::default(),
                children: vec![],
            };
        }
    }

    // Generic element
    let mut children = Vec::new();
    let (tag, class, style) = match node {
        HtmlNode::Element(el) => {
            let style = compute_style_for_node(el, class_styles);
            (
                el.tag.clone(),
                get_attr(el, "class"),
                style,
            )
        }
        HtmlNode::Text(_) => ("div".to_string(), None, ComputedStyle::default()),
    };
    if let HtmlNode::Element(el) = node {
        for child in &el.children {
            children.push(build_render_tree(ctx, class_styles, child, component));
        }
    }

    RenderNode {
        kind: RenderNodeKind::Element {
            tag,
            class,
        },
        style,
        children,
    }
}

fn parse_class_css_rules(css: &str) -> HashMap<String, HashMap<String, String>> {
    // Very small CSS parser: only supports `.class { key: value; ... }`
    let mut map: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut rest = css;
    loop {
        let dot = match rest.find('.') {
            Some(i) => i,
            None => break,
        };
        rest = &rest[dot + 1..];
        let brace = match rest.find('{') {
            Some(i) => i,
            None => break,
        };
        let class = rest[..brace].trim().split_whitespace().next().unwrap_or("").to_string();
        rest = &rest[brace + 1..];
        let end = match rest.find('}') {
            Some(i) => i,
            None => break,
        };
        let body = &rest[..end];
        rest = &rest[end + 1..];
        if class.is_empty() {
            continue;
        }
        let decls = map.entry(class).or_default();
        for part in body.split(';') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            if let Some((k, v)) = part.split_once(':') {
                decls.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
            }
        }
    }
    map
}

fn compute_style_for_node(
    el: &HtmlElement,
    class_styles: &HashMap<String, HashMap<String, String>>,
) -> ComputedStyle {
    let mut out = ComputedStyle::default();
    let class = get_attr(el, "class").unwrap_or_default();
    if class.is_empty() {
        return out;
    }
    let cls = class.split_whitespace().next().unwrap_or("");
    let decls = match class_styles.get(cls) {
        Some(d) => d,
        None => return out,
    };

    let mut style = Style::default();
    let mut padding = (0f32, 0f32, 0f32, 0f32);

    for (k, v) in decls {
        match k.as_str() {
            "background-color" => out.bg = Some(parse_color_hex(v)),
            "border-radius" => out.border_radius = parse_border_radius(v),
            "display" => {
                if v.trim().eq_ignore_ascii_case("flex") {
                    style.display = Display::Flex {
                        flex_direction: FlexDirection::Row,
                    };
                }
            }
            "flex-direction" => {
                if let Display::Flex { flex_direction } = &mut style.display {
                    *flex_direction = if v.trim().eq_ignore_ascii_case("column") {
                        FlexDirection::Column
                    } else {
                        FlexDirection::Row
                    };
                } else if v.trim().eq_ignore_ascii_case("column") {
                    style.display = Display::Flex {
                        flex_direction: FlexDirection::Column,
                    };
                }
            }
            "justify-content" => {
                style.justify_content = match v.trim() {
                    "center" => JustifyContent::Center,
                    "flex-end" | "end" => JustifyContent::End,
                    "space-between" => JustifyContent::SpaceBetween,
                    _ => JustifyContent::Start,
                };
            }
            "align-items" => {
                style.align_items = match v.trim() {
                    "center" => AlignItems::Center,
                    "flex-end" | "end" => AlignItems::End,
                    "stretch" => AlignItems::Stretch,
                    _ => AlignItems::Start,
                };
            }
            "flex" => {
                if v.trim() == "1" {
                    style.item_style = ItemStyle {
                        flex_grow: 1.0,
                        ..Default::default()
                    };
                }
            }
            "width" => {
                style.size.width = parse_length(v);
            }
            "height" => {
                style.size.height = parse_length(v);
            }
            "padding" => {
                padding = parse_box_1_2(v);
            }
            "margin-left" => {
                style.spacing.margin_left =
                    parse_px(v).map(Length::Px).unwrap_or(Length::Px(0.0));
            }
            "text-align" => {
                out.text_align_center = v.trim().eq_ignore_ascii_case("center");
            }
            _ => {}
        }
    }

    out.padding = padding;
    out.layout = style;
    out
}

fn parse_length(v: &str) -> Length {
    let v = v.trim();
    if v.eq_ignore_ascii_case("fit-content") || v.eq_ignore_ascii_case("auto") {
        return Length::Auto;
    }
    if let Some(px) = parse_px(v) {
        return Length::Px(px);
    }
    if let Some(pct) = v.strip_suffix('%') {
        if let Ok(f) = pct.trim().parse::<f32>() {
            return Length::Percent(f / 100.0);
        }
    }
    Length::Auto
}

fn parse_px(v: &str) -> Option<f32> {
    let v = v.trim();
    let v = v.strip_suffix("px").unwrap_or(v);
    v.trim().parse::<f32>().ok()
}

fn parse_box_1_2(v: &str) -> (f32, f32, f32, f32) {
    // CSS padding shorthand: 1 or 2 values
    let parts: Vec<_> = v.split_whitespace().collect();
    if parts.is_empty() {
        return (0.0, 0.0, 0.0, 0.0);
    }
    if parts.len() == 1 {
        let a = parse_px(parts[0]).unwrap_or(0.0);
        return (a, a, a, a);
    }
    let v0 = parse_px(parts[0]).unwrap_or(0.0);
    let v1 = parse_px(parts[1]).unwrap_or(0.0);
    (v0, v1, v0, v1)
}

fn parse_border_radius(v: &str) -> f32 {
    // For now, approximate multi-value border-radius by using the first value.
    let first = v.split_whitespace().next().unwrap_or("");
    parse_px(first).unwrap_or(0.0)
}

fn parse_color_hex(s: &str) -> u32 {
    let s = s.trim();
    let s = s.strip_prefix('#').unwrap_or(s);
    if s.len() == 6 {
        if let Ok(v) = u32::from_str_radix(s, 16) {
            return 0xFF00_0000u32 | v;
        }
    }
    0xFF00_0000u32
}

fn layout_and_paint(
    ctx: &RenderContext,
    pixmap: &mut Pixmap,
    node: &RenderNode,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) {
    let mut layout_root = to_layout_node(node);
    LayoutEngine::layout(&mut layout_root, w, h);
    paint_from_layout(ctx, pixmap, node, &layout_root, x, y);
}

fn to_layout_node(node: &RenderNode) -> LayoutNode {
    let mut style = node.style.layout.clone();
    // Apply padding into layout style
    style.spacing.padding_top = Length::Px(node.style.padding.0);
    style.spacing.padding_right = Length::Px(node.style.padding.1);
    style.spacing.padding_bottom = Length::Px(node.style.padding.2);
    style.spacing.padding_left = Length::Px(node.style.padding.3);

    let children: Vec<_> = node.children.iter().map(to_layout_node).collect();
    if children.is_empty() {
        LayoutNode::new(style)
    } else {
        LayoutNode::with_children(style, children)
    }
}

fn paint_from_layout(
    ctx: &RenderContext,
    pixmap: &mut Pixmap,
    node: &RenderNode,
    layout: &LayoutNode,
    ox: f32,
    oy: f32,
) {
    let (bx, by, bw, bh) = match &layout.layout_boxes {
        ui_layout::LayoutBoxes::Single(b) => (b.border_box.x, b.border_box.y, b.border_box.width, b.border_box.height),
        _ => (0.0, 0.0, 0.0, 0.0),
    };
    let x = ox + bx;
    let y = oy + by;

    // Paint background
    if let Some(argb) = node.style.bg {
        let mut paint = Paint::default();
        let r = ((argb >> 16) & 0xFF) as u8;
        let g = ((argb >> 8) & 0xFF) as u8;
        let b = (argb & 0xFF) as u8;
        paint.set_color(Color::from_rgba8(r, g, b, 255));
        if node.style.border_radius > 0.0 {
            if let Some(path) = rounded_rect_path(x, y, bw, bh, node.style.border_radius) {
                pixmap.fill_path(&path, &paint, tiny_skia::FillRule::Winding, Transform::identity(), None);
            }
        } else if let Some(rect) = Rect::from_xywh(x, y, bw, bh) {
            pixmap.fill_rect(rect, &paint, Transform::identity(), None);
        }
    }

    // Paint leaf content
    match &node.kind {
        RenderNodeKind::Text { text } => {
            paint_text(ctx, pixmap, text, x, y, bw, bh, node.style.text_align_center);
        }
        RenderNodeKind::Image { src } => {
            paint_image(ctx, pixmap, src, x, y, bw, bh);
        }
        _ => {}
    }

    // Paint children
    let mut idx = 0usize;
    for child_layout in layout.children.iter() {
        if idx >= node.children.len() {
            break;
        }
        paint_from_layout(ctx, pixmap, &node.children[idx], child_layout, ox, oy);
        idx += 1;
    }
}

fn rounded_rect_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    let r = r.min(w * 0.5).min(h * 0.5);
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
    pb.finish()
}

fn paint_text(
    ctx: &RenderContext,
    pixmap: &mut Pixmap,
    text: &str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    center: bool,
) {
    let Some(font) = &ctx.font else {
        return;
    };
    let font_size = 16.0f32;
    let mut cursor_x = x + 8.0;
    let baseline = y + (h * 0.5);
    // naive centering
    if center {
        let est_w = (text.chars().count() as f32) * (font_size * 0.6);
        cursor_x = x + (w - est_w).max(0.0) * 0.5;
    }

    for ch in text.chars() {
        let (metrics, bitmap) = font.rasterize(ch, font_size);
        let gx = cursor_x + metrics.xmin as f32;
        let gy = baseline + metrics.ymin as f32;
        for yy in 0..metrics.height {
            for xx in 0..metrics.width {
                let a = bitmap[yy * metrics.width + xx];
                if a == 0 {
                    continue;
                }
                let px = gx as i32 + xx as i32;
                let py = gy as i32 + yy as i32;
                if px < 0 || py < 0 {
                    continue;
                }
                let px = px as u32;
                let py = py as u32;
                if px >= pixmap.width() || py >= pixmap.height() {
                    continue;
                }
                let off = ((py * pixmap.width() + px) * 4) as usize;
                let data = pixmap.data_mut();
                // paint solid dark text with alpha
                data[off] = 0x11;
                data[off + 1] = 0x11;
                data[off + 2] = 0x11;
                data[off + 3] = a;
            }
        }
        cursor_x += metrics.advance_width;
    }
}

fn paint_image(ctx: &RenderContext, pixmap: &mut Pixmap, src: &str, x: f32, y: f32, w: f32, h: f32) {
    let img = {
        let mut cache = ctx.image_cache.borrow_mut();
        if let Some(i) = cache.get(src) {
            i.clone()
        } else {
            let data = match std::fs::read(src) {
                Ok(d) => d,
                Err(_) => return,
            };
            let decoded = match image::load_from_memory(&data) {
                Ok(d) => d.to_rgba8(),
                Err(_) => return,
            };
            cache.insert(src.to_string(), decoded.clone());
            decoded
        }
    };
    let (iw, ih) = img.dimensions();
    if iw == 0 || ih == 0 {
        return;
    }
    // very simple nearest scaling to fit
    let tw = w.max(1.0) as u32;
    let th = h.max(1.0) as u32;
    for yy in 0..th {
        for xx in 0..tw {
            let sx = (xx as f32 / tw as f32 * iw as f32) as u32;
            let sy = (yy as f32 / th as f32 * ih as f32) as u32;
            let p = img.get_pixel(sx.min(iw - 1), sy.min(ih - 1)).0;
            let dx = x as i32 + xx as i32;
            let dy = y as i32 + yy as i32;
            if dx < 0 || dy < 0 {
                continue;
            }
            let dx = dx as u32;
            let dy = dy as u32;
            if dx >= pixmap.width() || dy >= pixmap.height() {
                continue;
            }
            let off = ((dy * pixmap.width() + dx) * 4) as usize;
            let data = pixmap.data_mut();
            data[off] = p[0];
            data[off + 1] = p[1];
            data[off + 2] = p[2];
            data[off + 3] = p[3];
        }
    }
}
