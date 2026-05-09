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

pub(super) fn diag_error(report: &mut SafetyReport, rule: &str, message: String) {
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

pub(super) fn diag_warning(report: &mut SafetyReport, rule: &str, message: String) {
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

pub(super) fn diag_error_with(
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

pub(super) fn diag_warning_with(
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
