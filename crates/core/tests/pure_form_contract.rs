//! ADR-0014 Phase 3：pure_form 跨语言契约测试。
//!
//! 共享 fixture：`/tests/fixtures/pure_form_matrix.jsonc`（仓库根，与 Vitest 同源）。

#![allow(clippy::unwrap_used)]

use async_trait::async_trait;
use nazh_core::{
    is_pure_form, EngineError, NodeExecution, NodeTrait, PinDefinition, PinDirection, PinKind,
    PinType,
};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
struct PinSpec {
    kind: String,
}

#[derive(Debug, Deserialize)]
struct Case {
    name: String,
    input_pins: Vec<PinSpec>,
    output_pins: Vec<PinSpec>,
    expected_pure_form: bool,
}

struct Stub {
    id: String,
    inputs: Vec<PinDefinition>,
    outputs: Vec<PinDefinition>,
}

#[async_trait]
impl NodeTrait for Stub {
    fn id(&self) -> &str {
        &self.id
    }
    fn kind(&self) -> &'static str {
        "stub"
    }
    fn input_pins(&self) -> Vec<PinDefinition> {
        self.inputs.clone()
    }
    fn output_pins(&self) -> Vec<PinDefinition> {
        self.outputs.clone()
    }
    async fn transform(&self, _: Uuid, payload: Value) -> Result<NodeExecution, EngineError> {
        Ok(NodeExecution::broadcast(payload))
    }
}

fn pin(kind_str: &str, dir: PinDirection) -> PinDefinition {
    let kind = match kind_str {
        "exec" => PinKind::Exec,
        "data" => PinKind::Data,
        other => panic!("未知 pin kind: {other}"),
    };
    PinDefinition {
        id: format!("p_{kind_str}"),
        label: format!("p_{kind_str}"),
        pin_type: PinType::Any,
        direction: dir,
        required: matches!(dir, PinDirection::Input) && matches!(kind, PinKind::Exec),
        kind,
        description: None,
    }
}

#[test]
fn fixture_穷尽_4_配对() {
    let raw = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/pure_form_matrix.jsonc"),
    )
    .expect("读取 fixture");
    // 简单去注释（jsonc → json）：仅去除单行 `//` 注释
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
    let cases: Vec<Case> = serde_json::from_str(&stripped).expect("解析 fixture");
    assert_eq!(cases.len(), 4, "fixture 必须穷尽 4 配对");

    for case in cases {
        let stub = Stub {
            id: case.name.clone(),
            inputs: case
                .input_pins
                .iter()
                .map(|p| pin(&p.kind, PinDirection::Input))
                .collect(),
            outputs: case
                .output_pins
                .iter()
                .map(|p| pin(&p.kind, PinDirection::Output))
                .collect(),
        };
        assert_eq!(
            is_pure_form(&stub),
            case.expected_pure_form,
            "case `{}` 判定与 fixture 不符",
            case.name
        );
    }
}
