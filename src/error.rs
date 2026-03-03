use thiserror::Error;

#[derive(Debug, Error)]
pub enum SyncError {
    #[error("Authentication failed: invalid token")]
    Unauthorized,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Path error: {0}")]
    Path(String),

    #[error("Server error: {0}")]
    Server(String),
}
