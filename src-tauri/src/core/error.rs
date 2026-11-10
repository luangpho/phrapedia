//! Structured error returned by every Tauri command (serialized as `{ code, message, detail? }`).

use serde::Serialize;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidArgument,
    NotFound,
    Conflict,
    PathUnusable,
    InvalidFrontmatter,
    Io,
    Git,
    CliNotFound,
    CliFailed,
    Timeout,
    Aborted,
    Internal,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppError {
    pub code: ErrorCode,
    /// Japanese, safe to show in the UI.
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

pub type Result<T> = std::result::Result<T, AppError>;

impl AppError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        AppError { code, message: message.into(), detail: None }
    }
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidArgument, message)
    }
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, message)
    }
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Conflict, message)
    }
    pub fn path_unusable(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::PathUnusable, message)
    }
    pub fn io(message: impl Into<String>, err: impl fmt::Display) -> Self {
        Self::new(ErrorCode::Io, message).with_detail(err.to_string())
    }
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, message)
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(d) = &self.detail {
            write!(f, " ({d})")?;
        }
        Ok(())
    }
}

impl std::error::Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        if e.kind() == std::io::ErrorKind::NotFound {
            AppError::not_found("ファイルが見つかりません").with_detail(e.to_string())
        } else {
            AppError::io("ファイル操作に失敗しました", e)
        }
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::new(ErrorCode::Internal, "ローカル DB の操作に失敗しました").with_detail(e.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::new(ErrorCode::Internal, "JSON の処理に失敗しました").with_detail(e.to_string())
    }
}
