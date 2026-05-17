# ADR-0024: 设备信号读取与事件触发节点

- **状态**: 已实施
- **日期**: 2026-05-15
- **决策者**: ssXue
- **关联**: ADR-0021（三段式 DSL 编译）、ADR-0009（节点生命周期钩子）、ADR-0010（Pin 声明系统）、ADR-0014（引脚求值语义二分）、ADR-0018（nodes-io feature 门控）

## 背景

### 现状与痛点

Nazh 已有 `modbusRead`、`canRead`、`ethercatPdoRead`、`serialTrigger` 等协议级 I/O 节点，但存在三个结构性缺陷：

1. **无共享信号解码逻辑**。`modbusRead` 返回原始 `u16` 字（`crates/nodes-io/src/modbus_read.rs` `word_values_to_json`），`canRead` 返回原始 hex 字符串（`crates/nodes-io/src/can/can_read.rs`），`ethercatPdoRead` 返回裸 PDO 字节。三者均不做 `DataType` 解码、`ByteOrder` 转换或 `scale` 表达式求值。`SignalSpec` 已在 `dsl-core` 中定义了完整的 `DataType`、`ByteOrder`、`scale` 字段（`crates/dsl-core/src/device.rs` `SignalSource` 枚举），但没有任何节点消费这些字段。

2. **DSL 信号层与运行时节点断裂**。`dsl-compiler`（`crates/dsl-compiler/src/context.rs`）仅使用 `DeviceSpec.connection` 解析 `connection_id`（`connection_id_for_device`），未涉及 `SignalSpec`。编译器生成的 `capabilityCall` 节点（`crates/nodes-io/src/capability_call.rs`）使用 `CapabilityImplSnapshot` 枚举烘焙协议细节，但这是"写"路径的封装——"读"路径缺少等价抽象。

3. **事件归一化缺失**。`serialTrigger` 通过 `on_deploy` 后台循环接收串口帧并 emit（`crates/nodes-io/src/serial_trigger/mod.rs`），MQTT 订阅通过 `mqttClient` 的 `Subscribe` 模式。两者输出 payload 结构不同（`serial_data`/`serial_hex` vs MQTT 原始 topic payload），下游业务节点无法用统一模式处理。

### 设计目标

- **`deviceSignalRead`**：输入 `device_id` + `signal_id`，按 `DeviceSpec.connection` 解析连接，按 `SignalSource` 读取原始数据，按 `DataType`/`ByteOrder`/`bit` 解码，按 `scale` 缩放，输出 `device_id`/`signal_id`/`value`/`unit`/`sampled_at`。poll 语义——exec 触发 + data 缓存（对标 `modbusRead` 的 `out` + `latest` 双 pin）。
- **`deviceEventTrigger`**：归一化串口/MQTT/CAN 事件为设备事件，后台生命周期（对标 `serialTrigger` 的 `on_deploy` 模式），输出 `device_id`/`event_type`/`payload`/`received_at`。
- **低层协议细节进 metadata 不进 payload**。寄存器地址、CAN ID、topic 名称等协议元数据进入 `metadata` 字段（对标 `modbusRead` 的 `metadata.modbus`、`canRead` 的 `metadata.can`），payload 只保留语义化字段。
- **simulation fail-fast 模式**。未配置连接时必须显式 `simulation=true`，否则报错（对标 `canRead`/`modbusRead` 的 `simulation` 开关）。

## 决策

> 我们决定采用**方案 A：两个独立节点 + 编译期信号快照 + nodes-io 共享解码模块**。核心理由：poll 和 event 的生命周期模型根本不同（`transform` 同步读 vs `on_deploy` 后台循环），强行合并会让 config 膨胀且生命周期逻辑交错。编译期信号快照复用 `CapabilityImplSnapshot` 已验证的模式，运行时零注册表依赖。共享解码模块让两个节点及未来的 `deviceSignalWrite` 复用同一套解码逻辑。

### 节点一：`deviceSignalRead`

**能力标签**：`NodeCapabilities::DEVICE_IO`

**Config 结构**：

```rust
/// 信号源快照——编译期从 SignalSpec.source 复制。
/// 与 dsl-core::SignalSource 对应但独立定义，
/// conformance test 守护一致性（对标 CapabilityImplSnapshot 模式）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SignalSourceSnapshot {
    Register {
        register: u16,
        data_type: DataTypeSnapshot,
        bit: Option<u8>,
    },
    CanFrame {
        can_id: u32,
        is_extended: bool,
        byte_offset: u8,
        byte_length: u8,
        data_type: DataTypeSnapshot,
        byte_order: ByteOrderSnapshot,
    },
    Topic {
        topic: String,
    },
    SerialCommand {
        command: String,
    },
    EthercatPdo {
        slave_address: Option<u16>,
        pdo_index: u16,
        entry_index: u16,
        sub_index: u8,
        bit_len: u16,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceSignalReadConfig {
    pub connection_id: Option<String>,
    pub device_id: String,
    pub signal_id: String,
    pub source: SignalSourceSnapshot,
    /// Rhai 缩放表达式（如 `"raw * 35 / 65535"`）。
    pub scale: Option<String>,
    pub unit: Option<String>,
    pub simulation: bool,
}
```

**Pin 声明**（对标 `modbusRead`）：

| 引脚 | 类型 | Kind | 说明 |
|------|------|------|------|
| `out` | Json | Exec | 每次 exec 触发时的读取结果 |
| `latest` | Json | Data | 拉取式槽位，缓存最近读数 |

**Output payload**：

```json
{
  "device_id": "hydraulic_press_1",
  "signal_id": "pressure",
  "value": 17.5,
  "unit": "MPa",
  "sampled_at": "2026-05-15T10:30:00Z"
}
```

**Output metadata**：

```json
{
  "device_signal": {
    "device_id": "hydraulic_press_1",
    "signal_id": "pressure",
    "source_type": "register",
    "simulated": false
  },
  "modbus": {}
}
```

**Scale 求值**：复用 `crates/scripting` 已有的 Rhai `Engine` + `AST` 编译。对 `scale` 表达式编译一次，后续每次 `evaluate` 传入 `{ "raw": decoded_value }` 作为 payload scope。Rhai 已在依赖树中，不需要引入新依赖。

### 节点二：`deviceEventTrigger`

**能力标签**：`NodeCapabilities::TRIGGER | NodeCapabilities::DEVICE_IO`

**Config 结构**：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceEventTriggerConfig {
    pub connection_id: Option<String>,
    pub device_id: String,
    /// 监听的信号 ID 列表（编译期从 DeviceSpec.signals 过滤）。
    pub signals: Vec<SignalListenerSnapshot>,
    pub simulation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalListenerSnapshot {
    pub signal_id: String,
    pub source: SignalSourceSnapshot,
    pub scale: Option<String>,
    pub unit: Option<String>,
}
```

**生命周期**：`on_deploy` 启动后台事件循环（对标 `serialTrigger`）。按 `SignalSourceSnapshot` 类型决定监听方式：

| source 类型 | 监听方式 |
|------------|---------|
| `Topic` | MQTT 订阅（复用 `mqttClient` 的 `rumqttc` 连接） |
| `SerialCommand` | `spawn_blocking` 串口读循环（复用 `serialTrigger` 的帧协议） |
| `CanFrame` | 共享 CAN 会话 `recv` 循环（复用 `canRead` 的 `CanBusRuntime`） |
| `Register` | 定时 poll（内部 `tokio::time::interval` + Modbus TCP 读） |

**Output payload**：

```json
{
  "device_id": "hydraulic_press_1",
  "signal_id": "pressure",
  "event_type": "signal_update",
  "value": 17.5,
  "unit": "MPa",
  "received_at": "2026-05-15T10:30:00Z"
}
```

### 共享解码模块

在 `crates/nodes-io/src/signal_decode.rs` 中提供：

```rust
/// 按 DataType 解码原始字节数组为目标 Value。
pub fn decode_raw_bytes(
    raw: &[u8],
    data_type: DataTypeSnapshot,
    byte_order: ByteOrderSnapshot,
    bit: Option<u8>,
) -> Result<Value, DecodeError>;

/// 对已解码值执行 Rhai scale 表达式。scale 为 None 时直接返回原值。
pub fn apply_scale(
    raw_value: Value,
    scale: &Option<String>,
) -> Result<Value, ScaleError>;
```

此模块不依赖任何具体协议 crate——它只接受 `&[u8]` + 类型描述。具体协议读取仍由各节点按 `SignalSourceSnapshot` 分支调用。

### DSL 编译器集成

`dsl-compiler` 在编译 Workflow DSL 中引用设备信号的节点时：

1. 从 `CompilerContext.devices` 查找 `DeviceSpec`
2. 在 `DeviceSpec.signals` 中查找目标 `SignalSpec`
3. 将 `SignalSpec.source` / `scale` / `unit` 烘焙为 `SignalSourceSnapshot`
4. 从 `DeviceSpec.connection` 解析 `connection_id`
5. 输出 `deviceSignalRead` 或 `deviceEventTrigger` 节点的 config JSON

### Feature 门控

`deviceSignalRead` / `deviceEventTrigger` 内部按 `SignalSourceSnapshot` 分支依赖具体协议 crate，复用已有的 `io-modbus` / `io-can` / `io-serial` / `io-mqtt` feature 控制。编译时，节点对不支持协议的 source 类型返回明确错误（对标 `capability_call::executor.rs` 的 `#[cfg(not(feature = "io-modbus"))]` 模式）。

## 可选方案

### 方案 A：两个独立节点 + 编译期信号快照 + 共享解码模块（推荐）

- 优势：
  - **生命周期模型清晰**：poll 节点走 `transform` 同步读，event 节点走 `on_deploy` 后台循环——两者不交叉，不互相拖累
  - **复用已验证模式**：编译期快照复用 `CapabilityImplSnapshot`；simulation fail-fast 复用 `modbusRead` 模式；`on_deploy` 后台循环复用 `serialTrigger` 模式；`out` + `latest` 双 pin 复用 ADR-0014 的 Data pin 拉取语义
  - **运行时零注册表依赖**：信号源、解码参数、scale 表达式全部烘焙进 config，不查注册表
  - **共享解码逻辑复用**：`decode_raw_bytes` + `apply_scale` 被 `deviceSignalRead`、`deviceEventTrigger`、以及未来的 `deviceSignalWrite` 共用
  - **与 DSL 编译器自然对齐**：编译器已持有 `DeviceSpec`/`SignalSpec`，烘焙快照是增量工作
  - **Feature 门控兼容**：协议级依赖仍按已有 feature 控制，无需新增 feature

- 劣势：
  - 两个节点增加注册表条目和前端节点库展示
  - 编译期快照意味着 DSL 修改信号定义后需重新编译导入画布（与 ADR-0021 "画布载入后 DSL 源变为历史快照" 的心智模型一致）
  - `deviceEventTrigger` 内部按 `SignalSourceSnapshot` 分支走不同监听方式，实现复杂度较高

### 方案 B：单节点 `deviceIO` + mode 切换

一个节点，config 中 `mode: "poll" | "event"` 决定行为。poll 走 `transform`，event 走 `on_deploy`。

- 优势：
  - 注册表只多一条目，前端节点库更紧凑
  - config 共享 `device_id` / `connection_id` / `simulation` 等字段

- 劣势：
  - **生命周期逻辑在一个节点内分叉**：`transform` 和 `on_deploy` 的实现都变成 `match mode`，测试矩阵翻倍
  - **Pin 声明不一致**：poll 模式需要 `out`(Exec) + `latest`(Data)，event 模式只需要 `out`(Exec)，Pin 声明变成 `mode` 的函数——违反 ADR-0010 "Pin 声明是类型级契约"的设计
  - **NodeCapabilities 矛盾**：poll 不需要 `TRIGGER`，event 需要——注册时只能选一种
  - **config 膨胀**：poll 字段（`signal_id`）和 event 字段（`signals[]`）混合在一个 struct 里

### 方案 C：纯 DSL 生成，不提供手动画布放置

`deviceSignalRead` / `deviceEventTrigger` 不作为前端可拖拽节点，只由 `dsl-compiler` 在编译 Workflow DSL 时生成到 `WorkflowGraph` JSON 中。

- 优势：
  - 前端节点库不增加条目，产品心智模型更简单——"设备信号读写都由 DSL 声明"
  - 编译器对信号语义有完整控制，不存在"用户手配 signal_id 打错"的问题

- 劣势：
  - **违反 ADR-0021 的核心原则**："画布是唯一编辑/部署真值源"。如果 DSL 生成的节点不能在画布上可视化、不能手动调整 config、不能与其他手拖节点混用，画布就退化为"只读渲染器"
  - **调试困难**：画布上看不到信号读取节点的中间状态，无法在画布上直接调试信号值
  - **与已有模式不一致**：`capabilityCall` 也是 DSL 生成的，但仍然在画布上可见可调

### 方案 D：扩展 `capabilityCall` 加入读语义

在 `CapabilityImplSnapshot` 中新增 `ModbusRead`、`CanRead` 等变体，让 `capabilityCall` 节点同时承担读和写。

- 优势：
  - **零新节点**：不增加注册表条目
  - **与现有 capability 模型统一**：读写都是"能力调用"

- 劣势：
  - **`capabilityCall` 职责膨胀**：当前 `CapabilityCallNode::transform` 已是 `match implementation` 分支，加入 5 种 `SignalSource` 的读变体后分支数翻倍
  - **生命周期冲突**：`capabilityCall` 当前无 `on_deploy` / `on_undeploy`，纯 `transform` 模式。如果 event 类型信号需要后台监听，就必须给 `capabilityCall` 加 `on_deploy`，改变其生命周期模型
  - **Pin 声明冲突**：读操作需要 `latest`(Data) pin，写操作不需要——与现有单一 `out` pin 不兼容
  - **语义混淆**：`capabilityCall` 的产品心智模型是"执行设备能力"，读信号更接近"查询设备状态"，两者语义不同

## 后果

### 正面影响

- **闭环 DSL 信号层到运行时节点**：`DeviceSpec.signals` 定义 → 编译器烘焙为快照 → 节点执行解码/缩放 → 输出语义化值。信号定义不再停留在文档层
- **共享解码逻辑消除重复**：`DataType` 解码、`ByteOrder` 转换、bit 提取、`scale` Rhai 求值集中在一个模块，`modbusRead`/`canRead` 等现有节点未来可选择性迁移到共享模块
- **两个节点生命周期模型清晰**：poll 节点不需要 `on_deploy`，event 节点必须 `on_deploy`，两者不交叉
- **与现有架构对齐**：编译期快照模型（`CapabilityImplSnapshot` 已验证）、simulation fail-fast（`modbusRead`/`canRead` 已验证）、connection_id 继承（`inherit_connection_id` 已存在）、feature 门控（ADR-0018 已建立模式）
- **为 `deviceSignalWrite` 铺路**：共享解码模块 + 编译期快照模型可自然扩展到写路径

### 负面影响

- 新增两个节点类型增加前端节点库展示和用户认知负担
- `deviceEventTrigger` 内部协议分支复杂：需为 `Topic`/`SerialCommand`/`CanFrame`/`Register` 各实现不同的事件监听循环，代码量和测试矩阵都较大
- 编译期快照导致 DSL-画布分叉后信号定义不可更新——用户在画布上修改快照节点的 `register` 地址后，DSL 源不会同步更新（与 ADR-0021 已确立的"画布载入后变独立副本"心智模型一致）
- Rhai scale 求值引入运行时开销——每次执行都 eval Rhai AST，虽然 `ScriptNodeBase` 已做编译缓存，但仍比简单算术慢

### 风险

| 风险 | 缓解 |
|------|------|
| `deviceEventTrigger` 内部协议分支过多，维护困难 | 每种协议源的事件循环复用已有后端（`serial_trigger::serial_loop`、`can::session`、MQTT subscribe），只做"接收原始帧 → 解码 → emit"的胶水层 |
| 编译期快照与 `dsl-core::SignalSource` schema 漂移 | 复用 conformance test 模式：`dsl-compiler` dev-dependency 测试同时验证两种 serde 格式 |
| Rhai `scale` 表达式安全性 | 复用 `ScriptNodeBase` 的 `max_operations` 上限（默认 50,000 步） |
| `deviceEventTrigger` 的 `Register` 定时 poll 与 `deviceSignalRead` 功能重叠 | 明确分工：`deviceSignalRead` 是按需同步读（exec 触发），`deviceEventTrigger` 的 Register 分支是周期性异步推送（interval + emit）。两者 payload 结构一致但触发方式不同 |

## 备注

### 实施分阶段建议

**Phase 1（MVP）**：
- `deviceSignalRead` 节点，仅支持 `Register`（Modbus TCP）源
- `signal_decode.rs` 共享解码模块（`DataType` 解码 + `ByteOrder` 转换 + bit 提取）
- `scale` Rhai 求值集成
- `dsl-compiler` 新增 `device_signal_read` 节点生成路径
- simulation fail-fast + `out`/`latest` 双 pin
- 注册表合约测试更新（节点总数 27 → 28）

**Phase 2（事件监听）**：
- `deviceEventTrigger` 节点，支持 `Topic`（MQTT）和 `CanFrame` 事件源
- 复用 `mqttClient` 的 `rumqttc` 连接和 `can::session::CanBusRuntime`
- 注册表合约测试更新（节点总数 28 → 29）

**Phase 3（完整协议覆盖）**：
- `deviceSignalRead` 支持 `CanFrame` / `Topic` / `EthercatPdo` / `SerialCommand` 源
- `deviceEventTrigger` 支持 `Register`（定时 poll）和 `SerialCommand`（串口帧监听）
- 前端 FlowGram 节点库新增两个节点卡片，归入"设备能力"分组

### 与已有 ADR 的关系

- 与 ADR-0021 的关系：本 ADR 是 ADR-0021 "DSL 编译产物进入画布" 在信号读取场景下的具体实现。`dsl-compiler` 在编译包含设备信号引用的 Workflow DSL 时，将 `SignalSpec` 烘焙为 `SignalSourceSnapshot` 并生成节点 config。
- `SignalSourceSnapshot` 的 serde 格式应与 `dsl-core::SignalSource` 保持字段级兼容，conformance test 守护一致性。
- Rhai 依赖已在 `crates/scripting` 中引入，`nodes-io` 可通过依赖 `scripting` crate 复用，不引入新依赖。
- 节点边界评审阶段 3 的完整结论见根 `AGENTS.md` 设计原则 3（设备语义高于协议适配）及 `docs/project-status.md`「设备/连接节点边界收口」段。
