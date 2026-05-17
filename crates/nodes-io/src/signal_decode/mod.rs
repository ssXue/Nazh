//! 设备信号共享解码模块。
//!
//! 提供协议无关的原始字节 → `DataType` 解码、`ByteOrder` 转换、bit 提取、
//! Rhai scale 表达式求值。供 `deviceSignalRead`、`deviceEventTrigger`
//! 及未来的 `deviceSignalWrite` 共用。
//!
//! 快照类型（`DataTypeSnapshot` / `ByteOrderSnapshot`）镜像
//! `dsl-core::DataType` / `dsl-core::ByteOrder`，独立定义以避免 Ring 1 对
//! DSL 层的直接依赖。conformance test 守护两者 serde 格式一致。

pub(crate) mod decode;
pub mod types;

#[cfg(test)]
mod tests;

pub use types::SignalSourceSnapshot;

pub(crate) use decode::{
    apply_scale_with_engine, compile_scale, create_scale_engine, decode_raw_bytes,
    decode_topic_payload, extract_pdo_bytes,
};
pub(crate) use types::{ByteOrderSnapshot, DataTypeSnapshot};
