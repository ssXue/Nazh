//! 循环迭代节点，基于 Rhai 脚本返回值逐项分发到 `"body"` 端口，
//! 迭代完成后向 `"done"` 端口发送完成信号。
//!
//! 脚本可返回整数（生成 N 次无 item 的迭代）或数组（逐项迭代）。

use ::rhai::{Array, Dynamic, serde::from_dynamic};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use nazh_core::{ContextRef, DataStore, EngineError};
use nazh_core::{NodeDispatch, NodeExecution, NodeOutput, NodeTrait, into_payload_map};
use scripting::{RhaiNodeBase, default_max_operations};

/// Loop 节点单次执行的最大迭代数量，防止恶意脚本导致 OOM。
const MAX_LOOP_ITERATIONS: usize = 10_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopNodeConfig {
    pub script: String,
    #[serde(default = "default_max_operations")]
    pub max_operations: u64,
}

/// 循环迭代节点，基于 [`RhaiNodeBase`] 实现。
pub struct LoopNode {
    base: RhaiNodeBase,
}

impl LoopNode {
    /// # Errors
    ///
    /// Rhai 脚本编译失败时返回 [`EngineError::RhaiCompile`]。
    #[allow(clippy::needless_pass_by_value)]
    pub fn new(
        id: impl Into<String>,
        config: LoopNodeConfig,
        ai_description: impl Into<String>,
    ) -> Result<Self, EngineError> {
        Ok(Self {
            base: RhaiNodeBase::new(
                id,
                ai_description,
                &config.script,
                config.max_operations,
                None,
            )?,
        })
    }
}

/// 将循环状态元数据写入 payload 的 `_loop` 字段。
fn with_loop_state(
    payload: Value,
    phase: &str,
    index: Option<usize>,
    count: usize,
    item: Option<Value>,
) -> Value {
    let mut payload_map = into_payload_map(payload);

    let mut loop_map = Map::new();
    loop_map.insert("phase".to_owned(), Value::String(phase.to_owned()));
    loop_map.insert("count".to_owned(), Value::from(count as u64));

    if let Some(index) = index {
        loop_map.insert("index".to_owned(), Value::from(index as u64));
    }

    if let Some(item) = item {
        loop_map.insert("item".to_owned(), item);
    }

    payload_map.insert("_loop".to_owned(), Value::Object(loop_map));
    Value::Object(payload_map)
}

/// 解析脚本返回值为迭代项列表：整数 → N 个 None，数组 → 逐项 Some(Value)。
fn collect_loop_items(node_id: &str, result: Dynamic) -> Result<Vec<Option<Value>>, EngineError> {
    if let Some(count) = result.clone().try_cast::<i64>() {
        let n = usize::try_from(count).map_err(|_| {
            EngineError::payload_conversion(
                node_id.to_owned(),
                "Loop 节点脚本必须返回非负整数或数组",
            )
        })?;
        if n > MAX_LOOP_ITERATIONS {
            return Err(EngineError::payload_conversion(
                node_id.to_owned(),
                format!("Loop 迭代次数 {n} 超过上限 {MAX_LOOP_ITERATIONS}"),
            ));
        }
        return Ok((0..n).map(|_| None).collect());
    }

    if let Some(count) = result.clone().try_cast::<u64>() {
        let n = usize::try_from(count).map_err(|_| {
            EngineError::payload_conversion(node_id.to_owned(), "Loop 迭代次数超出平台 usize 容量")
        })?;
        if n > MAX_LOOP_ITERATIONS {
            return Err(EngineError::payload_conversion(
                node_id.to_owned(),
                format!("Loop 迭代次数 {n} 超过上限 {MAX_LOOP_ITERATIONS}"),
            ));
        }
        return Ok((0..n).map(|_| None).collect());
    }

    if let Some(items) = result.try_cast::<Array>() {
        if items.len() > MAX_LOOP_ITERATIONS {
            return Err(EngineError::payload_conversion(
                node_id.to_owned(),
                format!(
                    "Loop 数组长度 {} 超过上限 {MAX_LOOP_ITERATIONS}",
                    items.len()
                ),
            ));
        }
        return items
            .into_iter()
            .map(|item| {
                from_dynamic::<Value>(&item).map(Some).map_err(|error| {
                    EngineError::payload_conversion(
                        node_id.to_owned(),
                        format!("Loop 节点迭代项无法转换为 JSON: {error}"),
                    )
                })
            })
            .collect();
    }

    Err(EngineError::payload_conversion(
        node_id.to_owned(),
        "Loop 节点脚本必须返回非负整数或数组",
    ))
}

#[async_trait]
impl NodeTrait for LoopNode {
    scripting::delegate_node_base!("loop");

    async fn execute(
        &self,
        ctx: &ContextRef,
        store: &dyn DataStore,
    ) -> Result<NodeExecution, EngineError> {
        let input_payload = store.read_mut(&ctx.data_id)?;
        let (scope, result) = self.base.evaluate(input_payload)?;
        let payload = self.base.payload_from_scope(&scope)?;
        let items = collect_loop_items(self.base.id(), result)?;
        let item_count = items.len();
        let mut outputs = Vec::with_capacity(item_count + 1);
        for (index, item) in items.into_iter().enumerate() {
            outputs.push(NodeOutput {
                payload: with_loop_state(payload.clone(), "body", Some(index), item_count, item),
                dispatch: NodeDispatch::Route(vec!["body".to_owned()]),
            });
        }
        outputs.push(NodeOutput {
            payload: with_loop_state(payload, "done", None, item_count, None),
            dispatch: NodeDispatch::Route(vec!["done".to_owned()]),
        });
        Ok(NodeExecution::from_outputs(outputs))
    }
}
