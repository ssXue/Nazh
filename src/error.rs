//! 引擎全局错误类型。
//!
//! 所有可失败操作均返回 [`EngineError`]，引擎绝不 panic。

use thiserror::Error;
use uuid::Uuid;

/// 覆盖引擎所有失败模式的结构化错误类型。
///
/// 每个变体都携带足够的上下文信息（节点 ID、trace ID、阶段名称），
/// 无需在调用点额外打日志即可完成问题诊断。
#[derive(Debug, Error)]
pub enum EngineError {
    #[error("流水线配置无效: {0}")]
    InvalidPipeline(String),

    #[error("工作流图无效: {0}")]
    InvalidGraph(String),

    #[error("工作流图 JSON 解析失败: {0}")]
    GraphDeserialization(String),

    #[error("阶段 `{stage}` 在 trace `{trace_id}` 中执行失败: {message}")]
    StageExecution {
        stage: String,
        trace_id: Uuid,
        message: String,
    },

    #[error("阶段 `{stage}` 在 trace `{trace_id}` 中 timed out（{timeout_ms} ms）")]
    StageTimeout {
        stage: String,
        trace_id: Uuid,
        timeout_ms: u128,
    },

    #[error("阶段 `{stage}` 在 trace `{trace_id}` 中 panicked")]
    StagePanicked { stage: String, trace_id: Uuid },

    #[error("阶段 `{stage}` 的通道已关闭")]
    ChannelClosed { stage: String },

    #[error("节点 `{node_id}` 配置无效: {message}")]
    NodeConfig { node_id: String, message: String },

    #[error("不支持的节点类型 `{0}`")]
    UnsupportedNodeType(String),

    #[error("节点 `{node_id}` 的 Rhai 脚本编译失败: {message}")]
    RhaiCompile { node_id: String, message: String },

    #[error("节点 `{node_id}` 的 Rhai 脚本运行时错误: {message}")]
    RhaiRuntime { node_id: String, message: String },

    #[error("节点 `{node_id}` 的 payload 转换失败: {message}")]
    PayloadConversion { node_id: String, message: String },

    #[error("连接 `{0}` 已存在")]
    ConnectionAlreadyExists(String),

    #[error("连接 `{0}` 不存在")]
    ConnectionNotFound(String),

    #[error("连接 `{0}` 已被借出")]
    ConnectionBusy(String),

    #[error("尚未部署任何工作流")]
    WorkflowUnavailable,
}

impl EngineError {
    pub fn invalid_pipeline(message: impl Into<String>) -> Self {
        Self::InvalidPipeline(message.into())
    }

    pub fn invalid_graph(message: impl Into<String>) -> Self {
        Self::InvalidGraph(message.into())
    }

    pub fn graph_deserialization(message: impl Into<String>) -> Self {
        Self::GraphDeserialization(message.into())
    }

    pub fn stage_execution(
        stage: impl Into<String>,
        trace_id: Uuid,
        message: impl Into<String>,
    ) -> Self {
        Self::StageExecution {
            stage: stage.into(),
            trace_id,
            message: message.into(),
        }
    }

    pub fn node_config(node_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::NodeConfig {
            node_id: node_id.into(),
            message: message.into(),
        }
    }

    pub fn unsupported_node_type(node_type: impl Into<String>) -> Self {
        Self::UnsupportedNodeType(node_type.into())
    }

    pub fn rhai_compile(node_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::RhaiCompile {
            node_id: node_id.into(),
            message: message.into(),
        }
    }

    pub fn rhai_runtime(node_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::RhaiRuntime {
            node_id: node_id.into(),
            message: message.into(),
        }
    }

    pub fn payload_conversion(node_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::PayloadConversion {
            node_id: node_id.into(),
            message: message.into(),
        }
    }
}
