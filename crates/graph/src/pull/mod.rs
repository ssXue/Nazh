//! ADR-0014 Phase 3：Data 输入引脚的运行时拉路径。
//!
//! 当一个被 Exec 边触发的下游节点在 [`NodeTrait::input_pins`] 中声明了
//! [`PinKind::Data`](nazh_core::PinKind::Data) 引脚，本模块负责在 Runner 调用
//! `transform` **之前**：
//! 1. 反查每个 Data 输入引脚对应的上游边（[`EdgesByConsumer`]）
//! 2. 上游若为 pure-form 节点 → 递归求值
//! 3. 上游若为 Exec 节点（如 `modbusRead.latest`）→ 读取其 [`OutputCache`]
//! 4. 把收集到的 Data 值合并进 `transform` payload

mod collector;
mod index;
mod memo;

pub use collector::merge_payload;
pub(crate) use collector::pull_data_inputs;
pub(crate) use index::{EdgesByConsumer, build_edges_by_consumer};
pub(crate) use memo::PureMemo;
