use std::env;
use std::error::Error;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use qrcode::{Color, QrCode};

const MODULE_SCALE: usize = 8;
const QUIET_ZONE_MODULES: usize = 4;

fn main() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<String> = env::args().skip(1).collect();
    if arguments.len() != 4 {
        return Err(
            "usage: mochios-startup-qr <terms-url> <terms-png> <privacy-url> <privacy-png>".into(),
        );
    }

    write_qr_png(&arguments[0], Path::new(&arguments[1]))?;
    write_qr_png(&arguments[2], Path::new(&arguments[3]))?;
    Ok(())
}

fn write_qr_png(value: &str, output: &Path) -> Result<(), Box<dyn Error>> {
    let code = QrCode::new(value.as_bytes())?;
    let (dimension, pixels) = render_grayscale(&code)?;
    let dimension = u32::try_from(dimension)?;
    let file = File::create(output)?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), dimension, dimension);
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&pixels)?;
    Ok(())
}

fn render_grayscale(code: &QrCode) -> Result<(usize, Vec<u8>), Box<dyn Error>> {
    let modules = code.width();
    let padded_modules = modules
        .checked_add(QUIET_ZONE_MODULES * 2)
        .ok_or("QR dimension overflow")?;
    let dimension = padded_modules
        .checked_mul(MODULE_SCALE)
        .ok_or("QR dimension overflow")?;
    let pixel_count = dimension
        .checked_mul(dimension)
        .ok_or("QR image size overflow")?;
    let mut pixels = vec![u8::MAX; pixel_count];

    for module_y in 0..modules {
        for module_x in 0..modules {
            if code[(module_x, module_y)] != Color::Dark {
                continue;
            }
            let origin_x = (module_x + QUIET_ZONE_MODULES) * MODULE_SCALE;
            let origin_y = (module_y + QUIET_ZONE_MODULES) * MODULE_SCALE;
            for pixel_y in origin_y..origin_y + MODULE_SCALE {
                let row_start = pixel_y * dimension;
                pixels[row_start + origin_x..row_start + origin_x + MODULE_SCALE].fill(0);
            }
        }
    }

    Ok((dimension, pixels))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rendering_has_a_white_quiet_zone_and_dark_modules() {
        let code = QrCode::new(b"https://policy.mochios.org/terms/")
            .unwrap_or_else(|error| panic!("failed to encode test QR: {error}"));
        let (dimension, pixels) = render_grayscale(&code)
            .unwrap_or_else(|error| panic!("failed to render test QR: {error}"));

        assert!(dimension > code.width());
        assert!(
            pixels[..dimension * QUIET_ZONE_MODULES]
                .iter()
                .all(|pixel| *pixel == u8::MAX)
        );
        assert!(pixels.iter().any(|pixel| *pixel == 0));
    }
}
