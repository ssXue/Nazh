//! 多路分支节点，基于 Rhai 脚本返回值路由到对应分支端口。
//!
//! 脚本应返回与 [`SwitchBranchConfig::key`] 匹配的字符串；
//! 若返回空值或未匹配，则路由到 `default_branch`。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use serde_json::Value;
use uuid::Uuid;

use std::sync::Arc;

use nazh_core::EngineError;
use nazh_core::{
    NodeExecution, NodeTrait, PinDefinition, PinDirection, PinKind, PinType, WorkflowVariables,
};
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
    branches: Vec<SwitchBranchConfig>,
    default_branch: String,
}

impl SwitchNode {
    /// # Errors
    ///
    /// 脚本编译失败时返回 [`EngineError::ScriptCompile`]。
    #[allow(clippy::needless_pass_by_value)]
    pub fn new(
        id: impl Into<String>,
        config: SwitchNodeConfig,
        variables: Option<Arc<WorkflowVariables>>,
    ) -> Result<Self, EngineError> {
        let default_branch = if config.default_branch.trim().is_empty() {
            default_switch_branch()
        } else {
            config.default_branch
        };
        Ok(Self {
            base: ScriptNodeBase::new(id, &config.script, config.max_operations, None, variables)?,
            branches: config.branches,
            default_branch,
        })
    }
}

#[async_trait]
impl NodeTrait for SwitchNode {
    scripting::delegate_node_base!("switch");

    /// 动态 pin：根据用户配置的 `branches` + `default_branch` 生成端口列表。
    ///
    /// 这是 ADR-0010 把 `output_pins` 设计为 `&self` 实例方法的典型场景——
    /// 同一 `switch` 节点类型的不同实例端口数量与 id 完全不同，无法用 `'static`
    /// 表表达。
    fn output_pins(&self) -> Vec<PinDefinition> {
        // branches 中可能已含 default_branch（用户在 UI 显式列出）；先收集
        // 用户声明的，再补齐 default_branch（去重，避免 DuplicatePinId 报错）。
        let mut pins: Vec<PinDefinition> = self
            .branches
            .iter()
            .map(|branch| PinDefinition {
                id: branch.key.clone(),
                label: branch.label.clone().unwrap_or_else(|| branch.key.clone()),
                pin_type: PinType::Any,
                direction: PinDirection::Output,
                required: false,
                kind: PinKind::Exec,
                description: None,
            })
            .collect();

        if !pins.iter().any(|pin| pin.id == self.default_branch) {
            pins.push(PinDefinition {
                id: self.default_branch.clone(),
                label: format!("{}（默认）", self.default_branch),
                pin_type: PinType::Any,
                direction: PinDirection::Output,
                required: false,
                kind: PinKind::Exec,
                description: Some("脚本返回值未匹配任何分支 key 时路由到此".to_owned()),
            });
        }

        pins
    }

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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn build_node(branches: Vec<SwitchBranchConfig>, default_branch: &str) -> SwitchNode {
        SwitchNode::new(
            "switch-test",
            SwitchNodeConfig {
                script: "payload[\"k\"]".to_owned(),
                branches,
                default_branch: default_branch.to_owned(),
                max_operations: 50_000,
            },
            None,
        )
        .unwrap()
    }

    #[test]
    fn output_pins_包含全部分支并附加默认端口() {
        let node = build_node(
            vec![
                SwitchBranchConfig {
                    key: "high".to_owned(),
                    label: Some("High".to_owned()),
                },
                SwitchBranchConfig {
                    key: "low".to_owned(),
                    label: None,
                },
            ],
            "default",
        );
        let pins = node.output_pins();
        let ids: Vec<&str> = pins.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, vec!["high", "low", "default"]);
        // label 显式指定时透传，否则等于 id
        assert_eq!(pins[0].label, "High");
        assert_eq!(pins[1].label, "low");
    }

    #[test]
    fn output_pins_branches_为空时仅含默认端口() {
        let node = build_node(vec![], "default");
        let pins = node.output_pins();
        assert_eq!(pins.len(), 1);
        assert_eq!(pins[0].id, "default");
    }

    #[test]
    fn output_pins_用户在_branches_显式列出_default_时不重复() {
        // 防止 DuplicatePinId：default 已在用户 branches 里时不再追加。
        let node = build_node(
            vec![SwitchBranchConfig {
                key: "default".to_owned(),
                label: Some("默认".to_owned()),
            }],
            "default",
        );
        let pins = node.output_pins();
        assert_eq!(pins.len(), 1);
        assert_eq!(pins[0].id, "default");
        assert_eq!(pins[0].label, "默认"); // 用户 label 优先
    }
}
