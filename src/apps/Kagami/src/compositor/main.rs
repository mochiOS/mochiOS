use wayland_compositor::{Compositor, backend};
use std::env;

fn default_socket_path() -> String {
    if let Ok(runtime_dir) = env::var("XDG_RUNTIME_DIR") {
        if !runtime_dir.is_empty() {
            return format!("{}/wayland-0", runtime_dir.trim_end_matches('/'));
        }
    }
    "/run/user/0/wayland-0".to_string()
}

fn resolve_socket_path() -> String {
    match env::var("WAYLAND_DISPLAY") {
        Ok(display) if !display.is_empty() => {
            if display.contains('/') {
                display
            } else if let Ok(runtime_dir) = env::var("XDG_RUNTIME_DIR") {
                if !runtime_dir.is_empty() {
                    format!("{}/{}", runtime_dir.trim_end_matches('/'), display)
                } else {
                    format!("/run/user/0/{}", display)
                }
            } else {
                format!("/run/user/0/{}", display)
            }
        }
        _ => default_socket_path(),
    }
}

#[cfg(feature = "backend-linux-fb")]
fn create_backend() -> Box<dyn backend::FramebufferBackend> {
    Box::new(backend::LinuxFramebufferBackend::new())
}

#[cfg(all(feature = "backend-mochios-vga", not(feature = "backend-linux-fb")))]
fn create_backend() -> Box<dyn backend::FramebufferBackend> {
    Box::new(backend::MochiVgaBackend::new())
}

#[cfg(all(
    feature = "backend-generic-memory",
    not(any(feature = "backend-linux-fb", feature = "backend-mochios-vga"))
))]
fn create_backend() -> Box<dyn backend::FramebufferBackend> {
    Box::new(backend::MemoryFramebufferBackend::from_env())
}

#[cfg(all(
    feature = "backend-custom",
    not(any(
        feature = "backend-linux-fb",
        feature = "backend-mochios-vga",
        feature = "backend-generic-memory"
    ))
))]
fn create_backend() -> Box<dyn backend::FramebufferBackend> {
    Box::new(backend::CustomFramebufferBackend::new())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    let backend = create_backend();

    log::info!("Starting Wayland Compositor");
    log::info!("Backend: {}", backend.name());

    // ソケットパス取得
    let socket_path = resolve_socket_path();

    // Compositor 作成
    let mut compositor = Compositor::new(backend, socket_path.clone())?;

    // 初期化
    compositor.init().await?;

    log::info!("Wayland Compositor running");
    log::info!("Socket: {}", socket_path);

    // メインループ
    compositor.run().await?;

    Ok(())
}
