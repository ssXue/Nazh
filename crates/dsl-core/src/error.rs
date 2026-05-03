//! DSL 解析与校验错误类型。

use thiserror::Error;

/// DSL 相关操作错误。
#[derive(Debug, Error)]
pub enum DslError {
    /// YAML 语法解析失败。
    #[error("YAML 解析失败: {0}")]
    YamlParse(String),

    /// DSL 语义校验失败。
    #[error("DSL 校验失败: {context} — {detail}")]
    Validation { context: String, detail: String },

    /// JSON 序列化失败。
    #[error("JSON 序列化失败: {0}")]
    JsonSerialization(String),
}

impl From<serde_yaml::Error> for DslError {
    fn from(err: serde_yaml::Error) -> Self {
        Self::YamlParse(err.to_string())
    }
}

impl From<serde_json::Error> for DslError {
    fn from(err: serde_json::Error) -> Self {
        Self::JsonSerialization(err.to_string())
    }
}
