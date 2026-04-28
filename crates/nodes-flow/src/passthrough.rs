//! 子图桥接节点的 passthrough 实现——payload 直传，无副作用。

use async_trait::async_trait;
use nazh_core::{
    EngineError, NodeExecution, NodeTrait, PinDefinition, PinType, WorkflowNodeDefinition,
};
use serde_json::Value;
use uuid::Uuid;

#[derive(Clone, Copy)]
enum PassthroughKind {
    SubgraphInput,
    SubgraphOutput,
    Fallback,
}

impl PassthroughKind {
    fn from_node_type(node_type: &str) -> Self {
        match node_type {
            "subgraphInput" => Self::SubgraphInput,
            "subgraphOutput" => Self::SubgraphOutput,
            _ => Self::Fallback,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::SubgraphInput => "subgraphInput",
            Self::SubgraphOutput => "subgraphOutput",
            Self::Fallback => "passthrough",
        }
    }
}

/// Passthrough 节点：将输入 payload 原样广播输出。
///
/// 用于展平后子图桥接节点（`subgraphInput` / `subgraphOutput`），
/// 在执行 DAG 中充当透传桥梁，不做任何变换或副作用。
pub struct PassthroughNode {
    id: String,
    kind: PassthroughKind,
}

impl PassthroughNode {
    /// 从节点定义创建 [`PassthroughNode`] 实例。
    pub fn from_definition(definition: &WorkflowNodeDefinition) -> Result<Self, EngineError> {
        Ok(Self {
            id: definition.id().to_owned(),
            kind: PassthroughKind::from_node_type(definition.node_type()),
        })
    }
}

#[async_trait]
impl NodeTrait for PassthroughNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> &'static str {
        self.kind.as_str()
    }

    fn input_pins(&self) -> Vec<PinDefinition> {
        vec![PinDefinition::required_input(
            PinType::Json,
            "子图桥接节点接收上游 JSON payload。",
        )]
    }

    fn output_pins(&self) -> Vec<PinDefinition> {
        vec![PinDefinition::output(
            PinType::Json,
            "子图桥接节点原样输出 JSON payload。",
        )]
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

        let result = node.transform(Uuid::new_v4(), Value::Null).await.unwrap();

        assert_eq!(result.outputs[0].payload, Value::Null);
    }

    #[test]
    fn passthrough_工厂返回正确的_kind() {
        let node = PassthroughNode::from_definition(&make_def()).unwrap();
        assert_eq!(node.kind(), "passthrough");
    }

    #[test]
    fn 子图桥接_kind_跟随注册类型() {
        let input = WorkflowNodeDefinition::probe("subgraphInput", serde_json::Value::Null);
        let output = WorkflowNodeDefinition::probe("subgraphOutput", serde_json::Value::Null);

        assert_eq!(
            PassthroughNode::from_definition(&input).unwrap().kind(),
            "subgraphInput"
        );
        assert_eq!(
            PassthroughNode::from_definition(&output).unwrap().kind(),
            "subgraphOutput"
        );
    }

    #[test]
    fn passthrough_声明_json_exec_输入输出引脚() {
        let node = PassthroughNode::from_definition(&make_def()).unwrap();

        assert_eq!(node.input_pins()[0].pin_type, PinType::Json);
        assert_eq!(node.output_pins()[0].pin_type, PinType::Json);
    }
}
