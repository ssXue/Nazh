# `crates/dsl-core` — 三段式 DSL 类型定义与 YAML 解析（Ring 1）

## 这是什么

RFC-0004 Phase 0 + Phase 1 + Phase 2 的实现。定义设备（Device）、能力（Capability）、工作流（Workflow）
三种 DSL 的结构化类型（`*Spec` 系列），提供从 YAML 文本解析这些类型的 API，
`SignalSpec` → `PinDefinition` 映射函数（Phase 1），以及
能力校验和从设备信号自动生成能力（Phase 2）。当前 `CapabilityImpl` 不能无损表达 CAN /
Modbus / EtherCAT 写入编码语义时，生成入口必须 fail-fast 或跳过，不能生成看似可执行但丢失
`byte_offset` / `data_type` / `byte_order` / `scale` / PDO 位宽等信息的能力。

本 crate 是纯数据 + 解析 + 校验层，不含编译逻辑、运行时依赖或协议驱动。

## 对外暴露

```text
crates/dsl-core/src/
├── lib.rs          # re-exports
├── error.rs        # DslError (YamlParse / Validation / JsonSerialization)
├── device.rs       # DeviceSpec / SignalSpec / AlarmSpec / ConnectionRef / SignalSource（含 CanFrame / EthercatPdo）/ SignalType / AccessMode / DataType / ByteOrder / AlarmSeverity
├── capability.rs   # CapabilitySpec / CapabilityParam / CapabilityOutput / CapabilityImpl（含 CanWrite）/ SafetyConstraints / SafetyLevel + validate() + generate_capabilities_from_device() / try_generate_capabilities_from_device()（Phase 2）
├── workflow.rs     # WorkflowSpec / StateSpec / TransitionSpec / ActionSpec / ActionTarget / Range / HumanDuration
├── parser.rs       # parse_*_yaml / parse_*_yaml_validated
└── pin_mapping.rs  # signal_to_pin_type / signal_to_direction / signal_id_to_label / signals_to_pin_definitions（Phase 1）
```

## 内部约定

- 所有 `*Spec` 类型 derive `Serialize + Deserialize`，支持 YAML/JSON round-trip
- 信号、能力、动作均使用数组形式（`Vec<SignalSpec>`），ID 作为结构体字段而非 YAML 映射键
- `Range`（量程区间）YAML 表示为 `[min, max]` 数组，自定义反序列化为 `Range { min, max }`
- `HumanDuration`（时长）YAML 表示为字符串（"30s"/"5m"/"1h"/"500ms"），自定义反序列化为毫秒数
- `ActionTarget` 使用 `#[serde(flatten)]` 映射 `capability: <id>` / `action: <id>` 形式
- `pin_mapping` 模块依赖 `nazh-core` 的 `PinDefinition` / `PinType` / `PinDirection` 等类型
- `SignalSource::EthercatPdo.slave_address` 是可选字段：标准 ESI 设备目录可省略；ENI/主站配置导入多从站拓扑时必须填入物理从站地址，以便同一 PDO 条目能区分不同轴/从站。
- 保存、导入、AI 生成等资产入口应优先走 `parse_*_yaml_validated()`；兼容性解析入口只做反序列化，不代表资产可执行。
- `CapabilitySpec::validate()` 必须覆盖 required input range、模板变量是否存在于 inputs、重复 input/output id、fallback 非空/非重复/非自引用等可在单资产内验证的约束。
- 从 Device signal 生成 Capability 时，`try_generate_capabilities_from_device()` 是带诊断的入口；`generate_capabilities_from_device()` 仅保留兼容用途。当前结构无法无损表达 CAN / Modbus / EtherCAT 编码语义时必须拒绝生成，而不是用 `${value}` 伪装成功。

## 修改本 crate 时

- 加新字段：确保有 `#[serde(default)]` 或 `#[serde(skip_serializing_if)]`，保持向后兼容
- 加新 DSL 类型：在 `src/` 下新建模块，在 `lib.rs` 声明 `mod` + `pub use`，更新本 AGENTS.md
- 测试：每个模块内 `#[cfg(test)] mod tests`，使用内联 YAML 字符串作为 fixture

## 依赖约束

依赖 `serde` + `serde_json` + `serde_yaml` + `thiserror` + `nazh-core`（Phase 1 pin_mapping）。
**不得**依赖 `connections` / `scripting` / `nodes-*` / `ai` / `graph` / `store`——
本 crate 是 Ring 1 中"零运行时依赖"的纯数据层（`nazh-core` 是 Ring 0 类型依赖）。
