//! 节点执行系统：统一的异步节点 Trait 与分发策略。
//!
//! 本模块定义了工作流节点的核心抽象 [`NodeTrait`]，以及节点输出的
//! 分发机制 [`NodeDispatch`]。具体节点实现分布在各 Ring 1 crate 中。
//!
//! ## 元数据与业务数据分离
//!
//! 节点通过 [`NodeOutput::metadata`] 返回执行元数据（协议参数、连接信息等），
//! 与业务 payload 在结构上完全分离。元数据通过 [`ExecutionEvent::Completed`]
//! 事件通道传递给前端，不进入 payload。

use async_trait::async_trait;
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::EngineError;

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
/// 包含变换后的 payload、执行元数据和分发策略。Runner 负责将 payload
/// 写入 [`DataStore`] 并生成 [`ContextRef`] 发往下游，元数据通过事件通道独立传递。
#[derive(Debug, Clone)]
pub struct NodeOutput {
    pub payload: Value,
    /// 节点执行元数据（如 `"timer"` → `{...}`），通过事件通道传递，不进入 payload。
    pub metadata: Map<String, Value>,
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
                metadata: Map::new(),
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
                metadata: Map::new(),
                dispatch: NodeDispatch::Route(ports.into_iter().map(Into::into).collect()),
            }],
        }
    }

    /// 从多条输出构造执行结果。
    pub fn from_outputs(outputs: Vec<NodeOutput>) -> Self {
        Self { outputs }
    }

    /// 为所有输出附加执行元数据（Builder 模式）。
    ///
    /// 元数据键使用不带下划线的名称（如 `"timer"`）。
    #[must_use]
    #[allow(clippy::needless_pass_by_value)]
    pub fn with_metadata(mut self, metadata: Map<String, Value>) -> Self {
        let last = self.outputs.len().saturating_sub(1);
        for (i, output) in self.outputs.iter_mut().enumerate() {
            if i == last {
                output.metadata = metadata;
                return self;
            }
            output.metadata.clone_from(&metadata);
        }
        self
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
/// ## transform 签名
///
/// 节点接收 `trace_id`（追踪标识）和 `payload`（业务数据），
/// 返回包含变换后 payload、执行元数据和分发策略的 [`NodeExecution`]。
/// Runner 负责从 [`DataStore`](crate::DataStore) 读取输入数据、调用 `transform`，
/// 将 payload 写入 `DataStore` 并分发到下游，元数据通过
/// [`ExecutionEvent::Completed`](crate::ExecutionEvent::Completed) 事件独立传递。
///
/// 节点不接触 `DataStore` —— 它是 `(trace_id, payload) → (payload, metadata)` 的纯变换。
#[async_trait]
pub trait NodeTrait: Send + Sync {
    /// 节点在工作流图中的唯一标识。
    fn id(&self) -> &str;
    /// 返回节点类型标识（如 `"native"`、`"rhai"`、`"timer"` 等）。
    fn kind(&self) -> &'static str;
    /// 供 LLM 代码生成使用的自然语言描述。
    fn ai_description(&self) -> &str;
    /// 执行节点逻辑：接收业务数据，返回变换后的 payload 与执行元数据。
    ///
    /// `payload` 由 Runner 从 `DataStore` 读出（`read_mut`，已是 owned 副本），
    /// 节点只需做变换。执行元数据（如连接信息、协议详情）通过
    /// [`NodeExecution::with_metadata`] 返回，与业务数据分离。
    async fn transform(&self, trace_id: Uuid, payload: Value)
    -> Result<NodeExecution, EngineError>;
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
