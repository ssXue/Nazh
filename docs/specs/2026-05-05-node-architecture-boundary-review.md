# 节点架构边界评审

> **状态：** 评审结论，待收口
> **日期：** 2026-05-05
> **触发：** 复核项目初始原则：设备是高级节点，CAN 卡、串口等物理通道是全局连接资源
> **关联：** RFC-0004、ADR-0005、ADR-0009、ADR-0018、ADR-0021

## 背景

项目初始设计有一条关键边界：业务工作流不应直接以 CAN 卡、串口、Modbus 连接等物理/协议通道为主语。通道应进入全局 `ConnectionManager`，由连接治理、RAII 借出、健康状态和重连策略统一管理；工作流层应表达设备信号、设备事件、设备能力和业务状态机。

2026-05-05 的代码复核显示，这条边界的基础设施已经成形，但产品节点面仍然把低层协议适配器暴露为一等业务节点。当前最应收口的是：让 Device / Capability DSL 产出的高级节点真正接管协议执行，同时把 `serialTrigger` / `modbusRead` / `canRead` / `canWrite` 等低层节点降级为适配器、调试工具和兼容层。

## 总体判断

| 维度 | 判断 |
|------|------|
| 连接资源层 | 方向正确。`WorkflowGraph.connections` 是图级连接资源，部署时统一写入 `ConnectionManager`，见 `crates/graph/src/types.rs:26` 与 `crates/graph/src/deploy.rs:160`。 |
| 连接治理 | 方向正确。串口、Modbus、MQTT、HTTP、Bark、CAN 的连接配置校验集中在 `crates/connections/src/lib.rs:1113`。 |
| 节点暴露面 | 偏离初衷。前端普通节点库仍直接列出 `serialTrigger`、`modbusRead`、`mqttClient`、`canRead`、`canWrite`、`capabilityCall`，见 `web/src/components/flowgram/flowgram-node-library.ts:94`。 |
| DSL 高级层 | 已有骨架。`DeviceSpec` 已包含 `connection` 与 `SignalSource::{Register, Topic, SerialCommand, CanFrame}`，见 `crates/dsl-core/src/device.rs:10` 与 `crates/dsl-core/src/device.rs:53`。`CapabilityImpl` 已覆盖底层动作快照，见 `crates/dsl-core/src/capability.rs:58`。 |
| 运行时高级节点 | 未完成闭环。编译器已经把设备连接 ID 写入 `capabilityCall` 节点顶层字段，见 `crates/dsl-compiler/src/output.rs:290`；但 `CapabilityCallNode` 没保存该连接 ID，`connection_manager` 当前未使用，见 `crates/nodes-io/src/capability_call.rs:58`。 |

结论：**不要删除低层协议节点，但不要让它们继续作为业务构图主路径。** 它们应该成为高级设备节点背后的协议后端，或保留在高级/调试入口中。

## 节点逐类建议

| 节点/类别 | 当前状态 | 建议定位 |
|-----------|----------|----------|
| `stateMachine` | DSL 编译器生成的业务状态机节点，注册在 `FlowPlugin`，见 `crates/nodes-flow/src/lib.rs:121`。 | 保留为高级业务语义节点。 |
| `capabilityCall` | DSL 编译器生成的设备能力调用节点，注册为 `DEVICE_IO`，见 `crates/nodes-io/src/lib.rs:154`。当前只输出动作意图 payload，没有真实执行协议。 | 升级为设备动作主入口，必须真正借用 `ConnectionManager` 执行底层能力实现。 |
| `serialTrigger` | 部署时借串口连接并启动监听循环，见 `crates/nodes-io/src/serial_trigger.rs:327`。 | 保留为底层事件适配器和调试节点；普通业务应优先使用设备事件节点或由 Device DSL 生成的事件入口。 |
| `modbusRead` | 直接暴露 `unit_id/register/quantity/register_type`，并在无连接时模拟，见 `crates/nodes-io/src/modbus_read.rs:325`。 | 保留为点位调试和兼容节点；普通业务应使用设备信号读取节点，由 Device DSL 信号定义驱动寄存器读取、scale 和类型解码。 |
| `canRead` / `canWrite` | 直接暴露 CAN 帧读写语义，`canRead` 无连接时回退 Mock，见 `crates/nodes-io/src/can/can_read.rs:81`。 | 保留为 CAN 总线适配器；业务节点应表达设备信号或设备能力，而不是 `can_id` + `data`。 |
| `mqttClient` | `publish` / `subscribe` 双模式，按 config 切换 pin 声明。 | 作为外部消息系统节点可以保留；作为设备协议时应被 Device / Capability 层封装。 |
| `httpClient` / `barkPush` / `sqlWriter` | 外部通信、本地持久化与通知效果节点。 | 可以继续作为业务工作流效果节点，但连接参数仍必须放在 Connection Studio。 |
| `native` | 可选借连接并输出连接元数据。 | 降级为调试/兼容工具，避免成为绕过设备语义层的通道。 |
| 流程控制 / 纯计算 / 人工审批 / 子图 | 不直接涉及物理通道。 | 保持现状，继续作为业务编排积木。 |

## 收口顺序

### 1. 先修 `capabilityCall` 的真实执行闭环

这是最高优先级。`dsl-compiler` 已经生成 `connection_id`，但 `CapabilityCallNode` 没读取这个顶层字段。收口应包括：

- 在 `IoPlugin::register("capabilityCall", ...)` 中继承 `WorkflowNodeDefinition::connection_id()`，或把连接 ID 明确纳入 `CapabilityCallConfig`。
- `CapabilityImplSnapshot::ModbusWrite` 真实执行 Modbus 写入。
- `CapabilityImplSnapshot::MqttPublish` 真实发布 MQTT 消息。
- `CapabilityImplSnapshot::SerialCommand` 真实通过串口连接发送命令。
- `CapabilityImplSnapshot::CanWrite` 真实发送 CAN 帧。
- 元数据同时保留 `"capability_call"` 与底层协议元数据，避免观测层失去设备语义。
- 测试覆盖：无连接失败、连接类型不匹配失败、模拟/Mock 后端成功、模板参数解析成功、连接健康状态 mark success/failure。

注意：`capabilityCall` 不应通过调用其他 `NodeTrait` 实例来“复用节点”，否则会把节点生命周期、metadata 和 dispatch 语义混在一起。更合适的方向是在 `nodes-io` 内提取协议操作 helper，供 `capabilityCall` 与低层调试节点共同使用。

### 2. 补设备信号读取/事件入口

设备输入侧还缺高级主语。建议新增或由 DSL 编译生成：

- `deviceSignalRead`：输入为设备 ID + signal ID，内部根据 `DeviceSpec.connection` 与 `SignalSource` 读取寄存器、MQTT topic、串口帧或 CAN 帧，并执行 scale / byte order / bit 解码。
- `deviceEventTrigger`：把串口、MQTT、CAN 等长连接事件规范化为设备事件，而不是把原始协议帧直接暴露给业务图。
- 输出 payload 应以设备语义命名，如 `device_id`、`signal_id`、`value`、`unit`、`sampled_at`，底层 `register` / `can_id` / `topic` 等进入 metadata。

这一步完成后，`modbusRead` / `serialTrigger` / `canRead` 的普通业务使用场景就可以迁移到设备语义节点。

### 3. 调整前端节点库和 AI 编排默认面

前端当前把低层节点列在普通库里，且连接匹配逻辑直接围绕 `serialTrigger` / `canRead` / `canWrite` / `modbusRead` 展开，见 `web/src/components/flowgram/nodes/settings-shared.tsx:204`。

建议：

- 新增“设备能力”或“设备语义”分类，放置 `capabilityCall`、未来 `deviceSignalRead`、未来 `deviceEventTrigger`。
- 将 `serialTrigger` / `modbusRead` / `canRead` / `canWrite` 移到“底层适配器”或“调试工具”分组。
- AI 编排默认只生成设备语义节点；低层协议节点仅在用户明确要求“调试寄存器/CAN 帧/串口帧”时出现。
- 节点卡片文案避免把 CAN 卡、串口描述成业务能力；应描述为“适配器/调试入口”。

### 4. 收紧生产环境的模拟回退

`modbusRead` 无连接走正弦模拟，`canRead` / `canWrite` 无连接走 Mock。这个行为对 demo 和测试友好，但对工业运行容易掩盖配置缺失。

建议：

- 增加显式 `simulation: true` 或运行时策略开关。
- 生产运行策略默认禁止 `DEVICE_IO` 节点无连接执行。
- UI 在部署前提示“当前为模拟通道”，并把模拟状态写入审计事件。

### 5. 用 ADR 或 RFC 承接真正的架构变更

本文是评审结论，不直接替代 ADR。若后续 PR 改变以下任一项，应补 ADR 或更新 RFC-0004 实施进度：

- 新增设备信号读取节点或设备事件节点。
- 改变 `capabilityCall` 的运行时执行语义。
- 调整前端普通节点库中低层协议节点的默认可见性。
- 改变生产环境下无连接模拟回退策略。

## 文档同步项

已发现并需要同步的状态偏差：

- `docs/project-status.md` 曾写 `capabilityCall` 已“协议执行”，但当前代码还没有真实借用连接执行协议动作。应改为“动作快照/意图输出，真实协议执行待收口”。
- `docs/project-status.md` 的 RFC-0004 Phase 3 节点总数写 22，当前 `src/registry.rs` 合约测试为 24 种标准节点。

后续实现收口时，同步检查：

- Root `AGENTS.md` 设计原则。
- `crates/nodes-io/AGENTS.md` 节点定位与能力表。
- Root `README.md` 节点目录。
- `web/src/components/flowgram/nodes/catalog.ts` 与节点定义文案。
- `docs/project-status.md` 的日期标头与当前状态。
