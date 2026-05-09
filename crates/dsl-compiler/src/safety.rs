//! 安全编译器——Workflow DSL 编译后安全校验（RFC-0004 Phase 5）。
//!
//! 在引用校验和语义校验通过后，对已编译的 `WorkflowSpec` 执行 6 条安全规则，
//! 产出结构化诊断（错误 + 警告），为工业场景提供部署前安全保障。

use std::collections::HashSet;

use nazh_dsl_core::workflow::{ActionSpec, ActionTarget, WorkflowSpec};

use crate::context::CompilerContext;

mod action_rules;
mod interlock;
mod preconditions;
mod report;
mod state_graph;
mod template;

#[cfg(test)]
use preconditions::extract_identifiers;
pub use report::{DiagnosticLevel, SafetyDiagnostic, SafetyReport};
#[cfg(test)]
use template::extract_variable_ref;

// ---- 公共入口 ----

/// 对已通过引用校验和语义校验的 `WorkflowSpec` 执行安全编译器校验。
///
/// 前置条件：`ctx.validate_references(spec)` 和 `validate_workflow_spec(spec)` 均已成功。
///
/// 返回 [`SafetyReport`]，包含所有诊断条目（错误 + 警告）。
/// 调用者应检查 `report.has_errors()` 决定是否继续编译。
pub fn run_safety_checks(
    ctx: &CompilerContext,
    spec: &WorkflowSpec,
    initial_state: &str,
) -> SafetyReport {
    let mut report = SafetyReport::default();

    action_rules::check_unit_consistency(ctx, spec, &mut report);
    action_rules::check_range_boundary(ctx, spec, &mut report);
    preconditions::check_precondition_reachability(ctx, spec, &mut report);
    state_graph::check_state_machine_completeness(spec, initial_state, &mut report);
    action_rules::check_dangerous_action_approval(ctx, spec, &mut report);
    interlock::check_mechanical_interlock(ctx, spec, &mut report);

    report
}

// ---- 共享 helper ----

/// 从 action 列表中提取 Capability ID。
fn collect_capability_ids(actions: &[ActionSpec], out: &mut HashSet<String>) {
    for action in actions {
        if let ActionTarget::Capability(id) = &action.target {
            out.insert(id.clone());
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[path = "safety/tests/mod.rs"]
mod tests;
