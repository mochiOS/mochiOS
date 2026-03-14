use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::utils::{emit_rerun_if_changed, find_binary_in_dir, find_target_spec};

/// ドライバクレート (`src/drivers/*`) をビルドして `fs/Binaries/drivers/*.elf` に配置する。
///
/// 戻り値は `driver.service` が起動するドライバパス（例: `Binaries/drivers/usb3.0.elf`）。
pub fn build_drivers(drivers_dir: &Path, output_dir: &Path) -> Vec<String> {
    println!("cargo:rerun-if-changed={}", drivers_dir.display());

    let mut autostart_entries = Vec::new();

    let entries = match fs::read_dir(drivers_dir) {
        Ok(entries) => entries,
        Err(_) => return autostart_entries,
    };

    if let Err(e) = fs::create_dir_all(output_dir) {
        println!(
            "cargo:warning=Failed to create drivers output dir {}: {}",
            output_dir.display(),
            e
        );
        return autostart_entries;
    }

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let cargo_toml = path.join("Cargo.toml");
        if !cargo_toml.exists() {
            continue;
        }

        let driver_dir_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let driver_output_name = normalize_driver_name(&driver_dir_name);

        println!(
            "Building driver: {} -> {}.elf",
            driver_dir_name, driver_output_name
        );

        println!("cargo:rerun-if-changed={}", cargo_toml.display());
        let src_dir = path.join("src");
        if src_dir.is_dir() {
            emit_rerun_if_changed(&src_dir);
        }

        let target_spec = find_target_spec(&path);
        let uses_json_target = target_spec
            .as_deref()
            .map(|t| t.ends_with(".json"))
            .unwrap_or(false);

        let cargo_config = path.join(".cargo/config.toml");
        let cargo_config_text = fs::read_to_string(&cargo_config).ok();
        let has_config_target = cargo_config_text
            .as_deref()
            .map(|s| s.contains("[build]") && s.contains("target"))
            .unwrap_or(false);
        let config_uses_json_target = cargo_config_text
            .as_deref()
            .map(|s| s.contains(".json"))
            .unwrap_or(false);
        let uses_json_target = uses_json_target || config_uses_json_target;

        let mut cmd = Command::new("cargo");
        cmd.args(["build", "--release"]);
        if uses_json_target {
            cmd.args(["-Z", "json-target-spec"]);
        }

        for key in &[
            "RUSTFLAGS",
            "CARGO_ENCODED_RUSTFLAGS",
            "CARGO_TARGET_DIR",
            "CARGO_BUILD_TARGET",
            "CARGO_MAKEFLAGS",
            "__CARGO_TEST_CHANNEL_OVERRIDE_DO_NOT_USE_THIS",
            "CARGO_BUILD_RUSTC",
            "RUSTC",
            "RUSTC_WRAPPER",
            "RUSTC_WORKSPACE_WRAPPER",
        ] {
            cmd.env_remove(key);
        }

        if has_config_target {
            println!("  Using target from .cargo/config.toml");
        } else if let Some(target) = &target_spec {
            cmd.arg("--target").arg(target);
            println!("  Using target: {}", target);
        } else {
            let default_target = "x86_64-unknown-none";
            cmd.arg("--target").arg(default_target);
            println!("  Using default target: {}", default_target);
        }

        let output = cmd.current_dir(&path).output();

        match output {
            Ok(output) => {
                if !output.status.success() {
                    println!("cargo:warning=Failed to build driver {}", driver_dir_name);
                    if !output.stderr.is_empty() {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        for line in stderr.lines().take(20) {
                            println!("cargo:warning=  {}", line);
                        }
                    }
                    continue;
                }

                let target_dir = path.join("target");
                let target_name = if has_config_target {
                    Some("x86_64-mochios".to_string())
                } else if let Some(p) = &target_spec {
                    Path::new(p)
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                } else {
                    Some("x86_64-unknown-none".to_string())
                };

                if let Some(elf_path) = find_built_binary(&target_dir, target_name.as_deref()) {
                    let dest_name = format!("{}.elf", driver_output_name);
                    let dest = output_dir.join(&dest_name);
                    if let Err(e) = fs::copy(&elf_path, &dest) {
                        println!(
                            "cargo:warning=Failed to copy {} to {}: {}",
                            elf_path.display(),
                            dest.display(),
                            e
                        );
                    } else {
                        println!(
                            "Copied {} to {} (from {})",
                            dest_name,
                            output_dir.display(),
                            elf_path.display()
                        );
                        autostart_entries.push(format!("Binaries/drivers/{}", dest_name));
                    }
                } else {
                    println!(
                        "cargo:warning=Built driver binary not found for {}",
                        driver_dir_name
                    );
                }
            }
            Err(e) => {
                println!(
                    "cargo:warning=Failed to execute cargo for driver {}: {}",
                    driver_dir_name, e
                );
            }
        }
    }

    autostart_entries.sort();
    autostart_entries
}

fn normalize_driver_name(driver_dir_name: &str) -> String {
    // 例: usb3_0 -> usb3.0
    driver_dir_name.replace('_', ".")
}

fn find_built_binary(target_dir: &Path, target_name: Option<&str>) -> Option<PathBuf> {
    if let Some(target) = target_name {
        let custom_target = target_dir.join(format!("{}/release", target));
        if custom_target.is_dir() {
            if let Some(binary) = find_binary_in_dir(&custom_target) {
                return Some(binary);
            }
        }
    }

    let custom_target = target_dir.join("x86_64-mochios/release");
    if custom_target.is_dir() {
        if let Some(binary) = find_binary_in_dir(&custom_target) {
            return Some(binary);
        }
    }

    let release_dir = target_dir.join("release");
    if release_dir.is_dir() {
        if let Some(binary) = find_binary_in_dir(&release_dir) {
            return Some(binary);
        }
    }

    None
}
