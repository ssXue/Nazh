//! 节点执行系统：统一的异步节点 Trait 与分发策略。
//!
//! 本模块定义了工作流节点的核心抽象 [`NodeTrait`]，以及节点输出的
//! 分发机制 [`NodeDispatch`]。具体节点实现分布在各 Ring 1 crate 中。

use async_trait::async_trait;
use serde_json::{Map, Value};

use crate::{ContextRef, DataStore, EngineError};

/// 节点输出的分发策略。
#[derive(Debug, Clone)]
pub enum NodeDispatch {
    /// 向所有下游节点广播。
    Broadcast,
    /// 按端口名称路由到特定下游。
    Route(Vec<String>),
}

/// 节点执行后产出的单条输出。
///
/// 包含变换后的 payload 和分发策略。Runner 负责将 payload 写入
/// [`DataStore`] 并生成 [`ContextRef`] 发往下游。
#[derive(Debug, Clone)]
pub struct NodeOutput {
    pub payload: Value,
    pub dispatch: NodeDispatch,
}

/// 节点执行结果，可包含多条输出（如循环节点为每个元素生成一条）。
#[derive(Debug, Clone)]
pub struct NodeExecution {
    pub outputs: Vec<NodeOutput>,
}

impl NodeExecution {
    /// 创建一条广播到所有下游的执行结果。
    pub fn broadcast(payload: Value) -> Self {
        Self {
            outputs: vec![NodeOutput {
                payload,
                dispatch: NodeDispatch::Broadcast,
            }],
        }
    }

    /// 创建一条按端口路由的执行结果。
    pub fn route<I, S>(payload: Value, ports: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            outputs: vec![NodeOutput {
                payload,
                dispatch: NodeDispatch::Route(ports.into_iter().map(Into::into).collect()),
            }],
        }
    }

    /// 从多条输出构造执行结果。
    pub fn from_outputs(outputs: Vec<NodeOutput>) -> Self {
        Self { outputs }
    }

    /// 获取第一条输出（如果存在）。
    pub fn first(&self) -> Option<&NodeOutput> {
        self.outputs.first()
    }
}

/// 所有工作流节点的统一异步 Trait。
///
/// 实现必须满足 `Send + Sync`，因为每个节点在独立的 Tokio 任务中运行。
/// 新节点类型只需实现此 Trait 即可接入工作流 DAG。
///
/// ## execute 签名
///
/// 节点接收 [`ContextRef`]（轻量引用）和 [`DataStore`]（数据面），
/// 通过 `store.read()` / `store.read_mut()` 访问 payload，
/// 返回包含变换后 payload 和分发策略的 [`NodeExecution`]。
/// Runner 负责将输出写入 [`DataStore`] 并生成下游 [`ContextRef`]。
#[async_trait]
pub trait NodeTrait: Send + Sync {
    /// 节点在工作流图中的唯一标识。
    fn id(&self) -> &str;
    /// 返回节点类型标识（如 `"native"`、`"rhai"`、`"timer"` 等）。
    fn kind(&self) -> &'static str;
    /// 供 LLM 代码生成使用的自然语言描述。
    fn ai_description(&self) -> &str;
    /// 从 [`DataStore`] 读取数据，执行节点逻辑，返回变换后的 payload。
    async fn execute(
        &self,
        ctx: &ContextRef,
        store: &dyn DataStore,
    ) -> Result<NodeExecution, EngineError>;
}

/// 将 JSON payload 转换为 Map，非对象值会被包装为 `{"value": ...}`。
pub fn into_payload_map(payload: Value) -> Map<String, Value> {
    match payload {
        Value::Object(map) => map,
        other => {
            let mut map = Map::new();
            map.insert("value".to_owned(), other);
            map
        }
    }
}

/// 为持有 `id` 和 `ai_description` 字段的非脚本节点实现 [`NodeTrait`] 元数据方法。
#[macro_export]
macro_rules! impl_node_meta {
    ($kind:expr) => {
        fn id(&self) -> &str {
            &self.id
        }
        fn kind(&self) -> &'static str {
            $kind
        }
        fn ai_description(&self) -> &str {
            &self.ai_description
        }
    };
}
