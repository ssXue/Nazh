//! `c2f` 节点：摄氏转华氏。
//!
//! pure-form：单 Data Float 输入 (`value`)，单 Data Float 输出 (`out`)。
//! 公式：`out = value * 9.0 / 5.0 + 32.0`。

use async_trait::async_trait;
use nazh_core::{
    EmptyPolicy, EngineError, NodeExecution, NodeTrait, PinDefinition, PinDirection, PinKind,
    PinType,
};
use serde_json::Value;
use uuid::Uuid;

pub struct C2fNode {
    id: String,
}

impl C2fNode {
    pub fn new(id: String) -> Self {
        Self { id }
    }

    fn data_input() -> PinDefinition {
        PinDefinition {
            id: "value".to_owned(),
            label: "摄氏度".to_owned(),
            pin_type: PinType::Float,
            direction: PinDirection::Input,
            required: true,
            kind: PinKind::Data,
            description: Some("待转换的摄氏温度（Float）".to_owned()),
            empty_policy: EmptyPolicy::default(),
            block_timeout_ms: None,
            ttl_ms: None,
        }
    }

    fn data_output() -> PinDefinition {
        PinDefinition {
            id: "out".to_owned(),
            label: "华氏度".to_owned(),
            pin_type: PinType::Float,
            direction: PinDirection::Output,
            required: false,
            kind: PinKind::Data,
            description: Some("转换后的华氏温度（Float）".to_owned()),
            empty_policy: EmptyPolicy::default(),
            block_timeout_ms: None,
            ttl_ms: None,
        }
    }
}

#[async_trait]
impl NodeTrait for C2fNode {
    fn id(&self) -> &str {
        &self.id
    }
    fn kind(&self) -> &'static str {
        "c2f"
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
        // payload 由 Runner pull 收集器构造为 `{ "value": <Float> }`。
        let celsius = payload
            .get("value")
            .and_then(Value::as_f64)
            .ok_or_else(|| {
                EngineError::payload_conversion(
                    self.id.clone(),
                    "c2f 节点期望 payload.value 为 Float",
                )
            })?;
        let fahrenheit = celsius * 9.0 / 5.0 + 32.0;
        // 单 Data 输出节点，payload 用 `{ "out": ... }` 与 input 对偶
        Ok(NodeExecution::broadcast(
            serde_json::json!({ "out": fahrenheit }),
        ))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn 摄氏_0_转换为华氏_32() {
        let node = C2fNode::new("c2f_1".to_owned());
        let result = node
            .transform(Uuid::nil(), serde_json::json!({ "value": 0.0 }))
            .await
            .unwrap();
        let out = &result.outputs[0].payload;
        assert!((out.get("out").unwrap().as_f64().unwrap() - 32.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn 摄氏_100_转换为华氏_212() {
        let node = C2fNode::new("c2f_1".to_owned());
        let result = node
            .transform(Uuid::nil(), serde_json::json!({ "value": 100.0 }))
            .await
            .unwrap();
        let out = &result.outputs[0].payload;
        assert!((out.get("out").unwrap().as_f64().unwrap() - 212.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn payload_缺_value_键返回错误() {
        let node = C2fNode::new("c2f_1".to_owned());
        let err = node
            .transform(Uuid::nil(), serde_json::json!({}))
            .await
            .unwrap_err();
        assert!(matches!(err, EngineError::PayloadConversion { .. }));
    }

    #[test]
    fn c2f_是_pure_form() {
        let node = C2fNode::new("c2f_1".to_owned());
        assert!(nazh_core::is_pure_form(&node));
    }
}
