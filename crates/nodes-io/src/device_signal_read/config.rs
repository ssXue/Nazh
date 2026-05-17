//! 设备信号读取节点配置类型（ADR-0024 Phase 1/3）。

use serde::{Deserialize, Serialize};

fn default_poll_timeout_ms() -> u64 {
    2000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceSignalReadConfig {
    #[serde(default)]
    pub connection_id: Option<String>,
    pub device_id: String,
    pub signal_id: String,
    pub source: crate::signal_decode::SignalSourceSnapshot,
    /// Rhai 缩放表达式（如 `"raw * 35 / 65535"`）。
    #[serde(default)]
    pub scale: Option<String>,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub simulation: bool,
    /// 同步源读取超时（毫秒），默认 2000。
    #[serde(default = "default_poll_timeout_ms")]
    pub poll_timeout_ms: u64,
}
