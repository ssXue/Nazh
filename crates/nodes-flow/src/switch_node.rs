//! 多路分支节点，基于 Rhai 脚本返回值路由到对应分支端口。
//!
//! 脚本应返回与 [`SwitchBranchConfig::key`] 匹配的字符串；
//! 若返回空值或未匹配，则路由到 `default_branch`。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use serde_json::Value;
use uuid::Uuid;

use nazh_core::EngineError;
use nazh_core::{NodeExecution, NodeTrait};
use scripting::{ScriptNodeBase, default_max_operations};

fn default_switch_branch() -> String {
    "default".to_owned()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SwitchBranchConfig {
    pub key: String,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwitchNodeConfig {
    pub script: String,
    #[serde(default)]
    pub branches: Vec<SwitchBranchConfig>,
    #[serde(default = "default_switch_branch")]
    pub default_branch: String,
    #[serde(default = "default_max_operations")]
    pub max_operations: u64,
}

/// 多路分支节点，基于 [`ScriptNodeBase`] 实现。
pub struct SwitchNode {
    base: ScriptNodeBase,
    default_branch: String,
}

impl SwitchNode {
    /// # Errors
    ///
    /// 脚本编译失败时返回 [`EngineError::ScriptCompile`]。
    #[allow(clippy::needless_pass_by_value)]
    pub fn new(id: impl Into<String>, config: SwitchNodeConfig) -> Result<Self, EngineError> {
        let default_branch = if config.default_branch.trim().is_empty() {
            default_switch_branch()
        } else {
            config.default_branch
        };
        Ok(Self {
            base: ScriptNodeBase::new(id, &config.script, config.max_operations, None)?,
            default_branch,
        })
    }
}

#[async_trait]
impl NodeTrait for SwitchNode {
    scripting::delegate_node_base!("switch");

    async fn transform(
        &self,
        _trace_id: Uuid,
        payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        let (scope, result) = self.base.evaluate(payload)?;
        let new_payload = self.base.payload_from_scope(&scope)?;
        let next_branch = if result.is_unit() {
            self.default_branch.clone()
        } else {
            let branch = result.to_string();
            let normalized = branch.trim();
            if normalized.is_empty() || normalized == "()" {
                self.default_branch.clone()
            } else {
                normalized.to_owned()
            }
        };
        Ok(NodeExecution::route(new_payload, [next_branch]))
    }
}
