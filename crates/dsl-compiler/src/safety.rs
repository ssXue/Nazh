//! 安全编译器——Workflow DSL 编译后安全校验（RFC-0004 Phase 5）。
//!
//! 在引用校验和语义校验通过后，对已编译的 `WorkflowSpec` 执行 6 条安全规则，
//! 产出结构化诊断（错误 + 警告），为工业场景提供部署前安全保障。

use std::collections::HashMap;
use std::collections::HashSet;

use nazh_dsl_core::capability::{CapabilityImpl, CapabilitySpec, SafetyLevel};
use nazh_dsl_core::device::{AccessMode, SignalSource, SignalType};
use nazh_dsl_core::workflow::{ActionSpec, ActionTarget, WorkflowSpec};
use serde_json::Value;

use crate::context::CompilerContext;

// ---- 诊断类型 ----

/// 安全编译器诊断严重级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticLevel {
    /// 错误：阻塞编译，必须修正。
    Error,
    /// 警告：不阻塞编译，但需要人工审查。
    Warning,
}

/// 安全编译器诊断条目。
#[derive(Debug, Clone)]
pub struct SafetyDiagnostic {
    /// 严重级别。
    pub level: DiagnosticLevel,
    /// 诊断规则标识（如 `unit_consistency`、`range_boundary`）。
    pub rule: String,
    /// 人类可读消息（中文）。
    pub message: String,
    /// 位置上下文：状态名（如有）。
    pub state_name: Option<String>,
    /// 位置上下文：transition 索引（如有）。
    pub transition_index: Option<usize>,
    /// 位置上下文：能力 ID（如有）。
    pub capability_id: Option<String>,
    /// 位置上下文：entry/exit action 索引（如有）。
    pub action_index: Option<usize>,
}

/// 安全编译器校验结果。
#[derive(Debug, Clone, Default)]
pub struct SafetyReport {
    /// 所有诊断条目（错误 + 警告）。
    pub diagnostics: Vec<SafetyDiagnostic>,
}

impl SafetyReport {
    /// 是否包含至少一个错误级别诊断。
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.level == DiagnosticLevel::Error)
    }

    /// 只返回错误级别诊断。
    pub fn errors(&self) -> impl Iterator<Item = &SafetyDiagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.level == DiagnosticLevel::Error)
    }

    /// 只返回警告级别诊断。
    pub fn warnings(&self) -> impl Iterator<Item = &SafetyDiagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.level == DiagnosticLevel::Warning)
    }
}

// ---- 辅助宏：向 report 添加诊断 ----

fn diag_error(report: &mut SafetyReport, rule: &str, message: String) {
    report.diagnostics.push(SafetyDiagnostic {
        level: DiagnosticLevel::Error,
        rule: rule.to_owned(),
        message,
        state_name: None,
        transition_index: None,
        capability_id: None,
        action_index: None,
    });
}

fn diag_warning(report: &mut SafetyReport, rule: &str, message: String) {
    report.diagnostics.push(SafetyDiagnostic {
        level: DiagnosticLevel::Warning,
        rule: rule.to_owned(),
        message,
        state_name: None,
        transition_index: None,
        capability_id: None,
        action_index: None,
    });
}

fn diag_error_with(
    report: &mut SafetyReport,
    rule: &str,
    message: String,
    state_name: Option<&str>,
    transition_index: Option<usize>,
    capability_id: Option<&str>,
    action_index: Option<usize>,
) {
    report.diagnostics.push(SafetyDiagnostic {
        level: DiagnosticLevel::Error,
        rule: rule.to_owned(),
        message,
        state_name: state_name.map(String::from),
        transition_index,
        capability_id: capability_id.map(String::from),
        action_index,
    });
}

fn diag_warning_with(
    report: &mut SafetyReport,
    rule: &str,
    message: String,
    state_name: Option<&str>,
    transition_index: Option<usize>,
    capability_id: Option<&str>,
    action_index: Option<usize>,
) {
    report.diagnostics.push(SafetyDiagnostic {
        level: DiagnosticLevel::Warning,
        rule: rule.to_owned(),
        message,
        state_name: state_name.map(String::from),
        transition_index,
        capability_id: capability_id.map(String::from),
        action_index,
    });
}

// ---- 模板解析辅助 ----

/// 模板值分类。
enum TemplateValue {
    /// `${var_name}` 形式的变量引用。
    VariableRef(String),
    /// 数值字面量。
    Numeric(f64),
    /// 其他值（字符串、布尔、复合模板等）。
    Other,
}

/// 分类 `serde_json::Value` 的模板类型。
fn classify_template(value: &Value) -> TemplateValue {
    match value {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                TemplateValue::Numeric(f)
            } else {
                TemplateValue::Other
            }
        }
        Value::String(s) => {
            if let Some(var_name) = extract_variable_ref(s) {
                TemplateValue::VariableRef(var_name.to_owned())
            } else {
                TemplateValue::Other
            }
        }
        _ => TemplateValue::Other,
    }
}

/// 从 `${name}` 模式中提取变量名。
fn extract_variable_ref(s: &str) -> Option<&str> {
    let trimmed = s.trim();
    if trimmed.starts_with("${") && trimmed.ends_with('}') {
        let inner = &trimmed[2..trimmed.len() - 1];
        if !inner.is_empty() {
            return Some(inner);
        }
    }
    None
}

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

    check_unit_consistency(ctx, spec, &mut report);
    check_range_boundary(ctx, spec, &mut report);
    check_precondition_reachability(ctx, spec, &mut report);
    check_state_machine_completeness(spec, initial_state, &mut report);
    check_dangerous_action_approval(ctx, spec, &mut report);
    check_mechanical_interlock(ctx, spec, &mut report);

    report
}

// ---- 规则 1: 单位一致性校验 ----

/// 检查 action args 中的值单位是否与 Capability 输入参数声明的单位一致。
fn check_unit_consistency(ctx: &CompilerContext, spec: &WorkflowSpec, report: &mut SafetyReport) {
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
            continue; // 参数未声明单位，跳过
        };

        match classify_template(arg_value) {
            TemplateValue::Numeric(_) => {
                // 数值字面量无法推断单位，发警告提醒人工确认
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
                // 变量没有声明单位，无法静态校验
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
            TemplateValue::Other => {
                // 字符串或复合模板，无法校验
            }
        }
    }
}

// ---- 规则 2: 量程边界校验 ----

/// 检查 action args 中的值是否在 Capability 输入参数声明的量程范围内。
fn check_range_boundary(ctx: &CompilerContext, spec: &WorkflowSpec, report: &mut SafetyReport) {
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
                // 尝试从 spec.variables 获取初始值
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

// ---- 规则 3: 前置条件可达性校验 ----

/// 检查 Capability 的前置条件表达式中引用的信号是否存在于设备定义中且可读。
fn check_precondition_reachability(
    ctx: &CompilerContext,
    spec: &WorkflowSpec,
    report: &mut SafetyReport,
) {
    // 收集工作流中引用的所有 capability ID
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
                // 跳过已知的关键字和字面量
                if is_reserved_word(&ident) || ident.parse::<f64>().is_ok() {
                    continue;
                }

                if signal_names.contains(ident.as_str()) {
                    // 信号存在，检查可读性
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
                    // 可能是运行时变量，发警告
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

/// 从 action 列表中提取 Capability ID。
fn collect_capability_ids(actions: &[ActionSpec], out: &mut HashSet<String>) {
    for action in actions {
        if let ActionTarget::Capability(id) = &action.target {
            out.insert(id.clone());
        }
    }
}

/// 从 Rhai 风格表达式中提取标识符。
fn extract_identifiers(expr: &str) -> Vec<String> {
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

// ---- 规则 4: 状态机完整性校验 ----

/// 检查不可达状态、死胡同状态和循环触发。
fn check_state_machine_completeness(
    spec: &WorkflowSpec,
    initial_state: &str,
    report: &mut SafetyReport,
) {
    // 4a: 不可达状态
    let reachable = find_reachable_states(spec, initial_state);
    for state_name in spec.states.keys() {
        if !reachable.contains(state_name.as_str()) {
            diag_warning_with(
                report,
                "state_machine_completeness",
                format!("状态 `{state_name}` 不可达（无 incoming transition 且非初始状态）"),
                Some(state_name),
                None,
                None,
                None,
            );
        }
    }

    // 4b: 死胡同状态
    let dead_ends = find_dead_end_states(spec);
    for state_name in &dead_ends {
        diag_warning_with(
            report,
            "state_machine_completeness",
            format!("状态 `{state_name}` 为死胡同（无 outgoing transition 且无 timeout）"),
            Some(state_name),
            None,
            None,
            None,
        );
    }

    // 4c: 循环触发检测
    let cycles = find_trigger_cycles(spec);
    for cycle in cycles {
        let path = cycle.join(" → ");
        diag_error(
            report,
            "state_machine_completeness",
            format!("检测到循环触发路径: {path}"),
        );
    }
}

/// 找出所有可达状态（通过 transition 的 to 字段）。
fn find_reachable_states(spec: &WorkflowSpec, initial_state: &str) -> HashSet<String> {
    let mut reachable: HashSet<String> = HashSet::new();
    reachable.insert(initial_state.to_owned());

    for trans in &spec.transitions {
        // 任何 transition 的 to 都可达（通配符 from 使得任何状态都可以触发到 to）
        reachable.insert(trans.to.clone());
        // 非通配符 from 状态也是"被使用"的，但"可达"指有 incoming
        // 这里只关注 to，因为 from 状态本身是否可达由初始状态决定
    }

    reachable
}

/// 找出所有死胡同状态。
fn find_dead_end_states(spec: &WorkflowSpec) -> Vec<String> {
    let has_outgoing: HashSet<&str> = spec
        .transitions
        .iter()
        .filter(|t| t.from != "*")
        .map(|t| t.from.as_str())
        .collect();

    // 通配符 transition 为所有状态提供出口
    let has_wildcard_outgoing = spec.transitions.iter().any(|t| t.from == "*");

    spec.states
        .keys()
        .filter(|name| {
            !has_outgoing.contains(name.as_str())
                && !has_wildcard_outgoing
                && !spec.timeout.contains_key(*name)
                && !is_terminal_state_hint(name)
        })
        .cloned()
        .collect()
}

/// 终端状态名称启发式判断。
fn is_terminal_state_hint(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.contains("done")
        || lower.contains("complete")
        || lower.contains("end")
        || lower.contains("fault")
        || lower.contains("error")
        || lower.contains("finish")
}

/// 在确定性 transition 图上用 DFS 检测环。
fn find_trigger_cycles(spec: &WorkflowSpec) -> Vec<Vec<String>> {
    // 构建邻接表（仅确定性边，排除通配符）
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for trans in &spec.transitions {
        if trans.from != "*" {
            adj.entry(&trans.from).or_default().push(&trans.to);
        }
    }

    let mut visited: HashSet<String> = HashSet::new();
    let mut in_stack: HashSet<String> = HashSet::new();
    let mut path: Vec<String> = Vec::new();
    let mut cycles: Vec<Vec<String>> = Vec::new();

    for state_name in spec.states.keys() {
        dfs_find_cycles(
            state_name,
            &adj,
            &mut visited,
            &mut in_stack,
            &mut path,
            &mut cycles,
        );
    }

    cycles
}

fn dfs_find_cycles(
    node: &str,
    adj: &HashMap<&str, Vec<&str>>,
    visited: &mut HashSet<String>,
    in_stack: &mut HashSet<String>,
    path: &mut Vec<String>,
    cycles: &mut Vec<Vec<String>>,
) {
    if in_stack.contains(node) {
        // 找到环：从 path 中 node 出现的位置到末尾
        if let Some(start) = path.iter().position(|p| p == node) {
            let cycle: Vec<String> = path[start..].to_vec();
            let mut normalized = cycle.clone();
            normalized.sort();
            // 去重：同一环只报告一次
            if !cycles.iter().any(|existing| {
                let mut ex = existing.clone();
                ex.sort();
                ex == normalized
            }) {
                cycles.push(cycle);
            }
        }
        return;
    }

    if visited.contains(node) {
        return;
    }

    visited.insert(node.to_owned());
    in_stack.insert(node.to_owned());
    path.push(node.to_owned());

    if let Some(neighbors) = adj.get(node) {
        for &next in neighbors {
            dfs_find_cycles(next, adj, visited, in_stack, path, cycles);
        }
    }

    path.pop();
    in_stack.remove(node);
}

// ---- 规则 5: 危险动作审批校验 ----

/// 检查 High 安全等级且 `requires_approval` 的能力是否被使用，发出警告提醒人工审批。
fn check_dangerous_action_approval(
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

// ---- 规则 6: 机械互锁校验 ----

/// 检查同一设备上多个能力是否存在寄存器写入冲突。
fn check_mechanical_interlock(
    ctx: &CompilerContext,
    spec: &WorkflowSpec,
    report: &mut SafetyReport,
) {
    // 收集工作流中使用的所有 capability
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

    // 按设备分组
    let mut by_device: HashMap<String, Vec<&CapabilitySpec>> = HashMap::new();
    for cap_id in &cap_ids {
        if let Some(cap) = ctx.capabilities.get(cap_id) {
            by_device
                .entry(cap.device_id.clone())
                .or_default()
                .push(cap);
        }
    }

    // 每个设备内检查寄存器冲突
    for (device_id, caps) in &by_device {
        let conflicts = find_register_conflicts(caps);
        for (cap_a, cap_b, register) in conflicts {
            diag_warning(
                report,
                "mechanical_interlock",
                format!(
                    "设备 `{device_id}` 上的能力 `{cap_a}` 和 `{cap_b}` 均写入寄存器 {register}，可能存在并发冲突"
                ),
            );
        }
    }
}

/// 在同一设备的能力列表中查找 `ModbusWrite` 寄存器冲突。
fn find_register_conflicts(capabilities: &[&CapabilitySpec]) -> Vec<(String, String, u16)> {
    let mut conflicts = Vec::new();
    for i in 0..capabilities.len() {
        for j in (i + 1)..capabilities.len() {
            if let (
                CapabilityImpl::ModbusWrite { register: r1, .. },
                CapabilityImpl::ModbusWrite { register: r2, .. },
            ) = (
                &capabilities[i].implementation,
                &capabilities[j].implementation,
            ) && r1 == r2
            {
                conflicts.push((capabilities[i].id.clone(), capabilities[j].id.clone(), *r1));
            }
        }
    }
    conflicts
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use nazh_dsl_core::capability::{
        CapabilityImpl, CapabilityParam, CapabilitySpec, SafetyConstraints, SafetyLevel,
    };
    use nazh_dsl_core::device::{ConnectionRef, DeviceSpec, SignalSource, SignalSpec, SignalType};
    use nazh_dsl_core::workflow::{Range, WorkflowSpec};

    use super::*;
    use crate::CompilerContext;

    // ---- 测试辅助 ----

    fn sample_device_with_signals(id: &str, signals: Vec<SignalSpec>) -> DeviceSpec {
        DeviceSpec {
            id: id.to_owned(),
            device_type: "test".to_owned(),
            manufacturer: None,
            model: None,
            connection: ConnectionRef {
                connection_type: "modbus-tcp".to_owned(),
                id: format!("{id}_conn"),
                unit: Some(1),
            },
            signals,
            alarms: vec![],
        }
    }

    fn sample_device(id: &str) -> DeviceSpec {
        sample_device_with_signals(id, vec![])
    }

    fn readable_signal(id: &str) -> SignalSpec {
        SignalSpec {
            id: id.to_owned(),
            signal_type: SignalType::AnalogInput,
            unit: Some("MPa".to_owned()),
            range: Some(Range {
                min: 0.0,
                max: 35.0,
            }),
            source: SignalSource::Register {
                register: 40001,
                access: nazh_dsl_core::device::AccessMode::Read,
                data_type: nazh_dsl_core::device::DataType::Float32,
                bit: None,
            },
            scale: None,
        }
    }

    fn writable_signal(id: &str) -> SignalSpec {
        SignalSpec {
            id: id.to_owned(),
            signal_type: SignalType::AnalogOutput,
            unit: Some("mm".to_owned()),
            range: Some(Range {
                min: 0.0,
                max: 150.0,
            }),
            source: SignalSource::Register {
                register: 40010,
                access: nazh_dsl_core::device::AccessMode::Write,
                data_type: nazh_dsl_core::device::DataType::Float32,
                bit: None,
            },
            scale: None,
        }
    }

    fn cap_with_input(
        id: &str,
        device_id: &str,
        input_id: &str,
        unit: Option<&str>,
        range: Option<Range>,
        register: u16,
    ) -> CapabilitySpec {
        CapabilitySpec {
            id: id.to_owned(),
            device_id: device_id.to_owned(),
            description: String::new(),
            inputs: vec![CapabilityParam {
                id: input_id.to_owned(),
                unit: unit.map(String::from),
                range,
                required: true,
            }],
            outputs: vec![],
            preconditions: vec![],
            effects: vec![],
            implementation: CapabilityImpl::ModbusWrite {
                register,
                value: format!("${{{input_id}}}"),
            },
            fallback: vec![],
            safety: SafetyConstraints {
                level: SafetyLevel::Low,
                requires_approval: false,
                max_execution_time: None,
            },
        }
    }

    fn parse_spec(yaml: &str) -> WorkflowSpec {
        serde_yaml::from_str(yaml).unwrap()
    }

    // ---- 规则 1: 单位一致性 ----

    #[test]
    fn 单位一致性_数值字面量产生警告() {
        let cap = cap_with_input("cap.move", "dev1", "position", Some("mm"), None, 40010);
        let yaml = r#"
id: test
version: "1.0.0"
devices: [dev1]
states:
  idle:
    entry:
      - capability: cap.move
        args:
          position: 100.0
transitions: []
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![sample_device("dev1")], vec![cap]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        let unit_warnings: Vec<_> = report
            .warnings()
            .filter(|d| d.rule == "unit_consistency")
            .collect();
        assert_eq!(unit_warnings.len(), 1);
        assert!(unit_warnings[0].message.contains("无法静态校验单位"));
    }

    #[test]
    fn 单位一致性_变量引用产生警告() {
        let cap = cap_with_input("cap.move", "dev1", "position", Some("mm"), None, 40010);
        let yaml = r#"
id: test
version: "1.0.0"
devices: [dev1]
variables:
  pos: 100.0
states:
  idle:
    entry:
      - capability: cap.move
        args:
          position: "${pos}"
transitions: []
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![sample_device("dev1")], vec![cap]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        let unit_warnings: Vec<_> = report
            .warnings()
            .filter(|d| d.rule == "unit_consistency")
            .collect();
        assert_eq!(unit_warnings.len(), 1);
        assert!(unit_warnings[0].message.contains("pos"));
    }

    #[test]
    fn 单位一致性_无单位参数不产生诊断() {
        let cap = cap_with_input("cap.move", "dev1", "position", None, None, 40010);
        let yaml = r#"
id: test
version: "1.0.0"
devices: [dev1]
states:
  idle:
    entry:
      - capability: cap.move
        args:
          position: 100.0
transitions: []
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![sample_device("dev1")], vec![cap]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        assert!(
            report
                .diagnostics
                .iter()
                .all(|d| d.rule != "unit_consistency")
        );
    }

    #[test]
    fn 单位一致性_system_action不检查() {
        let yaml = r#"
id: test
version: "1.0.0"
states:
  idle:
    entry:
      - action: alarm.raise
        args:
          msg: "error"
transitions: []
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![], vec![]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        assert!(
            report
                .diagnostics
                .iter()
                .all(|d| d.rule != "unit_consistency")
        );
    }

    // ---- 规则 2: 量程边界 ----

    #[test]
    fn 量程边界_字面量在范围内通过() {
        let cap = cap_with_input(
            "cap.move",
            "dev1",
            "position",
            Some("mm"),
            Some(Range {
                min: 0.0,
                max: 150.0,
            }),
            40010,
        );
        let yaml = r#"
id: test
version: "1.0.0"
devices: [dev1]
states:
  idle:
    entry:
      - capability: cap.move
        args:
          position: 100.0
transitions: []
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![sample_device("dev1")], vec![cap]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        assert!(!report.has_errors());
    }

    #[test]
    fn 量程边界_字面量越界报错() {
        let cap = cap_with_input(
            "cap.move",
            "dev1",
            "position",
            Some("mm"),
            Some(Range {
                min: 0.0,
                max: 150.0,
            }),
            40010,
        );
        let yaml = r#"
id: test
version: "1.0.0"
devices: [dev1]
states:
  idle:
    entry:
      - capability: cap.move
        args:
          position: 200.0
transitions: []
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![sample_device("dev1")], vec![cap]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        let range_errors: Vec<_> = report
            .errors()
            .filter(|d| d.rule == "range_boundary")
            .collect();
        assert_eq!(range_errors.len(), 1);
        assert!(range_errors[0].message.contains("超出"));
    }

    #[test]
    fn 量程边界_变量初始值越界报错() {
        let cap = cap_with_input(
            "cap.move",
            "dev1",
            "position",
            Some("mm"),
            Some(Range {
                min: 0.0,
                max: 150.0,
            }),
            40010,
        );
        let yaml = r#"
id: test
version: "1.0.0"
devices: [dev1]
variables:
  target_pos: 200.0
states:
  idle:
    entry:
      - capability: cap.move
        args:
          position: "${target_pos}"
transitions: []
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![sample_device("dev1")], vec![cap]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        let range_errors: Vec<_> = report
            .errors()
            .filter(|d| d.rule == "range_boundary")
            .collect();
        assert_eq!(range_errors.len(), 1);
        assert!(range_errors[0].message.contains("target_pos"));
    }

    #[test]
    fn 量程边界_未声明变量产生警告() {
        let cap = cap_with_input(
            "cap.move",
            "dev1",
            "position",
            Some("mm"),
            Some(Range {
                min: 0.0,
                max: 150.0,
            }),
            40010,
        );
        let yaml = r#"
id: test
version: "1.0.0"
devices: [dev1]
states:
  idle:
    entry:
      - capability: cap.move
        args:
          position: "${unknown_var}"
transitions: []
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![sample_device("dev1")], vec![cap]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        let range_warnings: Vec<_> = report
            .warnings()
            .filter(|d| d.rule == "range_boundary")
            .collect();
        assert_eq!(range_warnings.len(), 1);
        assert!(range_warnings[0].message.contains("unknown_var"));
    }

    // ---- 规则 3: 前置条件可达性 ----

    #[test]
    fn 前置条件_可读信号通过() {
        let mut cap = cap_with_input("cap.move", "dev1", "position", None, None, 40010);
        cap.preconditions = vec!["pressure < 32".to_owned()];

        let device = sample_device_with_signals("dev1", vec![readable_signal("pressure")]);
        let yaml = r#"
id: test
version: "1.0.0"
devices: [dev1]
states:
  idle:
    entry:
      - capability: cap.move
transitions: []
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![device], vec![cap]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        assert!(
            report
                .diagnostics
                .iter()
                .all(|d| d.rule != "precondition_reachability")
        );
    }

    #[test]
    fn 前置条件_信号不存在产生警告() {
        let mut cap = cap_with_input("cap.move", "dev1", "position", None, None, 40010);
        cap.preconditions = vec!["nonexistent_signal > 10".to_owned()];

        let device = sample_device_with_signals("dev1", vec![readable_signal("pressure")]);
        let yaml = r#"
id: test
version: "1.0.0"
devices: [dev1]
states:
  idle:
    entry:
      - capability: cap.move
transitions: []
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![device], vec![cap]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        let prec_warnings: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.rule == "precondition_reachability")
            .collect();
        assert_eq!(prec_warnings.len(), 1);
        assert!(prec_warnings[0].message.contains("nonexistent_signal"));
    }

    #[test]
    fn 前置条件_不可写信号报错() {
        let mut cap = cap_with_input("cap.move", "dev1", "position", None, None, 40010);
        cap.preconditions = vec!["target_position > 0".to_owned()];

        // target_position 是 AnalogOutput（只写）
        let device = sample_device_with_signals("dev1", vec![writable_signal("target_position")]);
        let yaml = r#"
id: test
version: "1.0.0"
devices: [dev1]
states:
  idle:
    entry:
      - capability: cap.move
transitions: []
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![device], vec![cap]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        let prec_errors: Vec<_> = report
            .errors()
            .filter(|d| d.rule == "precondition_reachability")
            .collect();
        assert_eq!(prec_errors.len(), 1);
        assert!(prec_errors[0].message.contains("不可读"));
    }

    #[test]
    fn 前置条件_无前置条件的capability跳过() {
        let cap = cap_with_input("cap.move", "dev1", "position", None, None, 40010);
        // 无 preconditions
        let yaml = r#"
id: test
version: "1.0.0"
devices: [dev1]
states:
  idle:
    entry:
      - capability: cap.move
transitions: []
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![sample_device("dev1")], vec![cap]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        assert!(
            report
                .diagnostics
                .iter()
                .all(|d| d.rule != "precondition_reachability")
        );
    }

    // ---- 规则 4: 状态机完整性 ----

    #[test]
    fn 状态机_全部可达通过() {
        let yaml = r#"
id: test
version: "1.0.0"
states:
  idle:
  running:
  done:
transitions:
  - from: idle
    to: running
    when: "start"
  - from: running
    to: done
    when: "completed"
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![], vec![]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        let sm_diags: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.rule == "state_machine_completeness")
            .collect();
        assert!(sm_diags.is_empty());
    }

    #[test]
    fn 状态机_不可达状态产生警告() {
        let yaml = r#"
id: test
version: "1.0.0"
states:
  idle:
  active:
  orphan:
transitions:
  - from: idle
    to: active
    when: "start"
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![], vec![]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        let sm_warnings: Vec<_> = report
            .warnings()
            .filter(|d| d.rule == "state_machine_completeness" && d.message.contains("不可达"))
            .collect();
        assert_eq!(sm_warnings.len(), 1);
        assert!(sm_warnings[0].message.contains("orphan"));
    }

    #[test]
    fn 状态机_死胡同状态产生警告() {
        let yaml = r#"
id: test
version: "1.0.0"
states:
  idle:
  stuck:
transitions:
  - from: idle
    to: stuck
    when: "go"
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![], vec![]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        let sm_warnings: Vec<_> = report
            .warnings()
            .filter(|d| d.rule == "state_machine_completeness" && d.message.contains("死胡同"))
            .collect();
        assert_eq!(sm_warnings.len(), 1);
        assert!(sm_warnings[0].message.contains("stuck"));
    }

    #[test]
    fn 状态机_循环触发报错() {
        let yaml = r#"
id: test
version: "1.0.0"
states:
  a:
  b:
transitions:
  - from: a
    to: b
    when: "go_b"
  - from: b
    to: a
    when: "go_a"
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![], vec![]);
        let report = run_safety_checks(&ctx, &spec, "a");
        let sm_errors: Vec<_> = report
            .errors()
            .filter(|d| d.rule == "state_machine_completeness" && d.message.contains("循环"))
            .collect();
        assert_eq!(sm_errors.len(), 1);
    }

    #[test]
    fn 状态机_终端状态名不报死胡同() {
        let yaml = r#"
id: test
version: "1.0.0"
states:
  idle:
  fault:
transitions:
  - from: idle
    to: fault
    when: "error"
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![], vec![]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        let dead_end_warnings: Vec<_> = report
            .warnings()
            .filter(|d| d.message.contains("死胡同"))
            .collect();
        assert!(
            dead_end_warnings.is_empty(),
            "fault 是终端状态名，不应报死胡同"
        );
    }

    #[test]
    fn 状态机_通配符transition使所有状态可达() {
        let yaml = r#"
id: test
version: "1.0.0"
states:
  idle:
  fault:
transitions:
  - from: "*"
    to: fault
    when: "error"
    priority: 100
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![], vec![]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        let unreachable_warnings: Vec<_> = report
            .warnings()
            .filter(|d| d.message.contains("不可达"))
            .collect();
        assert!(
            unreachable_warnings.is_empty(),
            "通配符 transition 使 fault 可达"
        );
    }

    // ---- 规则 5: 危险动作审批 ----

    #[test]
    fn 审批_high等级需审批产生警告() {
        let mut cap = cap_with_input("cap.danger", "dev1", "value", None, None, 40010);
        cap.safety = SafetyConstraints {
            level: SafetyLevel::High,
            requires_approval: true,
            max_execution_time: None,
        };
        let yaml = r#"
id: test
version: "1.0.0"
devices: [dev1]
states:
  idle:
    entry:
      - capability: cap.danger
transitions: []
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![sample_device("dev1")], vec![cap]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        let approval_warnings: Vec<_> = report
            .warnings()
            .filter(|d| d.rule == "dangerous_action_approval")
            .collect();
        assert_eq!(approval_warnings.len(), 1);
        assert!(approval_warnings[0].message.contains("High"));
    }

    #[test]
    fn 审批_high等级无需审批不产生警告() {
        let mut cap = cap_with_input("cap.safe", "dev1", "value", None, None, 40010);
        cap.safety = SafetyConstraints {
            level: SafetyLevel::High,
            requires_approval: false,
            max_execution_time: None,
        };
        let yaml = r#"
id: test
version: "1.0.0"
devices: [dev1]
states:
  idle:
    entry:
      - capability: cap.safe
transitions: []
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![sample_device("dev1")], vec![cap]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        assert!(
            report
                .diagnostics
                .iter()
                .all(|d| d.rule != "dangerous_action_approval")
        );
    }

    #[test]
    fn 审批_low等级不产生警告() {
        let cap = cap_with_input("cap.low", "dev1", "value", None, None, 40010);
        let yaml = r#"
id: test
version: "1.0.0"
devices: [dev1]
states:
  idle:
    entry:
      - capability: cap.low
transitions: []
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![sample_device("dev1")], vec![cap]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        assert!(
            report
                .diagnostics
                .iter()
                .all(|d| d.rule != "dangerous_action_approval")
        );
    }

    // ---- 规则 6: 机械互锁 ----

    #[test]
    fn 互锁_同设备同寄存器产生警告() {
        let cap_a = cap_with_input("cap.write_a", "dev1", "value", None, None, 40010);
        let cap_b = cap_with_input("cap.write_b", "dev1", "value", None, None, 40010);
        let yaml = r#"
id: test
version: "1.0.0"
devices: [dev1]
states:
  idle:
    entry:
      - capability: cap.write_a
  running:
    entry:
      - capability: cap.write_b
transitions:
  - from: idle
    to: running
    when: "go"
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![sample_device("dev1")], vec![cap_a, cap_b]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        let interlock_warnings: Vec<_> = report
            .warnings()
            .filter(|d| d.rule == "mechanical_interlock")
            .collect();
        assert_eq!(interlock_warnings.len(), 1);
        assert!(interlock_warnings[0].message.contains("40010"));
    }

    #[test]
    fn 互锁_不同设备不产生警告() {
        let cap_a = cap_with_input("cap.write_a", "dev1", "value", None, None, 40010);
        let cap_b = cap_with_input("cap.write_b", "dev2", "value", None, None, 40010);
        let yaml = r#"
id: test
version: "1.0.0"
devices: [dev1, dev2]
states:
  idle:
    entry:
      - capability: cap.write_a
  running:
    entry:
      - capability: cap.write_b
transitions:
  - from: idle
    to: running
    when: "go"
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(
            vec![sample_device("dev1"), sample_device("dev2")],
            vec![cap_a, cap_b],
        );
        let report = run_safety_checks(&ctx, &spec, "idle");
        assert!(
            report
                .diagnostics
                .iter()
                .all(|d| d.rule != "mechanical_interlock")
        );
    }

    #[test]
    fn 互锁_同设备不同寄存器不产生警告() {
        let cap_a = cap_with_input("cap.write_a", "dev1", "value", None, None, 40010);
        let cap_b = cap_with_input("cap.write_b", "dev1", "value", None, None, 40020);
        let yaml = r#"
id: test
version: "1.0.0"
devices: [dev1]
states:
  idle:
    entry:
      - capability: cap.write_a
  running:
    entry:
      - capability: cap.write_b
transitions:
  - from: idle
    to: running
    when: "go"
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![sample_device("dev1")], vec![cap_a, cap_b]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        assert!(
            report
                .diagnostics
                .iter()
                .all(|d| d.rule != "mechanical_interlock")
        );
    }

    // ---- 辅助函数测试 ----

    #[test]
    fn extract_variable_ref_正常提取() {
        assert_eq!(extract_variable_ref("${position}"), Some("position"));
        assert_eq!(
            extract_variable_ref("${target_pressure}"),
            Some("target_pressure")
        );
    }

    #[test]
    fn extract_variable_ref_非变量模板返回none() {
        assert_eq!(extract_variable_ref("hello"), None);
        assert_eq!(extract_variable_ref("${}"), None);
        assert_eq!(extract_variable_ref("100.0"), None);
    }

    #[test]
    fn extract_identifiers_基本表达式() {
        let ids = extract_identifiers("pressure > 34");
        assert!(ids.contains(&"pressure".to_owned()));
        assert!(!ids.contains(&"34".to_owned())); // 数字被过滤
    }

    #[test]
    fn extract_identifiers_过滤保留字() {
        let ids = extract_identifiers("pressure > 34 and servo_ready == true");
        assert!(ids.contains(&"pressure".to_owned()));
        assert!(ids.contains(&"servo_ready".to_owned()));
        assert!(!ids.contains(&"and".to_owned()));
        assert!(!ids.contains(&"true".to_owned()));
    }
}
