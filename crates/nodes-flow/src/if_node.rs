//! 布尔条件分支节点，基于 Rhai 脚本求值结果路由到 `"true"` 或 `"false"` 端口。

use ::rhai::serde::from_dynamic;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use serde_json::Value;
use uuid::Uuid;

use nazh_core::EngineError;
use nazh_core::{NodeExecution, NodeTrait};
use scripting::{RhaiNodeBase, default_max_operations};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IfNodeConfig {
    pub script: String,
    #[serde(default = "default_max_operations")]
    pub max_operations: u64,
}

/// 布尔条件分支节点，基于 [`RhaiNodeBase`] 实现。
pub struct IfNode {
    base: RhaiNodeBase,
}

impl IfNode {
    /// # Errors
    ///
    /// Rhai 脚本编译失败时返回 [`EngineError::RhaiCompile`]。
    #[allow(clippy::needless_pass_by_value)]
    pub fn new(
        id: impl Into<String>,
        config: IfNodeConfig,
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

#[async_trait]
impl NodeTrait for IfNode {
    scripting::delegate_node_base!("if");

    async fn transform(
        &self,
        _trace_id: Uuid,
        payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        let (scope, result) = self.base.evaluate(payload)?;
        let branch = from_dynamic::<bool>(&result).map_err(|error| {
            EngineError::payload_conversion(
                self.base.id().to_owned(),
                format!("If 节点脚本必须返回布尔值: {error}"),
            )
        })?;
        let new_payload = self.base.payload_from_scope(&scope)?;
        Ok(NodeExecution::route(
            new_payload,
            [if branch { "true" } else { "false" }],
        ))
    }
}
