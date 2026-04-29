#![allow(clippy::doc_markdown)]
//! ADR-0014 Phase 4 契约测试：EmptyPolicy 序列化矩阵。
//!
//! 读取 `tests/fixtures/empty_policy_matrix.jsonc`，验证每个 case 的
//! 反序列化 + 重序列化与 `expected_serialized` 一致。
//! 与 `web/src/lib/__tests__/empty-policy.test.ts` 形成跨语言覆盖。

#![allow(clippy::unwrap_used)]

use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Case {
    name: String,
    policy: Value,
    expected_serialized: Value,
    is_default: bool,
}

#[test]
fn empty_policy_序列化契约() {
    let raw = std::fs::read_to_string("tests/fixtures/empty_policy_matrix.jsonc").unwrap();
    let stripped: String = raw
        .lines()
        .map(|l| {
            if let Some(idx) = l.find("//") {
                &l[..idx]
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let cases: Vec<Case> = serde_json::from_str(&stripped).unwrap();

    for case in cases {
        let policy: nazh_engine::EmptyPolicy = serde_json::from_value(case.policy.clone())
            .unwrap_or_else(|e| panic!("case `{}` 反序列化失败: {e}", case.name));

        // 默认性检查
        assert_eq!(
            policy == nazh_engine::EmptyPolicy::default(),
            case.is_default,
            "case `{}`: is_default 断言不一致",
            case.name
        );

        // 重序列化
        let serialized = serde_json::to_value(&policy).unwrap();
        assert_eq!(
            &serialized, &case.expected_serialized,
            "case `{}`: 序列化结果不匹配",
            case.name
        );
    }
}
