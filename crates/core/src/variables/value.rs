use serde_json::Value;

use crate::PinType;

/// 判定 JSON 值是否匹配 `PinType`。
///
/// 这是"运行时校验"——`PinType::Any` 接受任何 `Value`，标量精确匹配，
/// `Json` 接受 Object / Array，`Binary` 接受 Array of u8 或 base64 字符串
/// （Phase 1 仅校验形态，不解 base64）。
///
/// **`Custom` 在 Phase 1 完全拒绝**——既不能在 `from_declarations` 通过初值校验，
/// 也不能在 `set` / `compare_and_swap` 写入。命名类型语义需要"产出 Custom 输出
/// 的节点对齐"（参见 ADR-0010 Phase 4 deferred Item 2），变量与节点的 `Custom`
/// 引入要同步而非分头开启；触发条件就绪后将由专门 ADR 升级。
#[must_use]
pub fn pin_type_matches_value(pin_type: &PinType, value: &Value) -> bool {
    match (pin_type, value) {
        // Any 接受一切；Bool/Float/String/Json/Binary(base64字符串) 形态匹配
        (PinType::Any, _)
        | (PinType::Bool, Value::Bool(_))
        | (PinType::Float, Value::Number(_)) // i64/u64/f64 都接受
        | (PinType::String | PinType::Binary, Value::String(_)) // String 精确 / Binary base64假定
        | (PinType::Json, Value::Object(_) | Value::Array(_)) => true,

        (PinType::Integer, Value::Number(n)) => n.is_i64() || n.is_u64(),

        // Binary 字节数组：每个元素必须在 u8 范围内
        (PinType::Binary, Value::Array(arr)) => {
            arr.iter()
                .all(|v| v.as_u64().is_some_and(|n| u8::try_from(n).is_ok()))
        }

        // 同质数组：递归校验每个元素
        (PinType::Array { inner }, Value::Array(arr)) => {
            arr.iter().all(|item| pin_type_matches_value(inner, item))
        }

        // Phase 1: Custom 完全拒绝（声明初值与运行时写入皆然），见函数级 doc
        _ => false,
    }
}

pub(super) fn json_value_label(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
        Value::Number(_) => "float",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}
