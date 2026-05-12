//! 沙箱化脚本节点。
//!
//! 用户编写的业务逻辑脚本在有界 Rhai 虚拟机中执行，
//! 脚本可修改 `payload` 变量或返回新值作为输出 payload。

use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use uuid::Uuid;

use serde::{Deserialize, Serialize};

use nazh_core::EngineError;
use nazh_core::{NodeExecution, NodeTrait, WorkflowVariables};
use scripting::{ScriptNodeBase, default_max_operations};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeNodeConfig {
    pub script: String,
    #[serde(default = "default_max_operations")]
    pub max_operations: u64,
}

/// 沙箱化脚本节点，基于 [`ScriptNodeBase`] 实现。
pub struct CodeNode {
    base: ScriptNodeBase,
}

impl CodeNode {
    /// # Errors
    ///
    /// 脚本编译失败时返回 [`EngineError::ScriptCompile`]。
    pub fn new(
        id: impl Into<String>,
        config: CodeNodeConfig,
        variables: Option<Arc<WorkflowVariables>>,
    ) -> Result<Self, EngineError> {
        let id = id.into();
        let CodeNodeConfig {
            script,
            max_operations,
        } = config;

        Ok(Self {
            base: ScriptNodeBase::new(id, &script, max_operations, variables)?,
        })
    }
}

#[async_trait]
impl NodeTrait for CodeNode {
    scripting::delegate_node_base!("code");

    async fn transform(
        &self,
        _trace_id: Uuid,
        payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        let (scope, result) = self.base.evaluate(payload)?;
        let new_payload = if result.is_unit() {
            self.base.payload_from_scope(&scope)?
        } else {
            self.base.dynamic_to_value(&result)?
        };
        Ok(NodeExecution::broadcast(new_payload))
    }
}
