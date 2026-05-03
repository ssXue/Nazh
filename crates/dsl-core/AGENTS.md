# `crates/dsl-core` — 三段式 DSL 类型定义与 YAML 解析（Ring 1）

## 这是什么

RFC-0004 Phase 0 的实现。定义设备（Device）、能力（Capability）、工作流（Workflow）
三种 DSL 的结构化类型（`*Spec` 系列），并提供从 YAML 文本解析这些类型的 API。

本 crate 是纯数据 + 解析层，不含编译逻辑、运行时依赖或协议驱动。

## 对外暴露

```text
crates/dsl-core/src/
├── lib.rs          # re-exports
├── error.rs        # DslError (YamlParse / Validation / JsonSerialization)
├── device.rs       # DeviceSpec / SignalSpec / AlarmSpec / ConnectionRef / SignalSource / SignalType / AccessMode / DataType / AlarmSeverity
├── capability.rs   # CapabilitySpec / CapabilityParam / CapabilityOutput / CapabilityImpl / SafetyConstraints / SafetyLevel
├── workflow.rs     # WorkflowSpec / StateSpec / TransitionSpec / ActionSpec / ActionTarget / Range / HumanDuration
└── parser.rs       # parse_device_yaml / parse_capability_yaml / parse_workflow_yaml
```

## 内部约定

- 所有 `*Spec` 类型 derive `Serialize + Deserialize`，支持 YAML/JSON round-trip
- 信号、能力、动作均使用数组形式（`Vec<SignalSpec>`），ID 作为结构体字段而非 YAML 映射键
- `Range`（量程区间）YAML 表示为 `[min, max]` 数组，自定义反序列化为 `Range { min, max }`
- `HumanDuration`（时长）YAML 表示为字符串（"30s"/"5m"/"1h"/"500ms"），自定义反序列化为毫秒数
- `ActionTarget` 使用 `#[serde(flatten)]` 映射 `capability: <id>` / `action: <id>` 形式

## 修改本 crate 时

- 加新字段：确保有 `#[serde(default)]` 或 `#[serde(skip_serializing_if)]`，保持向后兼容
- 加新 DSL 类型：在 `src/` 下新建模块，在 `lib.rs` 声明 `mod` + `pub use`，更新本 AGENTS.md
- 测试：每个模块内 `#[cfg(test)] mod tests`，使用内联 YAML 字符串作为 fixture

## 依赖约束

仅依赖 `serde` + `serde_json` + `serde_yaml` + `thiserror`。
**不得**依赖 `nazh-core` / `connections` / `scripting` / `nodes-*` / `ai` / `graph` / `store`——
本 crate 是 Ring 1 中"零运行时依赖"的纯数据层。Phase 1+ 编译器 crate 按需引入 Ring 0 类型。
