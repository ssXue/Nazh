//! 设备事件触发节点（ADR-0024 Phase 2/3）。
//!
//! 通过 `on_deploy` 启动后台事件监听循环，归一化 MQTT `Topic` / CAN `CanFrame` / Modbus `Register` /
//! `SerialCommand` 事件为设备信号更新，经 `signal_decode` 解码、scale 求值后通过
//! `NodeHandle::emit` 推进 DAG。
//!
//! 生命周期模型：event 语义——`on_deploy` 后台循环，`LifecycleGuard` 管理清理。
//! `transform` 仅用于 simulation 模式下的单次模拟输出。

pub(crate) mod config;
pub(crate) mod node;
pub(crate) mod orchestrator;

#[cfg(feature = "io-can")]
pub(crate) mod can_loop;
#[cfg(feature = "io-modbus")]
pub(crate) mod modbus_loop;
#[cfg(feature = "io-mqtt")]
pub(crate) mod mqtt_loop;
#[cfg(feature = "io-serial")]
pub(crate) mod serial_loop;

#[cfg(test)]
mod tests;

pub use config::{DeviceEventTriggerConfig, SignalListenerSnapshot};
pub use node::DeviceEventTriggerNode;

// 重导出 CompiledSignal 供协议循环模块通过 super:: 引用。
pub(super) use config::CompiledSignal;
