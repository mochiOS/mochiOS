mod builders;

use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use builders::{
    build_apps, build_drivers, build_module, build_newlib, build_service, build_user_libs,
    build_utils, copy_newlib_libs, create_ext2_image, create_initfs_image, default_modules,
    parse_service_index, setup_fs_layout,
};

const BUSYBOX_URL: &str = "https://busybox.net/downloads/binaries/1.35.0-x86_64-linux-musl/busybox";
const BUSYBOX_SHA256: &str = "6e123e7f3202a8c1e9b1f94d8941580a25135382b99e8d3e34fb858bba311348";
const AUDIT_LOG_SIZE: u64 = 64 * 1024;

/// カーネル ELF をビルドして fs/system/kernel.elf にコピーする
fn build_kernel(manifest_dir: &PathBuf, fs_dir: &PathBuf, profile: &str) {
    let kernel_crate_dir = manifest_dir.join("src/core");
    let kernel_target_dir = manifest_dir.join("target/kernel");

    let mut clean = std::process::Command::new("cargo");
    clean.current_dir(&kernel_crate_dir);
    clean.arg("clean");
    let _ = clean.status();

    let mut cmd = std::process::Command::new("cargo");
    cmd.current_dir(&kernel_crate_dir);
    cmd.env("MOCHIOS_BUILDING_KERNEL", "1");
    cmd.env("CARGO_TARGET_DIR", &kernel_target_dir);
    cmd.args(["build", "-Z", "build-std=core,alloc"]);
    if profile == "release" {
        cmd.arg("--release");
    }
    let status = cmd.status();
    match status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            panic!(
                "kernel build failed with exit code {}",
                s.code().unwrap_or(-1)
            );
        }
        Err(e) => {
            panic!("failed to run kernel cargo build: {}", e);
        }
    }

    let kernel_bin = kernel_target_dir
        .join("x86_64-unknown-none")
        .join(profile)
        .join("kernel");
    let system_dir = fs_dir.join("system");
    fs::create_dir_all(&system_dir).expect("failed to create system directory");
    let dest = system_dir.join("kernel.elf");
    if !kernel_bin.exists() {
        panic!("kernel binary not found at {}", kernel_bin.display());
    }
    fs::copy(&kernel_bin, &dest)
        .unwrap_or_else(|e| panic!("failed to copy kernel ELF to {}: {}", dest.display(), e));
    println!("Kernel ELF copied to {}", dest.display());

    let meta_path = system_dir.join("kernel.meta");
    let meta = build_kernel_meta(&kernel_bin);
    fs::write(&meta_path, meta)
        .unwrap_or_else(|e| panic!("failed to write {}: {}", meta_path.display(), e));
    println!("Kernel metadata written to {}", meta_path.display());
}

fn build_kernel_meta(kernel_bin: &Path) -> String {
    let output = std::process::Command::new("nm")
        .args(["-a", "--defined-only"])
        .arg(kernel_bin)
        .output()
        .unwrap_or_else(|e| panic!("failed to run nm on {}: {}", kernel_bin.display(), e));
    if !output.status.success() {
        panic!("nm failed on {}", kernel_bin.display());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut secondary_entry = None;
    for line in stdout.lines() {
        let mut parts = line.split_whitespace();
        let Some(addr) = parts.next() else {
            continue;
        };
        let Some(_kind) = parts.next() else {
            continue;
        };
        let Some(name) = parts.next() else {
            continue;
        };
        if name == "secondary_cpu_entry" {
            secondary_entry = Some(addr.to_string());
            break;
        }
    }
    let secondary_entry = secondary_entry
        .unwrap_or_else(|| panic!("secondary_cpu_entry not found in {}", kernel_bin.display()));
    format!(
        "secondary_cpu_entry=0x{}\n",
        secondary_entry.trim_start_matches("0x")
    )
}

fn is_elf_binary(path: &Path) -> Result<bool, String> {
    use std::io::Read;

    let mut file =
        fs::File::open(path).map_err(|e| format!("Failed to open {}: {}", path.display(), e))?;
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)
        .map_err(|e| format!("Failed to read ELF magic from {}: {}", path.display(), e))?;
    Ok(magic == [0x7F, b'E', b'L', b'F'])
}

fn compute_sha256(path: &Path) -> Result<String, String> {
    use std::process::Command;

    let output = Command::new("sha256sum")
        .arg(path)
        .output()
        .map_err(|e| format!("Failed to run sha256sum: {}", e))?;

    if !output.status.success() {
        return Err("sha256sum failed".to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .split_whitespace()
        .next()
        .map(|s| s.to_lowercase())
        .ok_or_else(|| "No hash in sha256sum output".to_string())
}

/// BusyBoxをダウンロード
fn ensure_busybox_binary(fs_dir: &Path) -> Result<(), String> {
    let binaries_dir = fs_dir.join("bin");
    fs::create_dir_all(&binaries_dir)
        .map_err(|e| format!("Failed to create {}: {}", binaries_dir.display(), e))?;

    let dest = binaries_dir.join("busybox.elf");
    let temp = binaries_dir.join("busybox.elf.download");

    if dest.exists() {
        if !is_elf_binary(&dest)? {
            println!(
                "cargo:warning=Existing BusyBox is not ELF, replacing: {}",
                dest.display()
            );
            fs::remove_file(&dest)
                .map_err(|e| format!("Failed to remove invalid {}: {}", dest.display(), e))?;
        } else {
            let existing_hash = compute_sha256(&dest)?;
            if existing_hash == BUSYBOX_SHA256 {
                println!(
                    "BusyBox already exists and checksum verified at {}",
                    dest.display()
                );
                return Ok(());
            }
            println!(
                "cargo:warning=Existing BusyBox checksum mismatch (expected {}, got {}), replacing {}",
                BUSYBOX_SHA256,
                existing_hash,
                dest.display()
            );
            fs::remove_file(&dest).map_err(|e| {
                format!(
                    "Failed to remove checksum-mismatched {}: {}",
                    dest.display(),
                    e
                )
            })?;
        }
    }

    println!("Downloading busybox from {}", BUSYBOX_URL);

    let status = std::process::Command::new("curl")
        .args([
            "-L",
            "--fail",
            "--silent",
            "--show-error",
            "--max-time",
            "30",
            "--output",
        ])
        .arg(&temp)
        .arg(BUSYBOX_URL)
        .status();

    match status {
        Ok(s) if s.success() => {
            if !is_elf_binary(&temp)? {
                let _ = fs::remove_file(&temp);
                return Err(format!(
                    "Downloaded file is not a valid ELF binary: {}",
                    temp.display()
                ));
            }

            // SHA256整合性検証（既知の固定値と照合）
            let sha256_file = binaries_dir.join("busybox.elf.sha256");
            let actual_hash = compute_sha256(&temp)?;

            if actual_hash != BUSYBOX_SHA256 {
                let _ = fs::remove_file(&temp);
                return Err(format!(
                    "BusyBox SHA256 mismatch: expected {}, got {}. \
                     上流バイナリが変更された場合は {} を削除して再ビルドしてください",
                    BUSYBOX_SHA256,
                    actual_hash,
                    sha256_file.display()
                ));
            }

            let _ = fs::write(&sha256_file, &actual_hash);

            if let Err(rename_err) = fs::rename(&temp, &dest) {
                fs::copy(&temp, &dest).map_err(|copy_err| {
                    format!(
                        "Failed to place busybox at {} (rename: {}, copy: {})",
                        dest.display(),
                        rename_err,
                        copy_err
                    )
                })?;
                let _ = fs::remove_file(&temp);
            }

            println!("Downloaded busybox to {}", dest.display());
            Ok(())
        }
        Ok(s) => {
            let _ = fs::remove_file(&temp);
            if dest.exists() {
                let existing_hash = compute_sha256(&dest)?;
                if existing_hash != BUSYBOX_SHA256 {
                    return Err(format!(
                        "BusyBox download failed (status={}) and existing {} checksum mismatch: expected {}, got {}",
                        s,
                        dest.display(),
                        BUSYBOX_SHA256,
                        existing_hash
                    ));
                }
                println!(
                    "cargo:warning=BusyBox download failed (status={}), using existing {}",
                    s,
                    dest.display()
                );
                Ok(())
            } else {
                Err(format!(
                    "BusyBox download failed (status={}) and no fallback file exists at {}",
                    s,
                    dest.display()
                ))
            }
        }
        Err(e) => {
            let _ = fs::remove_file(&temp);
            if dest.exists() {
                println!(
                    "cargo:warning=Failed to execute curl ({}), using existing {}",
                    e,
                    dest.display()
                );
                Ok(())
            } else {
                Err(format!(
                    "Failed to execute curl ({}) and no fallback file exists at {}",
                    e,
                    dest.display()
                ))
            }
        }
    }
}

fn ensure_audit_log_file(fs_dir: &Path) -> Result<(), String> {
    let log_dir = fs_dir.join("log");
    fs::create_dir_all(&log_dir)
        .map_err(|e| format!("Failed to create {}: {}", log_dir.display(), e))?;
    let audit_path = log_dir.join("audit.log");
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&audit_path)
        .map_err(|e| format!("Failed to create {}: {}", audit_path.display(), e))?;
    file.set_len(AUDIT_LOG_SIZE)
        .map_err(|e| format!("Failed to size {}: {}", audit_path.display(), e))?;
    println!(
        "Initialized persistent audit log file: {} ({} bytes)",
        audit_path.display(),
        AUDIT_LOG_SIZE
    );
    Ok(())
}

fn copy_apps_autostart_config(
    manifest_dir: &Path,
    fs_dir: &Path,
) -> Result<(), String> {
    let src = manifest_dir.join("src/resources/config/autostart.list");
    let fs_dst_dir = fs_dir.join("config");
    fs::create_dir_all(&fs_dst_dir)
        .map_err(|e| format!("Failed to create {}: {}", fs_dst_dir.display(), e))?;

    let fs_dst = fs_dst_dir.join("autostart.list");
    fs::copy(&src, &fs_dst).map_err(|e| {
        format!(
            "Failed to copy {} to {}: {}",
            src.display(),
            fs_dst.display(),
            e
        )
    })?;
    println!("Copied app autostart config to {}", fs_dst.display());
    Ok(())
}

fn prune_stale_service_artifacts(
    services: &[builders::services::ServiceEntry],
    ramfs_dir: &Path,
    fs_dir: &Path,
) -> Result<(), String> {
    let initfs_expected: HashSet<String> = services
        .iter()
        .filter(|s| s.fs_type == "initfs")
        .map(|s| format!("{}.service", s.name))
        .collect();
    let ata_expected: HashSet<String> = services
        .iter()
        .filter(|s| s.fs_type != "initfs")
        .map(|s| format!("{}.service", s.name))
        .collect();

    if let Ok(entries) = fs::read_dir(ramfs_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if name.ends_with(".service") && !initfs_expected.contains(name) {
                fs::remove_file(&path)
                    .map_err(|e| format!("Failed to remove stale {}: {}", path.display(), e))?;
            }
        }
    }

    let fs_services_dir = fs_dir.join("services");
    if let Ok(entries) = fs::read_dir(&fs_services_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if name.ends_with(".service") && !ata_expected.contains(name) {
                fs::remove_file(&path)
                    .map_err(|e| format!("Failed to remove stale {}: {}", path.display(), e))?;
            }
        }
    }

    Ok(())
}

fn prune_stale_module_artifacts(
    modules: &[builders::modules::ModuleEntry],
    ramfs_dir: &Path,
) -> Result<(), String> {
    let expected: HashSet<String> = modules.iter().map(|m| format!("{}.cext", m.name)).collect();
    let modules_dir = ramfs_dir.join("Modules");
    if let Ok(entries) = fs::read_dir(&modules_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if name.ends_with(".cext") && !expected.contains(name) {
                fs::remove_file(&path)
                    .map_err(|e| format!("Failed to remove stale {}: {}", path.display(), e))?;
            }
        }
    }
    Ok(())
}

fn write_module_hash_manifest(
    modules: &[builders::modules::ModuleEntry],
    ramfs_dir: &Path,
) -> Result<(), String> {
    let modules_dir = ramfs_dir.join("Modules");
    fs::create_dir_all(&modules_dir)
        .map_err(|e| format!("Failed to create {}: {}", modules_dir.display(), e))?;

    let mut lines = Vec::new();
    for module in modules {
        let cext_path = modules_dir.join(format!("{}.cext", module.name));
        if !cext_path.exists() {
            return Err(format!(
                "Missing built module artifact for {} at {}",
                module.name,
                cext_path.display()
            ));
        }
        let hash = compute_sha256(&cext_path)?;
        lines.push(format!("{}.cext={}", module.name, hash));
    }

    let manifest_path = modules_dir.join("modules.sha256");
    fs::write(&manifest_path, lines.join("\n"))
        .map_err(|e| format!("Failed to write {}: {}", manifest_path.display(), e))?;
    println!("Generated {}", manifest_path.display());
    Ok(())
}

#[allow(unused)]
fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    // Emit rerun-if-changed for all source directories
    fn emit_rerun_for_dir(dir: &Path) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_file() {
                    if p.extension()
                        .map(|e| e == "rs" || e == "toml")
                        .unwrap_or(false)
                    {
                        println!("cargo:rerun-if-changed={}", p.display());
                    }
                } else if p.is_dir() {
                    if p.file_name()
                        .map(|n| n != "target" && n != ".git")
                        .unwrap_or(true)
                    {
                        emit_rerun_for_dir(&p);
                    }
                }
            }
        }
    }

    for dir in &[
        "src/user",
        "src/services",
        "src/modules",
        "src/apps",
        "src/drivers",
        "src/resources",
    ] {
        let p = manifest_dir.join(dir);
        if p.exists() {
            emit_rerun_for_dir(&p);
        }
    }

    println!("cargo:rerun-if-env-changed=PROFILE");
    println!("cargo:rerun-if-env-changed=TARGET");
    println!("cargo:rerun-if-env-changed=CARGO_TARGET_DIR");

    // カーネルビルドの再帰呼び出しの場合はプレースホルダーだけ作成して終了する
    // (initfs は埋め込まず、ブートローダーが実行時にロードして BootInfo で渡す)
    if env::var("MOCHIOS_BUILDING_KERNEL").is_ok() {
        let _ = fs::write(out_dir.join("initfs.ext2"), b"");
        let _ = fs::write(out_dir.join("rootfs.ext2"), b"");
        return;
    }

    // ramfsとfsディレクトリを作成
    let ramfs_dir = manifest_dir.join("ramfs");
    let fs_dir = manifest_dir.join("fs");

    for dir in &[&ramfs_dir, &fs_dir] {
        if !dir.is_dir() {
            fs::create_dir_all(dir)
                .unwrap_or_else(|_| panic!("Failed to create directory: {}", dir.display()));
        }
    }

    // fsの標準ディレクトリレイアウトを作成
    let resources_src = manifest_dir.join("src/resources");
    setup_fs_layout(&fs_dir, &resources_src)
        .unwrap_or_else(|e| println!("cargo:warning=setup_fs_layout failed: {}", e));
    copy_apps_autostart_config(&manifest_dir, &fs_dir)
        .expect("Failed to copy apps autostart config");

    // newlibのインストールディレクトリを取得
    let target = env::var("TARGET").unwrap_or("x86_64-unknown-uefi".to_string());
    let profile = env::var("PROFILE").unwrap_or("debug".to_string());
    let target_dir = PathBuf::from(env::var("CARGO_TARGET_DIR").unwrap_or("target".to_string()));

    // カーネル ELF をビルド
    build_kernel(&manifest_dir, &fs_dir, &profile);

    // newlibのビルド
    let newlib_src_dir = manifest_dir.join("src/lib");
    if !newlib_src_dir.exists() {
        panic!("Newlib source not found at {}", newlib_src_dir.display());
    }
    build_newlib(&newlib_src_dir);

    let abs_target_dir = if target_dir.is_absolute() {
        target_dir
    } else {
        manifest_dir.join(target_dir)
    };

    let newlib_install_dir = abs_target_dir
        .join(&target)
        .join(&profile)
        .join("newlib_install");

    // Verify newlib output exists before proceeding
    let libc_path = newlib_install_dir
        .join("x86_64-elf")
        .join("lib")
        .join("libc.a");
    if !libc_path.exists() {
        panic!(
            "newlib build completed but libc.a not found at {}",
            libc_path.display()
        );
    }

    let libc_dir = newlib_install_dir.join("x86_64-elf").join("lib");

    let glue_src_dir = manifest_dir.join("src/runtime_glue");
    build_user_libs(&glue_src_dir, &libc_dir);

    // newlibライブラリをramfsとfsにコピー
    copy_newlib_libs(&libc_dir, &ramfs_dir.join("lib"))
        .expect("cargo:warning=Failed to copy newlib libs to ramfs/lib");
    copy_newlib_libs(&libc_dir, &fs_dir.join("lib"))
        .expect("cargo:warning=Failed to copy newlib libs to fs/lib");

    // libgcc_sをfs/libにコピー
    if let Ok(out) = std::process::Command::new("gcc")
        .arg("-print-file-name=libgcc_s.so.1")
        .output()
    {
        if out.status.success() {
            let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            use std::path::Path;
            let libs_dir = fs_dir.join("lib");
            let _ = fs::create_dir_all(&libs_dir);
            if path != "libgcc_s.so.1" && Path::new(&path).exists() {
                let dest = libs_dir.join("libgcc_s.so.1");
                let _ = fs::copy(&path, &dest);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::symlink;
                    let link = libs_dir.join("libgcc_s.so");
                    if !link.exists() {
                        let _ = symlink("libgcc_s.so.1", &link);
                    }
                }
                println!("Copied libgcc_s to fs/lib: {}", path);
            } else {
                let candidates = [
                    "/usr/lib/x86_64-linux-gnu/libgcc_s.so.1",
                    "/lib/x86_64-linux-gnu/libgcc_s.so.1",
                    "/usr/lib64/libgcc_s.so.1",
                    "/lib64/libgcc_s.so.1",
                ];
                for c in &candidates {
                    if Path::new(c).exists() {
                        let _ = fs::copy(c, libs_dir.join("libgcc_s.so.1"));
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::symlink;
                            let link = libs_dir.join("libgcc_s.so");
                            if !link.exists() {
                                let _ = symlink("libgcc_s.so.1", &link);
                            }
                        }
                        println!("Copied libgcc_s to fs/lib from {}", c);
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

    prune_stale_service_artifacts(&services, &ramfs_dir, &fs_dir)
        .expect("Failed to prune stale service artifacts");

    let modules = default_modules();
    prune_stale_module_artifacts(&modules, &ramfs_dir)
        .expect("Failed to prune stale module artifacts");

    // サービスをビルド
    let services_base_dir = manifest_dir.join("src/services");

    for service in &services {
        let output_dir = if service.fs_type == "initfs" {
            &ramfs_dir
        } else {
            &fs_dir
        };

        build_service(service, &services_base_dir, output_dir)
            .unwrap_or_else(|e| panic!("Failed to build service {}: {}", service.name, e));
    }

    // カーネルモジュールをビルド（initfs/Modules/*.cext）
    let modules_base_dir = manifest_dir.join("src/modules");
    for module in &modules {
        build_module(module, &modules_base_dir, &ramfs_dir)
            .unwrap_or_else(|e| panic!("Failed to build module {}: {}", module.name, e));
    }
    write_module_hash_manifest(&modules, &ramfs_dir).expect("Failed to write module hash manifest");

    // アプリケーションをビルド
    let apps_dir = manifest_dir.join("src/apps");
    let applications_dir = fs_dir.join("applications");
    if apps_dir.is_dir() {
        println!("Building applications");
        build_apps(&apps_dir, &applications_dir, "elf");

        // Clean up build artifacts from applications
        for entry in fs::read_dir(&applications_dir)
            .unwrap_or_else(|_| panic!("Failed to read applications dir"))
        {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_dir() {
                    let target_dir = path.join("target");
                    if target_dir.exists() {
                        if let Err(e) = fs::remove_dir_all(&target_dir) {
                            eprintln!("Warning: Failed to remove {}: {}", target_dir.display(), e);
                        } else {
                            println!("Cleaned up: {}", target_dir.display());
                        }
                    }
                }
            }
        }
    }

    // ユーティリティコマンドをビルド
    let utils_dir = manifest_dir.join("src/utils");
    let binaries_dir = fs_dir.join("bin");
    if utils_dir.is_dir() {
        println!("Building utility commands");
        build_utils(&utils_dir, &binaries_dir);
    }

    ensure_busybox_binary(&fs_dir).expect("Failed to ensure busybox binary");
    ensure_audit_log_file(&fs_dir).expect("Failed to create persistent audit log file");

    // ドライバをビルド
    let drivers_dir = manifest_dir.join("src/drivers");
    let drivers_binaries_dir = binaries_dir.join("drivers");
    let driver_autostart_entries = if drivers_dir.is_dir() {
        println!("Building drivers");
        build_drivers(&drivers_dir, &drivers_binaries_dir)
    } else {
        panic!("Drivers directory not found: {}", drivers_dir.display());
    };

    // driver.service が参照する自動起動ドライバ一覧を生成
    let driver_autostart_path = fs_dir.join("config").join("drivers.list");
    match fs::write(&driver_autostart_path, driver_autostart_entries.join("\n")) {
        Ok(_) => println!("Generated {}", driver_autostart_path.display()),
        Err(e) => panic!(
            "Failed to write critical config {}: {}",
            driver_autostart_path.display(),
            e
        ),
    }
    // services.index に基づき、initfs 以外の autostart サービス一覧を生成 (Config/services.list)
    let mut services_autostart_entries: Vec<String> = Vec::new();
    for svc in &services {
        if svc.autostart {
            if svc.fs_type != "initfs" {
                services_autostart_entries.push(format!("/system/services/{}.service", svc.name));
            } else {
                // 場合によっては initfs に autostart=true が設定されていることがある。
                // 開発者に分かるようにビルド時警告を出す。
                println!("cargo:warning=Autostart service '{}' skipped for services.list because fs='initfs'. If you want it on ATA, set fs = 'ata' in src/services/index.toml", svc.name);
            }
        }
    }
    let services_autostart_path = fs_dir.join("config").join("services.list");
    match fs::write(
        &services_autostart_path,
        services_autostart_entries.join("\n"),
    ) {
        Ok(_) => println!("Generated {}", services_autostart_path.display()),
        Err(e) => panic!(
            "Failed to write critical config {}: {}",
            services_autostart_path.display(),
            e
        ),
    }

    // initfs イメージを生成
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
