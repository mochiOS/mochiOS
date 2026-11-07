use std::path::Path;

pub fn generate(source: &Path) {
    use std::fs::{self, File};
    use std::path::PathBuf;

    println!("cargo:rerun-if-changed={}", source.display());

    let mut decoder = png::Decoder::new(File::open(&source).expect("failed to open boot logo"));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder
        .read_info()
        .expect("failed to read boot logo metadata");
    let mut decoded = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut decoded)
        .expect("failed to decode boot logo");
    let source_pixels = &decoded[..info.buffer_size()];
    let pixel_count = usize::try_from(info.width)
        .expect("boot logo width is too large")
        .checked_mul(usize::try_from(info.height).expect("boot logo height is too large"))
        .expect("boot logo dimensions overflow");
    let mut rgba = Vec::with_capacity(pixel_count * 4);
    match info.color_type {
        png::ColorType::Rgba => rgba.extend_from_slice(source_pixels),
        png::ColorType::Rgb => source_pixels
            .chunks_exact(3)
            .for_each(|pixel| rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255])),
        png::ColorType::GrayscaleAlpha => source_pixels
            .chunks_exact(2)
            .for_each(|pixel| rgba.extend_from_slice(&[pixel[0], pixel[0], pixel[0], pixel[1]])),
        png::ColorType::Grayscale => source_pixels
            .iter()
            .for_each(|value| rgba.extend_from_slice(&[*value, *value, *value, 255])),
        png::ColorType::Indexed => panic!("boot logo palette was not expanded"),
    }
    assert_eq!(rgba.len(), pixel_count * 4, "invalid boot logo data");

    let output_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR is unavailable"));
    fs::write(output_dir.join("boot-logo.rgba"), rgba).expect("failed to write boot logo pixels");
    fs::write(
        output_dir.join("boot_logo.rs"),
        format!(
            "const BOOT_LOGO_WIDTH: u32 = {};\nconst BOOT_LOGO_HEIGHT: u32 = {};\nstatic BOOT_LOGO_RGBA: &[u8] = include_bytes!(concat!(env!(\"OUT_DIR\"), \"/boot-logo.rgba\"));\n",
            info.width, info.height
        ),
    )
    .expect("failed to write boot logo metadata");
}
