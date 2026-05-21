use std::sync::OnceLock;

use super::display_list::{DisplayCommand, DisplayList};
use super::framebuffer::Framebuffer;
use super::image;

const FONT_HEIGHT: usize = 12;
const ASCII_START: usize = 32;
const ASCII_END: usize = 127;
const GLYPH_COUNT: usize = ASCII_END - ASCII_START;

struct Font {
    glyphs: [[u8; FONT_HEIGHT]; GLYPH_COUNT],
}

impl Font {
    fn glyph(&self, ch: u8) -> &[u8; FONT_HEIGHT] {
        let idx = if (ASCII_START as u8..ASCII_END as u8).contains(&ch) {
            (ch as usize) - ASCII_START
        } else {
            (b'?' as usize) - ASCII_START
        };
        &self.glyphs[idx]
    }
}

pub fn rasterize(display_list: &DisplayList, width: u32, height: u32) -> Framebuffer {
    let mut fb = Framebuffer::new(width, height);
    fb.clear(0x00000000);

    for item in &display_list.items {
        match item {
            DisplayCommand::FillRect {
                rect,
                color,
                radius,
                opacity,
            } => {
                fb.fill_rounded_rect(rect.x, rect.y, rect.width, rect.height, *radius, *color, *opacity);
            }
            DisplayCommand::FillGradient {
                rect,
                from_color,
                to_color,
                radius,
                opacity,
                vertical,
            } => {
                fb.fill_linear_gradient_rounded_rect(
                    rect.x,
                    rect.y,
                    rect.width,
                    rect.height,
                    *radius,
                    *from_color,
                    *to_color,
                    *opacity,
                    *vertical,
                );
            }
            DisplayCommand::DrawShadow {
                rect,
                color,
                radius,
                opacity,
                offset_x,
                offset_y,
                blur,
            } => {
                fb.draw_box_shadow(
                    rect.x,
                    rect.y,
                    rect.width,
                    rect.height,
                    *radius,
                    *color,
                    *opacity,
                    *offset_x,
                    *offset_y,
                    *blur,
                );
            }
            DisplayCommand::DrawText {
                x,
                y,
                color,
                opacity,
                size,
                text,
            } => {
                rasterize_text(&mut fb, *x, *y, *color, *opacity, *size, text);
            }
            DisplayCommand::DrawImage {
                rect,
                opacity,
                src,
                radius,
                fit_cover,
            } => {
                if let Some((pixels, w, h)) = image::load_image_from_path(src) {
                    if *fit_cover || *radius > 0 {
                        fb.blit_image_pixels_cover_rounded(
                            &pixels,
                            w,
                            h,
                            rect.x,
                            rect.y,
                            rect.width,
                            rect.height,
                            *radius,
                            *opacity,
                        );
                    } else {
                        fb.blit_image_pixels_fit(
                            &pixels,
                            w,
                            h,
                            rect.x,
                            rect.y,
                            rect.width,
                            rect.height,
                            *opacity,
                            0,
                        );
                    }
                } else {
                    // Debug fallback: show missing decode/load as magenta.
                    fb.fill_rect(rect.x, rect.y, rect.width, rect.height, 0xFFFF00FF, *opacity);
                }
            }
        }
    }

    fb
}

fn rasterize_text(
    fb: &mut Framebuffer,
    x: i32,
    y: i32,
    color: u32,
    opacity: f32,
    size: f32,
    text: &str,
) {
    let Some(font) = font_cache() else {
        fallback_text(fb, x, y, color, opacity, text);
        return;
    };

    let mut pen_x = x as f32;
    let scale = (size / FONT_HEIGHT as f32).clamp(1.0, 3.0);
    let glyph_w = (8.0 * scale).round().max(6.0) as i32;
    let glyph_h = (FONT_HEIGHT as f32 * scale).round().max(8.0) as i32;
    let baseline_y = y as f32;
    let line_height = (glyph_h as f32 * 1.35).max(glyph_h as f32 + 2.0);

    for ch in text.chars() {
        if ch == '\n' {
            pen_x = x as f32;
            let _ = line_height;
            continue;
        }

        let glyph = font.glyph(ch as u8);
        let advance = glyph_w as f32;
        if glyph.iter().all(|&b| b == 0) {
            pen_x += advance.max(scale * 4.0);
            continue;
        }

        for row in 0..FONT_HEIGHT {
            let row_bits = glyph[row];
            for col in 0..8 {
                if (row_bits >> (7 - col)) & 1 == 0 {
                    continue;
                }
                let base_x = pen_x.round() as i32;
                let base_y = baseline_y.round() as i32;
                let px = base_x + (col as i32 * glyph_w / 8);
                let py = base_y + (row as i32 * glyph_h / FONT_HEIGHT as i32);
                fb.fill_rect(px, py, (glyph_w / 8).max(1), (glyph_h / FONT_HEIGHT as i32).max(1), color, opacity);
            }
        }

        pen_x += advance.max(scale * 4.0);
    }
}

fn fallback_text(fb: &mut Framebuffer, x: i32, y: i32, color: u32, opacity: f32, text: &str) {
    let mut pen_x = x;
    for _ in text.chars() {
        fb.fill_rect(pen_x, y, 6, 10, color, opacity);
        pen_x += 8;
    }
}

fn font_cache() -> Option<&'static Font> {
    static FONT: OnceLock<Option<Font>> = OnceLock::new();
    FONT.get_or_init(|| {
        let mut glyphs = [[0u8; FONT_HEIGHT]; GLYPH_COUNT];
        parse_bdf(include_bytes!("../../../../resources/system/fonts/ter-u12b.bdf"), &mut glyphs);
        Some(Font { glyphs })
    })
    .as_ref()
}

fn parse_bdf(data: &[u8], glyphs: &mut [[u8; FONT_HEIGHT]; GLYPH_COUNT]) {
    let text = core::str::from_utf8(data).unwrap_or("");
    let mut lines = text.lines();
    let mut encoding: Option<usize> = None;
    let mut in_bitmap = false;
    let mut row = 0usize;

    loop {
        let line = match lines.next() {
            Some(l) => l.trim(),
            None => break,
        };
        if line.starts_with("ENCODING ") {
            encoding = line[9..].trim().parse::<usize>().ok();
            in_bitmap = false;
            row = 0;
        } else if line == "BITMAP" {
            in_bitmap = true;
            row = 0;
        } else if line == "ENDCHAR" {
            in_bitmap = false;
            encoding = None;
            row = 0;
        } else if in_bitmap {
            if let Some(enc) = encoding {
                if (ASCII_START..ASCII_END).contains(&enc) && row < FONT_HEIGHT {
                    if let Ok(byte) = u8::from_str_radix(line, 16) {
                        glyphs[enc - ASCII_START][row] = byte;
                    }
                    row += 1;
                }
            }
        }
    }
}
