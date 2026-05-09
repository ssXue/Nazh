//! 编译器核心：`WorkflowSpec` + `CompilerContext` → `WorkflowGraph` JSON。

mod builder;
mod guards;
mod json;

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
#[path = "tests.rs"]
mod tests;

use nazh_dsl_core::workflow::WorkflowSpec;
use serde_json::Value;

use crate::context::CompilerContext;
use crate::error::CompileError;
use crate::safety::{SafetyReport, run_safety_checks};
use crate::validate::{determine_initial_state, validate_workflow_spec};

use builder::GraphBuilder;
use guards::{validate_sanitized_ids, validate_supported_runtime_features};

#[cfg(test)]
use guards::sanitize_node_id;

/// 将 `WorkflowSpec` 编译为符合 `WorkflowGraph` serde 契约的 JSON。
///
/// 编译流程：
/// 1. 引用校验（设备/能力存在性）
/// 2. 语义校验（状态机约束）
/// 3. 收集所有唯一 action → 生成 capabilityCall 节点
/// 4. 生成 stateMachine 节点
/// 5. 生成边（stateMachine → capabilityCall）
/// 6. 生成变量（用户变量 + 内部状态跟踪变量）
///
/// # Errors
///
/// 引用缺失、语义校验失败或 JSON 构建错误时返回 [`CompileError`]。
pub fn compile(ctx: &CompilerContext, spec: &WorkflowSpec) -> Result<Value, CompileError> {
    ctx.validate_references(spec)?;
    validate_workflow_spec(spec)?;
    validate_supported_runtime_features(spec)?;
    validate_sanitized_ids(spec)?;
    let initial_state = determine_initial_state(spec)?;

    let mut builder = GraphBuilder::new(ctx, spec, &initial_state);
    builder.collect_actions();
    builder.build_state_machine_node();
    builder.build_capability_call_nodes()?;
    builder.build_edges();
    builder.build_variables();
    Ok(builder.build_output())
}

/// 编译 `WorkflowSpec` 并同时执行安全编译器校验（RFC-0004 Phase 5）。
///
/// 与 [`compile`] 相同的编译流程，额外在引用校验和语义校验成功后
/// 运行安全编译器 6 条规则。安全诊断通过 [`SafetyReport`] 暴露。
///
/// 安全错误（`DiagnosticLevel::Error`）阻止编译产出 `WorkflowGraph` JSON。
/// 安全警告（`DiagnosticLevel::Warning`）不阻止编译。
pub fn compile_with_safety(
    ctx: &CompilerContext,
    spec: &WorkflowSpec,
) -> Result<(Value, SafetyReport), CompileError> {
    ctx.validate_references(spec)?;
    validate_workflow_spec(spec)?;
    validate_supported_runtime_features(spec)?;
    validate_sanitized_ids(spec)?;
    let initial_state = determine_initial_state(spec)?;

    // 安全编译器校验
    let safety_report = run_safety_checks(ctx, spec, &initial_state);

    // 安全错误阻止编译
    if safety_report.has_errors() {
        return Err(CompileError::Safety {
            report: safety_report,
        });
    }

    // 继续正常编译
    let mut builder = GraphBuilder::new(ctx, spec, &initial_state);
    builder.collect_actions();
    builder.build_state_machine_node();
    builder.build_capability_call_nodes()?;
    builder.build_edges();
    builder.build_variables();
    Ok((builder.build_output(), safety_report))
}
