//! 异常捕获节点，脚本执行成功路由到 `"try"`，失败路由到 `"catch"`。
//!
//! 使用 [`RhaiNodeBase::evaluate_catching`] 捕获脚本错误而非直接传播，
//! 错误信息会写入 payload 的 `_error` 字段。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::helpers::{default_max_operations, into_payload_map, RhaiNodeBase};
use super::{NodeExecution, NodeTrait};
use crate::{ContextRef, DataStore, EngineError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TryCatchNodeConfig {
    pub script: String,
    #[serde(default = "default_max_operations")]
    pub max_operations: u64,
}

/// 异常捕获节点，基于 [`RhaiNodeBase`] 实现。
pub struct TryCatchNode {
    base: RhaiNodeBase,
}

impl TryCatchNode {
    /// # Errors
    ///
    /// Rhai 脚本编译失败时返回 [`EngineError::RhaiCompile`]。
    #[allow(clippy::needless_pass_by_value)]
    pub fn new(
        id: impl Into<String>,
        config: TryCatchNodeConfig,
        ai_description: impl Into<String>,
    ) -> Result<Self, EngineError> {
        Ok(Self {
            base: RhaiNodeBase::new(id, ai_description, &config.script, config.max_operations)?,
        })
    }
}

#[async_trait]
impl NodeTrait for TryCatchNode {
    delegate_node_base!("tryCatch");

    async fn execute(&self, ctx: &ContextRef, store: &dyn DataStore) -> Result<NodeExecution, EngineError> {
        let input_payload = store.read_mut(&ctx.data_id)?;
        let (scope, script_result) = self.base.evaluate_catching(&input_payload)?;
        match script_result {
            Ok(result) => {
                let payload = if result.is_unit() {
                    self.base.payload_from_scope(&scope)?
                } else {
                    self.base.dynamic_to_value(&result)?
                };
                Ok(NodeExecution::route(payload, ["try"]))
            }
            Err(error_message) => {
                let base_payload = self
                    .base
                    .payload_from_scope(&scope)
                    .unwrap_or(input_payload);
                let mut map = into_payload_map(base_payload);
                map.insert("_error".to_owned(), Value::String(error_message));
                Ok(NodeExecution::route(Value::Object(map), ["catch"]))
            }
        }
    }
}
