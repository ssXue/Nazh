//! 异常捕获节点，脚本执行成功路由到 `"try"`，失败路由到 `"catch"`。
//!
//! 使用 [`ScriptNodeBase::evaluate_catching`] 捕获脚本错误而非直接传播，
//! 错误信息会写入 payload 的 `_error` 字段。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use uuid::Uuid;

use std::sync::Arc;

use nazh_core::{
    EngineError, NodeExecution, NodeTrait, PinDefinition, PinDirection, PinType, WorkflowVariables,
    into_payload_map,
};
use scripting::{ScriptNodeBase, default_max_operations};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TryCatchNodeConfig {
    pub script: String,
    #[serde(default = "default_max_operations")]
    pub max_operations: u64,
}

/// 异常捕获节点，基于 [`ScriptNodeBase`] 实现。
pub struct TryCatchNode {
    base: ScriptNodeBase,
}

impl TryCatchNode {
    /// # Errors
    ///
    /// 脚本编译失败时返回 [`EngineError::ScriptCompile`]。
    #[allow(clippy::needless_pass_by_value)]
    pub fn new(
        id: impl Into<String>,
        config: TryCatchNodeConfig,
        variables: Option<Arc<WorkflowVariables>>,
    ) -> Result<Self, EngineError> {
        Ok(Self {
            base: ScriptNodeBase::new(id, &config.script, config.max_operations, None, variables)?,
        })
    }
}

#[async_trait]
impl NodeTrait for TryCatchNode {
    scripting::delegate_node_base!("tryCatch");

    fn output_pins(&self) -> Vec<PinDefinition> {
        vec![
            PinDefinition {
                id: "try".to_owned(),
                label: "try".to_owned(),
                pin_type: PinType::Any,
                direction: PinDirection::Output,
                required: false,
                description: Some("脚本执行成功时路由到此".to_owned()),
            },
            PinDefinition {
                id: "catch".to_owned(),
                label: "catch".to_owned(),
                pin_type: PinType::Any,
                direction: PinDirection::Output,
                required: false,
                description: Some("脚本抛出异常时路由到此（payload._error 含错误信息）".to_owned()),
            },
        ]
    }

    async fn transform(
        &self,
        _trace_id: Uuid,
        input_payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        let (scope, script_result) = self.base.evaluate_catching(input_payload.clone())?;
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
