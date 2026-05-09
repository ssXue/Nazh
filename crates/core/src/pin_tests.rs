use super::*;

// ---- PinKind ----

#[test]
fn pin_kind_默认值是_exec() {
    assert_eq!(PinKind::default(), PinKind::Exec);
}

#[test]
fn pin_kind_序列化为小写字符串() {
    assert_eq!(serde_json::to_string(&PinKind::Exec).unwrap(), "\"exec\"");
    assert_eq!(serde_json::to_string(&PinKind::Data).unwrap(), "\"data\"");
    assert_eq!(
        serde_json::to_string(&PinKind::Reactive).unwrap(),
        "\"reactive\""
    );
}

#[test]
fn pin_kind_反序列化从小写字符串() {
    let exec: PinKind = serde_json::from_str("\"exec\"").unwrap();
    let data: PinKind = serde_json::from_str("\"data\"").unwrap();
    let reactive: PinKind = serde_json::from_str("\"reactive\"").unwrap();
    assert_eq!(exec, PinKind::Exec);
    assert_eq!(data, PinKind::Data);
    assert_eq!(reactive, PinKind::Reactive);
}

#[test]
fn pin_kind_兼容矩阵() {
    // 同种互连
    assert!(PinKind::Exec.is_compatible_with(PinKind::Exec));
    assert!(PinKind::Data.is_compatible_with(PinKind::Data));
    assert!(PinKind::Reactive.is_compatible_with(PinKind::Reactive));

    // Exec ↔ Data 互不兼容（ADR-0014 保持不变）
    assert!(!PinKind::Exec.is_compatible_with(PinKind::Data));
    assert!(!PinKind::Data.is_compatible_with(PinKind::Exec));

    // Reactive 输出 → 可连 Exec / Data / Reactive 输入
    assert!(PinKind::Reactive.is_compatible_with(PinKind::Exec));
    assert!(PinKind::Reactive.is_compatible_with(PinKind::Data));

    // Exec / Data 输出 → 不可连 Reactive 输入
    assert!(!PinKind::Exec.is_compatible_with(PinKind::Reactive));
    assert!(!PinKind::Data.is_compatible_with(PinKind::Reactive));
}

// ---- 兼容矩阵 ----

#[test]
fn any_可吃任何下游() {
    for ty in [
        PinType::Bool,
        PinType::Integer,
        PinType::Float,
        PinType::String,
        PinType::Json,
        PinType::Binary,
        PinType::Array {
            inner: Box::new(PinType::Integer),
        },
        PinType::Custom {
            name: "modbus-register".to_owned(),
        },
    ] {
        assert!(PinType::Any.is_compatible_with(&ty), "Any → {ty:?} 应通过");
    }
}

#[test]
fn 任何上游均可流入_any() {
    for ty in [
        PinType::Bool,
        PinType::Integer,
        PinType::Float,
        PinType::String,
        PinType::Json,
        PinType::Binary,
        PinType::Array {
            inner: Box::new(PinType::Integer),
        },
        PinType::Custom {
            name: "modbus-register".to_owned(),
        },
    ] {
        assert!(ty.is_compatible_with(&PinType::Any), "{ty:?} → Any 应通过");
    }
}

#[test]
fn 标量类型精确相等才兼容() {
    assert!(PinType::Integer.is_compatible_with(&PinType::Integer));
    assert!(PinType::String.is_compatible_with(&PinType::String));
    assert!(PinType::Bool.is_compatible_with(&PinType::Bool));

    assert!(!PinType::String.is_compatible_with(&PinType::Integer));
    assert!(!PinType::Integer.is_compatible_with(&PinType::Float));
    assert!(!PinType::Json.is_compatible_with(&PinType::Bool));
}

#[test]
fn 数组兼容性递归判定内层() {
    let arr_int = PinType::Array {
        inner: Box::new(PinType::Integer),
    };
    let arr_int_2 = PinType::Array {
        inner: Box::new(PinType::Integer),
    };
    let arr_str = PinType::Array {
        inner: Box::new(PinType::String),
    };
    let arr_any = PinType::Array {
        inner: Box::new(PinType::Any),
    };

    assert!(arr_int.is_compatible_with(&arr_int_2));
    assert!(arr_any.is_compatible_with(&arr_int)); // Array(Any) → Array(Integer) ✓
    assert!(arr_int.is_compatible_with(&arr_any)); // Array(Integer) → Array(Any) ✓
    assert!(!arr_int.is_compatible_with(&arr_str));
}

#[test]
fn custom_类型必须精确同名() {
    let a = PinType::Custom {
        name: "modbus-register".to_owned(),
    };
    let a_dup = PinType::Custom {
        name: "modbus-register".to_owned(),
    };
    let b = PinType::Custom {
        name: "opc-tag".to_owned(),
    };

    assert!(a.is_compatible_with(&a_dup));
    assert!(!a.is_compatible_with(&b));
    // Custom 与标量永不直连——必须经 Any 桥接
    assert!(!a.is_compatible_with(&PinType::String));
    assert!(!PinType::String.is_compatible_with(&a));
}

#[test]
fn json_与标量互不兼容() {
    assert!(PinType::Json.is_compatible_with(&PinType::Json));
    assert!(!PinType::Json.is_compatible_with(&PinType::Bool));
    assert!(!PinType::Bool.is_compatible_with(&PinType::Json));
}

// ---- PinDefinition kind 字段 ----

#[test]
fn pin_definition_默认工厂方法的_kind_是_exec() {
    assert_eq!(PinDefinition::default_input().kind, PinKind::Exec);
    assert_eq!(PinDefinition::default_output().kind, PinKind::Exec);
    assert_eq!(
        PinDefinition::required_input(PinType::Json, "test").kind,
        PinKind::Exec
    );
    assert_eq!(
        PinDefinition::output(PinType::Json, "test").kind,
        PinKind::Exec
    );
}

#[test]
fn pin_definition_缺_kind_字段反序列化默认_exec() {
    // 旧前端 / 旧节点 JSON 不带 kind 字段，必须能反序列化为 Exec
    let json =
        r#"{"id":"in","label":"in","pin_type":{"kind":"any"},"direction":"input","required":true}"#;
    let pin: PinDefinition = serde_json::from_str(json).unwrap();
    assert_eq!(pin.kind, PinKind::Exec);
}

#[test]
fn pin_definition_显式_kind_字段反序列化正确() {
    let json = r#"{"id":"latest","label":"latest","pin_type":{"kind":"any"},"direction":"output","required":false,"kind":"data"}"#;
    let pin: PinDefinition = serde_json::from_str(json).unwrap();
    assert_eq!(pin.kind, PinKind::Data);
}

#[test]
fn output_named_data_工厂方法生成正确字段() {
    let pin = PinDefinition::output_named_data(
        "latest",
        "最近读数",
        PinType::Json,
        "缓存最近一次读取的寄存器值",
    );
    assert_eq!(pin.id, "latest");
    assert_eq!(pin.label, "最近读数");
    assert_eq!(pin.pin_type, PinType::Json);
    assert_eq!(pin.direction, PinDirection::Output);
    assert!(!pin.required, "Data 输出非必需（拉取式）");
    assert_eq!(pin.kind, PinKind::Data);
    assert_eq!(
        pin.description.as_deref(),
        Some("缓存最近一次读取的寄存器值")
    );
}

// ---- 默认引脚 ----

#[test]
fn 默认输入引脚是必需的_any() {
    let pin = PinDefinition::default_input();
    assert_eq!(pin.id, "in");
    assert_eq!(pin.direction, PinDirection::Input);
    assert!(pin.required);
    assert_eq!(pin.pin_type, PinType::Any);
}

#[test]
fn 默认输出引脚不是必需的_any() {
    let pin = PinDefinition::default_output();
    assert_eq!(pin.id, "out");
    assert_eq!(pin.direction, PinDirection::Output);
    assert!(!pin.required);
    assert_eq!(pin.pin_type, PinType::Any);
}

// ---- 序列化形态（前端契约稳定） ----

#[test]
fn pin_type_serialization_uses_lowercase_kind_tag() {
    let any = serde_json::to_value(PinType::Any).unwrap();
    assert_eq!(any, serde_json::json!({ "kind": "any" }));

    let custom = serde_json::to_value(PinType::Custom {
        name: "modbus-register".to_owned(),
    })
    .unwrap();
    assert_eq!(
        custom,
        serde_json::json!({ "kind": "custom", "name": "modbus-register" })
    );

    let arr = serde_json::to_value(PinType::Array {
        inner: Box::new(PinType::Integer),
    })
    .unwrap();
    assert_eq!(
        arr,
        serde_json::json!({ "kind": "array", "inner": { "kind": "integer" } })
    );
}

#[test]
fn pin_type_可往返序列化() {
    let original = PinType::Array {
        inner: Box::new(PinType::Custom {
            name: "modbus-register".to_owned(),
        }),
    };
    let json = serde_json::to_string(&original).unwrap();
    let restored: PinType = serde_json::from_str(&json).unwrap();
    assert_eq!(original, restored);
}

#[test]
fn pin_definition_可往返序列化() {
    let original = PinDefinition {
        id: "true".to_owned(),
        label: "真".to_owned(),
        pin_type: PinType::Any,
        direction: PinDirection::Output,
        required: false,
        kind: PinKind::Exec,
        description: Some("条件为真时路由到此".to_owned()),
        empty_policy: EmptyPolicy::default(),
        block_timeout_ms: None,
        ttl_ms: None,
    };
    let json = serde_json::to_string(&original).unwrap();
    let restored: PinDefinition = serde_json::from_str(&json).unwrap();
    assert_eq!(original, restored);
}

// ---- EmptyPolicy（ADR-0014 Phase 4）----

#[test]
fn empty_policy_默认是_block_until_ready() {
    assert_eq!(EmptyPolicy::default(), EmptyPolicy::BlockUntilReady);
}

#[test]
fn empty_policy_默认值序列化为_block_until_ready() {
    let v = serde_json::to_value(EmptyPolicy::default()).unwrap();
    assert_eq!(v, serde_json::json!({"kind": "block_until_ready"}));
}

#[test]
fn empty_policy_default_value_序列化携带_value() {
    let p = EmptyPolicy::DefaultValue(serde_json::json!(42));
    let v = serde_json::to_value(&p).unwrap();
    assert_eq!(v, serde_json::json!({"kind": "default_value", "value": 42}));
}

#[test]
fn pin_definition_缺_empty_policy_反序列化为默认() {
    let json = r#"{"id":"x","label":"x","pin_type":{"kind":"any"},"direction":"input","required":true,"kind":"data"}"#;
    let pin: PinDefinition = serde_json::from_str(json).unwrap();
    assert_eq!(pin.empty_policy, EmptyPolicy::BlockUntilReady);
    assert!(pin.block_timeout_ms.is_none());
    assert!(pin.ttl_ms.is_none());
}
