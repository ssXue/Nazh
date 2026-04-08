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
    #[error("pipeline configuration is invalid: {0}")]
    InvalidPipeline(String),

    #[error("workflow graph is invalid: {0}")]
    InvalidGraph(String),

    #[error("workflow graph JSON could not be parsed: {0}")]
    GraphDeserialization(String),

    #[error("stage `{stage}` failed for trace `{trace_id}`: {message}")]
    StageExecution {
        stage: String,
        trace_id: Uuid,
        message: String,
    },

    #[error("stage `{stage}` timed out for trace `{trace_id}` after {timeout_ms} ms")]
    StageTimeout {
        stage: String,
        trace_id: Uuid,
        timeout_ms: u128,
    },

    #[error("stage `{stage}` panicked for trace `{trace_id}`")]
    StagePanicked { stage: String, trace_id: Uuid },

    #[error("channel for stage `{stage}` is closed")]
    ChannelClosed { stage: String },

    #[error("node `{node_id}` has invalid configuration: {message}")]
    NodeConfig { node_id: String, message: String },

    #[error("node type `{0}` is not supported")]
    UnsupportedNodeType(String),

    #[error("rhai script for node `{node_id}` failed to compile: {message}")]
    RhaiCompile { node_id: String, message: String },

    #[error("rhai script for node `{node_id}` failed at runtime: {message}")]
    RhaiRuntime { node_id: String, message: String },

    #[error("payload conversion failed for node `{node_id}`: {message}")]
    PayloadConversion { node_id: String, message: String },

    #[error("connection `{0}` already exists")]
    ConnectionAlreadyExists(String),

    #[error("connection `{0}` does not exist")]
    ConnectionNotFound(String),

    #[error("connection `{0}` is already borrowed")]
    ConnectionBusy(String),

    #[error("no workflow has been deployed yet")]
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
