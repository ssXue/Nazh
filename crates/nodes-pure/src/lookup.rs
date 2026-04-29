//! `lookup` 节点：根据 key 在 config 携带的查找表中取 value。
//!
//! pure-form：单 Data Any 输入 (`key`)、单 Data Any 输出 (`out`)。
//! key stringify 规则：标量 (Bool/Integer/Float/String) 直接转字符串；
//! 其他类型拒绝。

use std::collections::HashMap;

use async_trait::async_trait;
use nazh_core::{
    EmptyPolicy, EngineError, NodeExecution, NodeTrait, PinDefinition, PinDirection, PinKind,
    PinType,
};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize)]
pub struct LookupNodeConfig {
    /// 查找表 — key 是字符串，value 任意 JSON。
    #[serde(default)]
    pub table: HashMap<String, Value>,
    /// 未命中时回退 — `None` 表示直接返回错误。
    #[serde(default)]
    pub default: Option<Value>,
}

pub struct LookupNode {
    id: String,
    config: LookupNodeConfig,
}

impl LookupNode {
    pub fn new(id: String, config: LookupNodeConfig) -> Self {
        Self { id, config }
    }

    fn data_input() -> PinDefinition {
        PinDefinition {
            id: "key".to_owned(),
            label: "查找键".to_owned(),
            pin_type: PinType::Any,
            direction: PinDirection::Input,
            required: true,
            kind: PinKind::Data,
            description: Some("标量值（Bool/Integer/Float/String）".to_owned()),
            empty_policy: EmptyPolicy::default(),
            block_timeout_ms: None,
            ttl_ms: None,
        }
    }

    fn data_output() -> PinDefinition {
        PinDefinition {
            id: "out".to_owned(),
            label: "查找结果".to_owned(),
            pin_type: PinType::Any,
            direction: PinDirection::Output,
            required: false,
            kind: PinKind::Data,
            description: Some("命中的 value，或 default（若配置）".to_owned()),
            empty_policy: EmptyPolicy::default(),
            block_timeout_ms: None,
            ttl_ms: None,
        }
    }

    fn stringify_key(value: &Value) -> Result<String, EngineError> {
        match value {
            Value::String(s) => Ok(s.clone()),
            Value::Bool(b) => Ok(b.to_string()),
            Value::Number(n) => Ok(n.to_string()),
            _ => Err(EngineError::payload_conversion(
                "lookup",
                format!("lookup key 必须是标量（Bool/Integer/Float/String），收到 {value:?}"),
            )),
        }
    }
}

#[async_trait]
impl NodeTrait for LookupNode {
    fn id(&self) -> &str {
        &self.id
    }
    fn kind(&self) -> &'static str {
        "lookup"
    }
    fn input_pins(&self) -> Vec<PinDefinition> {
        vec![Self::data_input()]
    }
    fn output_pins(&self) -> Vec<PinDefinition> {
        vec![Self::data_output()]
    }

    async fn transform(
        &self,
        _trace_id: Uuid,
        payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        let key_value = payload.get("key").ok_or_else(|| {
            EngineError::payload_conversion(
                self.id.clone(),
                "lookup 节点期望 payload.key 存在（由 pull collector 注入）",
            )
        })?;
        let key = Self::stringify_key(key_value)?;
        let value = self
            .config
            .table
            .get(&key)
            .cloned()
            .or_else(|| self.config.default.clone());
        match value {
            Some(v) => Ok(NodeExecution::broadcast(serde_json::json!({ "out": v }))),
            None => Err(EngineError::payload_conversion(
                self.id.clone(),
                format!("lookup 表未命中 key=`{key}` 且无 default 配置"),
            )),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn n(table: &serde_json::Value, default: Option<&Value>) -> LookupNode {
        let cfg: LookupNodeConfig = serde_json::from_value(serde_json::json!({
            "table": table,
            "default": default,
        }))
        .unwrap();
        LookupNode::new("lk".to_owned(), cfg)
    }

    #[tokio::test]
    async fn 命中字符串_key_返回对应_value() {
        let node = n(&serde_json::json!({"alpha": 1, "beta": 2}), None);
        let r = node
            .transform(Uuid::nil(), serde_json::json!({"key": "alpha"}))
            .await
            .unwrap();
        assert_eq!(r.outputs[0].payload, serde_json::json!({"out": 1}));
    }

    #[tokio::test]
    async fn 命中数字_key_自动_stringify() {
        let node = n(&serde_json::json!({"42": "answer"}), None);
        let r = node
            .transform(Uuid::nil(), serde_json::json!({"key": 42}))
            .await
            .unwrap();
        assert_eq!(r.outputs[0].payload, serde_json::json!({"out": "answer"}));
    }

    #[tokio::test]
    async fn 命中布尔_key_stringify_为_true_或_false() {
        let node = n(&serde_json::json!({"true": "yes", "false": "no"}), None);
        let r = node
            .transform(Uuid::nil(), serde_json::json!({"key": true}))
            .await
            .unwrap();
        assert_eq!(r.outputs[0].payload, serde_json::json!({"out": "yes"}));
    }

    #[tokio::test]
    async fn 未命中_有_default_返回_default() {
        let node = n(&serde_json::json!({}), Some(&serde_json::json!("fallback")));
        let r = node
            .transform(Uuid::nil(), serde_json::json!({"key": "missing"}))
            .await
            .unwrap();
        assert_eq!(r.outputs[0].payload, serde_json::json!({"out": "fallback"}));
    }

    #[tokio::test]
    async fn 未命中_无_default_返回错误() {
        let node = n(&serde_json::json!({}), None);
        let err = node
            .transform(Uuid::nil(), serde_json::json!({"key": "missing"}))
            .await
            .unwrap_err();
        assert!(matches!(err, EngineError::PayloadConversion { .. }));
    }

    #[tokio::test]
    async fn 复杂_key_直接报错() {
        let node = n(&serde_json::json!({}), None);
        let err = node
            .transform(Uuid::nil(), serde_json::json!({"key": [1, 2, 3]}))
            .await
            .unwrap_err();
        assert!(matches!(err, EngineError::PayloadConversion { .. }));
    }

    #[test]
    fn lookup_是_pure_form() {
        let node = n(&serde_json::json!({}), None);
        assert!(nazh_core::is_pure_form(&node));
    }
}
