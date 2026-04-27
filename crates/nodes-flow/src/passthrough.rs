//! 子图桥接节点的 passthrough 实现——payload 直传，无副作用。

use async_trait::async_trait;
use nazh_core::{EngineError, NodeExecution, NodeTrait, WorkflowNodeDefinition};
use serde_json::Value;
use uuid::Uuid;

/// Passthrough 节点：将输入 payload 原样广播输出。
///
/// 用于展平后子图桥接节点（`subgraphInput` / `subgraphOutput`），
/// 在执行 DAG 中充当透传桥梁，不做任何变换或副作用。
pub struct PassthroughNode {
    id: String,
}

impl PassthroughNode {
    /// 从节点定义创建 [`PassthroughNode`] 实例。
    pub fn from_definition(definition: &WorkflowNodeDefinition) -> Result<Self, EngineError> {
        Ok(Self {
            id: definition.id().to_owned(),
        })
    }
}

#[async_trait]
impl NodeTrait for PassthroughNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> &'static str {
        "passthrough"
    }

    async fn transform(
        &self,
        _trace_id: Uuid,
        payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        Ok(NodeExecution::broadcast(payload))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_def() -> WorkflowNodeDefinition {
        WorkflowNodeDefinition::probe("passthrough", serde_json::Value::Null)
    }

    #[tokio::test]
    async fn passthrough_原样广播输入_payload() {
        let node = PassthroughNode::from_definition(&make_def()).unwrap();

        let input = json!({"temperature": 42.5, "unit": "C"});
        let result = node.transform(Uuid::new_v4(), input.clone()).await.unwrap();

        assert_eq!(result.outputs.len(), 1);
        assert_eq!(result.outputs[0].payload, input);
    }

    #[tokio::test]
    async fn passthrough_对空_payload_也能正常工作() {
        let node = PassthroughNode::from_definition(&make_def()).unwrap();

        let result = node
            .transform(Uuid::new_v4(), Value::Null)
            .await
            .unwrap();

        assert_eq!(result.outputs[0].payload, Value::Null);
    }

    #[test]
    fn passthrough_工厂返回正确的_kind() {
        let node = PassthroughNode::from_definition(&make_def()).unwrap();
        assert_eq!(node.kind(), "passthrough");
    }
}
