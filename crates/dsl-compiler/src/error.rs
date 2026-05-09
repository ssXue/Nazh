//! 编译器错误类型。

use thiserror::Error;

use crate::safety::SafetyReport;

/// Workflow DSL 编译过程中的错误。
#[derive(Debug, Error)]
pub enum CompileError {
    /// 引用校验失败——设备、能力等资产引用无法解析。
    #[error("引用校验失败: {detail}")]
    Reference { detail: String },

    /// 状态机语义校验失败——transition、timeout、状态定义等违反约束。
    #[error("状态机校验失败: {detail}")]
    StateMachine { detail: String },

    /// 能力调用编译失败——implementation 映射或参数解析错误。
    #[error("能力调用编译失败: {detail}")]
    CapabilityCall { detail: String },

    /// 安全编译器校验失败——保留完整结构化报告供调用方定位。
    #[error("安全编译器校验失败，详见 SafetyReport")]
    Safety { report: SafetyReport },

    /// JSON 输出构建失败——序列化或结构组装错误。
    #[error("JSON 输出构建失败: {detail}")]
    OutputBuild { detail: String },
}
