# AI 原生工业边缘工作流编排引擎项目 Review 文档

> 文档目的：用于发起一次阶段性技术 review，统一项目定位、系统边界、核心架构、技术栈选择、模块划分、风险点和后续研发路线。

---

## 1. 项目定位

本项目旨在设计一套 **AI 原生工业边缘工作流编排引擎**。

其核心目标不是做传统上位机、低代码平台或单纯的工业网关，而是：

> 将工业现场的设备说明书、设备数据流、控制约束和业务描述，编译为安全、可验证、可执行、可审计的边缘业务逻辑。

换句话说，工业现场工程师只需要：

1. 描述业务逻辑；
2. 提供子设备说明书、点表或协议文档；
3. 完成必要的人工确认与安全审批；

系统即可自动生成设备模型、能力模型和边缘工作流，并部署到现场运行时执行。

---

## 2. 一句话定义

### 产品化表达

> 一个面向工业边缘的 AI 工作流编排引擎：通过理解设备说明书和现场业务描述，自动生成、验证并部署可执行的工业逻辑。

### 工程化表达

> 将工业设备语义、数据流和控制约束编译为安全可执行的边缘工作流。

### 面向用户的表达

> 让工程师只描述“想让现场怎么运行”，系统自动理解设备、生成逻辑、校验安全并部署到边缘。

---

## 3. 项目不是什么

为了避免产品边界发散，本项目暂不应被定义为以下几类系统：

| 类型 | 为什么不是 |
|---|---|
| 普通工业上位机 | 上位机通常以监控、配置、操作为主，本项目核心是业务逻辑生成、编译、验证和部署 |
| 传统低代码平台 | 低代码通常依赖人工拖拽流程，本项目强调从说明书和自然语言自动生成结构化工业逻辑 |
| 工业协议网关 | 协议接入只是基础能力，不是核心壁垒 |
| AI 聊天助手 | AI 只是生成、解释和诊断模块，不能直接控制设备 |
| 实时控制器 | 毫秒级或微秒级硬实时闭环仍应由 PLC、MCU、伺服驱动器或专用控制器负责 |

---

## 4. 核心设计原则

项目应长期坚持以下原则：

```text
AI 生成，规则验证；
语义编排，确定执行；
边缘自治，云端辅助；
业务流程可解释，危险动作可审计；
LLM 不直接控制设备，所有动作必须经过能力层；
Tauri 是壳，Rust crate 是内核，edge-daemon 是执行体；
DSL 是产品核心资产，Safety Compiler 是核心护城河。
```

---

## 5. 总体系统架构

推荐整体架构如下：

```text
┌─────────────────────────────────────────────┐
│ Tauri Desktop App                           │
│ 设备建模 / 工作流编辑 / 仿真 / 部署 / Review │
└─────────────────────────────────────────────┘
                      ↓
┌─────────────────────────────────────────────┐
│ Rust Core Engine                            │
│ DSL 解析 / 编译 / 校验 / 状态机 / 安全策略   │
└─────────────────────────────────────────────┘
                      ↓
┌─────────────────────────────────────────────┐
│ Edge Runtime / Edge Daemon                  │
│ 数据采集 / 事件流 / 工作流执行 / 本地日志    │
└─────────────────────────────────────────────┘
                      ↓
┌─────────────────────────────────────────────┐
│ Industrial IO Layer                         │
│ Modbus / MQTT / CAN / OPC UA / ADC / Mock   │
└─────────────────────────────────────────────┘
```

---

## 6. 六层逻辑架构

### 6.1 设备语义层

目标：将设备说明书、点表、寄存器表、协议文档、报警码表转换为结构化设备模型。

这一层不应只做点位解析，而应抽象出设备能力。

例如，不只是识别：

```yaml
register: 40001
name: pressure
unit: MPa
access: read
```

而是进一步抽象为：

```text
读取压力
设定位移
启动泵站
判断过载
执行泄压
进入安全停机
```

#### 关键输出

- `DeviceSpec`
- `SignalSpec`
- `ProtocolSpec`
- `AlarmSpec`
- `DeviceCapabilityCandidate`

---

### 6.2 能力抽象层

目标：将底层寄存器、信号和协议操作封装成安全、受约束的设备能力。

AI 不能直接写寄存器，而应调用能力层工具。

示例：

```yaml
capability:
  id: hydraulic_axis.move_to
  inputs:
    position:
      unit: mm
      range: [0, 150]
  preconditions:
    - servo_ready == true
    - emergency_stop == false
    - pressure < 32MPa
  effects:
    - axis_state = moving
  fallback:
    - hydraulic_axis.stop
```

能力层需要表达：

- 输入参数；
- 单位；
- 量程；
- 前置条件；
- 状态依赖；
- 副作用；
- fallback；
- 安全等级；
- 是否需要人工审批。

---

### 6.3 意图编排层

目标：将用户自然语言业务描述转换为结构化工作流 DSL。

示例需求：

> 当压力超过 30MPa 持续 2 秒，停止推进，打开泄压阀，并记录报警。如果压力恢复到 20MPa 以下，允许复位。

生成 DSL：

```yaml
workflow:
  id: pressure_protection

  trigger:
    type: condition
    expression: pressure > 30MPa
    duration: 2s

  actions:
    - call: actuator.stop_motion
    - call: valve.open_relief
    - call: alarm.record
      args:
        level: high
        message: "Pressure exceeded 30MPa"

  recovery:
    condition: pressure < 20MPa
    actions:
      - call: alarm.allow_reset
```

---

### 6.4 安全编译与验证层

目标：AI 生成的设备模型、能力模型和工作流不能直接运行，必须经过静态验证和安全编译。

需要校验：

- DSL 语法；
- Schema；
- 单位一致性；
- 量程边界；
- 读写权限；
- 协议映射；
- 状态机完整性；
- 危险动作审批；
- 机械互锁；
- 故障 fallback；
- 资源占用；
- 运行周期；
- 数据源是否存在；
- 设备能力是否匹配；
- 是否存在不可达状态；
- 是否存在无出口状态；
- 是否存在循环触发风险。

这一层可以命名为：

```text
Safety Compiler
Industrial Policy Engine
Workflow Validator
```

这是项目最重要的护城河之一。

---

### 6.5 边缘运行时层

目标：在工业现场确定性执行工作流。

边缘侧应尽量避免依赖大模型。大模型负责生成和解释，运行时负责确定执行。

运行时模块：

```text
Workflow Runtime
├── Device Connectors
│   ├── Modbus TCP
│   ├── Modbus RTU
│   ├── CAN
│   ├── OPC UA
│   ├── MQTT
│   └── Mock Device
│
├── Stream Processor
│   ├── filter
│   ├── debounce
│   ├── moving average
│   ├── window
│   ├── threshold
│   └── event detection
│
├── Rule / State Machine Engine
│   ├── workflow execution
│   ├── timers
│   ├── transitions
│   ├── retries
│   └── compensation
│
├── Safety Guard
│   ├── interlocks
│   ├── limits
│   ├── emergency paths
│   ├── manual override
│   └── action gate
│
└── Observability
    ├── logs
    ├── traces
    ├── replay
    ├── alarm records
    └── audit records
```

---

### 6.6 观测、回放与诊断层

目标：每一次设备数据变化、条件触发、状态转移、动作调用、安全拒绝、人工审批都必须可追踪。

应支持：

- 运行日志；
- 工作流 trace；
- 设备读写记录；
- 报警记录；
- 安全拒绝记录；
- 人工审批记录；
- 数据回放；
- 仿真对比；
- AI 辅助诊断。

---

## 7. 推荐三段式 DSL

建议将 DSL 拆为三类，而不是所有内容混在一个配置文件里。

---

### 7.1 Device DSL

用于描述设备、信号、协议、寄存器和数据转换。

```yaml
device:
  id: pressure_sensor_1
  type: analog_sensor
  protocol: adc
  sample_rate: 1000Hz

signals:
  pressure:
    unit: MPa
    range: [0, 35]
    source: adc.ch1
    scale: "raw * 35 / 8388607"
```

---

### 7.2 Capability DSL

用于描述设备能力，而不是裸点位。

```yaml
capability:
  id: hydraulic_axis.move_to
  inputs:
    position:
      unit: mm
      range: [0, 150]

  preconditions:
    - servo_ready == true
    - emergency_stop == false
    - pressure < 32MPa

  effects:
    - axis_state = moving

  fallback:
    - hydraulic_axis.stop
```

---

### 7.3 Workflow DSL

用于描述业务状态机、触发条件、动作和异常处理。

```yaml
workflow:
  id: auto_pressing_cycle

  states:
    - idle
    - approaching
    - pressing
    - holding
    - returning
    - fault

  transitions:
    - from: idle
      to: approaching
      when: start_button == true
      do:
        - hydraulic_axis.move_to(approach_position)

    - from: approaching
      to: pressing
      when: position >= approach_position

    - from: pressing
      to: holding
      when: pressure >= target_pressure
      do:
        - hydraulic_axis.hold_pressure(target_pressure)

    - from: holding
      to: returning
      when: hold_time >= 5s
      do:
        - hydraulic_axis.move_to(home_position)

    - from: "*"
      to: fault
      when: pressure > 34MPa
      do:
        - hydraulic_axis.stop
        - alarm.raise("Over pressure")
```

---

## 8. Rust + Tauri 技术栈定位

### 8.1 技术栈选择结论

当前选择：

```text
核心语言：Rust
桌面端：Tauri
前端：TypeScript + React / Vue / Svelte
边缘运行时：Rust daemon
配置/DSL：YAML / JSON / 自定义 DSL
本地存储：SQLite
通信：HTTP / gRPC / WebSocket
```

这是合理选择，原因如下：

| 选择 | 价值 |
|---|---|
| Rust | 内存安全、长期运行稳定、适合协议接入和边缘 runtime |
| Tauri | 跨平台桌面端，Rust 后端与 Web UI 结合，适合工业工程工具 |
| TypeScript | 快速构建复杂可视化 UI 和工作流编辑器 |
| SQLite | 适合边缘本地持久化、部署记录、日志索引、项目配置 |
| DSL | 可审计、可验证、可回放、可编译 |

---

### 8.2 Tauri 的角色边界

Tauri 应该是：

```text
工业 AI 编排 IDE
本地工程配置工具
工作流可视化编辑器
仿真与部署控制台
边缘运行状态监控台
```

Tauri 不应该是：

```text
真正的工业运行时
硬实时控制器
直接设备驱动层
安全逻辑唯一执行点
```

重要原则：

> Tauri 是壳，Rust 核心 crate 是内核，edge-daemon 是现场执行体。

---

## 9. 推荐 Rust Workspace 结构

建议采用 workspace，避免 Tauri app 和核心逻辑耦合。

```text
industrial-ai-edge/
├── apps/
│   ├── desktop-tauri/
│   ├── edge-daemon/
│   └── edge-cli/
│
├── crates/
│   ├── device-model/
│   ├── capability-model/
│   ├── workflow-dsl/
│   ├── workflow-compiler/
│   ├── safety-checker/
│   ├── runtime-core/
│   ├── stream-engine/
│   ├── protocol-modbus/
│   ├── protocol-mqtt/
│   ├── protocol-can/
│   ├── simulator/
│   ├── deployment/
│   ├── observability/
│   └── ai-adapter/
```

---

## 10. 三个可执行程序

### 10.1 `desktop-tauri`

面向工程师使用。

功能：

- 项目管理；
- 上传说明书；
- 设备模型生成；
- 点表确认；
- 能力建模；
- 工作流编辑；
- 安全校验；
- 仿真回放；
- 部署到边缘；
- 查看日志、报警和 trace；
- AI 辅助解释。

---

### 10.2 `edge-daemon`

现场运行时。

功能：

- 设备数据采集；
- 设备动作执行；
- 工作流状态机运行；
- 安全策略执行；
- 本地日志；
- 本地报警；
- 断网自治；
- 远程部署包接收；
- WebSocket 实时数据推送。

---

### 10.3 `edge-cli`

工程调试工具。

示例命令：

```bash
edge-cli validate workflow.yaml
edge-cli simulate workflow.yaml --input test-data.csv
edge-cli deploy --target 192.168.1.20
edge-cli logs --tail
edge-cli device scan --modbus
edge-cli inspect bundle.tar
```

---

## 11. Rust 内部核心模型

建议定义以下核心结构。

### 11.1 DeviceSpec

```rust
pub struct DeviceSpec {
    pub id: String,
    pub name: String,
    pub protocol: ProtocolSpec,
    pub signals: Vec<SignalSpec>,
    pub capabilities: Vec<CapabilitySpec>,
}
```

---

### 11.2 SignalSpec

```rust
pub struct SignalSpec {
    pub id: String,
    pub name: String,
    pub unit: Option<String>,
    pub data_type: DataType,
    pub access: AccessMode,
    pub range: Option<Range>,
    pub source: SignalSource,
}
```

---

### 11.3 CapabilitySpec

```rust
pub struct CapabilitySpec {
    pub id: String,
    pub inputs: Vec<InputSpec>,
    pub preconditions: Vec<Expr>,
    pub effects: Vec<Effect>,
    pub action: ActionBinding,
}
```

---

### 11.4 WorkflowSpec

```rust
pub struct WorkflowSpec {
    pub id: String,
    pub states: Vec<State>,
    pub transitions: Vec<Transition>,
    pub safety: Vec<SafetyRule>,
}
```

---

## 12. 推荐依赖栈

### 12.1 异步运行时

```toml
tokio = { version = "1", features = ["full"] }
```

用途：

- 设备轮询；
- MQTT；
- WebSocket；
- HTTP/gRPC；
- 日志写入；
- 任务调度；
- AI 服务调用。

注意：Tokio 适合软实时业务逻辑，不适合严格硬实时控制。

---

### 12.2 序列化与 DSL

```toml
serde = "1"
serde_yaml = "0.9"
serde_json = "1"
schemars = "0.8"
jsonschema = "0.18"
```

用途：

- `device.yaml`
- `capability.yaml`
- `workflow.yaml`
- Schema 生成；
- 配置校验；
- 前端表单生成。

---

### 12.3 错误处理

```toml
thiserror = "1"
anyhow = "1"
```

建议：

- library crate 使用 `thiserror`；
- app 层使用 `anyhow`。

---

### 12.4 日志与可观测性

```toml
tracing = "0.1"
tracing-subscriber = "0.3"
tracing-appender = "0.2"
```

必须记录：

- 设备读写；
- 状态转移；
- workflow trace id；
- action call；
- safety rejection；
- alarm；
- deployment；
- manual approval。

---

### 12.5 数据库

```toml
sqlx = { version = "0.8", features = ["sqlite", "runtime-tokio"] }
```

建议存储：

- 项目配置；
- 设备模型；
- 工作流版本；
- 部署记录；
- 日志索引；
- 报警记录；
- 审批记录；
- 仿真结果。

---

### 12.6 通信层

可选：

```toml
axum = "0.7"
tonic = "0.12"
tokio-tungstenite = "0.23"
```

建议分工：

| 通信类型 | 建议方案 |
|---|---|
| 控制 API | HTTP 或 gRPC |
| 实时数据流 | WebSocket |
| 部署包 | tar + manifest |
| 边缘设备发现 | mDNS / 手动 IP / 配置文件 |

---

## 13. 工业协议支持路线

### 第一阶段

优先支持：

```text
Modbus TCP
Modbus RTU
MQTT
Mock Device
CSV / JSON 回放输入
```

原因：

- 工程复杂度可控；
- 足以验证核心工作流编排价值；
- 适合液压、电机、传感器、边缘盒子等场景；
- 便于仿真和测试。

---

### 第二阶段

可考虑：

```text
CANopen
OPC UA
串口自定义协议
厂商 SDK 适配
```

---

### 暂不优先

```text
EtherCAT 主站
Profinet 深度接入
高实时运动控制
复杂 PLC runtime 替代
```

原因：这些方向容易将项目拖入“工业协议栈/实时控制器”赛道，偏离 AI 原生编排引擎的核心价值。

---

## 14. AI 模块设计

AI 不应直接成为运行时控制器，而应作为生成、解释和诊断模块。

### 14.1 AI 可以做

```text
读取说明书
抽取点表
解释报警码
生成设备模型
生成能力模型
生成工作流 DSL
生成测试用例
解释校验失败原因
解释运行日志
辅助故障诊断
```

### 14.2 AI 不应直接做

```text
实时闭环控制
直接写寄存器
绕过安全策略
自动修改线上流程
未经审批执行危险动作
替代急停和安全链路
```

### 14.3 AI 输出约束

AI 输出必须是结构化结果，例如：

```json
{
  "devices": [],
  "signals": [],
  "capabilities": [],
  "uncertainties": [],
  "warnings": []
}
```

随后由 Rust 完成：

```text
Schema 校验
单位校验
量程校验
协议校验
安全校验
人工确认
```

---

## 15. Tauri 桌面端设计

### 15.1 页面模块

建议包含：

```text
Project
Devices
Signals
Capabilities
Workflows
Simulation
Deployments
Runtime Monitor
Alarms
AI Assistant
Settings
```

---

### 15.2 UI 布局建议

可采用类似现代 macOS / Liquid Glass 风格的桌面布局：

```text
左侧：浮动圆角导航栏
中间：主工作区 / 工作流画布 / 设备拓扑
右侧：属性检查器 / AI 解释 / 校验结果
底部：日志 / 诊断 / trace / 仿真时间轴
顶部：项目状态 / 连接状态 / 部署状态
```

---

### 15.3 工作流画布

建议前期使用成熟库，不要过早自研画布引擎。

可选：

```text
React Flow
Vue Flow
Rete.js
XState Visualizer 思路
```

原则：

> 前端画布只是 DSL 的可视化表达，真实存储和执行仍以 DSL 为准。

---

### 15.4 Tauri Command 示例

```rust
#[tauri::command]
async fn validate_workflow(workflow_yaml: String) -> Result<ValidationReport, String> {
    // 调用 workflow-compiler crate
}
```

```rust
#[tauri::command]
async fn simulate_workflow(
    workflow_yaml: String,
    test_data: String,
) -> Result<SimulationResult, String> {
    // 调用 simulator crate
}
```

```rust
#[tauri::command]
async fn deploy_to_edge(
    target: String,
    bundle: DeploymentBundle,
) -> Result<DeployResult, String> {
    // 调用 deployment client
}
```

---

## 16. MVP 路线建议

### V0.1：DSL 与校验器

目标：先证明“业务逻辑可被结构化表达并验证”。

内容：

- 定义 `device.yaml`；
- 定义 `capability.yaml`；
- 定义 `workflow.yaml`；
- Rust parser；
- Rust validator；
- CLI validate；
- 输出校验报告。

---

### V0.2：仿真器

目标：证明工作流可在离线数据上执行。

内容：

- 输入 CSV / JSON 传感器数据；
- 运行状态机；
- 输出状态转移；
- 输出动作列表；
- 输出报警；
- 输出 safety rejection；
- 支持回放。

---

### V0.3：Tauri 桌面端

目标：形成可 review、可演示、可交互的工程工具。

内容：

- 项目管理；
- 设备模型编辑；
- 工作流编辑；
- 校验报告；
- 仿真回放；
- AI 面板占位；
- 部署面板占位。

---

### V0.4：Modbus / MQTT 边缘运行时

目标：接入真实或半真实工业设备。

内容：

- Modbus TCP/RTU 采集；
- MQTT 上报；
- workflow runtime；
- 本地 SQLite 日志；
- WebSocket 实时监控；
- 本地部署包加载。

---

### V0.5：AI 说明书抽取

目标：体现 AI 原生能力。

内容：

- 上传 PDF；
- 抽取点表；
- 生成 `device.yaml`；
- 生成 `capability.yaml` 草案；
- 标注不确定字段；
- 人工确认；
- 自动生成测试用例。

---

## 17. Review 重点问题

本次 review 建议重点讨论以下问题。

### 17.1 产品边界

- 系统到底是“工业 AI 编排引擎”，还是“上位机 + AI 助手”？
- 是否明确不做硬实时控制？
- 是否明确不替代 PLC、伺服控制器和安全继电器？
- 第一阶段聚焦哪个工业场景？液压试验台？EHA？通用设备编排？

---

### 17.2 DSL 设计

- Device / Capability / Workflow 是否应该拆成三套 DSL？
- DSL 当前表达能力是否足够？
- 是否需要支持状态机、规则流、事件流三种模型？
- 表达式语言选型是什么？自研还是使用现成表达式引擎？
- DSL 如何版本化？
- DSL 如何迁移？

---

### 17.3 安全编译器

- 当前已经能检查哪些错误？
- 哪些检查必须在 MVP 完成？
- 单位、量程、状态机、互锁、动作权限如何表达？
- 安全拒绝是否可解释？
- 危险动作是否需要人工审批？
- 部署前是否必须仿真通过？

---

### 17.4 运行时

- edge-daemon 和 desktop-tauri 是否已经解耦？
- 运行时是否可以脱离桌面端独立运行？
- 断网后是否可以继续执行？
- 日志是否本地持久化？
- 部署包格式是否稳定？
- 版本回滚是否支持？

---

### 17.5 AI 接入

- AI 目前参与哪个阶段？
- AI 输出是否全部经过结构化 schema？
- 是否显式记录 AI 不确定项？
- 是否禁止 AI 直接发出设备动作？
- 说明书解析失败时的 fallback 是什么？

---

### 17.6 桌面端体验

- Tauri 端是否只是 UI 壳？
- 前端状态和 DSL 状态是否一致？
- 工作流画布是否可逆生成 DSL？
- 校验错误是否能定位到具体节点 / 字段？
- 仿真回放是否能解释每次状态转移？

---

## 18. 当前阶段建议优先级

如果项目已经开发到一定阶段，建议下一步优先收敛以下内容。

### P0：必须明确

- 三段式 DSL 边界；
- Rust 核心 crate 与 Tauri app 解耦；
- edge-daemon 独立运行；
- workflow validator 基础能力；
- 本地仿真闭环；
- 日志与 trace 数据结构。

### P1：尽快完成

- 设备模型 schema；
- 能力模型 schema；
- workflow schema；
- CLI validate；
- CSV/JSON 回放仿真；
- Tauri 校验报告页面；
- 部署包 manifest。

### P2：随后增强

- Modbus TCP/RTU 接入；
- MQTT 上报；
- WebSocket 实时监控；
- AI PDF 抽取；
- 设备模型库；
- 安全策略库；
- 工作流模板库。

---

## 19. 关键风险

| 风险 | 表现 | 建议 |
|---|---|---|
| 变成普通上位机 | 重点放在 UI、监控和手动配置 | 强化 DSL、编译器和运行时 |
| AI 权限过大 | AI 直接控制设备或修改线上逻辑 | AI 只生成候选，必须校验和审批 |
| 协议适配拖慢主线 | 过早支持 EtherCAT、Profinet 等复杂协议 | 第一阶段聚焦 Modbus/MQTT/Mock |
| DSL 过早复杂化 | 一开始做成完整编程语言 | 先覆盖状态机、条件、动作、报警、fallback |
| Tauri 与核心耦合 | 所有逻辑写进桌面 app | 核心逻辑沉淀为 Rust crates |
| 安全校验不足 | 生成流程能跑但不可控 | Safety Compiler 作为 P0 设计 |
| 无仿真能力 | 只能上线验证 | 先做 CSV/JSON 回放仿真 |
| 日志不可追踪 | 出问题无法解释 | 所有状态转移和动作必须 trace |

---

## 20. 推荐阶段性验收标准

### 技术验收

- 可以用 YAML 描述一个设备；
- 可以用 YAML 描述一个能力；
- 可以用 YAML 描述一个业务工作流；
- validator 可以发现非法单位、越界参数、缺失设备、非法动作；
- simulator 可以用离线数据跑完整工作流；
- edge-daemon 可以独立执行部署包；
- Tauri 可以展示校验结果、仿真过程和运行日志。

### 产品验收

- 用户可以上传或创建设备模型；
- 用户可以描述业务流程；
- 系统可以生成可编辑工作流；
- 系统可以解释为什么某个流程不能部署；
- 系统可以在部署前仿真；
- 系统可以记录每次部署与运行结果。

### 安全验收

- AI 不能直接写设备；
- 所有动作必须经过 Capability；
- 所有危险动作必须经过 Safety Guard；
- 所有部署必须有版本记录；
- 所有安全拒绝必须可解释；
- 所有运行异常必须可追溯。

---

## 21. 建议 Review 会议议程

```text
1. 项目定位确认：是否统一为“工业业务逻辑编译与边缘执行系统”
2. 当前实现状态介绍：已完成模块、未完成模块、技术债
3. DSL 设计评审：Device / Capability / Workflow 是否合理
4. Safety Compiler 评审：当前校验能力与缺口
5. Runtime 架构评审：desktop / daemon / cli 是否解耦
6. AI 接入边界评审：生成、解释、诊断，但不直接控制
7. MVP 路线确认：下一阶段优先级和验收标准
8. 风险与决策项记录
```

---

## 22. 建议 Review 输出物

本次 review 结束后建议形成以下输出：

- 明确的项目边界；
- 确认后的系统架构图；
- DSL v0.1 草案；
- Safety Compiler v0.1 检查项列表；
- Runtime 模块边界；
- Tauri 页面结构；
- MVP 里程碑；
- 风险清单；
- 后续两周开发计划。

---

## 23. 总结

本项目真正有价值的地方不在于“用 AI 控制工业设备”，而在于：

> 用 AI 帮助工程师理解设备、生成业务逻辑，再用 Rust 编译器、安全规则和边缘运行时将其变成可靠、可验证、可审计的工业现场执行系统。

技术栈上，Rust + Tauri 是合理组合：

- Rust 承担 DSL、编译器、运行时、安全策略、协议接入；
- Tauri 承担工程师桌面 IDE、可视化、仿真、部署和监控；
- edge-daemon 承担现场确定性执行；
- AI 承担说明书理解、逻辑生成、日志解释和诊断辅助。

最终目标应是：

```text
从设备说明书和业务描述，生成安全可执行的工业边缘工作流。
```

这不是一个普通上位机项目，而是一个面向工业边缘的 **业务逻辑编译与执行基础设施**。
