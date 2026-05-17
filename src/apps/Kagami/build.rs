use resvg::{tiny_skia, usvg};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn find_project_root(manifest_dir: &Path) -> PathBuf {
    if let Ok(workspace_dir) = env::var("CARGO_WORKSPACE_DIR") {
        return PathBuf::from(workspace_dir);
    }

    for ancestor in manifest_dir.ancestors() {
        if ancestor.join("ramfs").join("lib").exists() {
            return ancestor.to_path_buf();
        }
    }

    for ancestor in manifest_dir.ancestors() {
        if ancestor.join("Cargo.toml").exists() {
            return ancestor.to_path_buf();
        }
    }

    manifest_dir.to_path_buf()
}

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let manifest_path = Path::new(&manifest_dir);
    let project_root = find_project_root(manifest_path);
    let libs_dir = project_root.join("ramfs").join("lib");

    println!("cargo:rustc-link-search=native={}", libs_dir.display());
    println!("cargo:rustc-link-arg={}/crt0.o", libs_dir.display());
    println!("cargo:rustc-link-arg=-static");
    println!("cargo:rustc-link-arg=-no-pie");
    println!("cargo:rustc-link-arg=-T{}/linker.ld", manifest_dir);
    println!("cargo:rustc-link-arg=--allow-multiple-definition");
    println!("cargo:rerun-if-changed={}", manifest_path.join("linker.ld").display());

    println!("cargo:rustc-link-lib=static=c");
    println!("cargo:rustc-link-lib=static=g");
    println!("cargo:rustc-link-lib=static=m");
    println!("cargo:rustc-link-lib=static=nosys");

    let libgcc_s = libs_dir.join("libgcc_s.a");
    let libg = libs_dir.join("libg.a");
    if !libgcc_s.exists() && libg.exists() {
        let tmp = libs_dir.join("libgcc_s.a.tmp");
        if let Err(err) = std::fs::copy(&libg, &tmp) {
            panic!(
                "failed to copy {} to {} for static gcc_s linking: {}",
                libg.display(),
                tmp.display(),
                err
            );
        }
        if let Err(err) = std::fs::rename(&tmp, &libgcc_s) {
            let _ = std::fs::remove_file(&tmp);
            if !libgcc_s.exists() {
                panic!(
                    "failed to rename {} to {} for static gcc_s linking: {}",
                    tmp.display(),
                    libgcc_s.display(),
                    err
                );
            }
        }
    }
    println!("cargo:rustc-link-lib=static=gcc_s");
    println!("cargo:rerun-if-changed={}", libs_dir.join("libc.a").display());

    let svg_path = manifest_path.join("resources").join("mouse.svg");
    println!("cargo:rerun-if-changed={}", svg_path.display());
    generate_cursor_sprite(&svg_path);
}

fn generate_cursor_sprite(svg_path: &Path) {
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));
    let out_path = out_dir.join("cursor_pixels.rs");

    let (width, height, pixels) = load_svg_pixels(svg_path).unwrap_or_else(|| fallback_cursor());

    let mut src = String::new();
    src.push_str("pub const CURSOR_WIDTH: usize = ");
    src.push_str(&width.to_string());
    src.push_str(";\n");
    src.push_str("pub const CURSOR_HEIGHT: usize = ");
    src.push_str(&height.to_string());
    src.push_str(";\n");
    src.push_str("pub const CURSOR_PIXELS: [u32; ");
    src.push_str(&(pixels.len()).to_string());
    src.push_str("] = [\n");
    for (i, p) in pixels.iter().enumerate() {
        src.push_str(&format!("0x{p:08X},"));
        if i % 12 == 11 {
            src.push('\n');
        }
    }
    src.push_str("\n];\n");

    fs::write(&out_path, src)
        .unwrap_or_else(|e| panic!("failed to write {}: {}", out_path.display(), e));
}

fn load_svg_pixels(svg_path: &Path) -> Option<(usize, usize, Vec<u32>)> {
    let svg = fs::read_to_string(svg_path).ok()?;
    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_str(&svg, &opt).ok()?;
    let size = tree.size().to_int_size();
    let mut pixmap = tiny_skia::Pixmap::new(size.width(), size.height())?;
    let mut pixmap_mut = pixmap.as_mut();
    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap_mut);

    let src_w = size.width() as usize;
    let src_h = size.height() as usize;
    let dst_w = core::cmp::max(1, src_w / 3);
    let dst_h = core::cmp::max(1, src_h / 3);

    let mut pixels = vec![0u32; dst_w * dst_h];
    for y in 0..dst_h {
        for x in 0..dst_w {
            let x0 = (x * src_w) / dst_w;
            let x1 = ((x + 1) * src_w).div_ceil(dst_w).min(src_w);
            let y0 = (y * src_h) / dst_h;
            let y1 = ((y + 1) * src_h).div_ceil(dst_h).min(src_h);

            let mut sum_a = 0u64;
            let mut sum_r = 0u64;
            let mut sum_g = 0u64;
            let mut sum_b = 0u64;
            let mut count = 0u64;

            for sy in y0..y1 {
                for sx in x0..x1 {
                    let src_idx = (sy * src_w + sx) * 4;
                    let r = pixmap.data()[src_idx] as u64;
                    let g = pixmap.data()[src_idx + 1] as u64;
                    let b = pixmap.data()[src_idx + 2] as u64;
                    let a = pixmap.data()[src_idx + 3] as u64;
                    sum_a += a;
                    sum_r += r * a;
                    sum_g += g * a;
                    sum_b += b * a;
                    count += 1;
                }
            }

            if count == 0 || sum_a == 0 {
                pixels[y * dst_w + x] = 0;
                continue;
            }
            let a = (sum_a / count) as u32;
            let r = (sum_r / sum_a) as u32;
            let g = (sum_g / sum_a) as u32;
            let b = (sum_b / sum_a) as u32;
            pixels[y * dst_w + x] = (a << 24) | (r << 16) | (g << 8) | b;
        }
    }

    Some((dst_w, dst_h, pixels))
}

fn fallback_cursor() -> (usize, usize, Vec<u32>) {
    let width = 12usize;
    let height = 20usize;
    let mut pixels = vec![0u32; width * height];
    for y in 0..height {
        for x in 0..=y.min(width - 1) {
            let idx = y * width + x;
            let edge = x == 0 || x == y.min(width - 1) || y == height - 1;
            pixels[idx] = if edge { 0xFFFF_FFFF } else { 0xFF3A_3A3A };
        }
    }
    (width, height, pixels)
}
