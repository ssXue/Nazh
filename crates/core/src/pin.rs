//! 节点引脚（Pin）声明系统：把节点的输入/输出端口提升为一等契约。
//!
//! 设计动机与决策见 ADR-0010；落地范围见
//! `docs/superpowers/plans/2026-04-26-pin-declaration-system.md`。
//!
//! # 与 [`NodeCapabilities`](crate::NodeCapabilities) 的关系
//!
//! - `NodeCapabilities` 是**类型级**契约——同类型所有实例 + 所有 config 必同。
//! - `PinDefinition` 是**实例级**契约——`switch` 节点的输出 pin 由 `branches`
//!   配置决定；因此 [`NodeTrait::output_pins`](crate::NodeTrait::output_pins) 是
//!   `&self` 实例方法而非 `'static` 表。
//!
//! 两套机制是互补的，不要尝试用 caps 表达 pin、或反之。
//!
//! # 默认值与渐进式迁移
//!
//! [`NodeTrait`] 默认实现把每个节点声明为单 [`Any`](PinType::Any) 输入 + 单
//! [`Any`](PinType::Any) 输出，老节点无需改动即可通过部署期校验。需要"具名
//! 多端口"或"严格类型"的节点显式 override 即可。
//!
//! # 序列化形态
//!
//! `PinType` 用 `#[serde(tag = "kind", rename_all = "lowercase")]` 形成可辨识
//! 联合，前端用 `switch (pin.kind)` 分派。比 ts-rs 默认 `{ Array: ... }` map
//! 形式好用，且对递归 `Box<PinType>` 友好。

use std::fmt;

use serde::{Deserialize, Serialize};
#[cfg(feature = "ts-export")]
use ts_rs::TS;

/// 引脚方向：输入 / 输出。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum PinDirection {
    Input,
    Output,
}

impl fmt::Display for PinDirection {
    /// 中文标签，供 `EngineError` 的 `#[error(...)]` 模板与日志使用。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Input => "输入",
            Self::Output => "输出",
        })
    }
}

/// 引脚的求值语义。与 [`PinType`]（数据形状）正交。
///
/// 设计动机与决策见 ADR-0014（重构后的"引脚二分"方案）。
///
/// - [`Exec`](Self::Exec)：上游完成 transform → MPSC push → 下游 transform。
///   这是 Nazh 1.0 的默认语义；所有现有节点不显式声明时走这条路径。
/// - [`Data`](Self::Data)：上游完成 transform → 写入输出缓存槽（不 push）；
///   下游被自己的 `Exec` 边触发时在 transform 前从缓存槽拉取（Phase 2 起）。
///
/// **设计前提**：引脚对引脚必须 `PinKind` 一致——`Exec` 只能连 `Exec`、`Data` 只能连 `Data`。
/// 部署期 [`pin_validator`](crate::PinDefinition) 拒绝跨 Kind 连接。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum PinKind {
    /// 推语义。**默认值**——所有现有引脚不声明时为 Exec，向后兼容。
    #[default]
    Exec,
    /// 拉语义。上游写缓存、下游被自己的 Exec 边触发时读缓存。
    Data,
}

impl fmt::Display for PinKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Exec => "exec",
            Self::Data => "data",
        })
    }
}

impl PinKind {
    /// 判断"上游引脚 self → 下游引脚 other"在求值语义维度上是否兼容。
    /// 规则：必须严格相等——Exec ↔ Exec、Data ↔ Data。
    #[must_use]
    pub fn is_compatible_with(self, other: Self) -> bool {
        self == other
    }
}

impl fmt::Display for PinType {
    /// 类型名标签，匹配 `#[serde(tag = "kind", rename_all = "lowercase")]` 序列化形态。
    /// 供 `EngineError::Variable*Mismatch` 等错误消息与日志复用。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Any => "any",
            Self::Bool => "bool",
            Self::Integer => "integer",
            Self::Float => "float",
            Self::String => "string",
            Self::Json => "json",
            Self::Binary => "binary",
            Self::Array { .. } => "array",
            Self::Custom { .. } => "custom",
        })
    }
}

/// 引脚承载的数据形状。
///
/// **Phase 1 不带 JSON Schema payload**——`Json` 仅声明"该端口流通的是任意 JSON"，
/// 结构校验留待未来独立 ADR（避免 Ring 0 引入 `schemars`/`jsonschema` 依赖）。
///
/// `Custom { name }` 是协议特定类型的逃生口（如 `"modbus-register"`）。
/// 兼容矩阵要求两端的 `name` **精确相等**——这是有意为之，避免协议级类型被
/// 错误地与 `Any` 之外的"近义"类型自动桥接。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum PinType {
    /// 兼容所有类型；常用于默认值与脚本节点（`code` / `if` 等）。
    Any,
    Bool,
    Integer,
    Float,
    String,
    /// 任意 JSON 对象/数组——Phase 1 不附 schema。
    Json,
    /// 二进制字节流（`Vec<u8>` 或 base64 字符串）。
    Binary,
    /// 同质数组，元素类型由 `inner` 给出。
    Array {
        inner: Box<PinType>,
    },
    /// 命名的协议级自定义类型；兼容性需精确同名匹配。
    Custom {
        name: String,
    },
}

impl PinType {
    /// 判断"上游产出 self → 下游期望 other"是否兼容。
    ///
    /// 兼容矩阵（ADR-0010 部署期校验规则的代码化）：
    /// - 任一端是 [`Any`](Self::Any) → 通过
    /// - 标量类型 → 精确相等才通过
    /// - [`Array`](Self::Array) → 嵌套递归 + 内层各自兼容
    /// - [`Custom`](Self::Custom) → name 精确相等
    /// - 跨类（`String` ↔ `Integer`、`Json` ↔ `Bool` 等）→ 不通过
    ///
    /// **注意**：`Json → Json` 通过、`Json → Any` 通过、`Any → Json` 通过；
    /// 但 `Json → Integer` 拒绝——`Json` 是结构上的"任意"，类型上仍是独立类。
    pub fn is_compatible_with(&self, other: &Self) -> bool {
        // Any 双向吃一切——匹配 ADR 矩阵的前两行。
        if matches!(self, Self::Any) || matches!(other, Self::Any) {
            return true;
        }

        match (self, other) {
            (Self::Bool, Self::Bool)
            | (Self::Integer, Self::Integer)
            | (Self::Float, Self::Float)
            | (Self::String, Self::String)
            | (Self::Json, Self::Json)
            | (Self::Binary, Self::Binary) => true,

            (Self::Array { inner: a }, Self::Array { inner: b }) => a.is_compatible_with(b),

            (Self::Custom { name: a }, Self::Custom { name: b }) => a == b,

            _ => false,
        }
    }
}

/// 节点引脚声明。
///
/// `id` 是该节点上的稳定标识（部署后不可变）；运行时 [`NodeDispatch::Route`]
/// 路由的 port id 必须能在 `output_pins()` 里找到，否则部署期校验失败。
///
/// [`NodeDispatch::Route`]: crate::NodeDispatch::Route
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
pub struct PinDefinition {
    /// 引脚稳定标识（如 `"in"` / `"true"` / `"body"` / `"high"`）。
    pub id: String,
    /// 前端展示名；可与 `id` 不同（例如分支节点的中文标签）。
    pub label: String,
    /// 引脚承载的数据形状。
    pub pin_type: PinType,
    /// 引脚方向。
    pub direction: PinDirection,
    /// **输入引脚**：是否必须有上游边指向。**输出引脚**：是否每次执行必触发。
    pub required: bool,
    /// 求值语义（ADR-0014 引脚二分）。未声明默认 [`PinKind::Exec`]，向后兼容现有节点。
    #[serde(default)]
    pub kind: PinKind,
    /// 给前端 / AI 的描述文本。
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub description: Option<String>,
}

impl PinDefinition {
    /// 默认输入引脚：`id = "in"`、`Any`、必需。
    ///
    /// `NodeTrait::input_pins` 的默认实现返回 `vec![default_input()]`——存量节点
    /// 不重写就能继续用单输入语义。
    ///
    /// **关于 `required: true` 与根节点**：根节点（拓扑入度为 0）不通过
    /// [`WorkflowEdge`](crate::WorkflowEdge) 接收数据，而是由
    /// [`WorkflowIngress::submit`](crate::context::WorkflowContext) 直接喂入。
    /// 部署期校验器对 `id == "in"` 的默认输入引脚豁免"必有上游入边"检查，
    /// 让根节点可以是单 `Any` 输入而不被误判为缺边。具名 required input
    /// （`id != "in"`）则一律要求上游入边——这种节点不该是根节点。
    pub fn default_input() -> Self {
        Self {
            id: "in".to_owned(),
            label: "in".to_owned(),
            pin_type: PinType::Any,
            direction: PinDirection::Input,
            required: true,
            kind: PinKind::Exec,
            description: None,
        }
    }

    /// 默认输出引脚：`id = "out"`、`Any`、非必需。
    ///
    /// `required: false` 反映"广播节点不一定每次都产出（例如 `tryCatch` 走
    /// catch 分支时主输出不触发）"——把所有输出都当 required 会让 phase 1
    /// 校验在大多数节点上误报。
    pub fn default_output() -> Self {
        Self {
            id: "out".to_owned(),
            label: "out".to_owned(),
            pin_type: PinType::Any,
            direction: PinDirection::Output,
            required: false,
            kind: PinKind::Exec,
            description: None,
        }
    }

    /// 单端口节点的"必需输入"工厂——`id="in"` / `label="in"` / `required=true`。
    ///
    /// 协议节点（如 `sqlWriter` / `httpClient`）大多数声明 `Json` 类型且要求
    /// 上游必有入边（除非作为根节点；详见 [`Self::default_input`]）。比逐字段
    /// 拼 `PinDefinition { ... }` 字面量短得多。
    pub fn required_input(pin_type: PinType, description: impl Into<String>) -> Self {
        Self {
            id: "in".to_owned(),
            label: "in".to_owned(),
            pin_type,
            direction: PinDirection::Input,
            required: true,
            kind: PinKind::Exec,
            description: Some(description.into()),
        }
    }

    /// 单端口节点的输出工厂——`id="out"` / `label="out"` / `required=false`。
    pub fn output(pin_type: PinType, description: impl Into<String>) -> Self {
        Self {
            id: "out".to_owned(),
            label: "out".to_owned(),
            pin_type,
            direction: PinDirection::Output,
            required: false,
            kind: PinKind::Exec,
            description: Some(description.into()),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
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
    }

    #[test]
    fn pin_kind_反序列化从小写字符串() {
        let exec: PinKind = serde_json::from_str("\"exec\"").unwrap();
        let data: PinKind = serde_json::from_str("\"data\"").unwrap();
        assert_eq!(exec, PinKind::Exec);
        assert_eq!(data, PinKind::Data);
    }

    #[test]
    fn pin_kind_兼容性必须严格相等() {
        assert!(PinKind::Exec.is_compatible_with(PinKind::Exec));
        assert!(PinKind::Data.is_compatible_with(PinKind::Data));
        assert!(!PinKind::Exec.is_compatible_with(PinKind::Data));
        assert!(!PinKind::Data.is_compatible_with(PinKind::Exec));
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
        let json = r#"{"id":"in","label":"in","pin_type":{"kind":"any"},"direction":"input","required":true}"#;
        let pin: PinDefinition = serde_json::from_str(json).unwrap();
        assert_eq!(pin.kind, PinKind::Exec);
    }

    #[test]
    fn pin_definition_显式_kind_字段反序列化正确() {
        let json = r#"{"id":"latest","label":"latest","pin_type":{"kind":"any"},"direction":"output","required":false,"kind":"data"}"#;
        let pin: PinDefinition = serde_json::from_str(json).unwrap();
        assert_eq!(pin.kind, PinKind::Data);
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
        };
        let json = serde_json::to_string(&original).unwrap();
        let restored: PinDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(original, restored);
    }
}
