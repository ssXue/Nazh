//! 沙箱化 Rhai 脚本节点。
//!
//! 用户编写的业务逻辑脚本在有界 Rhai 虚拟机中执行，
//! 脚本可修改 `payload` 变量或返回新值作为输出 payload。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use nazh_core::EngineError;
use nazh_core::{NodeExecution, NodeTrait};
use scripting::{RhaiNodeBase, default_max_operations};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RhaiNodeConfig {
    pub script: String,
    #[serde(default = "default_max_operations")]
    pub max_operations: u64,
}

/// 沙箱化 Rhai 脚本节点，基于 [`RhaiNodeBase`] 实现。
pub struct RhaiNode {
    base: RhaiNodeBase,
}

impl RhaiNode {
    /// # Errors
    ///
    /// Rhai 脚本编译失败时返回 [`EngineError::RhaiCompile`]。
    #[allow(clippy::needless_pass_by_value)]
    pub fn new(
        id: impl Into<String>,
        config: RhaiNodeConfig,
        ai_description: impl Into<String>,
    ) -> Result<Self, EngineError> {
        Ok(Self {
            base: RhaiNodeBase::new(id, ai_description, &config.script, config.max_operations)?,
        })
    }
}

#[async_trait]
impl NodeTrait for RhaiNode {
    scripting::delegate_node_base!("rhai");

    async fn transform(
        &self,
        _trace_id: nazh_core::Uuid,
        payload: serde_json::Value,
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
