//! Capability 前置条件可达性校验。

use std::collections::HashSet;

use nazh_dsl_core::device::{AccessMode, SignalSource, SignalType};
use nazh_dsl_core::workflow::{ActionTarget, WorkflowSpec};

use crate::context::CompilerContext;

use super::collect_capability_ids;
use super::report::{SafetyReport, diag_error_with, diag_warning_with};

/// 检查 Capability 的前置条件表达式中引用的信号是否存在于设备定义中且可读。
pub(super) fn check_precondition_reachability(
    ctx: &CompilerContext,
    spec: &WorkflowSpec,
    report: &mut SafetyReport,
) {
    let mut cap_ids: HashSet<String> = HashSet::new();
    for state in spec.states.values() {
        collect_capability_ids(&state.entry, &mut cap_ids);
        collect_capability_ids(&state.exit, &mut cap_ids);
    }
    for trans in &spec.transitions {
        if let Some(action) = &trans.action
            && let ActionTarget::Capability(id) = &action.target
        {
            cap_ids.insert(id.clone());
        }
    }

    for cap_id in &cap_ids {
        let Some(cap) = ctx.capabilities.get(cap_id) else {
            continue;
        };

        if cap.preconditions.is_empty() {
            continue;
        }

        let Some(device) = ctx.devices.get(&cap.device_id) else {
            continue;
        };

        let signal_names: HashSet<&str> = device.signals.iter().map(|s| s.id.as_str()).collect();

        for cond in &cap.preconditions {
            let identifiers = extract_identifiers(cond);
            for ident in identifiers {
                if is_reserved_word(&ident) || ident.parse::<f64>().is_ok() {
                    continue;
                }

                if signal_names.contains(ident.as_str()) {
                    let Some(signal) = device.signals.iter().find(|s| s.id == ident) else {
                        continue;
                    };
                    if !is_signal_readable(signal) {
                        diag_error_with(
                            report,
                            "precondition_reachability",
                            format!(
                                "能力 `{cap_id}` 的前置条件 `{cond}` 引用了信号 `{ident}`，但该信号不可读（类型为 {:?}）",
                                signal.signal_type
                            ),
                            None,
                            None,
                            Some(cap_id),
                            None,
                        );
                    }
                } else {
                    diag_warning_with(
                        report,
                        "precondition_reachability",
                        format!(
                            "能力 `{cap_id}` 的前置条件 `{cond}` 引用了标识符 `{ident}`，该标识符不在设备 `{}` 的信号列表中，可能是运行时变量",
                            device.id
                        ),
                        None,
                        None,
                        Some(cap_id),
                        None,
                    );
                }
            }
        }
    }
}

/// 从 Rhai 风格表达式中提取标识符。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn extract_identifiers(expr: &str) -> Vec<String> {
    expr.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|t| !t.is_empty())
        .filter(|t| !is_reserved_word(t))
        .filter(|t| t.parse::<f64>().is_err())
        .map(String::from)
        .collect()
}

/// Rhai 保留字和字面量关键字。
fn is_reserved_word(s: &str) -> bool {
    matches!(
        s,
        "true"
            | "false"
            | "let"
            | "if"
            | "else"
            | "and"
            | "or"
            | "not"
            | "while"
            | "loop"
            | "for"
            | "in"
            | "return"
            | "fn"
            | "import"
            | "export"
    )
}

/// 判断信号是否可读。
fn is_signal_readable(signal: &nazh_dsl_core::device::SignalSpec) -> bool {
    if matches!(
        signal.signal_type,
        SignalType::AnalogInput | SignalType::DigitalInput
    ) {
        return true;
    }
    if let SignalSource::Register { access, .. } = &signal.source {
        return matches!(access, AccessMode::Read | AccessMode::ReadWrite);
    }
    false
}
