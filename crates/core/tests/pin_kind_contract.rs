//! `PinKind` 兼容矩阵合约测试（ADR-0014）。
//!
//! 本测试与 `tests/fixtures/pin_kind_matrix.jsonc` 配对：fixture 是合约
//! 源头，Rust 实现 [`PinKind::is_compatible_with`] 必须与 fixture 的每条
//! 断言一致。
//!
//! 同一份 fixture 也被前端 TS 实现 (`web/src/lib/pin-compat.ts` 中的
//! `isKindCompatible`，Phase 2 Task 5 接入) 共享消费——任意一方与 fixture
//! 漂移即触发 CI 红。与 `pin_compat_contract.rs` 共同构成 `PinType` + `PinKind`
//! 双维度的引脚兼容契约。
//!
//! 扩展覆盖时：直接改 fixture，无需改本文件——表驱动设计。

#![allow(clippy::unwrap_used, clippy::expect_used)]

use json_comments::StripComments;
use nazh_core::PinKind;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct KindPair {
    from: PinKind,
    to: PinKind,
    compatible: bool,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    pairs: Vec<KindPair>,
}

fn fixture_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("pin_kind_matrix.jsonc")
}

fn load_fixture() -> Fixture {
    let path = fixture_path();
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("读取 fixture 失败 ({}): {e}", path.display()));
    let stripped = StripComments::new(raw.as_bytes());
    serde_json::from_reader(stripped)
        .expect("fixture 反序列化失败：检查 JSONC 格式或 PinKind 序列化标签")
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
        let actual = pair.from.is_compatible_with(pair.to);
        if actual != pair.compatible {
            failures.push(format!(
                "  pair #{i}: {:?} → {:?}\n    期望 compatible = {}\n    实际 Rust 返回 = {}",
                pair.from, pair.to, pair.compatible, actual
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "PinKind::is_compatible_with 与 fixture 不一致（{} 条失败）：\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn fixture_覆盖_pin_kind_全部变体两两组合() {
    let fixture = load_fixture();

    // PinKind 是封闭枚举，仅 Exec / Data 两个变体——矩阵应穷尽 2×2 笛卡儿积。
    let variants = [PinKind::Exec, PinKind::Data];
    for from in variants {
        for to in variants {
            assert!(
                fixture.pairs.iter().any(|p| p.from == from && p.to == to),
                "fixture 缺配对 {from:?} → {to:?} —— 必须穷尽 PinKind × PinKind 笛卡儿积"
            );
        }
    }
}
