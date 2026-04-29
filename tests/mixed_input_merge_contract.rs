#![allow(clippy::doc_markdown)]
//! ADR-0014 Phase 3b: mixed_input_merge fixture 跨语言契约。

#![allow(clippy::unwrap_used)]

use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Case {
    name: String,
    exec_payload: Value,
    data_values: serde_json::Map<String, Value>,
    merged: Value,
}

#[test]
fn fixture_穷尽_6_case() {
    let raw = std::fs::read_to_string("tests/fixtures/mixed_input_merge.jsonc").unwrap();
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
    assert_eq!(cases.len(), 6);

    for case in cases {
        let actual = nazh_engine::__test_only_merge_payload(
            case.exec_payload.clone(),
            case.data_values.clone(),
        );
        assert_eq!(actual, case.merged, "case `{}` 合并结果不匹配", case.name);
    }
}
