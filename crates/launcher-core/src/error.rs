use std::path::PathBuf;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, LauncherError>;

#[derive(Debug, Error)]
pub enum LauncherError {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("JSON error at {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("validation failed: {0}")]
    Validation(String),

    #[error("plan not found: {0}")]
    PlanNotFound(String),

    #[error("item not found: {0}")]
    ItemNotFound(String),

    #[error("launch failed for {item_id}: {message}")]
    LaunchFailed { item_id: String, message: String },
}
