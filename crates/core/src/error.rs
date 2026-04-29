//! 引擎全局错误类型。
//!
//! 所有可失败操作均返回 [`EngineError`]，引擎绝不 panic。

use thiserror::Error;
use uuid::Uuid;

use crate::data::DataId;
use crate::pin::{PinDirection, PinKind};

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

    #[error("阶段 `{stage}` 在 trace `{trace_id}` 中超时（{timeout_ms} ms）")]
    StageTimeout {
        stage: String,
        trace_id: Uuid,
        timeout_ms: u128,
    },

    #[error("阶段 `{stage}` 在 trace `{trace_id}` 中发生 panic")]
    StagePanicked { stage: String, trace_id: Uuid },

    #[error("阶段 `{stage}` 的通道已关闭")]
    ChannelClosed { stage: String },

    #[error("节点 `{node_id}` 配置无效: {message}")]
    NodeConfig { node_id: String, message: String },

    #[error("不支持的节点类型 `{0}`")]
    UnsupportedNodeType(String),

    #[error("节点 `{node_id}` 的脚本编译失败: {message}")]
    ScriptCompile { node_id: String, message: String },

    #[error("节点 `{node_id}` 的脚本运行时错误: {message}")]
    ScriptRuntime { node_id: String, message: String },

    #[error("节点 `{node_id}` 的 payload 转换失败: {message}")]
    PayloadConversion { node_id: String, message: String },

    #[error("连接 `{0}` 已存在")]
    ConnectionAlreadyExists(String),

    #[error("连接 `{0}` 不存在")]
    ConnectionNotFound(String),

    #[error("连接 `{0}` 已被借出")]
    ConnectionBusy(String),

    #[error("连接 `{connection_id}` 配置无效: {reason}")]
    ConnectionInvalidConfiguration {
        connection_id: String,
        reason: String,
    },

    #[error("连接 `{connection_id}` 已被限流，请在 {retry_after_ms} ms 后重试")]
    ConnectionRateLimited {
        connection_id: String,
        retry_after_ms: u64,
    },

    #[error("连接 `{connection_id}` 已熔断，请在 {retry_after_ms} ms 后重试: {reason}")]
    ConnectionCircuitOpen {
        connection_id: String,
        retry_after_ms: u64,
        reason: String,
    },

    #[error("尚未部署任何工作流")]
    WorkflowUnavailable,

    #[error("数据 `{0}` 在 DataStore 中不存在")]
    DataNotFound(DataId),

    #[error("DataStore 已达容量上限（{capacity} 条）")]
    DataStoreCapacityExceeded { capacity: usize },

    #[error("AI 节点 `{node_id}` 调用失败: {message}")]
    AiNodeError { node_id: String, message: String },

    /// `from` / `to` 形如 `"node_id.pin_id"`；合并字符串是为了把整个 Result 控制在
    /// `clippy::result_large_err` 阈值（128 字节）以内——结构化字段拆开会让最大变体
    /// 撑到 144 字节，触发 lint。诊断信息无损。
    #[error("边 `{from}` → `{to}` 类型不兼容：上游 `{from_type}`，下游期望 `{to_type}`")]
    IncompatiblePinTypes {
        from: String,
        to: String,
        from_type: String,
        to_type: String,
    },

    /// 边两端引脚的求值语义不一致——上游 Exec / 下游 Data 或反之。
    /// `from` / `to` 形如 `"node_id.pin_id"`。
    #[error(
        "边 `{from}` → `{to}` 求值语义不匹配：上游 `{from_kind}`，下游 `{to_kind}`（ADR-0014：引脚二分要求 Kind 一致）"
    )]
    IncompatiblePinKinds {
        from: String,
        to: String,
        from_kind: PinKind,
        to_kind: PinKind,
    },

    #[error("节点 `{node}` 不存在 {direction} 引脚 `{pin}`")]
    UnknownPin {
        node: String,
        pin: String,
        direction: PinDirection,
    },

    #[error("节点 `{node}` 声明了重复的 {direction} 引脚 `{pin}`")]
    DuplicatePinId {
        node: String,
        pin: String,
        direction: PinDirection,
    },

    #[error("工作流变量 `{name}` 不存在")]
    UnknownVariable { name: String },

    #[error("写入工作流变量 `{name}` 失败：声明类型 `{declared}` 与实际值类型 `{actual}` 不匹配")]
    VariableTypeMismatch {
        name: String,
        declared: String,
        actual: String,
    },

    #[error("工作流变量 `{name}` 初值类型不匹配：声明 `{declared}` / 初值实际 `{actual}`")]
    VariableInitialMismatch {
        name: String,
        declared: String,
        actual: String,
    },

    /// ADR-0014 Phase 3：被 Exec 触发的下游节点声明了 Data 输入引脚，但图中
    /// 找不到指向该 pin 的 Data 边。部署期 `pin_validator` 应已捕获 `required`
    /// 输入缺边——本错误用于运行时 Data 收集器对非 required Data 输入的兜底。
    #[error("节点 `{consumer}` 的 Data 输入引脚 `{pin}` 没有上游 Data 边")]
    DataPinUpstreamMissing { consumer: String, pin: String },

    /// ADR-0014 Phase 3：从上游节点的 Data 输出缓存槽读取时槽位为空——
    /// 上游节点尚未执行过 transform。Phase 3 直接拒绝；Phase 4 引入引脚级
    /// 兜底策略（`default_value` / `block_until_ready` / `skip`）后此错误
    /// 仅在 `block_until_ready` 超时时触发。
    #[error("上游节点 `{upstream}` 的 Data 输出引脚 `{pin}` 缓存为空（尚未执行）")]
    DataPinCacheEmpty { upstream: String, pin: String },

    /// ADR-0014 Phase 4：`BlockUntilReady` 等上游写槽位超时。
    #[error("拉取上游 `{upstream}` 引脚 `{pin}` 超时（{timeout_ms} ms）——上游可能未执行")]
    DataPinPullTimeout {
        upstream: String,
        pin: String,
        timeout_ms: u64,
    },

    /// ADR-0014 Phase 3b：Data 输入引脚使用了保留的 pin id。
    /// `"in"` 是混合输入节点 payload 合并约定中 Exec 主输入的固定键，
    /// Data 输入若也用 `"in"` 会覆盖 Exec payload，违反节点作者契约。
    #[error("节点 `{node}` 的引脚 `{pin}` 使用了保留 id：{reason}")]
    ReservedPinId {
        node: String,
        pin: String,
        reason: String,
    },
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

    pub fn script_compile(node_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::ScriptCompile {
            node_id: node_id.into(),
            message: message.into(),
        }
    }

    pub fn script_runtime(node_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::ScriptRuntime {
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

    pub fn ai_node_error(node_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::AiNodeError {
            node_id: node_id.into(),
            message: message.into(),
        }
    }

    pub fn unknown_variable(name: impl Into<String>) -> Self {
        Self::UnknownVariable { name: name.into() }
    }

    pub fn variable_type_mismatch(
        name: impl Into<String>,
        declared: impl Into<String>,
        actual: impl Into<String>,
    ) -> Self {
        Self::VariableTypeMismatch {
            name: name.into(),
            declared: declared.into(),
            actual: actual.into(),
        }
    }

    pub fn variable_initial_mismatch(
        name: impl Into<String>,
        declared: impl Into<String>,
        actual: impl Into<String>,
    ) -> Self {
        Self::VariableInitialMismatch {
            name: name.into(),
            declared: declared.into(),
            actual: actual.into(),
        }
    }
}
