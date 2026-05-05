//! EtherCAT 主站节点支持。
#![allow(clippy::doc_markdown, clippy::large_futures)]
//!
//! 使用 `ethercrab` 纯 Rust 实现，通过共享会话模式支持多节点复用同一主站实例。
//!
//! ## 共享会话模型
//!
//! 同一 `connection_id` 的所有 ethercat* 节点共享一个主站实例，
//! 会话在首次 PDO 操作时初始化，生命周期跟随工程部署/撤销。
//!
//! ## 后端矩阵
//!
//! | 后端 | 平台 | 状态 |
//! |------|------|------|
//! | `ethercrab` | Linux / Windows / macOS | **已实现** |
//! | `mock` | 全平台 | **已实现** — 模拟从站，用于测试 |

mod backends;
mod pdo_read;
mod pdo_write;
mod session;
mod status;

pub use self::pdo_read::{EthercatPdoReadConfig, EthercatPdoReadNode};
pub use self::pdo_write::{EthercatPdoWriteConfig, EthercatPdoWriteNode};
pub use self::status::{EthercatStatusConfig, EthercatStatusNode};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// EtherCAT 从站状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlaveState {
    /// 从站地址（站地址）。
    pub address: u16,
    /// 从站名称（从 EEPROM 读取）。
    pub name: String,
    /// AL 状态码。
    pub al_status: u16,
    /// AL 状态描述。
    pub al_status_text: String,
    /// 是否在线。
    pub online: bool,
    /// 输入数据字节数。
    pub input_bytes: usize,
    /// 输出数据字节数。
    pub output_bytes: usize,
}

/// EtherCAT 总线抽象 trait —— 借鉴 CAN `CanBus` 设计。
///
/// 所有后端（ethercrab / mock）统一实现此接口。
#[async_trait]
pub trait EthercatBus: Send + Sync {
    /// 刷新 I/O 并读取指定从站的输入 PDO。
    async fn read_inputs(&self, slave_address: u16) -> Result<Vec<u8>, EthercatError>;

    /// 写入指定从站的输出 PDO 并立即触发一次 TX/RX，让数据上线。
    ///
    /// EtherCAT 是周期性总线协议：写本地输出缓冲后还需触发一次 `tx_rx` 才会真正发到从站。
    /// Nazh 当前没有全局周期 ticker（节点是事件驱动），因此每次写都顺带触发一次刷帧，
    /// 保证调用方不需关心刷新时机。
    async fn write_outputs(&self, slave_address: u16, data: &[u8]) -> Result<(), EthercatError>;

    /// 查询所有从站状态。
    fn get_slave_states(&self) -> Vec<SlaveState>;

    /// 释放主站会话引用。可安全多次调用。
    ///
    /// 注意：进程级 TX/RX 后台任务的生命周期与进程一致，本方法**不会**停止它。
    /// 工作流撤销时只是丢弃 `EthercatBus` 这个壳；实际的 socket / `MainDevice`
    /// 单例继续保活，等待下一次部署复用。
    fn shutdown(&self) -> Result<(), EthercatError>;

    /// 返回通道信息（用于 metadata）。
    fn channel_info(&self) -> String;
}

/// EtherCAT 操作错误。
#[allow(dead_code)]
#[derive(Debug, Clone, thiserror::Error)]
pub enum EthercatError {
    #[error("不支持的后端类型: {0}")]
    UnsupportedBackend(String),
    #[error("初始化失败: {0}")]
    InitFailed(String),
    #[error("从站 {address} 未找到")]
    SlaveNotFound { address: u16 },
    #[error("PDO 读取失败: {0}")]
    PdoReadFailed(String),
    #[error("PDO 写入失败: {0}")]
    PdoWriteFailed(String),
    #[error("数据长度不匹配: 期望 {expected} 字节，实际 {actual} 字节")]
    DataLengthMismatch { expected: usize, actual: usize },
    #[error("主站已关闭")]
    Closed,
    #[error("锁竞争")]
    LockContended,
    #[error("IO 错误: {0}")]
    Io(String),
}

/// EtherCAT 主站配置 —— 从 `ConnectionDefinition::metadata` 解析。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EthercatConfig {
    /// 后端类型: `"ethercrab"`, `"mock"`。
    #[serde(default = "default_backend")]
    pub backend: String,
    /// 网络接口名: `"eth0"`, `"enp0s31f6"`, `"\\Device\\NPF_{...}"`。
    #[serde(default)]
    pub interface: String,
    /// PDO 刷新周期（毫秒），默认 5ms。
    #[serde(default = "default_cycle_time_ms")]
    pub cycle_time_ms: u64,
    /// 进入 OP 状态等待超时（毫秒），默认 15s。
    #[serde(default = "default_op_timeout_ms")]
    pub op_timeout_ms: u64,
}

fn default_backend() -> String {
    "ethercrab".to_owned()
}

fn default_cycle_time_ms() -> u64 {
    5
}

fn default_op_timeout_ms() -> u64 {
    15_000
}

impl EthercatConfig {
    /// 从 `serde_json::Value` 解析配置。
    pub fn from_metadata(metadata: &serde_json::Value) -> Result<Self, EthercatError> {
        let mut config: Self = serde_json::from_value(metadata.clone())
            .map_err(|e| EthercatError::InitFailed(format!("EtherCAT 配置解析失败: {e}")))?;
        config.backend = config.backend.trim().to_ascii_lowercase();
        if config.backend.is_empty() {
            config.backend = default_backend();
        }
        config.interface = config.interface.trim().to_owned();
        if config.backend == "mock" && config.interface.is_empty() {
            "mock-eth0".clone_into(&mut config.interface);
        }
        if config.backend != "mock" && config.interface.is_empty() {
            return Err(EthercatError::InitFailed(
                "EtherCAT 配置缺少 interface（网络接口名）".to_owned(),
            ));
        }
        if config.cycle_time_ms == 0 {
            config.cycle_time_ms = default_cycle_time_ms();
        }
        if config.op_timeout_ms == 0 {
            config.op_timeout_ms = default_op_timeout_ms();
        }
        Ok(config)
    }
}
