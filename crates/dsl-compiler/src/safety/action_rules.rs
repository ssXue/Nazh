//! Action 参数与审批规则。
//!
//! 规则覆盖单位一致性、量程边界和高风险动作审批提醒。

use nazh_dsl_core::capability::SafetyLevel;
use nazh_dsl_core::workflow::{ActionSpec, ActionTarget, WorkflowSpec};

use crate::context::CompilerContext;

use super::report::{SafetyReport, diag_error_with, diag_warning_with};
use super::template::{TemplateValue, classify_template};

/// 检查 action args 中的值单位是否与 Capability 输入参数声明的单位一致。
pub(super) fn check_unit_consistency(
    ctx: &CompilerContext,
    spec: &WorkflowSpec,
    report: &mut SafetyReport,
) {
    for (state_name, state) in &spec.states {
        for (i, action) in state.entry.iter().enumerate() {
            check_action_unit(ctx, action, Some(state_name), None, i, report);
        }
        for (i, action) in state.exit.iter().enumerate() {
            check_action_unit(ctx, action, Some(state_name), None, i, report);
        }
    }
    for (i, trans) in spec.transitions.iter().enumerate() {
        if let Some(action) = &trans.action {
            check_action_unit(ctx, action, None, Some(i), 0, report);
        }
    }
}

fn check_action_unit(
    ctx: &CompilerContext,
    action: &ActionSpec,
    state_name: Option<&str>,
    transition_index: Option<usize>,
    action_index: usize,
    report: &mut SafetyReport,
) {
    let cap_id = match &action.target {
        ActionTarget::Capability(id) => id,
        ActionTarget::Action(_) => return,
    };

    let Some(cap) = ctx.capabilities.get(cap_id) else {
        return;
    };

    for (arg_key, arg_value) in &action.args {
        let Some(param) = cap.inputs.iter().find(|p| p.id == *arg_key) else {
            continue;
        };

        let Some(param_unit) = &param.unit else {
            continue;
        };

        match classify_template(arg_value) {
            TemplateValue::Numeric(_) => {
                diag_warning_with(
                    report,
                    "unit_consistency",
                    format!(
                        "参数 `{arg_key}` 传入数值字面量，无法静态校验单位，请确认与能力输入单位 `{param_unit}` 一致"
                    ),
                    state_name,
                    transition_index,
                    Some(cap_id),
                    Some(action_index),
                );
            }
            TemplateValue::VariableRef(var_name) => {
                diag_warning_with(
                    report,
                    "unit_consistency",
                    format!(
                        "参数 `{arg_key}` 使用变量 `${{{var_name}}}` 的单位无法静态校验，请确认与能力输入单位 `{param_unit}` 一致"
                    ),
                    state_name,
                    transition_index,
                    Some(cap_id),
                    Some(action_index),
                );
            }
            TemplateValue::Other => {}
        }
    }
}

/// 检查 action args 中的值是否在 Capability 输入参数声明的量程范围内。
pub(super) fn check_range_boundary(
    ctx: &CompilerContext,
    spec: &WorkflowSpec,
    report: &mut SafetyReport,
) {
    for (state_name, state) in &spec.states {
        for (i, action) in state.entry.iter().enumerate() {
            check_action_range(ctx, spec, action, Some(state_name), None, i, report);
        }
        for (i, action) in state.exit.iter().enumerate() {
            check_action_range(ctx, spec, action, Some(state_name), None, i, report);
        }
    }
    for (i, trans) in spec.transitions.iter().enumerate() {
        if let Some(action) = &trans.action {
            check_action_range(ctx, spec, action, None, Some(i), 0, report);
        }
    }
}

fn check_action_range(
    ctx: &CompilerContext,
    spec: &WorkflowSpec,
    action: &ActionSpec,
    state_name: Option<&str>,
    transition_index: Option<usize>,
    action_index: usize,
    report: &mut SafetyReport,
) {
    let cap_id = match &action.target {
        ActionTarget::Capability(id) => id,
        ActionTarget::Action(_) => return,
    };

    let Some(cap) = ctx.capabilities.get(cap_id) else {
        return;
    };

    for (arg_key, arg_value) in &action.args {
        let Some(param) = cap.inputs.iter().find(|p| p.id == *arg_key) else {
            continue;
        };

        let Some(param_range) = &param.range else {
            continue;
        };

        match classify_template(arg_value) {
            TemplateValue::Numeric(num) => {
                if !param_range.contains(num) {
                    diag_error_with(
                        report,
                        "range_boundary",
                        format!(
                            "参数 `{arg_key}` 的值 {num} 超出能力输入量程 [{}, {}]",
                            param_range.min, param_range.max
                        ),
                        state_name,
                        transition_index,
                        Some(cap_id),
                        Some(action_index),
                    );
                }
            }
            TemplateValue::VariableRef(var_name) => {
                if let Some(initial) = spec.variables.get(&var_name) {
                    if let Some(num) = initial.as_f64() {
                        if !param_range.contains(num) {
                            diag_error_with(
                                report,
                                "range_boundary",
                                format!(
                                    "变量 `${{{var_name}}}` 的初始值 {num} 超出能力输入参数 `{arg_key}` 量程 [{}, {}]",
                                    param_range.min, param_range.max
                                ),
                                state_name,
                                transition_index,
                                Some(cap_id),
                                Some(action_index),
                            );
                        }
                    } else {
                        diag_warning_with(
                            report,
                            "range_boundary",
                            format!(
                                "变量 `${{{var_name}}}` 的初始值非数值，无法静态校验参数 `{arg_key}` 的量程 [{}, {}]",
                                param_range.min, param_range.max
                            ),
                            state_name,
                            transition_index,
                            Some(cap_id),
                            Some(action_index),
                        );
                    }
                } else {
                    diag_warning_with(
                        report,
                        "range_boundary",
                        format!(
                            "变量 `${{{var_name}}}` 未在 variables 中声明初始值，无法静态校验参数 `{arg_key}` 的量程 [{}, {}]",
                            param_range.min, param_range.max
                        ),
                        state_name,
                        transition_index,
                        Some(cap_id),
                        Some(action_index),
                    );
                }
            }
            TemplateValue::Other => {}
        }
    }
}

/// 检查 High 安全等级且 `requires_approval` 的能力是否被使用，发出警告提醒人工审批。
pub(super) fn check_dangerous_action_approval(
    ctx: &CompilerContext,
    spec: &WorkflowSpec,
    report: &mut SafetyReport,
) {
    for (state_name, state) in &spec.states {
        for (i, action) in state.entry.iter().enumerate() {
            check_action_approval(ctx, action, Some(state_name), None, i, report);
        }
        for (i, action) in state.exit.iter().enumerate() {
            check_action_approval(ctx, action, Some(state_name), None, i, report);
        }
    }
    for (i, trans) in spec.transitions.iter().enumerate() {
        if let Some(action) = &trans.action {
            check_action_approval(ctx, action, None, Some(i), 0, report);
        }
    }
}

fn check_action_approval(
    ctx: &CompilerContext,
    action: &ActionSpec,
    state_name: Option<&str>,
    transition_index: Option<usize>,
    action_index: usize,
    report: &mut SafetyReport,
) {
    let cap_id = match &action.target {
        ActionTarget::Capability(id) => id,
        ActionTarget::Action(_) => return,
    };

    let Some(cap) = ctx.capabilities.get(cap_id) else {
        return;
    };

    if cap.safety.level == SafetyLevel::High && cap.safety.requires_approval {
        let loc = state_name.unwrap_or("transition");
        diag_warning_with(
            report,
            "dangerous_action_approval",
            format!("能力 `{cap_id}` 安全等级为 High 且需人工审批，在状态 `{loc}` 中被使用"),
            state_name,
            transition_index,
            Some(cap_id),
            Some(action_index),
        );
    }
}
