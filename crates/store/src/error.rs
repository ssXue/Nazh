//! Store 错误类型。

use std::fmt;

/// 本地存储层错误。
#[derive(Debug)]
pub enum StoreError {
    /// `SQLite` 操作失败。
    Rusqlite(rusqlite::Error),
    /// JSON 序列化/反序列化失败。
    SerdeJson(serde_json::Error),
    /// I/O 错误。
    Io(std::io::Error),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rusqlite(err) => write!(f, "SQLite 错误: {err}"),
            Self::SerdeJson(err) => write!(f, "JSON 错误: {err}"),
            Self::Io(err) => write!(f, "I/O 错误: {err}"),
        }
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Rusqlite(err) => Some(err),
            Self::SerdeJson(err) => Some(err),
            Self::Io(err) => Some(err),
        }
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(err: rusqlite::Error) -> Self {
        Self::Rusqlite(err)
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(err: serde_json::Error) -> Self {
        Self::SerdeJson(err)
    }
}

impl From<std::io::Error> for StoreError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}
