use std::collections::HashMap;
use tokio::net::UnixStream;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Wayland クライアント
pub struct Client {
    pub id: u32,
    pub stream: Arc<Mutex<UnixStream>>,
    pub surfaces: HashMap<u32, u32>, // object_id surface_id
}

impl Client {
    pub fn new(id: u32, stream: UnixStream) -> Self {
        Client {
            id,
            stream: Arc::new(Mutex::new(stream)),
            surfaces: HashMap::new(),
        }
    }

    pub fn add_surface(&mut self, object_id: u32, surface_id: u32) {
        self.surfaces.insert(object_id, surface_id);
    }

    pub fn remove_surface(&mut self, object_id: u32) -> Option<u32> {
        self.surfaces.remove(&object_id)
    }

    pub fn get_surface(&self, object_id: u32) -> Option<u32> {
        self.surfaces.get(&object_id).copied()
    }

    pub fn surface_count(&self) -> usize {
        self.surfaces.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::UnixListener;
    use std::fs;
    use std::path::Path;

    #[tokio::test]
    async fn test_client_creation() {
        let test_socket_path = "/tmp/test-wayland-client.sock";
        if Path::new(test_socket_path).exists() {
            fs::remove_file(test_socket_path).ok();
        }

        let listener = UnixListener::bind(test_socket_path).ok();
        if let Ok(_listener) = listener {
            fs::remove_file(test_socket_path).ok();
        }
    }

    #[test]
    fn test_client_surface_tracking() {
    }
}