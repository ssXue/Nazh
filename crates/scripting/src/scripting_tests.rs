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

// ── 位操作 / 进制转换 测试 ──────────────────────────────────────────────────

#[test]
fn bit_get_基本操作() {
    let base = ScriptNodeBase::new("test-bit-get", r"bit_get(0b1010, 1)", 10_000, None).unwrap();
    let (_, result) = base.evaluate(serde_json::Value::Null).unwrap();
    assert_eq!(
        base.dynamic_to_value(&result).unwrap(),
        serde_json::Value::from(true)
    );
}

#[test]
fn bit_get_零位为假() {
    let base = ScriptNodeBase::new("test-bit-get-0", r"bit_get(0b1010, 0)", 10_000, None).unwrap();
    let (_, result) = base.evaluate(serde_json::Value::Null).unwrap();
    assert_eq!(
        base.dynamic_to_value(&result).unwrap(),
        serde_json::Value::from(false)
    );
}

#[test]
fn bit_get_越界返回错误() {
    let base = ScriptNodeBase::new("test-bit-get-oob", r"bit_get(0, 64)", 10_000, None).unwrap();
    let err = base.evaluate(serde_json::Value::Null).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("0 <= bit <= 63"), "实际错误：{msg}");
}

#[test]
fn bit_set_置位() {
    let base = ScriptNodeBase::new("test-bit-set", r"bit_set(0, 3, true)", 10_000, None).unwrap();
    let (_, result) = base.evaluate(serde_json::Value::Null).unwrap();
    assert_eq!(
        base.dynamic_to_value(&result).unwrap(),
        serde_json::Value::from(8)
    );
}

#[test]
fn bit_set_清零() {
    let base =
        ScriptNodeBase::new("test-bit-clear", r"bit_set(0b1111, 1, false)", 10_000, None).unwrap();
    let (_, result) = base.evaluate(serde_json::Value::Null).unwrap();
    assert_eq!(
        base.dynamic_to_value(&result).unwrap(),
        serde_json::Value::from(0b1101)
    );
}

#[test]
fn to_hex_基本转换() {
    let base = ScriptNodeBase::new("test-to-hex", r"to_hex(255)", 10_000, None).unwrap();
    let (_, result) = base.evaluate(serde_json::Value::Null).unwrap();
    assert_eq!(
        base.dynamic_to_value(&result).unwrap(),
        serde_json::Value::from("ff")
    );
}

#[test]
fn from_hex_带前缀() {
    let base = ScriptNodeBase::new("test-from-hex", r#"from_hex("0xff")"#, 10_000, None).unwrap();
    let (_, result) = base.evaluate(serde_json::Value::Null).unwrap();
    assert_eq!(
        base.dynamic_to_value(&result).unwrap(),
        serde_json::Value::from(255)
    );
}

#[test]
fn from_hex_无前缀() {
    let base =
        ScriptNodeBase::new("test-from-hex-no-prefix", r#"from_hex("ff")"#, 10_000, None).unwrap();
    let (_, result) = base.evaluate(serde_json::Value::Null).unwrap();
    assert_eq!(
        base.dynamic_to_value(&result).unwrap(),
        serde_json::Value::from(255)
    );
}

#[test]
fn from_hex_非法输入返回错误() {
    let base =
        ScriptNodeBase::new("test-from-hex-err", r#"from_hex("xyz")"#, 10_000, None).unwrap();
    let err = base.evaluate(serde_json::Value::Null).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("from_hex"), "实际错误：{msg}");
}

#[test]
fn to_bin_基本转换() {
    let base = ScriptNodeBase::new("test-to-bin", r"to_bin(10)", 10_000, None).unwrap();
    let (_, result) = base.evaluate(serde_json::Value::Null).unwrap();
    assert_eq!(
        base.dynamic_to_value(&result).unwrap(),
        serde_json::Value::from("1010")
    );
}

// ── 数值工具 测试 ──────────────────────────────────────────────────────────

#[test]
fn round_两位小数() {
    let base = ScriptNodeBase::new("test-round", r"round(2.56789, 2)", 10_000, None).unwrap();
    let (_, result) = base.evaluate(serde_json::Value::Null).unwrap();
    let val = base.dynamic_to_value(&result).unwrap();
    assert!((val.as_f64().unwrap() - 2.57).abs() < 1e-10);
}

#[test]
fn round_零位小数() {
    let base = ScriptNodeBase::new("test-round-0", r"round(3.7, 0)", 10_000, None).unwrap();
    let (_, result) = base.evaluate(serde_json::Value::Null).unwrap();
    let val = base.dynamic_to_value(&result).unwrap();
    assert!((val.as_f64().unwrap() - 4.0).abs() < 1e-10);
}

#[test]
fn round_越界返回错误() {
    let base = ScriptNodeBase::new("test-round-oob", r"round(1.0, 16)", 10_000, None).unwrap();
    let err = base.evaluate(serde_json::Value::Null).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("0 <= decimals <= 15"), "实际错误：{msg}");
}

#[test]
fn clamp_在范围内() {
    let base =
        ScriptNodeBase::new("test-clamp-in", r"clamp(50.0, 0.0, 100.0)", 10_000, None).unwrap();
    let (_, result) = base.evaluate(serde_json::Value::Null).unwrap();
    let val = base.dynamic_to_value(&result).unwrap();
    assert!((val.as_f64().unwrap() - 50.0).abs() < 1e-10);
}

#[test]
fn clamp_超出上限() {
    let base =
        ScriptNodeBase::new("test-clamp-over", r"clamp(105.0, 0.0, 100.0)", 10_000, None).unwrap();
    let (_, result) = base.evaluate(serde_json::Value::Null).unwrap();
    let val = base.dynamic_to_value(&result).unwrap();
    assert!((val.as_f64().unwrap() - 100.0).abs() < 1e-10);
}

#[test]
fn clamp_低于下限() {
    let base =
        ScriptNodeBase::new("test-clamp-under", r"clamp(-5.0, 0.0, 100.0)", 10_000, None).unwrap();
    let (_, result) = base.evaluate(serde_json::Value::Null).unwrap();
    let val = base.dynamic_to_value(&result).unwrap();
    assert!((val.as_f64().unwrap() - 0.0).abs() < 1e-10);
}

#[test]
fn clamp_min大于max返回错误() {
    let base =
        ScriptNodeBase::new("test-clamp-inv", r"clamp(1.0, 100.0, 0.0)", 10_000, None).unwrap();
    let err = base.evaluate(serde_json::Value::Null).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("min <= max"), "实际错误：{msg}");
}

#[test]
fn scale_线性映射中点() {
    let base = ScriptNodeBase::new(
        "test-scale",
        r"scale(12.0, 4.0, 20.0, 0.0, 100.0)",
        10_000,
        None,
    )
    .unwrap();
    let (_, result) = base.evaluate(serde_json::Value::Null).unwrap();
    let val = base.dynamic_to_value(&result).unwrap();
    assert!((val.as_f64().unwrap() - 50.0).abs() < 1e-10);
}

#[test]
fn scale_线性映射下限() {
    let base = ScriptNodeBase::new(
        "test-scale-min",
        r"scale(4.0, 4.0, 20.0, 0.0, 100.0)",
        10_000,
        None,
    )
    .unwrap();
    let (_, result) = base.evaluate(serde_json::Value::Null).unwrap();
    let val = base.dynamic_to_value(&result).unwrap();
    assert!((val.as_f64().unwrap() - 0.0).abs() < 1e-10);
}

#[test]
fn scale_零宽度区间返回错误() {
    let base = ScriptNodeBase::new(
        "test-scale-err",
        r"scale(1.0, 5.0, 5.0, 0.0, 100.0)",
        10_000,
        None,
    )
    .unwrap();
    let err = base.evaluate(serde_json::Value::Null).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("零宽度"), "实际错误：{msg}");
}

// ── 时间格式化 测试 ────────────────────────────────────────────────────────

#[test]
fn format_ts_已知时间戳() {
    // 2026-01-01 00:00:00 UTC = 1767225600000 ms
    let base = ScriptNodeBase::new(
        "test-format-ts",
        r#"format_ts(1767225600000, "%Y-%m-%d %H:%M:%S")"#,
        10_000,
        None,
    )
    .unwrap();
    let (_, result) = base.evaluate(serde_json::Value::Null).unwrap();
    assert_eq!(
        base.dynamic_to_value(&result).unwrap(),
        serde_json::Value::from("2026-01-01 00:00:00")
    );
}

#[test]
fn duration_ms_正值() {
    let base =
        ScriptNodeBase::new("test-dur-pos", r"duration_ms(1000, 3000)", 10_000, None).unwrap();
    let (_, result) = base.evaluate(serde_json::Value::Null).unwrap();
    assert_eq!(
        base.dynamic_to_value(&result).unwrap(),
        serde_json::Value::from(2000)
    );
}

#[test]
fn duration_ms_负值() {
    let base =
        ScriptNodeBase::new("test-dur-neg", r"duration_ms(5000, 1000)", 10_000, None).unwrap();
    let (_, result) = base.evaluate(serde_json::Value::Null).unwrap();
    assert_eq!(
        base.dynamic_to_value(&result).unwrap(),
        serde_json::Value::from(-4000)
    );
}

// ── 组合场景测试 ────────────────────────────────────────────────────────────

#[test]
fn 位操作与进制转换联合() {
    let base = ScriptNodeBase::new(
        "test-combo-bits",
        r#"
            let reg = from_hex("0xAB");
            let high_nibble = bit_get(reg, 7);
            let modified = bit_set(reg, 0, false);
            #{ hex: to_hex(modified), bin: to_bin(modified), bit7: high_nibble }
        "#,
        10_000,
        None,
    )
    .unwrap();
    let (_, result) = base.evaluate(serde_json::Value::Null).unwrap();
    let val = base.dynamic_to_value(&result).unwrap();
    assert_eq!(val["bit7"], serde_json::Value::from(true));
    assert_eq!(val["hex"], serde_json::Value::from("aa"));
    // 0xAA = 0b10101010, bit 0 cleared → 0b10101010 = 170, but 0xAB = 10101011, bit 0 cleared = 10101010 = 170 = 0xaa
    assert_eq!(val["bin"], serde_json::Value::from("10101010"));
}

#[test]
fn 数值工具与时间联合() {
    let base = ScriptNodeBase::new(
        "test-combo-math-time",
        r#"
            let raw = 12.5;
            let scaled = scale(raw, 4.0, 20.0, 0.0, 100.0);
            let clamped = round(clamp(scaled, 0.0, 100.0), 1);
            let ts = format_ts(0, "%Y-%m-%d");
            #{ temp: clamped, epoch: ts }
        "#,
        10_000,
        None,
    )
    .unwrap();
    let (_, result) = base.evaluate(serde_json::Value::Null).unwrap();
    let val = base.dynamic_to_value(&result).unwrap();
    assert!((val["temp"].as_f64().unwrap() - 53.1).abs() < 1e-10);
    assert_eq!(val["epoch"], serde_json::Value::from("1970-01-01"));
}
