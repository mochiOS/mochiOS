mod builders;

use std::env;
use std::fs;
use std::path::PathBuf;

use builders::{
    build_apps, build_newlib, build_service, build_user_libs, copy_newlib_libs, create_ext2_image,
    create_initfs_image, parse_service_index,
};

#[allow(unused)]
fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    // ramfsとfsディレクトリを作成
    let ramfs_dir = manifest_dir.join("ramfs");
    let fs_dir = manifest_dir.join("fs");

    for dir in &[&ramfs_dir, &fs_dir] {
        if !dir.is_dir() {
            fs::create_dir_all(dir)
                .unwrap_or_else(|_| panic!("Failed to create directory: {}", dir.display()));
        }
    }

    // newlibのビルド
    let newlib_src_dir = manifest_dir.join("src/lib");
    if !newlib_src_dir.exists() {
        panic!("Newlib source not found at {}", newlib_src_dir.display());
    }
    build_newlib(&newlib_src_dir);

    // newlibのインストールディレクトリを取得
    let target = env::var("TARGET").unwrap_or("x86_64-unknown-uefi".to_string());
    let profile = env::var("PROFILE").unwrap_or("debug".to_string());
    let target_dir = PathBuf::from(env::var("CARGO_TARGET_DIR").unwrap_or("target".to_string()));

    let abs_target_dir = if target_dir.is_absolute() {
        target_dir
    } else {
        manifest_dir.join(target_dir)
    };

    let newlib_install_dir = abs_target_dir
        .join(&target)
        .join(&profile)
        .join("newlib_install");

    let libc_dir = newlib_install_dir.join("x86_64-elf").join("lib");

    // ユーザーライブラリをビルド
    let user_src_dir = manifest_dir.join("src/user");
    build_user_libs(&user_src_dir, &libc_dir);

    // newlibライブラリをramfsとfsにコピー
    copy_newlib_libs(&libc_dir, &ramfs_dir)
        .expect("cargo:warning=Failed to copy newlib libs to ramfs");
    copy_newlib_libs(&libc_dir, &fs_dir).expect("cargo:warning=Failed to copy newlib libs to fs");

    // libgcc_sをramfsにコピー
    if let Ok(out) = std::process::Command::new("gcc")
        .arg("-print-file-name=libgcc_s.so.1")
        .output()
    {
        if out.status.success() {
            let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            use std::path::Path;
            if path != "libgcc_s.so.1" && Path::new(&path).exists() {
                let dest = ramfs_dir.join("libgcc_s.so.1");
                let _ = std::fs::copy(&path, &dest);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::symlink;
                    let link = ramfs_dir.join("libgcc_s.so");
                    if !link.exists() {
                        let _ = symlink("libgcc_s.so.1", &link);
                    }
                }
                println!("Copied libgcc_s to ramfs: {}", path);
            } else {
                let candidates = [
                    "/usr/lib/x86_64-linux-gnu/libgcc_s.so.1",
                    "/lib/x86_64-linux-gnu/libgcc_s.so.1",
                    "/usr/lib64/libgcc_s.so.1",
                    "/lib64/libgcc_s.so.1",
                ];
                for c in &candidates {
                    if Path::new(c).exists() {
                        let _ = std::fs::copy(c, ramfs_dir.join("libgcc_s.so.1"));
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::symlink;
                            let link = ramfs_dir.join("libgcc_s.so");
                            if !link.exists() {
                                let _ = symlink("libgcc_s.so.1", &link);
                            }
                        }
                        println!("Copied libgcc_s to ramfs from {}", c);
                        break;
                    }
                }
            }
        } else {
            println!("gcc returned non-zero when locating libgcc_s");
        }
    } else {
        println!("Failed to run gcc to locate libgcc_s");
    }

    // services/index.toml を解析
    let index_path = manifest_dir.join("src/services/index.toml");
    println!("cargo:rerun-if-changed={}", index_path.display());

    let services = parse_service_index(&index_path).expect("Failed to parse index.toml");

    // サービスをビルド
    let services_base_dir = manifest_dir.join("src/services");

    for service in &services {
        let output_dir = if service.fs_type == "initfs" {
            &ramfs_dir
        } else {
            &fs_dir
        };

        if let Err(e) = build_service(service, &services_base_dir, output_dir) {
            println!(
                "cargo:warning=Failed to build service {}: {}",
                service.name, e
            );
        }
    }

    // アプリケーションをビルド
    let apps_dir = manifest_dir.join("src/apps");
    if apps_dir.is_dir() {
        println!("Building test applications");
        build_apps(&apps_dir, &fs_dir, "elf");
    }

    // initfs イメージを生成
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let initfs_image_path = out_dir.join("initfs.ext2");

    create_initfs_image(&ramfs_dir, &initfs_image_path).expect("Failed to create initfs image");

    // ext2 イメージを生成
    let ext2_image_path = out_dir.join("rootfs.ext2");
    create_ext2_image(&fs_dir, &ext2_image_path).expect("Failed to create ext2 image");

    // make_image.sh を実行（UEFIイメージ作成）
    let mkimage_script = manifest_dir.join("scripts/make_image.sh");
    if mkimage_script.exists() {
        let _ = std::process::Command::new(mkimage_script).status();
    }

    println!("Build completed successfully!");
    println!("  ramfs/ -> {}", initfs_image_path.display());
    println!("  fs/    -> {}", ext2_image_path.display());
}

/*
fn find_service_bin(manifest_dir: &Path, name: &str) -> Option<PathBuf> {
    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let profile_dir = if profile == "release" { "release" } else { "debug" };

    if let Ok(target_dir) = env::var("CARGO_TARGET_DIR") {
        let target_dir = PathBuf::from(target_dir);
        let candidate = target_dir
            .join("x86_64-unknown-none")
            .join(profile_dir)
            .join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    let candidates = [
        manifest_dir
            .join("target/x86_64-unknown-none")
            .join(profile_dir)
            .join(name),
        manifest_dir
            .join("src/services")
            .join(name)
            .join("target/x86_64-unknown-none")
            .join(profile_dir)
            .join(name),
    ];

    candidates.into_iter().find(|p| p.is_file())
}

fn build_service(manifest_dir: &Path, name: &str) -> Option<PathBuf> {
    if env::var("SWIFTCORE_SKIP_SHELL_BUILD").ok().as_deref() == Some("1") {
        return None;
    }

    let svc_dir = manifest_dir.join("src/services").join(name);
    if !svc_dir.is_dir() {
        return None;
    }

    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let mut cmd = Command::new("cargo");
    cmd.current_dir(&svc_dir)
        .env("SWIFTCORE_SKIP_SHELL_BUILD", "1")
        .args(["build", "--target", "x86_64-unknown-none"]);

    if profile == "release" {
        cmd.arg("--release");
    }

    let status = cmd
        .status();

    match status {
        Ok(s) if s.success() => find_service_bin(manifest_dir, name),
        Ok(_) => None,
        Err(_) => None,
    }
}

fn copy_service(manifest_dir: &Path, name: &str, stage_dir: &Path) {
    let env_key = match name {
        "shell" => "SWIFTCORE_SHELL_BIN",
        "keyboard" => "SWIFTCORE_KEYBOARD_BIN",
        _ => "SWIFTCORE_SERVICE_BIN",
    };

    let env_bin = env::var(env_key).ok().and_then(|p| {
        let path = PathBuf::from(p);
        if path.is_file() { Some(path) } else { None }
    });

    let bin = env_bin
        .or_else(|| find_service_bin(manifest_dir, name))
        .or_else(|| build_service(manifest_dir, name));

    if let Some(bin) = bin {
        let dest = stage_dir.join(format!("{}.service", name));
        let _ = fs::copy(&bin, &dest);
    } else {
        println!(
            "cargo:warning=initfs: {} binary not found; set {} or build service crate",
            name, env_key
        );
    }
}
*/
