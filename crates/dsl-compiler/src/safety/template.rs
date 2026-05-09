use serde_json::Value;

/// 模板值分类。
pub(super) enum TemplateValue {
    /// `${var_name}` 形式的变量引用。
    VariableRef(String),
    /// 数值字面量。
    Numeric(f64),
    /// 其他值（字符串、布尔、复合模板等）。
    Other,
}

/// 分类 `serde_json::Value` 的模板类型。
pub(super) fn classify_template(value: &Value) -> TemplateValue {
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
pub(super) fn extract_variable_ref(s: &str) -> Option<&str> {
    let trimmed = s.trim();
    if trimmed.starts_with("${") && trimmed.ends_with('}') {
        let inner = &trimmed[2..trimmed.len() - 1];
        if !inner.is_empty() {
            return Some(inner);
        }
    }
    None
}
