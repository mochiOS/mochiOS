use wayland_compositor::{Compositor, backend};
use std::env;

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
    let socket_path = env::var("WAYLAND_DISPLAY")
        .unwrap_or_else(|_| "/tmp/wayland-0".to_string());

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
