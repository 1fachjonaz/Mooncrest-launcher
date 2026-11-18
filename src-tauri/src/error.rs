//! One error type for every command. It serializes to `{ code, message }` so the
//! UI can show a translated text for the code and the technical detail below it.

use serde::Serialize;

#[derive(Debug, Clone, thiserror::Error)]
pub enum AppError {
    #[error("network error: {0}")]
    Network(String),
    #[error("server answered HTTP {0}")]
    HttpStatus(u16),
    #[error("file not found on the server (HTTP 404)")]
    NotFound,
    #[error("download is not available yet: {0}")]
    NotAvailable(String),
    #[error("checksum mismatch: expected {expected}, got {actual}")]
    VerifyFailed { expected: String, actual: String },
    #[error("size mismatch: expected {expected} bytes, got {actual} bytes")]
    SizeMismatch { expected: u64, actual: u64 },
    #[error("not enough disk space: {needed} bytes needed, {free} bytes free")]
    DiskFull { needed: u64, free: u64 },
    #[error("{0}")]
    Io(String),
    #[error("cancelled")]
    Cancelled,
    #[error("installation failed: {0}")]
    InstallFailed(String),
    #[error("product is not installed")]
    NotInstalled,
    #[error("product is running – close it first")]
    Running,
    #[error("another task for this product is in progress")]
    Busy,
    #[error("invalid catalog: {0}")]
    InvalidManifest(String),
    #[error("not supported on this platform")]
    Unsupported,
    #[error("{0}")]
    Invalid(String),
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Network(_) => "network",
            AppError::HttpStatus(_) => "http_status",
            AppError::NotFound => "not_found",
            AppError::NotAvailable(_) => "not_available",
            AppError::VerifyFailed { .. } => "verify_failed",
            AppError::SizeMismatch { .. } => "size_mismatch",
            AppError::DiskFull { .. } => "disk_full",
            AppError::Io(_) => "io",
            AppError::Cancelled => "cancelled",
            AppError::InstallFailed(_) => "install_failed",
            AppError::NotInstalled => "not_installed",
            AppError::Running => "running",
            AppError::Busy => "busy",
            AppError::InvalidManifest(_) => "invalid_manifest",
            AppError::Unsupported => "unsupported",
            AppError::Invalid(_) => "invalid",
        }
    }

    /// Errors worth retrying automatically (connection drops, timeouts, 5xx, 408/429).
    pub fn is_transient(&self) -> bool {
        match self {
            AppError::Network(_) => true,
            AppError::HttpStatus(s) => *s >= 500 || *s == 408 || *s == 429,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq)]
pub struct ErrorInfo {
    pub code: String,
    pub message: String,
}

impl From<&AppError> for ErrorInfo {
    fn from(e: &AppError) -> Self {
        ErrorInfo { code: e.code().to_string(), message: e.to_string() }
    }
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        ErrorInfo::from(self).serialize(s)
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io(e.to_string())
    }
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        if let Some(s) = e.status() {
            if s.as_u16() == 404 {
                return AppError::NotFound;
            }
            return AppError::HttpStatus(s.as_u16());
        }
        AppError::Network(e.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Io(format!("invalid data: {e}"))
    }
}

pub type AppResult<T> = Result<T, AppError>;
