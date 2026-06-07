use thiserror::Error;

#[derive(Error, Debug)]
pub enum CompositorError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Backend error: {0}")]
    Backend(String),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Surface not found: {0}")]
    SurfaceNotFound(u32),

    #[error("Client not found: {0}")]
    ClientNotFound(u32),

    #[error("Invalid message: {0}")]
    InvalidMessage(String),

    #[error("Lock failed: {0}")]
    LockFailed(String),

    #[error("Other error: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, CompositorError>;

impl From<String> for CompositorError {
    fn from(msg: String) -> Self {
        CompositorError::Other(msg)
    }
}

impl From<&str> for CompositorError {
    fn from(msg: &str) -> Self {
        CompositorError::Other(msg.to_string())
    }
}