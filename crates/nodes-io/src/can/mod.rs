//! CAN 总线节点支持。
//!
//! 借鉴 python-can 的 `BusABC` 设计，为所有后端提供统一的抽象接口。
//!
//! ## 后端矩阵
//!
//! | 后端 | 平台 | 状态 |
//! |------|------|------|
//! | `slcan` | Linux / Windows / macOS | **已实现** — 通过 USB 串口适配器 |
//! | `mock` | 全平台 | **已实现** — 模拟帧生成，用于测试 |
//! | `socketcan` | Linux | 计划 Phase 2 |
//!
//! ## SLCAN 协议
//!
//! SLCAN（Serial Line CAN）是 Lawicel CAN232/CANUSB 定义的文本协议，
//! 通过串口收发 ASCII 命令：
//!
//! - 发送标准帧：`t<ID 3位十六进制><DLC 1位><数据 2*DLC位>\r`
//! - 发送扩展帧：`T<ID 8位十六进制><DLC 1位><数据 2*DLC位>\r`
//! - 设置波特率：`S{代码}\r`（6 = 500kbps）
//! - 打开 CAN：`O\r`
//! - 关闭 CAN：`C\r`

pub mod filter;
pub mod frame;
pub mod hex;

mod backends;
mod can_read;
mod can_write;

use std::time::Duration;

pub use self::can_read::{CanReadNode, CanReadNodeConfig};
pub use self::can_write::{CanWriteNode, CanWriteNodeConfig};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub use self::backends::create_can_bus;
pub use self::filter::CanFilter;
pub use self::frame::CanFrame;

#[allow(dead_code)]
/// CAN 总线抽象 trait —— 借鉴 python-can `BusABC`。
///
/// 所有平台后端（SLCAN / `SocketCAN` / Mock）统一实现此接口。
#[async_trait]
pub trait CanBus: Send + Sync {
    /// 发送单帧。
    async fn send(&self, frame: &CanFrame) -> Result<(), CanError>;

    /// 接收单帧，超时返回 `None`。
    async fn recv(&self, timeout: Duration) -> Result<Option<CanFrame>, CanError>;

    /// 设置硬件/内核层接收过滤器。
    ///
    /// 若后端不支持硬件过滤，应在内部记录并在 `recv()` 时做软件过滤。
    fn set_filters(&self, filters: &[CanFilter]) -> Result<(), CanError>;

    /// 关闭总线，释放资源。可安全多次调用。
    fn shutdown(&self) -> Result<(), CanError>;

    /// 返回总线状态信息（用于 metadata）。
    fn channel_info(&self) -> String;

    /// 返回当前总线状态。
    fn state(&self) -> BusState;
}

/// CAN 总线状态。
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusState {
    /// 正常运行中。
    Active,
    /// 被动错误状态（错误计数器超限）。
    Passive,
    /// 总线关闭（错误计数器严重超限）。
    Error,
}

/// CAN 操作错误。
#[allow(dead_code)]
#[derive(Debug, Clone, thiserror::Error)]
pub enum CanError {
    #[error("不支持的接口类型: {0}")]
    UnsupportedInterface(String),
    #[error("打开 CAN 总线失败: {0}")]
    OpenFailed(String),
    #[error("发送失败: {0}")]
    SendFailed(String),
    #[error("接收失败: {0}")]
    RecvFailed(String),
    #[error("接收超时")]
    Timeout,
    #[error("过滤器配置失败: {0}")]
    FilterFailed(String),
    #[error("帧编码错误: {0}")]
    EncodeFailed(String),
    #[error("帧解码错误: {0}")]
    DecodeFailed(String),
    #[error("串口错误: {0}")]
    Serial(String),
    #[error("锁已被污染")]
    LockPoisoned,
    #[error("IO 错误: {0}")]
    Io(String),
}

/// CAN 总线配置 —— 从 `ConnectionDefinition::metadata` 解析。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanBusConfig {
    /// 后端接口类型: `"slcan"`, `"mock"`。
    #[serde(default = "default_interface")]
    pub interface: String,
    /// 通道标识: `"can0"`, `"/dev/ttyUSB0"`, `"COM3"`。
    pub channel: String,
    /// 串口波特率（bps），仅 SLCAN 使用。
    #[serde(default = "default_baud_rate")]
    pub baud_rate: u32,
    /// CAN 总线波特率（bps）。
    #[serde(default = "default_bitrate")]
    pub bitrate: u32,
    /// 接收过滤器列表。
    #[serde(default)]
    pub filters: Vec<CanFilter>,
    /// 是否启用 CAN-FD。
    #[serde(default)]
    pub fd: bool,
    /// 是否接收自身发送的帧。
    #[serde(default)]
    pub receive_own_messages: bool,
}

fn default_baud_rate() -> u32 {
    115_200
}

fn default_interface() -> String {
    "slcan".to_owned()
}

fn default_bitrate() -> u32 {
    500_000
}

impl CanBusConfig {
    /// 从 `serde_json::Value` 解析配置。
    pub fn from_metadata(metadata: &serde_json::Value) -> Result<Self, CanError> {
        serde_json::from_value(metadata.clone())
            .map_err(|e| CanError::OpenFailed(format!("CAN 配置解析失败: {e}")))
    }
}

/// 将 CAN 波特率映射到 SLCAN 波特率代码。
///
/// | 代码 | 波特率 |
/// |------|--------|
/// | 0 | 10 kbps |
/// | 1 | 20 kbps |
/// | 2 | 50 kbps |
/// | 3 | 100 kbps |
/// | 4 | 125 kbps |
/// | 5 | 250 kbps |
/// | 6 | **500 kbps** |
/// | 7 | 800 kbps |
/// | 8 | 1 Mbps |
pub fn slcan_bitrate_code(bitrate: u32) -> Option<char> {
    match bitrate {
        10_000 => Some('0'),
        20_000 => Some('1'),
        50_000 => Some('2'),
        100_000 => Some('3'),
        125_000 => Some('4'),
        250_000 => Some('5'),
        500_000 => Some('6'),
        800_000 => Some('7'),
        1_000_000 => Some('8'),
        _ => None,
    }
}

/// 校验仲裁 ID 是否落在所选帧格式允许范围内。
pub fn validate_can_id(can_id: u32, is_extended: bool) -> Result<(), CanError> {
    if is_extended {
        if can_id <= 0x1FFF_FFFF {
            return Ok(());
        }
        return Err(CanError::EncodeFailed(format!(
            "扩展帧 ID 0x{can_id:08X} 超过 29-bit 上限"
        )));
    }

    if can_id <= 0x7FF {
        return Ok(());
    }
    Err(CanError::EncodeFailed(format!(
        "标准帧 ID 0x{can_id:03X} 超过 11-bit 上限"
    )))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn 波特率代码映射() {
        assert_eq!(slcan_bitrate_code(500_000), Some('6'));
        assert_eq!(slcan_bitrate_code(250_000), Some('5'));
        assert_eq!(slcan_bitrate_code(1_000_000), Some('8'));
        assert_eq!(slcan_bitrate_code(123_456), None);
    }

    #[test]
    fn can_id_按帧格式校验范围() {
        assert!(validate_can_id(0x7FF, false).is_ok());
        assert!(validate_can_id(0x800, false).is_err());
        assert!(validate_can_id(0x1FFF_FFFF, true).is_ok());
        assert!(validate_can_id(0x2000_0000, true).is_err());
    }
}
