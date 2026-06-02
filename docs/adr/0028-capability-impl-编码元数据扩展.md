# ADR-0028: CapabilityImpl 编码元数据扩展

- **状态**: 已接受
- **日期**: 2026-06-02
- **决策者**: ssXue
- **关联**: RFC-0004（三段式 DSL）、RFC-0006（设备建模 Copilot 接管）

## 背景

`CapabilityImpl` 是 DSL 中描述能力底层实现方式的枚举，定义在 `crates/dsl-core/src/capability.rs`。当前有 5 个变体：

| 变体 | 字段 | 写入能力 |
|------|------|----------|
| `ModbusWrite` | `register`, `value`（模板） | 存在但不可自动生成 |
| `MqttPublish` | `topic`, `payload`（模板） | ✅ |
| `SerialCommand` | `command`（模板） | ✅ |
| `CanWrite` | `can_id`, `data`（模板）, `is_extended` | 存在但不可自动生成 |
| `Script` | `content` | EtherCAT PDO 写入的占位 |

### 问题

`build_capability_from_signal` 对 `SignalSource::Register` / `CanFrame` / `EthercatPdo` 三种写信号源**拒绝自动生成**能力，原因：

1. **`ModbusWrite` 缺编码元数据**：只有 `register` + `value` 模板。丢失 `data_type`（float32/u16/bool 等）、`bit`（位操作）、`byte_order`、`scale`（缩放表达式）。运行时 `execute_modbus_write` 仅支持 `write_single_register` 写单个 u16——无法正确编码 float32、i32、位域等数据类型。

2. **`CanWrite` 缺编码元数据**：只有 `can_id` + `data` 模板。丢失 `byte_offset`、`byte_length`、`data_type`、`byte_order`、`scale`。运行时 `execute_can_write` 做的是 `parse_hex_bytes`（解析十六进制字符串）再原样发帧——用户必须自行组装二进制帧。

3. **无 EtherCAT PDO 写入变体**：EtherCAT 从站 PDO 写入需要 `slave_address`、`pdo_index`、`entry_index`、`sub_index`、`bit_len` 等参数。当前没有对应变体，ESI 导入的 PDO 写信号完全不可能自动生成能力。

### 影响

- RFC-0006 的 `infer_capabilities_from_signals` 对 Modbus/CAN/EtherCAT 写信号全部返回 `Unsupported`，copilot 只能返回"结构化建议"让 AI 手动建模
- 工业现场最常见的三类协议（Modbus 写寄存器、CAN 写帧、EtherCAT PDO 写入）的能力自动生成被阻断
- `try_generate_capabilities_from_device()` fail-fast 正确，但这是功能缺失而非设计决策

## 决策

> 我们决定为 `CapabilityImpl` 的 `ModbusWrite` / `CanWrite` 变体补充编码元数据字段（`data_type`、`byte_order`、`scale` 等），新增 `EthercatPdoWrite` 变体，同步扩展 `CapabilityImplSnapshot` 和执行器，使 `build_capability_from_signal` 能为全部五种 `SignalSource` 自动生成能力。

### 具体变更

#### `CapabilityImpl` 枚举扩展

```rust
pub enum CapabilityImpl {
    ModbusWrite {
        register: u16,
        data_type: DataType,       // 新增：值编码类型
        #[serde(default)]
        bit: Option<u8>,           // 新增：位偏移（位域操作）
        #[serde(default)]
        byte_order: ByteOrder,     // 新增：字节序
        #[serde(default)]
        scale: Option<String>,     // 新增：缩放表达式
        value: String,             // 模板
    },
    MqttPublish {
        topic: String,
        payload: String,
    },
    SerialCommand {
        command: String,
    },
    CanWrite {
        can_id: u32,
        is_extended: bool,
        byte_offset: u8,           // 新增：帧内偏移
        byte_length: u8,           // 新增：数据长度
        data_type: DataType,       // 新增：值编码类型
        byte_order: ByteOrder,     // 新增：字节序
        #[serde(default)]
        scale: Option<String>,     // 新增：缩放表达式
        data: String,              // 模板（用户输入值，由执行器按编码字段编码到帧内）
    },
    EthercatPdoWrite {             // 新增变体
        #[serde(default)]
        slave_address: Option<u16>,
        pdo_index: u16,
        entry_index: u16,
        sub_index: u8,
        bit_len: u16,
        #[serde(default)]
        data_type: Option<String>,
        #[serde(default)]
        byte_order: ByteOrder,
        #[serde(default)]
        scale: Option<String>,
        value: String,             // 模板
    },
    Script {
        content: String,
    },
}
```

#### `CapabilityImplSnapshot` 同步

`CapabilityImplSnapshot`（`crates/nodes-io/src/capability_call.rs`）同步新字段，保持 conformance test 守护。

#### 执行器编码逻辑

- **Modbus 写入**：按 `data_type` 将模板解析值编码为正确字节数（float32 → 2 registers，u32 → 2 registers，bool → 0x0000/0x0001），`scale` 在编码前应用（逆缩放：物理值 → 原始值）。`bit` 字段支持位域写入（读-改-写）。
- **CAN 写入**：构建空帧 → 按 `byte_offset` / `byte_length` / `data_type` / `byte_order` 将值编码到帧的指定位置。`scale` 同理逆缩放。
- **EtherCAT PDO 写入**：通过 ethercrab 写入指定从站的 PDO 条目，按 `bit_len` / `data_type` 编码。

#### `build_capability_from_signal` 打通

对 `SignalSource::Register` / `CanFrame` / `EthercatPdo` 的写信号，直接从 `SignalSpec` 映射编码字段到 `CapabilityImpl`，不再返回错误。

#### 向后兼容

- 新增字段全部有 `#[serde(default)]`，现有 YAML 能力资产（只有 `register` + `value` 的 `ModbusWrite`，只有 `can_id` + `data` + `is_extended` 的 `CanWrite`）可正常反序列化
- 反序列化后编码字段走默认值（`data_type` 默认 `U16`，`byte_order` 默认 `BigEndian`）——与当前执行器行为一致（当前就是按 u16 + 大端序处理）
- `EthercatPdoWrite` 是全新变体，不存在旧 YAML

## 可选方案

### 方案 A：补编码字段（本提案）

- **优势**：类型系统保证编码完整性；`build_capability_from_signal` 全部打通；运行时不再丢失编码语义；YAML 能力资产可审查可审查
- **劣势**：`CapabilityImpl` 和 `CapabilityImplSnapshot` 字段膨胀；执行器复杂度增加（值编码逻辑）；需要同步更新 dsl-compiler conformance test、ts-rs 导出、前端 copilot 工具 schema

### 方案 B：仅新增 EthercatPdoWrite

- **优势**：最小改动；只解决 EtherCAT 完全不可能的问题
- **劣势**：Modbus 写 float32、CAN 写多字节仍不可能；`build_capability_from_signal` 对 Register/CanFrame 仍拒绝；问题只解决了 1/3

### 方案 C：用 Script 表达复杂编码

- **优势**：不扩展枚举；Rhai 脚本可表达任意编码逻辑
- **劣势**：Script 变体未接入执行器（当前 fail-fast）；脚本不可静态校验；丢失结构化元数据，运行时无法做安全校验；与"声明式 DSL 优于过程式脚本"的设计方向矛盾

## 后果

### 正面影响

- 全部五种 `SignalSource` 的写信号可自动生成能力——`infer_capabilities_from_signals` 不再返回 `Unsupported`（Register/CanFrame/EthercatPdo 的情况消除）
- Copilot 设备建模流程中"无法自动生成，AI 接手"的降级路径不再需要（至少对于标准信号编码）
- 运行时 `capabilityCall` 节点能正确编码非 u16 数据类型（float32、i32、位域等）
- 能力 YAML 资产包含完整编码信息，可审查、可校验、可跨环境复用

### 负面影响

- `CapabilityImpl` 枚举变体的字段数量增加，YAML 序列化体积增大
- `CapabilityImplSnapshot` 和执行器需同步维护——conformance test 守护一致性
- 新增字段的默认值（`U16` / `BigEndian`）意味着旧能力资产隐式升级到新编码语义，需在 CHANGELOG 中标注

### 风险

- **编码逻辑正确性**：值→帧编码是安全相关逻辑（工业现场寄存器写错值可引发事故）。缓解：每个 `DataType` × `ByteOrder` 组合都有单元测试覆盖；`scale` 逆缩放用 Rhai 表达式求值（复用已验证的引擎）
- **Modbus RTU vs TCP 写入差异**：当前执行器只支持 `write_single_register`（Function Code 06），`data_type` 为 u32/float32 时需用 `write_multiple_registers`（FC 16）。缓解：编码逻辑根据 `data_type` 字节数自动选择 FC
- **EtherCAT PDO 写入的并发安全**：ethercrab 的 PDO 写入需要持有 TX/RX 任务锁。缓解：与现有 `ethercatPdoRead` 节点共享同一会话管理机制

## 备注

- 此 ADR 解决 RFC-0006 未解决问题 #1（`CapabilityImpl` 编码表达力扩展）
- `CapabilityImpl::Script` 变体保留作为无法用结构化编码表达的兜底，但不再是 EtherCAT 的唯一选择
- ADR-0029（Copilot 资产操作两阶段确认门控）是配套变更，与本 ADR 独立但互补
