use std::path::{Path, PathBuf};

/// Errors produced by NEXUS core. Messages are safe to show to the user: they never contain
/// secrets, environment values or file contents.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Unsupported(String),
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("database integrity problem: {0}")]
    Corrupt(String),
    #[error("{action} {path}: {source}")]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{what} is not valid JSON: {source}")]
    Json {
        what: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("{program}: {message}")]
    External { program: String, message: String },
}

impl CoreError {
    /// Stable machine-readable code for IPC consumers.
    pub fn code(&self) -> &'static str {
        match self {
            CoreError::NotFound(_) => "not_found",
            CoreError::Invalid(_) => "invalid",
            CoreError::Conflict(_) => "conflict",
            CoreError::Unsupported(_) => "unsupported",
            CoreError::Db(_) => "database",
            CoreError::Corrupt(_) => "corrupt",
            CoreError::Io { .. } => "io",
            CoreError::Json { .. } => "json",
            CoreError::External { .. } => "external",
        }
    }

    pub fn io(action: &'static str, path: impl AsRef<Path>, source: std::io::Error) -> Self {
        CoreError::Io {
            action,
            path: path.as_ref().to_path_buf(),
            source,
        }
    }

    pub fn invalid(msg: impl Into<String>) -> Self {
        CoreError::Invalid(msg.into())
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        CoreError::NotFound(msg.into())
    }
}

pub type Result<T> = std::result::Result<T, CoreError>;
