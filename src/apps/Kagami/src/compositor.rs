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

const WL_COMPOSITOR_GLOBAL_NAME: u32 = 1;
const WL_COMPOSITOR_GLOBAL_VERSION: u32 = 4;
const WL_SHM_GLOBAL_NAME: u32 = 2;
const WL_SHM_GLOBAL_VERSION: u32 = 1;

/// Wayland Compositor
pub struct Compositor<B: FramebufferBackend> {
    backend: Arc<RwLock<B>>,
    clients: Arc<RwLock<HashMap<u32, Client>>>,
    surfaces: Arc<RwLock<HashMap<u32, Surface>>>,
    buffers: Arc<RwLock<HashMap<u32, Buffer>>>,
    next_client_id: Arc<RwLock<u32>>,
    next_object_id: Arc<RwLock<u32>>,
    socket_path: String,
}

#[derive(Clone, Debug)]
struct Buffer {
    object_id: u32,
    client_id: u32,
    width: u32,
    height: u32,
    stride: u32,
    format: u32,
    data: Vec<u8>,
    destroyed: bool,
}

impl<B: FramebufferBackend + 'static> Compositor<B> {
    /// 新規 Compositor 作成
    pub fn new(backend: B, socket_path: String) -> Result<Self> {
        Ok(Compositor {
            backend: Arc::new(RwLock::new(backend)),
            clients: Arc::new(RwLock::new(HashMap::new())),
            surfaces: Arc::new(RwLock::new(HashMap::new())),
            buffers: Arc::new(RwLock::new(HashMap::new())),
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
            buffers: Arc::clone(&self.buffers),
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
            let mut released_buffer_ids = Vec::new();
            for surface_id in owned_surfaces {
                if let Some(surface) = surfaces.get_mut(&surface_id) {
                    if let Some(buffer_id) = surface.buffer_object_id {
                        released_buffer_ids.push(buffer_id);
                    }
                    surface.detach_buffer();
                    surface.clear_damage();
                }
                surfaces.remove(&surface_id);
            }
            drop(surfaces);
            for buffer_id in released_buffer_ids {
                let _ = self.release_buffer_if_unused(buffer_id).await;
            }
        }

        let owned_buffers = {
            let buffers = self.buffers.read().await;
            buffers
                .values()
                .filter(|buffer| buffer.client_id == client_id)
                .map(|buffer| buffer.object_id)
                .collect::<Vec<_>>()
        };
        if !owned_buffers.is_empty() {
            let mut buffers = self.buffers.write().await;
            for buffer_id in owned_buffers {
                buffers.remove(&buffer_id);
            }
        }
    }

    async fn is_buffer_attached(&self, buffer_id: u32) -> bool {
        self.surfaces
            .read()
            .await
            .values()
            .any(|surface| surface.buffer_object_id == Some(buffer_id))
    }

    async fn release_buffer_if_unused(&self, buffer_id: u32) -> Result<()> {
        if self.is_buffer_attached(buffer_id).await {
            return Ok(());
        }
        let removal = self
            .buffers
            .read()
            .await
            .get(&buffer_id)
            .map(|buffer| (buffer.destroyed, buffer.client_id));
        if let Some((true, client_id)) = removal {
            self.buffers.write().await.remove(&buffer_id);
            self.send_delete_id(client_id, buffer_id).await?;
        }
        Ok(())
    }

    async fn attach_fallback_buffer_if_needed(&self, surface: &mut Surface) {
        if surface.buffer_object_id.is_some() || surface.buffer_data.is_some() {
            return;
        }
        if surface.damage.width <= 0 || surface.damage.height <= 0 {
            return;
        }

        let info = { self.backend.read().await.info() };
        let bpp = info.format.bytes_per_pixel() as u32;
        let width = surface.damage.width as u32;
        let height = surface.damage.height as u32;
        let stride = width.saturating_mul(bpp);

        let mut data = vec![0u8; (stride * height) as usize];
        match bpp {
            4 => {
                let color = 0x00_20_a0_e0u32.to_le_bytes();
                for px in data.chunks_exact_mut(4) {
                    px.copy_from_slice(&color);
                }
            }
            2 => {
                let color_565: u16 = (0x10 << 11) | (0x30 << 5) | 0x1c;
                let bytes = color_565.to_le_bytes();
                for px in data.chunks_exact_mut(2) {
                    px.copy_from_slice(&bytes);
                }
            }
            _ => {}
        }

        surface.attach_buffer(data, width, height, stride, None);
    }

    async fn reserve_registry_id(&self, client_id: u32, requested_id: Option<u32>) -> Result<u32> {
        let mut clients = self.clients.write().await;
        let Some(client) = clients.get_mut(&client_id) else {
            return Err(CompositorError::ClientNotFound(client_id));
        };

        let registry_id = if let Some(registry_id) = requested_id {
            if registry_id == 0 {
                return Err(CompositorError::InvalidMessage(
                    "wl_display::get_registry id must be non-zero".to_string(),
                ));
            }
            if client.registry_object_id == Some(registry_id) {
                return Ok(registry_id);
            }
            if client.registry_object_id.is_some()
                || client.compositor_object_id == Some(registry_id)
                || client.shm_object_id == Some(registry_id)
                || client.surfaces.contains_key(&registry_id)
                || client.buffers.contains_key(&registry_id)
                || client.callbacks.contains(&registry_id)
            {
                return Err(CompositorError::InvalidMessage(format!(
                    "wl_display::get_registry duplicate id {}",
                    registry_id
                )));
            }
            registry_id
        } else if let Some(registry_id) = client.registry_object_id {
            return Ok(registry_id);
        } else {
            loop {
                let candidate = {
                    let mut id = self.next_object_id.write().await;
                    let oid = *id;
                    *id += 1;
                    oid
                };
                if candidate == 0
                    || client.compositor_object_id == Some(candidate)
                    || client.shm_object_id == Some(candidate)
                    || client.surfaces.contains_key(&candidate)
                    || client.buffers.contains_key(&candidate)
                    || client.callbacks.contains(&candidate)
                {
                    continue;
                }
                break candidate;
            }
        };

        client.registry_object_id = Some(registry_id);
        Ok(registry_id)
    }

    /// クライアントメッセージ処理
    async fn process_client_messages(&self, client_id: u32, buf: &[u8]) -> Result<()> {
        let mut offset = 0;

        while offset < buf.len() {
            if let Some((msg, size)) = Message::from_bytes(&buf[offset..]) {
                self.validate_message(client_id, &msg).await?;
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

    async fn validate_message(&self, client_id: u32, msg: &Message) -> Result<()> {
        let object_id = msg.header.object_id;
        let opcode = msg.header.opcode;
        let len = msg.data.len();
        let (registry_object_id, compositor_object_id, shm_object_id, buffer_ids, surface_ids) = {
            let clients = self.clients.read().await;
            clients
                .get(&client_id)
                .map(|client| {
                    (
                        client.registry_object_id,
                        client.compositor_object_id,
                        client.shm_object_id,
                        client.buffers.keys().copied().collect::<Vec<_>>(),
                        client.surfaces.keys().copied().collect::<Vec<_>>(),
                    )
                })
                .unwrap_or((None, None, None, Vec::new(), Vec::new()))
        };

        let legacy_compositor = object_id == 2;
        let is_surface = surface_ids.contains(&object_id);
        let is_buffer = buffer_ids.contains(&object_id);
        let is_compositor = legacy_compositor || compositor_object_id == Some(object_id);

        match (object_id, opcode) {
            (1, 0) => {
                if len != 0 && len != 4 {
                    return Err(CompositorError::InvalidMessage(format!(
                        "wl_display::sync/get_registry expects 0 or 4 bytes, got {}",
                        len
                    )));
                }
            }
            (1, 1) => {
                if len != 4 {
                    return Err(CompositorError::InvalidMessage(format!(
                        "wl_display::get_registry expects 4 bytes, got {}",
                        len
                    )));
                }
            }
            _ if Some(object_id) == registry_object_id => {
                if opcode != 0 {
                    return Err(CompositorError::InvalidMessage(format!(
                        "unsupported wl_registry opcode {}",
                        opcode
                    )));
                }
                if len < 16 {
                    return Err(CompositorError::InvalidMessage(format!(
                        "wl_registry::bind payload too short: {}",
                        len
                    )));
                }
            }
            _ if is_compositor && opcode == 0 => {
                if len != 4 {
                    return Err(CompositorError::InvalidMessage(format!(
                        "wl_compositor::create_surface expects 4 bytes, got {}",
                        len
                    )));
                }
            }
            _ if Some(object_id) == shm_object_id => {
                if opcode != 0 {
                    return Err(CompositorError::InvalidMessage(format!(
                        "unsupported wl_shm opcode {}",
                        opcode
                    )));
                }
                if len != 20 {
                    return Err(CompositorError::InvalidMessage(format!(
                        "wl_shm::create_buffer expects 20 bytes, got {}",
                        len
                    )));
                }
            }
            _ if is_buffer => {
                if opcode != 0 {
                    return Err(CompositorError::InvalidMessage(format!(
                        "unsupported wl_buffer opcode {}",
                        opcode
                    )));
                }
                if len != 0 {
                    return Err(CompositorError::InvalidMessage(format!(
                        "wl_buffer::destroy expects no payload, got {}",
                        len
                    )));
                }
            }
            _ if is_surface => {
                let expected = match opcode {
                    0 => 0usize,
                    1 => 12usize,
                    2 => 16usize,
                    4 => 0usize,
                    _ => {
                        return Err(CompositorError::InvalidMessage(format!(
                            "unsupported wl_surface opcode {}",
                            opcode
                        )))
                    }
                };
                if len != expected {
                    return Err(CompositorError::InvalidMessage(format!(
                        "invalid payload size for object_id={} opcode={}: got {}, expected {}",
                        object_id, opcode, len, expected
                    )));
                }
            }
            _ => {
                return Err(CompositorError::InvalidMessage(format!(
                    "unknown object/opcode combination object_id={} opcode={}",
                    object_id, opcode
                )));
            }
        }

        Ok(())
    }

    /// 個別メッセージ処理
    async fn process_message(&self, client_id: u32, msg: &Message) -> Result<()> {
        let object_id = msg.header.object_id;
        let opcode = msg.header.opcode;
        let mut needs_render = false;
        let mut release_buffer_id: Option<u32> = None;
        let (registry_object_id, compositor_object_id, shm_object_id, buffer_ids) = {
            let clients = self.clients.read().await;
            clients
                .get(&client_id)
                .map(|client| {
                    (
                        client.registry_object_id,
                        client.compositor_object_id,
                        client.shm_object_id,
                        client.buffers.keys().copied().collect::<Vec<_>>(),
                    )
                })
                .unwrap_or((None, None, None, Vec::new()))
        };

        match (object_id, opcode) {
            // wl_display
            (1, 0) => {
                if msg.data.len() == 4 {
                    // wl_display::sync
                    let mut parser = MessageParser::new(&msg.data);
                    if let Some(callback_id) = parser.read_u32() {
                        if callback_id == 0 {
                            return Err(CompositorError::InvalidMessage(
                                "wl_display::sync callback_id must be non-zero".to_string(),
                            ));
                        }
                        let mut clients = self.clients.write().await;
                        let Some(client) = clients.get_mut(&client_id) else {
                            return Err(CompositorError::ClientNotFound(client_id));
                        };
                        if client.registry_object_id == Some(callback_id)
                            || client.compositor_object_id == Some(callback_id)
                            || client.shm_object_id == Some(callback_id)
                            || client.surfaces.contains_key(&callback_id)
                            || client.buffers.contains_key(&callback_id)
                            || client.callbacks.contains(&callback_id)
                        {
                            return Err(CompositorError::InvalidMessage(format!(
                                "wl_display::sync duplicate callback id {}",
                                callback_id
                            )));
                        }
                        client.add_callback(callback_id);
                        drop(clients);
                        self.send_callback_done(client_id, callback_id).await?;
                        if let Some(client) = self.clients.write().await.get_mut(&client_id) {
                            client.remove_callback(callback_id);
                        }
                        self.send_delete_id(client_id, callback_id).await?;
                    }
                    return Ok(());
                }

                // Legacy: get_registry without a new_id payload.
                let registry_id = self.reserve_registry_id(client_id, None).await?;
                self.send_registry_globals(client_id, registry_id).await?;
            }
            (1, 1) => {
                // wl_display::get_registry
                let mut parser = MessageParser::new(&msg.data);
                if let Some(registry_id) = parser.read_u32() {
                    let registry_id = self.reserve_registry_id(client_id, Some(registry_id)).await?;
                    self.send_registry_globals(client_id, registry_id).await?;
                }
            }
            _ if Some(object_id) == registry_object_id =>
            {
                let mut parser = MessageParser::new(&msg.data);
                let Some(name) = parser.read_u32() else {
                    return Err(CompositorError::InvalidMessage(
                        "wl_registry::bind missing name".to_string(),
                    ));
                };
                let Some(interface) = parser.read_string() else {
                    return Err(CompositorError::InvalidMessage(
                        "wl_registry::bind missing interface".to_string(),
                    ));
                };
                let Some(version) = parser.read_u32() else {
                    return Err(CompositorError::InvalidMessage(
                        "wl_registry::bind missing version".to_string(),
                    ));
                };
                let Some(new_id) = parser.read_u32() else {
                    return Err(CompositorError::InvalidMessage(
                        "wl_registry::bind missing new_id".to_string(),
                    ));
                };
                if parser.remaining() != 0 {
                    return Err(CompositorError::InvalidMessage(
                        "wl_registry::bind has trailing payload".to_string(),
                    ));
                }
                if new_id == 0 {
                    return Err(CompositorError::InvalidMessage(
                        "wl_registry::bind new_id must be non-zero".to_string(),
                    ));
                }
                let mut clients = self.clients.write().await;
                let Some(client) = clients.get_mut(&client_id) else {
                    return Err(CompositorError::ClientNotFound(client_id));
                };
                if client.registry_object_id == Some(new_id)
                    || client.compositor_object_id == Some(new_id)
                    || client.shm_object_id == Some(new_id)
                    || client.surfaces.contains_key(&new_id)
                    || client.buffers.contains_key(&new_id)
                {
                    return Err(CompositorError::InvalidMessage(format!(
                        "wl_registry::bind duplicate object id {}",
                        new_id
                    )));
                }
                if interface == "wl_compositor" {
                    if name != WL_COMPOSITOR_GLOBAL_NAME {
                        return Err(CompositorError::InvalidMessage(format!(
                            "wl_registry::bind name {} does not match wl_compositor",
                            name
                        )));
                    }
                    if version == 0 || version > WL_COMPOSITOR_GLOBAL_VERSION {
                        return Err(CompositorError::InvalidMessage(format!(
                            "wl_registry::bind invalid wl_compositor version {}",
                            version
                        )));
                    }
                    client.compositor_object_id = Some(new_id);
                    log::debug!("Client {} bound wl_compositor as object {}", client_id, new_id);
                } else if interface == "wl_shm" {
                    if name != WL_SHM_GLOBAL_NAME {
                        return Err(CompositorError::InvalidMessage(format!(
                            "wl_registry::bind name {} does not match wl_shm",
                            name
                        )));
                    }
                    if version == 0 || version > WL_SHM_GLOBAL_VERSION {
                        return Err(CompositorError::InvalidMessage(format!(
                            "wl_registry::bind invalid wl_shm version {}",
                            version
                        )));
                    }
                    client.shm_object_id = Some(new_id);
                    log::debug!("Client {} bound wl_shm as object {}", client_id, new_id);
                } else {
                    return Err(CompositorError::InvalidMessage(format!(
                        "unsupported wl_registry interface {}",
                        interface
                    )));
                }
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
            _ if Some(object_id) == shm_object_id => {
                let mut parser = MessageParser::new(&msg.data);
                let Some(buffer_id) = parser.read_u32() else {
                    return Err(CompositorError::InvalidMessage(
                        "wl_shm::create_buffer missing buffer_id".to_string(),
                    ));
                };
                let Some(width) = parser.read_u32() else {
                    return Err(CompositorError::InvalidMessage(
                        "wl_shm::create_buffer missing width".to_string(),
                    ));
                };
                let Some(height) = parser.read_u32() else {
                    return Err(CompositorError::InvalidMessage(
                        "wl_shm::create_buffer missing height".to_string(),
                    ));
                };
                let Some(stride) = parser.read_u32() else {
                    return Err(CompositorError::InvalidMessage(
                        "wl_shm::create_buffer missing stride".to_string(),
                    ));
                };
                let Some(format) = parser.read_u32() else {
                    return Err(CompositorError::InvalidMessage(
                        "wl_shm::create_buffer missing format".to_string(),
                    ));
                };
                if parser.remaining() != 0 {
                    return Err(CompositorError::InvalidMessage(
                        "wl_shm::create_buffer has trailing payload".to_string(),
                    ));
                }
                if buffer_id == 0 {
                    return Err(CompositorError::InvalidMessage(
                        "wl_shm::create_buffer buffer_id must be non-zero".to_string(),
                    ));
                }
                if width == 0 || height == 0 {
                    return Err(CompositorError::InvalidMessage(format!(
                        "wl_shm::create_buffer invalid size {}x{}",
                        width, height
                    )));
                }
                if format != 0 {
                    return Err(CompositorError::InvalidMessage(format!(
                        "unsupported wl_shm format {}",
                        format
                    )));
                }
                let bytes_per_pixel = 4u32;
                let min_stride = width.saturating_mul(bytes_per_pixel);
                if stride < min_stride {
                    return Err(CompositorError::InvalidMessage(format!(
                        "wl_shm::create_buffer stride {} too small for width {}",
                        stride, width
                    )));
                }
                if self.buffers.read().await.contains_key(&buffer_id) {
                    return Err(CompositorError::InvalidMessage(format!(
                        "duplicate buffer id {}",
                        buffer_id
                    )));
                }
                let pixel_count = (stride as usize).saturating_mul(height as usize);
                let mut data = vec![0u8; pixel_count];
                if format == 0 {
                    for px in data.chunks_exact_mut(4) {
                        px.copy_from_slice(&0x00_20_a0_e0u32.to_le_bytes());
                    }
                }
                self.buffers.write().await.insert(
                    buffer_id,
                    Buffer {
                        object_id: buffer_id,
                        client_id,
                        width,
                        height,
                        stride,
                        format,
                        data,
                        destroyed: false,
                    },
                );
                if let Some(client) = self.clients.write().await.get_mut(&client_id) {
                    client.add_buffer(buffer_id, buffer_id);
                }
                log::debug!(
                    "Client {} created wl_shm buffer {} ({}x{} stride={})",
                    client_id, buffer_id, width, height, stride
                );
            }
            _ if buffer_ids.contains(&object_id) => {
                if opcode != 0 {
                    return Err(CompositorError::InvalidMessage(format!(
                        "unsupported wl_buffer opcode {}",
                        opcode
                    )));
                }
                if !msg.data.is_empty() {
                    return Err(CompositorError::InvalidMessage(format!(
                        "wl_buffer::destroy expects no payload, got {}",
                        msg.data.len()
                    )));
                }
                if let Some(client) = self.clients.write().await.get_mut(&client_id) {
                    client.remove_buffer(object_id);
                }
                if self.is_buffer_attached(object_id).await {
                    if let Some(buffer) = self.buffers.write().await.get_mut(&object_id) {
                        buffer.destroyed = true;
                    }
                    log::debug!(
                        "Buffer {} destroy deferred while attached for client {}",
                        object_id,
                        client_id
                    );
                } else {
                    self.buffers.write().await.remove(&object_id);
                    self.send_delete_id(client_id, object_id).await?;
                    log::debug!("Buffer {} destroyed by client {}", object_id, client_id);
                }
            }
            _ if compositor_object_id == Some(object_id) =>
            {
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
                if opcode == 0 {
                    let removed = self.surfaces.write().await.remove(&object_id);
                    let Some(mut surface) = removed else {
                        return Err(CompositorError::SurfaceNotFound(object_id));
                    };
                    let was_visible = surface.visible;
                    let previous_buffer_id = surface.buffer_object_id;
                    surface.detach_buffer();
                    surface.clear_damage();
                    if let Some(client) = self.clients.write().await.get_mut(&client_id) {
                        client.remove_surface(object_id);
                    }
                    if let Some(previous_buffer_id) = previous_buffer_id {
                        self.release_buffer_if_unused(previous_buffer_id).await?;
                    }
                    self.send_delete_id(client_id, object_id).await?;
                    needs_render = was_visible;
                    if needs_render {
                        log::debug!("Surface {} destroyed by client {}", object_id, client_id);
                    }
                } else if let Some(surface) = self.surfaces.write().await.get_mut(&object_id) {
                    match opcode {
                            1 => {
                                // attach
                                let mut parser = MessageParser::new(&msg.data);
                                let previous_buffer_id = surface.buffer_object_id;
                                if let Some(buffer_id) = parser.read_u32() {
                                    let Some(offset_x) = parser.read_i32() else {
                                        return Err(CompositorError::InvalidMessage(
                                            "wl_surface::attach missing x offset".to_string(),
                                        ));
                                    };
                                    let Some(offset_y) = parser.read_i32() else {
                                        return Err(CompositorError::InvalidMessage(
                                            "wl_surface::attach missing y offset".to_string(),
                                        ));
                                    };
                                    if offset_x != 0 || offset_y != 0 {
                                        return Err(CompositorError::InvalidMessage(format!(
                                            "non-zero wl_surface::attach offsets are unsupported: {}, {}",
                                            offset_x, offset_y
                                        )));
                                    }
                                    if buffer_id == 0 {
                                        surface.detach_buffer();
                                        surface.clear_damage();
                                    } else {
                                        let buffer = self.buffers.read().await.get(&buffer_id).cloned();
                                        if let Some(buffer) = buffer {
                                            if buffer.destroyed {
                                                return Err(CompositorError::InvalidMessage(format!(
                                                    "buffer {} is pending destruction",
                                                    buffer_id
                                                )));
                                            }
                                            surface.attach_buffer(
                                                buffer.data,
                                                buffer.width,
                                                buffer.height,
                                                buffer.stride,
                                                Some(buffer_id),
                                            );
                                            log::debug!(
                                                "Buffer {} attached to surface {}",
                                                buffer_id,
                                                object_id
                                            );
                                        } else {
                                            return Err(CompositorError::InvalidMessage(format!(
                                                "unknown buffer id {}",
                                                buffer_id
                                            )));
                                        }
                                    }
                                }
                                if parser.remaining() != 0 {
                                    return Err(CompositorError::InvalidMessage(format!(
                                        "wl_surface::attach expects 12 bytes, got {}",
                                        msg.data.len()
                                    )));
                                }
                                if let Some(previous_buffer_id) = previous_buffer_id {
                                    if previous_buffer_id != surface.buffer_object_id.unwrap_or(0) {
                                        self.release_buffer_if_unused(previous_buffer_id).await?;
                                    }
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
                                    if w < 0 || h < 0 {
                                        return Err(CompositorError::InvalidMessage(format!(
                                            "wl_surface::damage negative size is unsupported: {}x{}",
                                            w, h
                                        )));
                                    }
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

                                self.attach_fallback_buffer_if_needed(surface).await;

                                release_buffer_id = surface.buffer_object_id;
                                surface.clear_damage();
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

        if let Some(buffer_id) = release_buffer_id {
            self.send_buffer_release(client_id, buffer_id).await?;
            if let Some(surface) = self.surfaces.write().await.get_mut(&object_id) {
                if surface.buffer_object_id == Some(buffer_id) {
                    surface.buffer_object_id = None;
                }
            }
            self.release_buffer_if_unused(buffer_id).await?;
        }

        Ok(())
    }

    /// Registry グローバルを送信
    async fn send_registry_globals(&self, client_id: u32, registry_id: u32) -> Result<()> {
        let compositor_msg = MessageBuilder::new(registry_id, 0)
            .push_u32(WL_COMPOSITOR_GLOBAL_NAME)
            .push_string("wl_compositor")
            .push_u32(WL_COMPOSITOR_GLOBAL_VERSION)
            .build();
        let shm_msg = MessageBuilder::new(registry_id, 0)
            .push_u32(WL_SHM_GLOBAL_NAME)
            .push_string("wl_shm")
            .push_u32(WL_SHM_GLOBAL_VERSION)
            .build();

        self.send_message(client_id, &compositor_msg).await?;
        self.send_message(client_id, &shm_msg).await?;
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

    async fn send_callback_done(&self, client_id: u32, callback_id: u32) -> Result<()> {
        let msg = MessageBuilder::new(callback_id, 0)
            .push_u32(0)
            .build();
        self.send_message(client_id, &msg).await
    }

    async fn send_delete_id(&self, client_id: u32, object_id: u32) -> Result<()> {
        let msg = MessageBuilder::new(1, 1).push_u32(object_id).build();
        self.send_message(client_id, &msg).await
    }

    async fn send_buffer_release(&self, client_id: u32, buffer_id: u32) -> Result<()> {
        let msg = MessageBuilder::new(buffer_id, 0).build();
        self.send_message(client_id, &msg).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::memory::MemoryFramebufferBackend;
    use crate::backend::PixelFormat;
    use crate::client::Client;
    use crate::protocol::Message;
    use std::os::unix::net::UnixStream as StdUnixStream;
    use tokio::io::AsyncReadExt;
    use tokio::net::UnixStream;

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

    #[tokio::test]
    async fn test_registry_globals_are_sent_as_separate_events() {
        let backend = MemoryFramebufferBackend::new(64, 64, PixelFormat::XRGB8888);
        let compositor = Compositor::new(backend, "/tmp/test-compositor3.sock".to_string())
            .expect("Failed to create compositor");
        let (left, right) = StdUnixStream::pair().expect("pair");
        left.set_nonblocking(true).expect("nonblocking left");
        right.set_nonblocking(true).expect("nonblocking right");
        let left = UnixStream::from_std(left).expect("tokio left");
        let mut right = UnixStream::from_std(right).expect("tokio right");
        compositor
            .clients
            .write()
            .await
            .insert(1, Client::new(1, left));

        compositor.send_registry_globals(1, 42).await.expect("send globals");

        let mut buf = [0u8; 128];
        let n = right.read(&mut buf).await.expect("read globals");
        let (msg1, size1) = Message::from_bytes(&buf[..n]).expect("first global");
        let (msg2, _) = Message::from_bytes(&buf[size1..n]).expect("second global");

        assert_eq!(msg1.header.object_id, 42);
        assert_eq!(msg1.header.opcode, 0);
        assert_eq!(msg2.header.object_id, 42);
        assert_eq!(msg2.header.opcode, 0);
    }

    #[tokio::test]
    async fn test_sync_sends_done_and_delete_id() {
        let backend = MemoryFramebufferBackend::new(64, 64, PixelFormat::XRGB8888);
        let compositor = Compositor::new(backend, "/tmp/test-compositor-sync.sock".to_string())
            .expect("Failed to create compositor");
        let (left, right) = StdUnixStream::pair().expect("pair");
        left.set_nonblocking(true).expect("nonblocking left");
        right.set_nonblocking(true).expect("nonblocking right");
        let left = UnixStream::from_std(left).expect("tokio left");
        let mut right = UnixStream::from_std(right).expect("tokio right");

        compositor
            .clients
            .write()
            .await
            .insert(1, Client::new(1, left));

        let sync = MessageBuilder::new(1, 0).push_u32(7).build();
        compositor
            .process_client_messages(1, &sync.to_bytes())
            .await
            .expect("sync");

        let mut buf = [0u8; 64];
        let n = right.read(&mut buf).await.expect("read sync events");
        let (first, first_size) = Message::from_bytes(&buf[..n]).expect("first event");
        let (second, _) = Message::from_bytes(&buf[first_size..n]).expect("second event");
        assert_eq!(first.header.object_id, 7);
        assert_eq!(first.header.opcode, 0);
        assert_eq!(second.header.object_id, 1);
        assert_eq!(second.header.opcode, 1);
        assert_eq!(
            u32::from_le_bytes(second.data[..4].try_into().expect("payload")),
            7
        );
        assert_eq!(
            compositor
                .clients
                .read()
                .await
                .get(&1)
                .expect("client")
                .callback_count(),
            0
        );
    }

    #[tokio::test]
    async fn test_surface_destroy_removes_client_mapping() {
        let backend = MemoryFramebufferBackend::new(64, 64, PixelFormat::XRGB8888);
        let compositor = Compositor::new(backend, "/tmp/test-compositor4.sock".to_string())
            .expect("Failed to create compositor");
        let (left, right) = StdUnixStream::pair().expect("pair");
        left.set_nonblocking(true).expect("nonblocking left");
        right.set_nonblocking(true).expect("nonblocking right");
        let left = UnixStream::from_std(left).expect("tokio left");
        let mut right = UnixStream::from_std(right).expect("tokio right");

        let mut client = Client::new(1, left);
        client.add_surface(10, 10);
        compositor.clients.write().await.insert(1, client);
        compositor.surfaces.write().await.insert(10, Surface::new(10, 1));

        let destroy = MessageBuilder::new(10, 0).build();
        compositor
            .process_client_messages(1, &destroy.to_bytes())
            .await
            .expect("destroy surface");

        assert!(!compositor.surfaces.read().await.contains_key(&10));
        let clients = compositor.clients.read().await;
        assert!(
            !clients
                .get(&1)
                .expect("client")
                .surfaces
                .contains_key(&10)
        );
        drop(clients);

        let mut buf = [0u8; 32];
        let n = right.read(&mut buf).await.expect("read delete_id");
        let (msg, _) = Message::from_bytes(&buf[..n]).expect("delete_id event");
        assert_eq!(msg.header.object_id, 1);
        assert_eq!(msg.header.opcode, 1);
        assert_eq!(u32::from_le_bytes(msg.data[..4].try_into().expect("payload")), 10);
    }

    #[tokio::test]
    async fn test_registry_bind_rejects_duplicate_object_id() {
        let backend = MemoryFramebufferBackend::new(64, 64, PixelFormat::XRGB8888);
        let compositor = Compositor::new(backend, "/tmp/test-compositor5.sock".to_string())
            .expect("Failed to create compositor");
        let (left, _right) = StdUnixStream::pair().expect("pair");
        left.set_nonblocking(true).expect("nonblocking left");
        let left = UnixStream::from_std(left).expect("tokio left");

        let mut client = Client::new(1, left);
        client.registry_object_id = Some(2);
        client.compositor_object_id = Some(4);
        compositor.clients.write().await.insert(1, client);

        let bind = MessageBuilder::new(2, 0)
            .push_u32(2)
            .push_string("wl_shm")
            .push_u32(1)
            .push_u32(4)
            .build();
        let err = compositor
            .process_client_messages(1, &bind.to_bytes())
            .await
            .expect_err("duplicate object id must fail");
        assert!(matches!(err, CompositorError::InvalidMessage(_)));
    }

    #[tokio::test]
    async fn test_get_registry_rejects_duplicate_existing_object_id() {
        let backend = MemoryFramebufferBackend::new(64, 64, PixelFormat::XRGB8888);
        let compositor = Compositor::new(backend, "/tmp/test-compositor12.sock".to_string())
            .expect("Failed to create compositor");
        let (left, _right) = StdUnixStream::pair().expect("pair");
        left.set_nonblocking(true).expect("nonblocking left");
        let left = UnixStream::from_std(left).expect("tokio left");

        let mut client = Client::new(1, left);
        client.compositor_object_id = Some(4);
        compositor.clients.write().await.insert(1, client);

        let get_registry = MessageBuilder::new(1, 1).push_u32(4).build();
        let err = compositor
            .process_client_messages(1, &get_registry.to_bytes())
            .await
            .expect_err("duplicate registry id must fail");
        assert!(matches!(err, CompositorError::InvalidMessage(_)));
    }

    #[tokio::test]
    async fn test_legacy_get_registry_reuses_existing_registry_id() {
        let backend = MemoryFramebufferBackend::new(64, 64, PixelFormat::XRGB8888);
        let compositor = Compositor::new(backend, "/tmp/test-compositor13.sock".to_string())
            .expect("Failed to create compositor");
        let (left, right) = StdUnixStream::pair().expect("pair");
        left.set_nonblocking(true).expect("nonblocking left");
        right.set_nonblocking(true).expect("nonblocking right");
        let left = UnixStream::from_std(left).expect("tokio left");
        let mut right = UnixStream::from_std(right).expect("tokio right");

        let mut client = Client::new(1, left);
        client.registry_object_id = Some(9);
        compositor.clients.write().await.insert(1, client);

        let legacy = MessageBuilder::new(1, 0).build();
        compositor
            .process_client_messages(1, &legacy.to_bytes())
            .await
            .expect("legacy get_registry");

        let mut buf = [0u8; 128];
        let n = right.read(&mut buf).await.expect("read globals");
        let (msg1, size1) = Message::from_bytes(&buf[..n]).expect("first global");
        let (msg2, _) = Message::from_bytes(&buf[size1..n]).expect("second global");
        assert_eq!(msg1.header.object_id, 9);
        assert_eq!(msg2.header.object_id, 9);
        assert_eq!(
            compositor
                .clients
                .read()
                .await
                .get(&1)
                .expect("client")
                .registry_object_id,
            Some(9)
        );
    }

    #[tokio::test]
    async fn test_registry_bind_rejects_wrong_global_name() {
        let backend = MemoryFramebufferBackend::new(64, 64, PixelFormat::XRGB8888);
        let compositor = Compositor::new(backend, "/tmp/test-compositor10.sock".to_string())
            .expect("Failed to create compositor");
        let (left, _right) = StdUnixStream::pair().expect("pair");
        left.set_nonblocking(true).expect("nonblocking left");
        let left = UnixStream::from_std(left).expect("tokio left");

        let mut client = Client::new(1, left);
        client.registry_object_id = Some(2);
        compositor.clients.write().await.insert(1, client);

        let bind = MessageBuilder::new(2, 0)
            .push_u32(WL_SHM_GLOBAL_NAME)
            .push_string("wl_compositor")
            .push_u32(WL_COMPOSITOR_GLOBAL_VERSION)
            .push_u32(4)
            .build();
        let err = compositor
            .process_client_messages(1, &bind.to_bytes())
            .await
            .expect_err("wrong advertised name must fail");
        assert!(matches!(err, CompositorError::InvalidMessage(_)));
    }

    #[tokio::test]
    async fn test_registry_bind_rejects_unsupported_version() {
        let backend = MemoryFramebufferBackend::new(64, 64, PixelFormat::XRGB8888);
        let compositor = Compositor::new(backend, "/tmp/test-compositor11.sock".to_string())
            .expect("Failed to create compositor");
        let (left, _right) = StdUnixStream::pair().expect("pair");
        left.set_nonblocking(true).expect("nonblocking left");
        let left = UnixStream::from_std(left).expect("tokio left");

        let mut client = Client::new(1, left);
        client.registry_object_id = Some(2);
        compositor.clients.write().await.insert(1, client);

        let bind = MessageBuilder::new(2, 0)
            .push_u32(WL_SHM_GLOBAL_NAME)
            .push_string("wl_shm")
            .push_u32(WL_SHM_GLOBAL_VERSION + 1)
            .push_u32(3)
            .build();
        let err = compositor
            .process_client_messages(1, &bind.to_bytes())
            .await
            .expect_err("unsupported version must fail");
        assert!(matches!(err, CompositorError::InvalidMessage(_)));
    }

    #[tokio::test]
    async fn test_shm_create_buffer_rejects_invalid_stride() {
        let backend = MemoryFramebufferBackend::new(64, 64, PixelFormat::XRGB8888);
        let compositor = Compositor::new(backend, "/tmp/test-compositor6.sock".to_string())
            .expect("Failed to create compositor");
        let (left, _right) = StdUnixStream::pair().expect("pair");
        left.set_nonblocking(true).expect("nonblocking left");
        let left = UnixStream::from_std(left).expect("tokio left");

        let mut client = Client::new(1, left);
        client.shm_object_id = Some(3);
        compositor.clients.write().await.insert(1, client);

        let create = MessageBuilder::new(3, 0)
            .push_u32(6)
            .push_u32(320)
            .push_u32(240)
            .push_u32(320 * 3)
            .push_u32(0)
            .build();
        let err = compositor
            .process_client_messages(1, &create.to_bytes())
            .await
            .expect_err("invalid stride must fail");
        assert!(matches!(err, CompositorError::InvalidMessage(_)));
    }

    #[tokio::test]
    async fn test_attached_buffer_destroy_is_deferred_until_release() {
        let backend = MemoryFramebufferBackend::new(64, 64, PixelFormat::XRGB8888);
        let compositor = Compositor::new(backend, "/tmp/test-compositor7.sock".to_string())
            .expect("Failed to create compositor");
        let (left, right) = StdUnixStream::pair().expect("pair");
        left.set_nonblocking(true).expect("nonblocking left");
        right.set_nonblocking(true).expect("nonblocking right");
        let left = UnixStream::from_std(left).expect("tokio left");
        let mut right = UnixStream::from_std(right).expect("tokio right");

        let mut client = Client::new(1, left);
        client.shm_object_id = Some(3);
        client.add_surface(5, 5);
        client.add_buffer(6, 6);
        compositor.clients.write().await.insert(1, client);
        compositor.surfaces.write().await.insert(5, Surface::new(5, 1));
        compositor.buffers.write().await.insert(
            6,
            Buffer {
                object_id: 6,
                client_id: 1,
                width: 8,
                height: 8,
                stride: 32,
                format: 0,
                data: vec![0; 8 * 8 * 4],
                destroyed: false,
            },
        );

        let attach = MessageBuilder::new(5, 1)
            .push_u32(6)
            .push_i32(0)
            .push_i32(0)
            .build();
        compositor
            .process_client_messages(1, &attach.to_bytes())
            .await
            .expect("attach");

        let destroy = MessageBuilder::new(6, 0).build();
        compositor
            .process_client_messages(1, &destroy.to_bytes())
            .await
            .expect("destroy deferred");
        assert!(compositor.buffers.read().await.contains_key(&6));

        let commit = MessageBuilder::new(5, 4).build();
        compositor
            .process_client_messages(1, &commit.to_bytes())
            .await
            .expect("commit");
        assert!(!compositor.buffers.read().await.contains_key(&6));

        let mut buf = [0u8; 64];
        let n = right.read(&mut buf).await.expect("read release/delete_id");
        let (first, first_size) = Message::from_bytes(&buf[..n]).expect("first event");
        let (second, _) = Message::from_bytes(&buf[first_size..n]).expect("second event");
        assert_eq!(first.header.object_id, 6);
        assert_eq!(first.header.opcode, 0);
        assert_eq!(second.header.object_id, 1);
        assert_eq!(second.header.opcode, 1);
        assert_eq!(
            u32::from_le_bytes(second.data[..4].try_into().expect("payload")),
            6
        );
    }

    #[tokio::test]
    async fn test_attach_null_releases_destroyed_buffer() {
        let backend = MemoryFramebufferBackend::new(64, 64, PixelFormat::XRGB8888);
        let compositor = Compositor::new(backend, "/tmp/test-compositor8.sock".to_string())
            .expect("Failed to create compositor");
        let (left, _right) = StdUnixStream::pair().expect("pair");
        left.set_nonblocking(true).expect("nonblocking left");
        let left = UnixStream::from_std(left).expect("tokio left");

        let mut client = Client::new(1, left);
        client.add_surface(5, 5);
        client.add_buffer(6, 6);
        compositor.clients.write().await.insert(1, client);
        compositor.surfaces.write().await.insert(5, Surface::new(5, 1));
        compositor.buffers.write().await.insert(
            6,
            Buffer {
                object_id: 6,
                client_id: 1,
                width: 8,
                height: 8,
                stride: 32,
                format: 0,
                data: vec![0; 8 * 8 * 4],
                destroyed: false,
            },
        );

        let attach = MessageBuilder::new(5, 1)
            .push_u32(6)
            .push_i32(0)
            .push_i32(0)
            .build();
        compositor
            .process_client_messages(1, &attach.to_bytes())
            .await
            .expect("attach");

        let destroy = MessageBuilder::new(6, 0).build();
        compositor
            .process_client_messages(1, &destroy.to_bytes())
            .await
            .expect("destroy deferred");
        assert!(compositor.buffers.read().await.contains_key(&6));

        let detach = MessageBuilder::new(5, 1)
            .push_u32(0)
            .push_i32(0)
            .push_i32(0)
            .build();
        compositor
            .process_client_messages(1, &detach.to_bytes())
            .await
            .expect("attach null");
        assert!(!compositor.buffers.read().await.contains_key(&6));
    }

    #[tokio::test]
    async fn test_cleanup_releases_attached_destroyed_buffer() {
        let backend = MemoryFramebufferBackend::new(64, 64, PixelFormat::XRGB8888);
        let compositor = Compositor::new(backend, "/tmp/test-compositor9.sock".to_string())
            .expect("Failed to create compositor");
        let (left, _right) = StdUnixStream::pair().expect("pair");
        left.set_nonblocking(true).expect("nonblocking left");
        let left = UnixStream::from_std(left).expect("tokio left");

        let mut client = Client::new(1, left);
        client.add_surface(5, 5);
        client.add_buffer(6, 6);
        compositor.clients.write().await.insert(1, client);

        let mut surface = Surface::new(5, 1);
        surface.attach_buffer(vec![0; 8 * 8 * 4], 8, 8, 32, Some(6));
        compositor.surfaces.write().await.insert(5, surface);
        compositor.buffers.write().await.insert(
            6,
            Buffer {
                object_id: 6,
                client_id: 1,
                width: 8,
                height: 8,
                stride: 32,
                format: 0,
                data: vec![0; 8 * 8 * 4],
                destroyed: true,
            },
        );

        compositor.cleanup_client_state(1).await;

        assert!(!compositor.surfaces.read().await.contains_key(&5));
        assert!(!compositor.buffers.read().await.contains_key(&6));
        assert!(!compositor.clients.read().await.contains_key(&1));
    }

    #[tokio::test]
    async fn test_attach_rejects_non_zero_offsets() {
        let backend = MemoryFramebufferBackend::new(64, 64, PixelFormat::XRGB8888);
        let compositor = Compositor::new(backend, "/tmp/test-compositor14.sock".to_string())
            .expect("Failed to create compositor");
        let (left, _right) = StdUnixStream::pair().expect("pair");
        left.set_nonblocking(true).expect("nonblocking left");
        let left = UnixStream::from_std(left).expect("tokio left");

        let mut client = Client::new(1, left);
        client.add_surface(5, 5);
        client.add_buffer(6, 6);
        compositor.clients.write().await.insert(1, client);
        compositor.surfaces.write().await.insert(5, Surface::new(5, 1));
        compositor.buffers.write().await.insert(
            6,
            Buffer {
                object_id: 6,
                client_id: 1,
                width: 8,
                height: 8,
                stride: 32,
                format: 0,
                data: vec![0; 8 * 8 * 4],
                destroyed: false,
            },
        );

        let attach = MessageBuilder::new(5, 1)
            .push_u32(6)
            .push_i32(1)
            .push_i32(0)
            .build();
        let err = compositor
            .process_client_messages(1, &attach.to_bytes())
            .await
            .expect_err("non-zero offset must fail");
        assert!(matches!(err, CompositorError::InvalidMessage(_)));
    }

    #[tokio::test]
    async fn test_damage_rejects_negative_size() {
        let backend = MemoryFramebufferBackend::new(64, 64, PixelFormat::XRGB8888);
        let compositor = Compositor::new(backend, "/tmp/test-compositor15.sock".to_string())
            .expect("Failed to create compositor");
        let (left, _right) = StdUnixStream::pair().expect("pair");
        left.set_nonblocking(true).expect("nonblocking left");
        let left = UnixStream::from_std(left).expect("tokio left");

        let mut client = Client::new(1, left);
        client.add_surface(5, 5);
        compositor.clients.write().await.insert(1, client);
        compositor.surfaces.write().await.insert(5, Surface::new(5, 1));

        let damage = MessageBuilder::new(5, 2)
            .push_i32(0)
            .push_i32(0)
            .push_i32(-1)
            .push_i32(10)
            .build();
        let err = compositor
            .process_client_messages(1, &damage.to_bytes())
            .await
            .expect_err("negative damage size must fail");
        assert!(matches!(err, CompositorError::InvalidMessage(_)));
    }

    #[tokio::test]
    async fn test_attach_rejects_destroyed_buffer() {
        let backend = MemoryFramebufferBackend::new(64, 64, PixelFormat::XRGB8888);
        let compositor = Compositor::new(backend, "/tmp/test-compositor16.sock".to_string())
            .expect("Failed to create compositor");
        let (left, _right) = StdUnixStream::pair().expect("pair");
        left.set_nonblocking(true).expect("nonblocking left");
        let left = UnixStream::from_std(left).expect("tokio left");

        let mut client = Client::new(1, left);
        client.add_surface(5, 5);
        client.add_buffer(6, 6);
        compositor.clients.write().await.insert(1, client);
        compositor.surfaces.write().await.insert(5, Surface::new(5, 1));
        compositor.buffers.write().await.insert(
            6,
            Buffer {
                object_id: 6,
                client_id: 1,
                width: 8,
                height: 8,
                stride: 32,
                format: 0,
                data: vec![0; 8 * 8 * 4],
                destroyed: true,
            },
        );

        let attach = MessageBuilder::new(5, 1)
            .push_u32(6)
            .push_i32(0)
            .push_i32(0)
            .build();
        let err = compositor
            .process_client_messages(1, &attach.to_bytes())
            .await
            .expect_err("destroyed buffer must fail");
        assert!(matches!(err, CompositorError::InvalidMessage(_)));
    }

    #[tokio::test]
    async fn test_commit_clears_damage_after_render() {
        let backend = MemoryFramebufferBackend::new(64, 64, PixelFormat::XRGB8888);
        let compositor = Compositor::new(backend, "/tmp/test-compositor17.sock".to_string())
            .expect("Failed to create compositor");
        let (left, _right) = StdUnixStream::pair().expect("pair");
        left.set_nonblocking(true).expect("nonblocking left");
        let left = UnixStream::from_std(left).expect("tokio left");

        let mut client = Client::new(1, left);
        client.add_surface(5, 5);
        client.add_buffer(6, 6);
        compositor.clients.write().await.insert(1, client);
        compositor.surfaces.write().await.insert(5, Surface::new(5, 1));
        compositor.buffers.write().await.insert(
            6,
            Buffer {
                object_id: 6,
                client_id: 1,
                width: 8,
                height: 8,
                stride: 32,
                format: 0,
                data: vec![0; 8 * 8 * 4],
                destroyed: false,
            },
        );

        let attach = MessageBuilder::new(5, 1)
            .push_u32(6)
            .push_i32(0)
            .push_i32(0)
            .build();
        compositor
            .process_client_messages(1, &attach.to_bytes())
            .await
            .expect("attach");
        let damage = MessageBuilder::new(5, 2)
            .push_i32(1)
            .push_i32(2)
            .push_i32(8)
            .push_i32(8)
            .build();
        compositor
            .process_client_messages(1, &damage.to_bytes())
            .await
            .expect("damage");
        let commit = MessageBuilder::new(5, 4).build();
        compositor
            .process_client_messages(1, &commit.to_bytes())
            .await
            .expect("commit");

        let surface = compositor.surfaces.read().await;
        let surface = surface.get(&5).expect("surface");
        assert_eq!(surface.damage.width, 0);
        assert_eq!(surface.damage.height, 0);
    }
}
