// src/compositor.rs - Wayland Compositor コア実装

use crate::backend::FramebufferBackend;
use crate::client::Client;
use crate::error::{CompositorError, Result};
use crate::protocol::{Message, MessageBuilder, MessageParser};
use crate::surface::Surface;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::RwLock;

/// Wayland Compositor
pub struct Compositor<B: FramebufferBackend> {
    backend: Arc<RwLock<B>>,
    clients: Arc<RwLock<HashMap<u32, Client>>>,
    surfaces: Arc<RwLock<HashMap<u32, Surface>>>,
    next_client_id: Arc<RwLock<u32>>,
    next_object_id: Arc<RwLock<u32>>,
    socket_path: String,
}

impl<B: FramebufferBackend + 'static> Compositor<B> {
    /// 新規 Compositor 作成
    pub fn new(backend: B, socket_path: String) -> Result<Self> {
        Ok(Compositor {
            backend: Arc::new(RwLock::new(backend)),
            clients: Arc::new(RwLock::new(HashMap::new())),
            surfaces: Arc::new(RwLock::new(HashMap::new())),
            next_client_id: Arc::new(RwLock::new(1)),
            next_object_id: Arc::new(RwLock::new(2)),
            socket_path,
        })
    }

    /// 初期化
    pub async fn init(&mut self) -> Result<()> {
        let mut backend = self.backend.write().await;
        let info = backend.init().await
            .map_err(|e| CompositorError::Backend(e.to_string()))?;

        log::info!(
            "Compositor initialized with backend: {}",
            backend.name()
        );
        log::info!(
            "Framebuffer: {}x{} (stride={})",
            info.width, info.height, info.stride
        );

        Ok(())
    }

    /// メインループ実行
    pub async fn run(&self) -> Result<()> {
        // ソケットファイルを削除
        let _ = std::fs::remove_file(&self.socket_path);

        let listener = UnixListener::bind(&self.socket_path)
            .map_err(|e| CompositorError::Io(e))?;

        log::info!("Listening on {}", self.socket_path);

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let compositor = self.clone_for_client();
                    tokio::spawn(async move {
                        if let Err(e) = compositor.handle_client(stream).await {
                            log::error!("Client error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    log::error!("Accept error: {}", e);
                }
            }
        }
    }

    /// 描画する
    pub async fn render(&self) -> Result<()> {
        let mut backend = self.backend.write().await;
        let surfaces = self.surfaces.read().await;

        // フレームバッファをクリア
        backend.clear(0x1f1f1fff)
            .map_err(|e| CompositorError::Backend(e.to_string()))?;

        // Z-order でサーフェスを描画
        let mut sorted_surfaces: Vec<_> = surfaces.values().collect();
        sorted_surfaces.sort_by_key(|s| s.z_index);

        for surface in sorted_surfaces {
            if surface.visible {
                if let Some(ref buffer) = surface.buffer_data {
                    backend.write_region(
                        surface.x as u32,
                        surface.y as u32,
                        surface.width,
                        surface.height,
                        buffer,
                    )
                        .map_err(|e| CompositorError::Backend(e.to_string()))?;
                }
            }
        }

        backend.flush().await
            .map_err(|e| CompositorError::Backend(e.to_string()))?;

        Ok(())
    }

    /// 内部用：クライアント処理用にクローン
    fn clone_for_client(&self) -> Self {
        Compositor {
            backend: Arc::clone(&self.backend),
            clients: Arc::clone(&self.clients),
            surfaces: Arc::clone(&self.surfaces),
            next_client_id: Arc::clone(&self.next_client_id),
            next_object_id: Arc::clone(&self.next_object_id),
            socket_path: self.socket_path.clone(),
        }
    }

    /// クライアント接続処理
    async fn handle_client(&self, stream: UnixStream) -> Result<()> {
        // クライアント ID を取得
        let client_id = {
            let mut id = self.next_client_id.write().await;
            let cid = *id;
            *id += 1;
            cid
        };

        log::info!("Client {} connected", client_id);

        let client = Client::new(client_id, stream);
        self.clients.write().await.insert(client_id, client);

        // メッセージ受信ループ
        let mut buf = vec![0u8; 4096];
        let stream = {
            let clients = self.clients.read().await;
            if let Some(c) = clients.get(&client_id) {
                Arc::clone(&c.stream)
            } else {
                return Err(CompositorError::ClientNotFound(client_id));
            }
        };

        let mut client_error: Option<CompositorError> = None;
        loop {
            let n = {
                let mut s = stream.lock().await;
                s.read(&mut buf).await
            };

            match n {
                Ok(0) => {
                    log::info!("Client {} disconnected", client_id);
                    break;
                }
                Ok(n) => {
                    if let Err(e) = self.process_client_messages(client_id, &buf[..n]).await {
                        client_error = Some(e);
                        break;
                    }
                }
                Err(e) => {
                    client_error = Some(CompositorError::Io(e));
                    break;
                }
            }
        }

        // クライアント削除
        self.cleanup_client_state(client_id).await;
        log::info!("Client {} cleaned up", client_id);

        if let Some(err) = client_error {
            Err(err)
        } else {
            Ok(())
        }
    }

    async fn cleanup_client_state(&self, client_id: u32) {
        let mut clients = self.clients.write().await;
        let owned_surfaces = clients
            .get(&client_id)
            .map(|client| client.surfaces.keys().copied().collect::<Vec<_>>())
            .unwrap_or_default();

        clients.remove(&client_id);
        drop(clients);

        if !owned_surfaces.is_empty() {
            let mut surfaces = self.surfaces.write().await;
            for surface_id in owned_surfaces {
                if let Some(surface) = surfaces.get_mut(&surface_id) {
                    surface.detach_buffer();
                    surface.clear_damage();
                }
                surfaces.remove(&surface_id);
            }
        }
    }

    /// クライアントメッセージ処理
    async fn process_client_messages(&self, client_id: u32, buf: &[u8]) -> Result<()> {
        let mut offset = 0;

        while offset < buf.len() {
            if let Some((msg, size)) = Message::from_bytes(&buf[offset..]) {
                self.validate_message(&msg)?;
                self.process_message(client_id, &msg).await?;
                offset += size;
            } else {
                return Err(CompositorError::InvalidMessage(
                    "truncated or malformed message".to_string(),
                ));
            }
        }

        Ok(())
    }

    fn validate_message(&self, msg: &Message) -> Result<()> {
        let object_id = msg.header.object_id;
        let opcode = msg.header.opcode;
        let len = msg.data.len();

        let expected = match (object_id, opcode) {
            (1, 0) => Some(0usize),  // wl_display::get_registry
            (2, 0) => Some(4usize),  // wl_compositor::create_surface
            (3, 1) => Some(12usize), // wl_surface::attach
            (3, 2) => Some(16usize), // wl_surface::damage
            (3, 4) => Some(0usize),  // wl_surface::commit
            _ => None,
        };

        let Some(expected) = expected else {
            return Err(CompositorError::InvalidMessage(format!(
                "unknown object/opcode combination object_id={} opcode={}",
                object_id, opcode
            )));
        };

        if len != expected {
            return Err(CompositorError::InvalidMessage(format!(
                "invalid payload size for object_id={} opcode={}: got {}, expected {}",
                object_id, opcode, len, expected
            )));
        }

        Ok(())
    }

    /// 個別メッセージ処理
    async fn process_message(&self, client_id: u32, msg: &Message) -> Result<()> {
        let object_id = msg.header.object_id;
        let opcode = msg.header.opcode;
        let mut needs_render = false;

        match (object_id, opcode) {
            // wl_display
            (1, 0) => {
                // get_registry
                let registry_id = {
                    let mut id = self.next_object_id.write().await;
                    let oid = *id;
                    *id += 1;
                    oid
                };
                self.send_registry_globals(client_id, registry_id).await?;
            }
            // wl_compositor
            (2, 0) => {
                // create_surface
                let mut parser = MessageParser::new(&msg.data);
                if let Some(surface_id) = parser.read_u32() {
                    if self.surfaces.read().await.contains_key(&surface_id) {
                        return Err(CompositorError::InvalidMessage(format!(
                            "duplicate surface id {}",
                            surface_id
                        )));
                    }
                    let surface = Surface::new(surface_id, client_id);
                    self.surfaces.write().await.insert(surface_id, surface);

                    if let Some(client) = self.clients.write().await.get_mut(&client_id) {
                        client.add_surface(surface_id, surface_id);
                    }

                    log::debug!("Surface {} created for client {}", surface_id, client_id);
                }
            }
            // wl_surface
            _ => {
                if let Some(surface) = self.surfaces.write().await.get_mut(&object_id) {
                    match opcode {
                            1 => {
                                // attach
                                let mut parser = MessageParser::new(&msg.data);
                                if let Some(buffer_id) = parser.read_u32() {
                                    if buffer_id == 0 {
                                        surface.detach_buffer();
                                        surface.clear_damage();
                                    } else {
                                        // 簡略化：buffer_id を直接使用
                                        // 実装では wl_shm でバッファを管理
                                        log::debug!(
                                            "Buffer {} attached to surface {}",
                                            buffer_id,
                                            object_id
                                        );
                                    }
                                }
                                if parser.remaining() != 8 {
                                    return Err(CompositorError::InvalidMessage(format!(
                                        "wl_surface::attach expects 12 bytes, got {}",
                                        msg.data.len()
                                    )));
                                }
                            }
                            2 => {
                                // damage
                                let mut parser = MessageParser::new(&msg.data);
                                if let (Some(x), Some(y), Some(w), Some(h)) = (
                                parser.read_i32(),
                                parser.read_i32(),
                                parser.read_i32(),
                                parser.read_i32(),
                                ) {
                                    surface.set_damage(x, y, w, h);
                                } else {
                                    return Err(CompositorError::InvalidMessage(
                                        "wl_surface::damage payload too short".to_string(),
                                    ));
                                }
                            }
                            4 => {
                                // commit
                                surface.commit();
                            log::debug!("Surface {} committed", object_id);

                            // MVP: wl_shm 等が未実装のため、バッファが無い場合は
                            // damage サイズのダミーバッファを生成して描画できるようにする。
                            if surface.buffer_data.is_none()
                                && surface.damage.width > 0
                                && surface.damage.height > 0
                            {
                                let info = { self.backend.read().await.info() };
                                let bpp = info.format.bytes_per_pixel() as u32;
                                let width = surface.damage.width as u32;
                                let height = surface.damage.height as u32;
                                let stride = width.saturating_mul(bpp);

                                let mut data = vec![0u8; (stride * height) as usize];
                                match bpp {
                                    4 => {
                                        // XRGB8888 想定（alpha無視）
                                        let color = 0x00_20_a0_e0u32.to_le_bytes();
                                        for px in data.chunks_exact_mut(4) {
                                            px.copy_from_slice(&color);
                                        }
                                    }
                                    2 => {
                                        // RGB565: (R=0x10, G=0x30, B=0x1c) くらいの水色
                                        let color_565: u16 = (0x10 << 11) | (0x30 << 5) | 0x1c;
                                        let bytes = color_565.to_le_bytes();
                                        for px in data.chunks_exact_mut(2) {
                                            px.copy_from_slice(&bytes);
                                        }
                                    }
                                    _ => {}
                                }

                                surface.attach_buffer(data, width, height, stride);
                            }

                            needs_render = true;
                            }
                        _ => {
                            return Err(CompositorError::InvalidMessage(format!(
                                "unsupported wl_surface opcode {}",
                                opcode
                            )));
                        }
                    }
                } else {
                    return Err(CompositorError::SurfaceNotFound(object_id));
                }
            }
        }

        if needs_render {
            self.render().await?;
        }

        Ok(())
    }

    /// Registry グローバルを送信
    async fn send_registry_globals(&self, client_id: u32, registry_id: u32) -> Result<()> {
        let msg = MessageBuilder::new(registry_id, 0) // global event
            .push_u32(1) // name
            .push_string("wl_compositor")
            .push_u32(4) // version
            .build();

        self.send_message(client_id, &msg).await?;
        Ok(())
    }

    /// メッセージをクライアントに送信
    async fn send_message(&self, client_id: u32, msg: &Message) -> Result<()> {
        let clients = self.clients.read().await;
        if let Some(client) = clients.get(&client_id) {
            let bytes = msg.to_bytes();
            let stream = client.stream.lock().await;
            stream.try_write(&bytes)
                .map_err(|e| CompositorError::Io(e))?;
            Ok(())
        } else {
            Err(CompositorError::ClientNotFound(client_id))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::memory::MemoryFramebufferBackend;
    use crate::backend::PixelFormat;

    #[tokio::test]
    async fn test_compositor_creation() {
        let backend = MemoryFramebufferBackend::new(800, 600, PixelFormat::XRGB8888);
        let mut compositor = Compositor::new(backend, "/tmp/test-compositor.sock".to_string())
            .expect("Failed to create compositor");
        let result = compositor.init().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_compositor_render() {
        let backend = MemoryFramebufferBackend::new(800, 600, PixelFormat::XRGB8888);
        let compositor = Compositor::new(backend, "/tmp/test-compositor2.sock".to_string())
            .expect("Failed to create compositor");
        let result = compositor.render().await;
        assert!(result.is_ok());
    }
}
