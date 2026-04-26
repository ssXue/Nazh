//! `PinType` 兼容矩阵合约测试。
//!
//! 本测试与 `tests/fixtures/pin_compat_matrix.jsonc` 配对：fixture 是合约
//! 源头，Rust 实现 [`PinType::is_compatible_with`] 必须与 fixture 的每条
//! 断言一致。
//!
//! 同一份 fixture 也被前端 TS 实现 (`web/src/lib/pin-compat.ts`) 共享消费——
//! 任意一方与 fixture 漂移即触发 CI 红，杜绝"两份兼容判断悄悄走偏"的隐藏 bug。
//!
//! 扩展覆盖时：直接改 fixture，无需改本文件——表驱动设计。

#![allow(clippy::unwrap_used, clippy::expect_used)]

use json_comments::StripComments;
use nazh_core::PinType;
use serde::Deserialize;

/// 合约文件中的单条配对。
///
/// 字段命名直接 match fixture 的 JSON key——`from`/`to` 是 `PinType`，
/// `compatible` 是期望布尔。
#[derive(Debug, Deserialize)]
struct CompatPair {
    from: PinType,
    to: PinType,
    compatible: bool,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    pairs: Vec<CompatPair>,
}

/// fixture 路径相对于 `crates/core` manifest dir。
/// 工作空间根 `tests/fixtures/pin_compat_matrix.jsonc` 是单一权威源——
/// 不要把 fixture 复制到 crate 内部，那会让前后端无法共享同一份合约。
fn fixture_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("pin_compat_matrix.jsonc")
}

fn load_fixture() -> Fixture {
    let path = fixture_path();
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("读取 fixture 失败 ({}): {e}", path.display()));
    let stripped = StripComments::new(raw.as_bytes());
    serde_json::from_reader(stripped).expect("fixture 反序列化失败：检查 JSONC 格式或 PinType 标签")
}

#[test]
fn rust_实现与合约_fixture_一致() {
    let fixture = load_fixture();
    assert!(
        !fixture.pairs.is_empty(),
        "fixture 必须至少包含 1 条配对——疑似文件被清空"
    );

    let mut failures: Vec<String> = Vec::new();
    for (i, pair) in fixture.pairs.iter().enumerate() {
        let actual = pair.from.is_compatible_with(&pair.to);
        if actual != pair.compatible {
            failures.push(format!(
                "  pair #{i}: {:?} → {:?}\n    期望 compatible = {}\n    实际 Rust 返回 = {}",
                pair.from, pair.to, pair.compatible, actual
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "PinType::is_compatible_with 与 fixture 不一致（{} 条失败）：\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn fixture_覆盖纪律_每个变体至少出现() {
    let fixture = load_fixture();

    let mut seen_kinds: std::collections::HashSet<&'static str> = std::collections::HashSet::new();
    for pair in &fixture.pairs {
        record_kind(&pair.from, &mut seen_kinds);
        record_kind(&pair.to, &mut seen_kinds);
    }

    let required = [
        "Any", "Bool", "Integer", "Float", "String", "Json", "Binary", "Array", "Custom",
    ];
    for kind in required {
        assert!(
            seen_kinds.contains(kind),
            "fixture 未覆盖 PinType::{kind} —— 至少加 1 条自反兼容配对（fixture 文件头有覆盖纪律说明）"
        );
    }
}

fn record_kind(pin: &PinType, set: &mut std::collections::HashSet<&'static str>) {
    let label = match pin {
        PinType::Any => "Any",
        PinType::Bool => "Bool",
        PinType::Integer => "Integer",
        PinType::Float => "Float",
        PinType::String => "String",
        PinType::Json => "Json",
        PinType::Binary => "Binary",
        PinType::Array { inner } => {
            record_kind(inner, set);
            "Array"
        }
        PinType::Custom { .. } => "Custom",
    };
    set.insert(label);
}
