//! `minutesSince` 节点：给定 RFC3339 时间戳字符串，返回当前距其分钟数。
//!
//! pure-form：单 Data String 输入 (`since`)，单 Data Integer 输出 (`out`)。
//! 时钟来源 [`chrono::Utc::now()`]——节点对系统时钟有显式依赖，但这是节点
//! 语义本身，并不构成"副作用"（无外部 IO、无 mutable state）。

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use nazh_core::{
    EmptyPolicy, EngineError, NodeExecution, NodeTrait, PinDefinition, PinDirection, PinKind,
    PinType,
};
use serde_json::Value;
use uuid::Uuid;

pub struct MinutesSinceNode {
    id: String,
}

impl MinutesSinceNode {
    pub fn new(id: String) -> Self {
        Self { id }
    }

    fn data_input() -> PinDefinition {
        PinDefinition {
            id: "since".to_owned(),
            label: "起点时间".to_owned(),
            pin_type: PinType::String,
            direction: PinDirection::Input,
            required: true,
            kind: PinKind::Data,
            description: Some("RFC3339 格式时间戳（如 `2026-04-28T08:00:00Z`）".to_owned()),
            empty_policy: EmptyPolicy::default(),
            block_timeout_ms: None,
            ttl_ms: None,
        }
    }

    fn data_output() -> PinDefinition {
        PinDefinition {
            id: "out".to_owned(),
            label: "距今分钟数".to_owned(),
            pin_type: PinType::Integer,
            direction: PinDirection::Output,
            required: false,
            kind: PinKind::Data,
            description: Some("`Utc::now() - since` 的分钟数（向下取整）".to_owned()),
            empty_policy: EmptyPolicy::default(),
            block_timeout_ms: None,
            ttl_ms: None,
        }
    }
}

#[async_trait]
impl NodeTrait for MinutesSinceNode {
    fn id(&self) -> &str {
        &self.id
    }
    fn kind(&self) -> &'static str {
        "minutesSince"
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
        let since_str = payload
            .get("since")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                EngineError::payload_conversion(
                    self.id.clone(),
                    "minutesSince 节点期望 payload.since 为 RFC3339 字符串",
                )
            })?;
        let since: DateTime<Utc> = since_str.parse().map_err(|e| {
            EngineError::payload_conversion(
                self.id.clone(),
                format!("minutesSince 解析时间戳失败：{e}"),
            )
        })?;
        let minutes = (Utc::now() - since).num_minutes();
        Ok(NodeExecution::broadcast(
            serde_json::json!({ "out": minutes }),
        ))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[tokio::test]
    async fn 起点为_5_分钟前返回_5_左右() {
        let node = MinutesSinceNode::new("ms_1".to_owned());
        let five_min_ago = (Utc::now() - Duration::minutes(5)).to_rfc3339();
        let result = node
            .transform(Uuid::nil(), serde_json::json!({ "since": five_min_ago }))
            .await
            .unwrap();
        let out = result.outputs[0]
            .payload
            .get("out")
            .unwrap()
            .as_i64()
            .unwrap();
        // 容忍 0~1 分钟漂移
        assert!((4..=5).contains(&out), "expected 4 or 5, got {out}");
    }

    #[tokio::test]
    async fn 非法_rfc3339_返回错误() {
        let node = MinutesSinceNode::new("ms_1".to_owned());
        let err = node
            .transform(Uuid::nil(), serde_json::json!({ "since": "not-a-date" }))
            .await
            .unwrap_err();
        assert!(matches!(err, EngineError::PayloadConversion { .. }));
    }

    #[tokio::test]
    async fn payload_缺_since_键返回错误() {
        let node = MinutesSinceNode::new("ms_1".to_owned());
        let err = node
            .transform(Uuid::nil(), serde_json::json!({}))
            .await
            .unwrap_err();
        assert!(matches!(err, EngineError::PayloadConversion { .. }));
    }

    #[test]
    fn minutes_since_是_pure_form() {
        let node = MinutesSinceNode::new("ms_1".to_owned());
        assert!(nazh_core::is_pure_form(&node));
    }
}
