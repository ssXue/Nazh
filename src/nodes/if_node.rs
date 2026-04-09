//! 布尔条件分支节点，基于 Rhai 脚本求值结果路由到 `"true"` 或 `"false"` 端口。

use ::rhai::serde::from_dynamic;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::helpers::{default_max_operations, RhaiNodeBase};
use super::{NodeExecution, NodeTrait};
use crate::{EngineError, WorkflowContext};

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
            base: RhaiNodeBase::new(id, ai_description, &config.script, config.max_operations)?,
        })
    }
}

#[async_trait]
impl NodeTrait for IfNode {
    delegate_node_base!("if");

    async fn execute(&self, ctx: WorkflowContext) -> Result<NodeExecution, EngineError> {
        let (scope, result) = self.base.evaluate(&ctx)?;
        let branch = from_dynamic::<bool>(&result).map_err(|error| {
            EngineError::payload_conversion(
                self.base.id().to_owned(),
                format!("If 节点脚本必须返回布尔值: {error}"),
            )
        })?;
        let payload = self.base.payload_from_scope(&scope)?;

        Ok(NodeExecution::route(
            ctx.with_payload(payload),
            [if branch { "true" } else { "false" }],
        ))
    }
}
