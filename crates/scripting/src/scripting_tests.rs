use std::collections::HashMap;
use std::sync::Arc;

use nazh_core::{PinType, VariableDeclaration, WorkflowVariables};

use super::*;

fn vars_arc(name: &str, ty: PinType, initial: serde_json::Value) -> Arc<WorkflowVariables> {
    let mut decls = HashMap::new();
    decls.insert(
        name.to_owned(),
        VariableDeclaration {
            variable_type: ty,
            initial,
        },
    );
    Arc::new(WorkflowVariables::from_declarations(&decls).unwrap())
}

#[test]
fn rhai_脚本可读写变量() {
    let vars = vars_arc("counter", PinType::Integer, serde_json::Value::from(5_i64));
    let base = ScriptNodeBase::new(
        "test-script",
        r#"
                let v = vars.get("counter");
                vars.set("counter", v + 1);
                vars.get("counter")
            "#,
        10_000,
        Some(Arc::clone(&vars)),
    )
    .unwrap();

    let (_, result) = base.evaluate(serde_json::Value::Null).unwrap();
    let final_value = base.dynamic_to_value(&result).unwrap();
    assert_eq!(final_value, serde_json::Value::from(6_i64));
    assert_eq!(
        vars.get("counter").unwrap().value,
        serde_json::Value::from(6_i64)
    );
}

#[test]
fn rhai_脚本写入未声明变量返回错误() {
    let vars = vars_arc("a", PinType::Integer, serde_json::Value::from(0_i64));
    let base = ScriptNodeBase::new(
        "test-script-2",
        r#"vars.set("undeclared", 42)"#,
        10_000,
        Some(Arc::clone(&vars)),
    )
    .unwrap();
    let err = base.evaluate(serde_json::Value::Null).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("undeclared") || msg.contains("UnknownVariable"),
        "错误消息应包含变量名，实际：{msg}"
    );
}

#[test]
fn rhai_脚本_cas_成功返回_true() {
    let vars = vars_arc("c", PinType::Integer, serde_json::Value::from(0_i64));
    let base = ScriptNodeBase::new(
        "test-script-3",
        r#"vars.cas("c", 0, 1)"#,
        10_000,
        Some(Arc::clone(&vars)),
    )
    .unwrap();
    let (_, result) = base.evaluate(serde_json::Value::Null).unwrap();
    let final_value = base.dynamic_to_value(&result).unwrap();
    assert_eq!(final_value, serde_json::Value::from(true));
}

#[test]
fn rhai_脚本_cas_期望值不匹配时返回_false() {
    let vars = vars_arc("c", PinType::Integer, serde_json::Value::from(0_i64));
    let base = ScriptNodeBase::new(
        "test-script-cas-mismatch",
        r#"
                let ok = vars.cas("c", 99, 1);
                #{ ok: ok, current: vars.get("c") }
            "#,
        10_000,
        Some(Arc::clone(&vars)),
    )
    .unwrap();
    let (_, result) = base.evaluate(serde_json::Value::Null).unwrap();
    let final_value = base.dynamic_to_value(&result).unwrap();
    assert_eq!(
        final_value["ok"],
        serde_json::Value::from(false),
        "CAS 期望值不匹配应返回 false"
    );
    assert_eq!(
        final_value["current"],
        serde_json::Value::from(0_i64),
        "CAS 失败后变量应保持原值"
    );
}

#[test]
fn rhai_脚本无_variables_注入时_vars_未定义() {
    let base =
        ScriptNodeBase::new("test-script-4", r#"vars.get("anything")"#, 10_000, None).unwrap();
    let err = base.evaluate(serde_json::Value::Null).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("未注入"),
        "未注入 variables 时调用 vars.* 应返回 `未注入` 错误，实际：{msg}"
    );
}
