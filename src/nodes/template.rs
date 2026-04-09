//! `{{placeholder}}` 模板渲染引擎。
//!
//! 本模块从 HTTP 节点中抽取，提供通用的占位符替换能力。
//! 内置变量（`trace_id`、`node_id`、`timestamp`、`payload.*`）
//! 来自工作流上下文，调用方可通过 `extras` 注入额外变量。

use serde_json::Value;
use uuid::Uuid;

/// 模板渲染时可用的变量上下文。
pub(crate) struct TemplateVars<'a> {
    pub payload: &'a Value,
    pub trace_id: &'a Uuid,
    pub node_id: &'a str,
    pub timestamp: &'a str,
    pub extras: &'a [(&'a str, &'a str)],
}

/// 沿 JSON 路径（如 `"a.b.0.c"`）在树中定位值。
pub(crate) fn resolve_json_path<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.')
        .filter(|segment| !segment.is_empty())
        .try_fold(root, |current, segment| match current {
            Value::Object(map) => map.get(segment),
            Value::Array(items) => segment
                .parse::<usize>()
                .ok()
                .and_then(|index| items.get(index)),
            _ => None,
        })
}

/// 将 JSON Value 转为人类可读的字符串（Null → 空串）。
pub(crate) fn value_to_display_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// 截断字符串到指定字符数，超出部分用省略号替代。
pub(crate) fn truncate(text: &str, limit: usize) -> String {
    let mut result = text.chars().take(limit).collect::<String>();
    if text.chars().count() > limit {
        result.push('\u{2026}');
    }
    result
}

/// 渲染 `{{key}}` 模板，从 [`TemplateVars`] 中解析变量。
pub(crate) fn render(template: &str, vars: &TemplateVars<'_>) -> String {
    let mut result = String::with_capacity(template.len() + 48);
    let mut remaining = template;

    while let Some(start) = remaining.find("{{") {
        result.push_str(&remaining[..start]);
        let after_open = &remaining[start + 2..];

        if let Some(end) = after_open.find("}}") {
            let key = after_open[..end].trim();
            result.push_str(&resolve_key(key, vars));
            remaining = &after_open[end + 2..];
        } else {
            result.push_str(&remaining[start..]);
            return result;
        }
    }

    result.push_str(remaining);
    result
}

/// 解析单个模板变量 key。
fn resolve_key(key: &str, vars: &TemplateVars<'_>) -> String {
    match key {
        "trace_id" => vars.trace_id.to_string(),
        "node_id" => vars.node_id.to_owned(),
        "timestamp" | "event_at" => vars.timestamp.to_owned(),
        "payload" => vars.payload.to_string(),
        _ => {
            if let Some((_, value)) = vars.extras.iter().find(|(k, _)| *k == key) {
                return (*value).to_owned();
            }
            if let Some(path) = key.strip_prefix("payload.") {
                resolve_json_path(vars.payload, path)
                    .map(value_to_display_string)
                    .unwrap_or_default()
            } else {
                String::new()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_vars(payload: &Value) -> TemplateVars<'_> {
        // 使用固定 UUID 避免测试中依赖随机值
        let trace_id = Box::leak(Box::new(
            Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000")
                .unwrap_or_else(|_| Uuid::nil()),
        ));
        TemplateVars {
            payload,
            trace_id,
            node_id: "test-node",
            timestamp: "2026-01-01T00:00:00Z",
            extras: &[("custom_key", "custom_value")],
        }
    }

    #[test]
    fn 渲染内置变量() {
        let payload = json!({"temperature": 42});
        let vars = test_vars(&payload);
        let result = render("节点 {{node_id}} 时间 {{timestamp}}", &vars);
        assert_eq!(result, "节点 test-node 时间 2026-01-01T00:00:00Z");
    }

    #[test]
    fn 渲染_payload_路径() {
        let payload = json!({"sensor": {"temp": 55.3}});
        let vars = test_vars(&payload);
        assert_eq!(render("温度={{payload.sensor.temp}}", &vars), "温度=55.3");
    }

    #[test]
    fn 渲染额外变量() {
        let payload = json!({});
        let vars = test_vars(&payload);
        assert_eq!(
            render("自定义={{custom_key}}", &vars),
            "自定义=custom_value"
        );
    }

    #[test]
    fn 未闭合占位符保留原文() {
        let payload = json!({});
        let vars = test_vars(&payload);
        assert_eq!(render("前缀 {{未闭合", &vars), "前缀 {{未闭合");
    }

    #[test]
    fn json_path_支持数组索引() {
        let data = json!({"items": [10, 20, 30]});
        assert_eq!(
            resolve_json_path(&data, "items.1"),
            Some(&Value::from(20))
        );
    }

    #[test]
    fn 截断超长文本() {
        assert_eq!(truncate("abcde", 3), "abc\u{2026}");
        assert_eq!(truncate("ab", 3), "ab");
    }
}
