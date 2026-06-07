use wayland_compositor::protocol::MessageBuilder;
use std::env;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    // ソケットパス取得
    let socket_path = resolve_socket_path();

    log::info!("Connecting to {}", socket_path);

    // Compositor に接続
    let mut stream = UnixStream::connect(&socket_path).await?;
    log::info!("Connected to compositor");

    let registry_id = 2u32;
    let shm_id = 3u32;
    let compositor_id = 4u32;
    let surface_id = 5u32;
    let buffer_id = 6u32;
    let sync_callback_id = 7u32;

    // wl_display.get_registry
    let msg = MessageBuilder::new(1, 1)
        .push_u32(registry_id)
        .build();
    let bytes = msg.to_bytes();
    stream.write_all(&bytes).await?;
    log::info!("Registry request sent");

    // レスポンス受信
    let mut buf = vec![0u8; 1024];
    if let Ok(n) = stream.read(&mut buf).await {
        log::info!("Received {} bytes", n);
        if n > 0 {
            log::info!("Response: {:?}", &buf[..std::cmp::min(n, 32)]);
        }
    }

    // wl_registry.bind -> wl_shm
    let msg = MessageBuilder::new(registry_id, 0)
        .push_u32(2) // name
        .push_string("wl_shm")
        .push_u32(1) // version
        .push_u32(shm_id) // new object id
        .build();

    let bytes = msg.to_bytes();
    stream.write_all(&bytes).await?;
    log::info!("SHM bind request sent");

    // wl_registry.bind -> wl_compositor
    let msg = MessageBuilder::new(registry_id, 0)
        .push_u32(1) // name
        .push_string("wl_compositor")
        .push_u32(4) // version
        .push_u32(compositor_id) // new object id
        .build();

    let bytes = msg.to_bytes();
    stream.write_all(&bytes).await?;
    log::info!("Compositor bind request sent");

    // wl_compositor.create_surface
    let msg = MessageBuilder::new(compositor_id, 0)
        .push_u32(surface_id)
        .build();

    let bytes = msg.to_bytes();
    stream.write_all(&bytes).await?;
    log::info!("Surface creation request sent");

    // wl_shm.create_buffer
    let msg = MessageBuilder::new(shm_id, 0)
        .push_u32(buffer_id)
        .push_u32(320)
        .push_u32(240)
        .push_u32(320 * 4)
        .push_u32(0)
        .build();
    let bytes = msg.to_bytes();
    stream.write_all(&bytes).await?;
    log::info!("Buffer create request sent");

    // バッファアタッチ
    // 簡略版：ダミーバッファIDを使用
    let msg = MessageBuilder::new(surface_id, 1) // wl_surface::attach
        .push_u32(buffer_id) // buffer id
        .push_i32(0) // x offset
        .push_i32(0) // y offset
        .build();

    let bytes = msg.to_bytes();
    stream.write_all(&bytes).await?;
    log::info!("Buffer attach request sent");

    // Damage 設定
    let msg = MessageBuilder::new(surface_id, 2) // wl_surface::damage
        .push_i32(0)
        .push_i32(0)
        .push_i32(320)
        .push_i32(240)
        .build();

    let bytes = msg.to_bytes();
    stream.write_all(&bytes).await?;
    log::info!("Damage request sent");

    // Commit
    let msg = MessageBuilder::new(surface_id, 4); // wl_surface::commit
    let bytes = msg.build().to_bytes();
    stream.write_all(&bytes).await?;
    log::info!("Commit request sent");

    // wl_display.sync -> callback done を受け取る
    let msg = MessageBuilder::new(1, 0)
        .push_u32(sync_callback_id)
        .build();
    let bytes = msg.to_bytes();
    stream.write_all(&bytes).await?;
    log::info!("Sync request sent");

    let mut seen_sync_done = false;
    let mut seen_buffer_release = false;
    let deadline = tokio::time::Instant::now() + Duration::from_millis(1500);
    while tokio::time::Instant::now() < deadline && (!seen_sync_done || !seen_buffer_release) {
        let mut event_buf = [0u8; 256];
        match tokio::time::timeout(Duration::from_millis(200), stream.read(&mut event_buf)).await {
            Ok(Ok(0)) => break,
            Ok(Ok(n)) if n >= 8 => {
                let object_id = u32::from_le_bytes([
                    event_buf[0], event_buf[1], event_buf[2], event_buf[3],
                ]);
                let opcode = u16::from_le_bytes([event_buf[6], event_buf[7]]);
                log::info!("Event received: object_id={} opcode={} len={}", object_id, opcode, n);
                if object_id == sync_callback_id && opcode == 0 {
                    seen_sync_done = true;
                }
                if object_id == buffer_id && opcode == 0 {
                    seen_buffer_release = true;
                }
            }
            Ok(Ok(_)) => {}
            Ok(Err(err)) => return Err(Box::<dyn std::error::Error>::from(err)),
            Err(_) => {}
        }
    }

    log::info!(
        "Test completed (sync_done={}, buffer_release={})",
        seen_sync_done,
        seen_buffer_release
    );

    Ok(())
}
