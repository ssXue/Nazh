//! HITL 节点配置类型。

use serde::{Deserialize, Serialize};

/// 超时默认动作。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum DefaultAction {
    /// 注入表单默认值，工作流继续。
    AutoApprove,
    /// 发射 `ExecutionEvent::Failed`，工作流中断。
    #[default]
    AutoReject,
    /// 路由到指定节点（需 fallback output pin）。
    FallbackNode(String),
}

/// HITL 节点配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanLoopNodeConfig {
    /// 节点显示标题。
    #[serde(default)]
    pub title: Option<String>,
    /// 节点描述 / 审批说明。
    #[serde(default)]
    pub description: Option<String>,
    /// 结构化表单 schema。
    #[serde(default)]
    pub form_schema: Vec<super::form::FormSchemaField>,
    /// 审批独立超时（毫秒）。None = 无限等待。
    #[serde(default)]
    pub approval_timeout_ms: Option<u64>,
    /// 超时默认动作。
    #[serde(default)]
    pub default_action: DefaultAction,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn config_默认动作为_auto_reject() {
        let config: HumanLoopNodeConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(config.default_action, DefaultAction::AutoReject);
        assert!(config.title.is_none());
        assert!(config.approval_timeout_ms.is_none());
    }

    #[test]
    fn config_完整反序列化() {
        let json = r#"{
            "title": "液压确认",
            "description": "请确认液压操作",
            "form_schema": [
                {"type": "boolean", "name": "confirmed", "label": "确认", "required": true}
            ],
            "approval_timeout_ms": 30000,
            "default_action": "autoApprove"
        }"#;
        let config: HumanLoopNodeConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.title.as_deref(), Some("液压确认"));
        assert_eq!(config.form_schema.len(), 1);
        assert_eq!(config.approval_timeout_ms, Some(30000));
        assert_eq!(config.default_action, DefaultAction::AutoApprove);
    }
}
