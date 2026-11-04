use nexus_core::CoreError;
use serde::Serialize;

/// Error returned to the webview: a stable code and a user-safe message (never secrets or stack traces).
#[derive(Debug, Serialize)]
pub struct IpcError {
    pub code: String,
    pub message: String,
}

impl IpcError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        IpcError {
            code: code.to_string(),
            message: message.into(),
        }
    }
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new("invalid", message)
    }
    pub fn terminal(message: impl Into<String>) -> Self {
        Self::new("terminal", message)
    }
}

impl From<CoreError> for IpcError {
    fn from(e: CoreError) -> Self {
        if matches!(e, CoreError::Db(_)) {
            tracing::error!(error = %e, "database error");
        }
        IpcError::new(e.code(), e.to_string())
    }
}

impl From<tauri::Error> for IpcError {
    fn from(e: tauri::Error) -> Self {
        IpcError::new("internal", e.to_string())
    }
}

pub type IpcResult<T> = Result<T, IpcError>;

/// Run blocking work (process spawns, filesystem scans) off the async runtime.
pub async fn blocking<T: Send + 'static>(f: impl FnOnce() -> IpcResult<T> + Send + 'static) -> IpcResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| IpcError::new("internal", format!("background task failed: {e}")))?
}
