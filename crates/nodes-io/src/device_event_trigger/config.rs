//! 设备事件触发节点配置类型与连接校验工具。

use serde::{Deserialize, Serialize};
use serde_json::Value;

use nazh_core::EngineError;

use crate::signal_decode::SignalSourceSnapshot;

/// 单个信号的监听配置快照。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalListenerSnapshot {
    pub signal_id: String,
    pub source: SignalSourceSnapshot,
    #[serde(default)]
    pub scale: Option<String>,
    #[serde(default)]
    pub unit: Option<String>,
}

/// 默认 poll 间隔（毫秒）。
fn default_poll_interval_ms() -> u64 {
    1000
}

/// 设备事件触发节点配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceEventTriggerConfig {
    #[serde(default)]
    pub connection_id: Option<String>,
    pub device_id: String,
    pub signals: Vec<SignalListenerSnapshot>,
    #[serde(default)]
    pub simulation: bool,
    /// Modbus Register 轮询间隔（毫秒），默认 1000。
    #[serde(default = "default_poll_interval_ms")]
    pub poll_interval_ms: u64,
}

/// 预编译的信号监听项（signal 配置 + scale AST）。
pub(crate) struct CompiledSignal {
    pub(crate) listener: SignalListenerSnapshot,
    pub(crate) scale_ast: Option<rhai::AST>,
    pub(crate) engine: rhai::Engine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ListenerProtocol {
    Mqtt,
    Can,
    Modbus,
    Serial,
}

#[derive(Debug)]
pub(crate) struct ListenerConnectionPlan {
    pub(crate) mqtt_endpoint: Option<(String, u16)>,
}

pub(crate) fn normalize_connection_kind(kind: &str) -> String {
    kind.trim().to_ascii_lowercase()
}

pub(crate) fn ensure_connection_kind(
    connection_id: &str,
    actual: &str,
    allowed: &[&str],
) -> Result<(), EngineError> {
    let actual = normalize_connection_kind(actual);
    if allowed.iter().any(|kind| *kind == actual) {
        return Ok(());
    }
    Err(EngineError::ConnectionInvalidConfiguration {
        connection_id: connection_id.to_owned(),
        reason: format!(
            "deviceEventTrigger 监听协议与连接类型不匹配，当前 type=`{actual}`，期望: {}",
            allowed.join(", ")
        ),
    })
}

pub(crate) fn required_str(
    connection_id: &str,
    metadata: &Value,
    key: &str,
    label: &str,
) -> Result<String, EngineError> {
    metadata
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| EngineError::ConnectionInvalidConfiguration {
            connection_id: connection_id.to_owned(),
            reason: format!("{label} 连接需要配置 {key}"),
        })
}

pub(crate) fn required_u64(
    connection_id: &str,
    metadata: &Value,
    key: &str,
    label: &str,
) -> Result<u64, EngineError> {
    metadata
        .get(key)
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
        .ok_or_else(|| EngineError::ConnectionInvalidConfiguration {
            connection_id: connection_id.to_owned(),
            reason: format!("{label} 连接需要配置有效的 {key}"),
        })
}

pub(crate) fn required_u16(
    connection_id: &str,
    metadata: &Value,
    key: &str,
    label: &str,
) -> Result<u16, EngineError> {
    let value = required_u64(connection_id, metadata, key, label)?;
    u16::try_from(value).map_err(|_| EngineError::ConnectionInvalidConfiguration {
        connection_id: connection_id.to_owned(),
        reason: format!("{label} 连接 {key} 必须在 1-65535 之间"),
    })
}

pub(crate) fn required_u8(
    connection_id: &str,
    metadata: &Value,
    key: &str,
    label: &str,
) -> Result<u8, EngineError> {
    let value = required_u64(connection_id, metadata, key, label)?;
    u8::try_from(value).map_err(|_| EngineError::ConnectionInvalidConfiguration {
        connection_id: connection_id.to_owned(),
        reason: format!("{label} 连接 {key} 必须在 1-255 之间"),
    })
}
