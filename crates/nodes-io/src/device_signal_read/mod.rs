//! 设备信号读取节点（ADR-0024 Phase 1/3）。
//!
//! 按 [`SignalSourceSnapshot`](crate::signal_decode::SignalSourceSnapshot) 从设备读取原始数据，
//! 经 [`DataTypeSnapshot`](crate::signal_decode::DataTypeSnapshot) 解码、`scale` 缩放后输出语义化值。
//! 支持 `Register` / `CanFrame` / `Topic` / `SerialCommand` / `EthercatPdo` 五种信号源。
//!
//! 生命周期模型：poll 语义——exec 触发 + data 缓存（对标 `modbusRead`）。
//! 无 `on_deploy`/`on_undeploy`。

pub(crate) mod config;
pub(crate) mod reader;
pub(crate) mod reader_modbus;
pub(crate) mod reader_protocols;
#[cfg(test)]
mod tests;

pub use config::DeviceSignalReadConfig;
pub use reader::DeviceSignalReadNode;
