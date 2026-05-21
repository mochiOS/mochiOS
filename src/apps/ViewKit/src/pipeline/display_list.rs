use super::layout::{LayoutNode, LayoutNodeKind, LayoutTree, Rect};

#[derive(Debug, Clone)]
pub struct DisplayList {
    pub items: Vec<DisplayCommand>,
}

#[derive(Debug, Clone)]
pub enum DisplayCommand {
    FillRect {
        rect: Rect,
        color: u32,
        radius: i32,
        opacity: f32,
    },
    FillGradient {
        rect: Rect,
        from_color: u32,
        to_color: u32,
        radius: i32,
        opacity: f32,
        vertical: bool,
    },
    DrawShadow {
        rect: Rect,
        color: u32,
        radius: i32,
        opacity: f32,
        offset_x: i32,
        offset_y: i32,
        blur: i32,
    },
    DrawText {
        x: i32,
        y: i32,
        color: u32,
        opacity: f32,
        size: f32,
        text: String,
    },
    DrawImage {
        rect: Rect,
        opacity: f32,
        src: String,
        radius: i32,
        fit_cover: bool,
    },
}

pub fn build(layout: &LayoutTree) -> DisplayList {
    let mut items = Vec::new();
    build_for_node(&layout.root, &mut items, 0xFFFFFFFF, 1.0);
    DisplayList { items }
}

fn build_for_node(
    node: &LayoutNode,
    out: &mut Vec<DisplayCommand>,
    inherited_text_color: u32,
    inherited_opacity: f32,
) {
    let self_opacity = parse_opacity(node.styles.get("opacity"));
    let effective_opacity = (inherited_opacity * self_opacity).clamp(0.0, 1.0);

    match &node.kind {
        LayoutNodeKind::Element { tag_name, attributes } => {
            if let Some(shadow) = box_shadow_from_styles(&node.styles) {
                out.push(DisplayCommand::DrawShadow {
                    rect: node.rect,
                    color: shadow.color,
                    radius: parse_border_radius_px(node.styles.get("border-radius")),
                    opacity: effective_opacity * shadow.opacity,
                    offset_x: shadow.offset_x,
                    offset_y: shadow.offset_y,
                    blur: shadow.blur,
                });
            }

            if let Some(background) = background_paint_from_styles(&node.styles) {
                let radius = parse_border_radius_px(node.styles.get("border-radius"));
                match background {
                    BackgroundPaint::Solid(color) => out.push(DisplayCommand::FillRect {
                        rect: node.rect,
                        color,
                        radius,
                        opacity: effective_opacity,
                    }),
                    BackgroundPaint::Gradient {
                        from_color,
                        to_color,
                        vertical,
                    } => out.push(DisplayCommand::FillGradient {
                        rect: node.rect,
                        from_color,
                        to_color,
                        radius,
                        opacity: effective_opacity,
                        vertical,
                    }),
                }
            }

            if tag_name == "img" {
                if let Some(src) = attributes.get("src") {
                    let radius = parse_border_radius_px(node.styles.get("border-radius"));
                    let fit_cover = attributes.get("data-vk-fit").map(|v| v == "cover").unwrap_or(false);
                    let clip_radius = attributes
                        .get("data-vk-clip-radius")
                        .and_then(|v| v.parse::<i32>().ok())
                        .unwrap_or(radius);
                    out.push(DisplayCommand::DrawImage {
                        rect: node.rect,
                        opacity: effective_opacity,
                        src: src.clone(),
                        radius: clip_radius,
                        fit_cover,
                    });
                }
            }
        }
        LayoutNodeKind::Text { content } => {
            let color = text_color_from_styles(&node.styles).unwrap_or(inherited_text_color);
            let size = parse_font_size(&node.styles).unwrap_or(14.0);
            out.push(DisplayCommand::DrawText {
                x: node.rect.x,
                y: node.rect.y,
                color,
                opacity: effective_opacity,
                size,
                text: content.clone(),
            });
        }
    }

    let next_text_color = text_color_from_styles(&node.styles).unwrap_or(inherited_text_color);
    for child in &node.children {
        build_for_node(child, out, next_text_color, effective_opacity);
    }
}

enum BackgroundPaint {
    Solid(u32),
    Gradient {
        from_color: u32,
        to_color: u32,
        vertical: bool,
    },
}

fn text_color_from_styles(styles: &std::collections::BTreeMap<String, String>) -> Option<u32> {
    styles.get("color").and_then(|v| parse_css_color(v))
}

struct BoxShadow {
    color: u32,
    opacity: f32,
    offset_x: i32,
    offset_y: i32,
    blur: i32,
}

fn background_paint_from_styles(
    styles: &std::collections::BTreeMap<String, String>,
) -> Option<BackgroundPaint> {
    if let Some(v) = styles.get("background-color").and_then(|v| parse_css_color(v)) {
        return Some(BackgroundPaint::Solid(v));
    }
    if let Some(v) = styles.get("background") {
        if let Some((from_color, to_color, vertical)) = parse_linear_gradient(v) {
            return Some(BackgroundPaint::Gradient {
                from_color,
                to_color,
                vertical,
            });
        }
        for token in v.split_whitespace() {
            if let Some(c) = parse_css_color(token) {
                return Some(BackgroundPaint::Solid(c));
            }
        }
    }
    None
}

fn box_shadow_from_styles(styles: &std::collections::BTreeMap<String, String>) -> Option<BoxShadow> {
    let raw = styles.get("box-shadow")?;
    let mut parts = raw.split_whitespace().collect::<Vec<_>>();
    if parts.len() < 4 {
        return None;
    }
    let offset_x = parse_length_px(parts[0])?;
    let offset_y = parse_length_px(parts[1])?;
    let blur = parse_length_px(parts[2])?.max(0);
    let color_raw = parts.split_off(3).join(" ");
    let color = parse_css_color(&color_raw)?;
    Some(BoxShadow {
        color,
        opacity: 1.0,
        offset_x,
        offset_y,
        blur,
    })
}

fn parse_linear_gradient(raw: &str) -> Option<(u32, u32, bool)> {
    let body = raw.trim();
    let lower = body.to_ascii_lowercase();
    let inner = lower.strip_prefix("linear-gradient(")?.strip_suffix(')')?;
    let parts: Vec<_> = inner.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
    match parts.as_slice() {
        [from, to] => Some((parse_css_color(from)?, parse_css_color(to)?, true)),
        [dir, from, to] if dir.starts_with("to ") => {
            let vertical = !dir.contains("left") && !dir.contains("right");
            Some((parse_css_color(from)?, parse_css_color(to)?, vertical))
        }
        _ => None,
    }
}

fn parse_css_color(raw: &str) -> Option<u32> {
    let s = raw.trim().to_ascii_lowercase();
    if s.starts_with("rgba(") {
        return parse_rgba_function(&s);
    }
    if s.starts_with("rgb(") {
        return parse_rgb_function(&s);
    }
    match s.as_str() {
        "white" => Some(0xFFFFFFFF),
        "black" => Some(0xFF000000),
        "transparent" => Some(0x00000000),
        "red" => Some(0xFFFF0000),
        "green" => Some(0xFF00FF00),
        "blue" => Some(0xFF0000FF),
        _ => parse_hex_color(&s),
    }
}

fn parse_hex_color(s: &str) -> Option<u32> {
    let hex = s.strip_prefix('#')?;
    match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
            Some(0xFF00_0000 | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32))
        }
        6 => {
            let rgb = u32::from_str_radix(hex, 16).ok()?;
            Some(0xFF00_0000 | rgb)
        }
        8 => {
            // CSS #RRGGBBAA -> ARGB
            let rrggbbaa = u32::from_str_radix(hex, 16).ok()?;
            let r = (rrggbbaa >> 24) & 0xff;
            let g = (rrggbbaa >> 16) & 0xff;
            let b = (rrggbbaa >> 8) & 0xff;
            let a = rrggbbaa & 0xff;
            Some((a << 24) | (r << 16) | (g << 8) | b)
        }
        _ => None,
    }
}

fn parse_rgb_function(s: &str) -> Option<u32> {
    let args = s.strip_prefix("rgb(")?.strip_suffix(')')?;
    let mut parts = args.split(',').map(str::trim);
    let r = parts.next()?.parse::<u8>().ok()?;
    let g = parts.next()?.parse::<u8>().ok()?;
    let b = parts.next()?.parse::<u8>().ok()?;
    Some(0xFF00_0000 | ((r as u32) << 16) | ((g as u32) << 8) | b as u32)
}

fn parse_rgba_function(s: &str) -> Option<u32> {
    let args = s.strip_prefix("rgba(")?.strip_suffix(')')?;
    let mut parts = args.split(',').map(str::trim);
    let r = parts.next()?.parse::<u8>().ok()?;
    let g = parts.next()?.parse::<u8>().ok()?;
    let b = parts.next()?.parse::<u8>().ok()?;
    let a = parse_alpha(parts.next()?)?;
    let a8 = (a * 255.0).round().clamp(0.0, 255.0) as u32;
    Some((a8 << 24) | ((r as u32) << 16) | ((g as u32) << 8) | b as u32)
}

fn parse_alpha(raw: &str) -> Option<f32> {
    let s = raw.trim();
    if let Some(p) = s.strip_suffix('%') {
        return p.trim().parse::<f32>().ok().map(|v| (v / 100.0).clamp(0.0, 1.0));
    }
    s.parse::<f32>().ok().map(|v| v.clamp(0.0, 1.0))
}

fn parse_opacity(value: Option<&String>) -> f32 {
    value
        .and_then(|s| parse_alpha(s))
        .unwrap_or(1.0)
}

fn parse_border_radius_px(value: Option<&String>) -> i32 {
    let Some(raw) = value else {
        return 0;
    };
    let token = raw.split_whitespace().next().unwrap_or("");
    let num = token.strip_suffix("px").unwrap_or(token).trim();
    num.parse::<f32>().ok().unwrap_or(0.0).max(0.0).round() as i32
}

fn parse_font_size(styles: &std::collections::BTreeMap<String, String>) -> Option<f32> {
    styles.get("font-size").and_then(|raw| parse_length_px_f32(raw))
}

fn parse_length_px(raw: &str) -> Option<i32> {
    parse_length_px_f32(raw).map(|v| v.round() as i32)
}

fn parse_length_px_f32(raw: &str) -> Option<f32> {
    let token = raw.split_whitespace().next().unwrap_or("").trim();
    let token = token.strip_suffix("px").unwrap_or(token);
    token.parse::<f32>().ok()
}
