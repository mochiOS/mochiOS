use std::collections::{HashMap, HashSet};
use tokio::net::UnixStream;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Wayland クライアント
pub struct Client {
    pub id: u32,
    pub stream: Arc<Mutex<UnixStream>>,
    pub surfaces: HashMap<u32, u32>, // object_id surface_id
    pub registry_object_id: Option<u32>,
    pub compositor_object_id: Option<u32>,
    pub shm_object_id: Option<u32>,
    pub buffers: HashMap<u32, u32>, // buffer object_id -> pixel buffer id
    pub callbacks: HashSet<u32>,
}

impl Client {
    pub fn new(id: u32, stream: UnixStream) -> Self {
        Client {
            id,
            stream: Arc::new(Mutex::new(stream)),
            surfaces: HashMap::new(),
            registry_object_id: None,
            compositor_object_id: None,
            shm_object_id: None,
            buffers: HashMap::new(),
            callbacks: HashSet::new(),
        }
    }

    pub fn add_surface(&mut self, object_id: u32, surface_id: u32) {
        self.surfaces.insert(object_id, surface_id);
    }

    pub fn add_buffer(&mut self, object_id: u32, buffer_id: u32) {
        self.buffers.insert(object_id, buffer_id);
    }

    pub fn remove_buffer(&mut self, object_id: u32) -> Option<u32> {
        self.buffers.remove(&object_id)
    }

    pub fn remove_surface(&mut self, object_id: u32) -> Option<u32> {
        self.surfaces.remove(&object_id)
    }

    pub fn add_callback(&mut self, object_id: u32) -> bool {
        self.callbacks.insert(object_id)
    }

    pub fn remove_callback(&mut self, object_id: u32) -> bool {
        self.callbacks.remove(&object_id)
    }

    pub fn get_surface(&self, object_id: u32) -> Option<u32> {
        self.surfaces.get(&object_id).copied()
    }

    pub fn surface_count(&self) -> usize {
        self.surfaces.len()
    }

    pub fn buffer_count(&self) -> usize {
        self.buffers.len()
    }

    pub fn callback_count(&self) -> usize {
        self.callbacks.len()
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
        if let Some(_listener) = listener {
            fs::remove_file(test_socket_path).ok();
        }
    }

    #[tokio::test]
    async fn test_client_surface_tracking() {
        let (s1, _s2) = std::os::unix::net::UnixStream::pair().expect("pair");
        s1.set_nonblocking(true).expect("nonblocking");
        let s1 = UnixStream::from_std(s1).expect("tokio stream");
        let mut client = Client::new(1, s1);

        assert_eq!(client.surface_count(), 0);
        client.add_surface(10, 20);
        assert_eq!(client.surface_count(), 1);
        assert_eq!(client.get_surface(10), Some(20));
        assert_eq!(client.remove_surface(10), Some(20));
        assert_eq!(client.get_surface(10), None);
    }

    #[tokio::test]
    async fn test_client_callback_tracking() {
        let (s1, _s2) = std::os::unix::net::UnixStream::pair().expect("pair");
        s1.set_nonblocking(true).expect("nonblocking");
        let s1 = UnixStream::from_std(s1).expect("tokio stream");
        let mut client = Client::new(1, s1);

        assert_eq!(client.callback_count(), 0);
        assert!(client.add_callback(7));
        assert_eq!(client.callback_count(), 1);
        assert!(!client.add_callback(7));
        assert!(client.remove_callback(7));
        assert_eq!(client.callback_count(), 0);
    }
}
