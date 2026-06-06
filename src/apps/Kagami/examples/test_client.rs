use wayland_compositor::protocol::MessageBuilder;
use std::env;
use std::thread;
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    // ソケットパス取得
    let socket_path = env::var("WAYLAND_DISPLAY")
        .unwrap_or_else(|_| default_socket_path());

    log::info!("Connecting to {}", socket_path);

    // Compositor に接続
    let mut stream = UnixStream::connect(&socket_path).await?;
    log::info!("Connected to compositor");

    // ウェイトタイム（コンポジター起動待機）
    tokio::time::sleep(Duration::from_millis(100)).await;

    // registry リクエスト送信
    let msg = MessageBuilder::new(1, 0).build();
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

    // Surface 作成リクエスト
    let msg = MessageBuilder::new(2, 0) // wl_compositor::create_surface
        .push_u32(3) // new surface object id
        .build();

    let bytes = msg.to_bytes();
    stream.write_all(&bytes).await?;
    log::info!("Surface creation request sent");

    thread::sleep(Duration::from_millis(100));

    // バッファアタッチ
    // 簡略版：ダミーバッファIDを使用
    let msg = MessageBuilder::new(3, 1) // wl_surface::attach
        .push_u32(100) // buffer id
        .push_i32(0) // x offset
        .push_i32(0) // y offset
        .build();

    let bytes = msg.to_bytes();
    stream.write_all(&bytes).await?;
    log::info!("Buffer attach request sent");

    // Damage 設定
    let msg = MessageBuilder::new(3, 2) // wl_surface::damage
        .push_i32(0)
        .push_i32(0)
        .push_i32(320)
        .push_i32(240)
        .build();

    let bytes = msg.to_bytes();
    stream.write_all(&bytes).await?;
    log::info!("Damage request sent");

    // Commit
    let msg = MessageBuilder::new(3, 4); // wl_surface::commit
    let bytes = msg.build().to_bytes();
    stream.write_all(&bytes).await?;
    log::info!("Commit request sent");

    tokio::time::sleep(Duration::from_millis(500)).await;

    log::info!("Test completed");

    Ok(())
}
