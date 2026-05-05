# crates/nodes-io — I/O 节点与模板引擎

> **Ring**: Ring 1
> **对外 crate 名**: `nodes-io`
> **职责**: 所有协议 I/O 节点（13 个） + payload 模板渲染
>
> 根 `AGENTS.md` 的约束对本 crate 同样适用。

## 这个 crate 做什么

本 crate 实现 Nazh 全部 I/O 相关节点，组成 `IoPlugin` 插件，以及一套通用的模板渲染模块：

| 节点 | 协议 | 用途 |
|------|------|------|
| `timer` | — | 定时触发 |
| `serialTrigger` | 串口 | 串口扫码枪/RFID 等外设触发（Tauri 壳层监听，节点做规范化） |
| `native` | — | 数据透传 + 可选连接借用 |
| `modbusRead` | Modbus TCP | 寄存器读取（有连接走真实协议，无连接走正弦函数模拟） |
| `httpClient` | HTTP(S) | 通用 HTTP 请求（三种 body 模式：json / template / dingtalk_markdown） |
| `mqttClient` | MQTT | 两种模式：publish（变换节点）/ subscribe（触发器，壳层启动订阅） |
| `canRead` | CAN/SLCAN | 通过 USB-CAN SLCAN 适配器接收 CAN 帧（无连接走 Mock 回退） |
| `canWrite` | CAN/SLCAN | 通过 USB-CAN SLCAN 适配器发送 CAN 帧（无连接走 Mock 回退） |
| `barkPush` | HTTP | 向 Bark 服务推送 iOS 通知 |
| `sqlWriter` | sqlite | 本地持久化写入（内部 `spawn_blocking` 包装 `rusqlite`） |
| `debugConsole` | — | 格式化 payload 打印到控制台 |
| `capabilityCall` | DSL 适配 | Workflow DSL 编译出的设备能力调用快照 |
| `humanLoop` | 人机协同 | 人工审批 / 表单确认节点 |

**模板引擎** (`template` 模块)：给 `httpClient` / `barkPush` 等节点用，支持 `{{ payload.field }}` /
`{{ now }}` 等占位符、JSON path 数组索引、超长截断。

## 对外暴露

```text
crates/nodes-io/src/
├── lib.rs            # IoPlugin + re-exports + connection/资源辅助
├── template.rs       # pub mod — 模板渲染
├── timer.rs
├── serial_trigger.rs
├── native.rs
├── modbus_read.rs
├── http_client.rs
├── mqtt_client.rs
├── can/
├── bark_push.rs
├── sql_writer.rs
├── capability_call.rs
├── human_loop/
└── debug_console.rs
```

Plugin 注册入口：`IoPlugin::register(&mut NodeRegistry)`，在 `lib.rs` 集中声明 13 个节点类型的工厂 + 能力标签。

## 内部约定

### 节点能力标签（ADR-0011）

| 节点 | 能力 | 备注 |
|------|------|------|
| `timer` | `TRIGGER` | 纯时钟驱动 |
| `serialTrigger` | `TRIGGER \| DEVICE_IO` | 串口设备事件 |
| `native` | `empty()` | 工具节点，config 决定有无连接 |
| `modbusRead` | `DEVICE_IO` | 设备总线 |
| `httpClient` | `NETWORK_IO` | 通用网络 |
| `mqttClient` | `NETWORK_IO` | **publish + subscribe 都归为网络**；TRIGGER 仅 subscribe 模式成立，类型级保守不声明 |
| `canRead` | `DEVICE_IO` | CAN/SLCAN 设备总线读 |
| `canWrite` | `DEVICE_IO` | CAN/SLCAN 设备总线写 |
| `barkPush` | `NETWORK_IO` | HTTP 到 Bark |
| `sqlWriter` | `FILE_IO` | **不标 BLOCKING**：内部已 `spawn_blocking` 自包装 |
| `debugConsole` | `empty()` | 副作用是 stdout，可忽略 |
| `capabilityCall` | `DEVICE_IO` | 设备能力调用适配器 |
| `humanLoop` | `BRANCHING` | approve / reject 分支 |

这张表由 `src/registry.rs::标准注册表节点能力标签与_adr_0011_契约一致` 单测守住。

### 连接访问约定

1. **所有硬件/网络 I/O 都借用连接**。要么走 `ConnectionManager::acquire(id)`，要么显式声明为"无连接工具节点"（`timer` / `debugConsole` / 无 config 的 `native`）。
2. **连接 id 可从 `WorkflowNodeDefinition` 顶层字段继承**。辅助函数 `inherit_connection_id(&mut config.connection_id, def)` 实现这个 fallback；新节点添加连接支持时沿用这个模式。
3. **Drop 自动归还**：`ConnectionGuard` 离开作用域时归还，不要手写 `drop(guard)` 或 `release`。必要时 `guard.mark_success()` 通知连接池健康反馈。

### 元数据约定（ADR-0008）

所有节点通过 `NodeExecution::with_metadata()` 返回执行元数据，键名非下划线：
`"timer"`、`"http"`、`"modbus"`、`"serial"`、`"can"`、`"sql_writer"`、`"debug_console"`、`"connection"`、`"bark"`、`"mqtt"`、`"capability_call"`。payload 只保留 `_loop` / `_error` 等路由上下文。

### Pin 声明（ADR-0010 Phase 3）

协议节点逐步落地具体 pin 类型；其余节点保留 trait 默认（单 `Any` 进、单 `Any` 出）：

| 节点 | mode | input | output | 备注 |
|------|------|-------|--------|------|
| `modbusRead` | — | `Any` | `out`: `Json` (Exec) + `latest`: `Json` (Data) | ADR-0014 Phase 2 加 `latest` 拉取式 Data 引脚（首个真实 Data 用例）；`out` Exec 语义不变 |
| `sqlWriter` | — | `Json` (required) | `Any` | 纯 sink；payload 必须有列结构 |
| `httpClient` | — | `Json` (required) | `Json` | body / template / 钉钉三种 mode 都期待 JSON 对象输入 |
| `mqttClient` | publish | `Json` (required) | `Any` | 实例方法按 `self.config.mode` 切换 pin 类型 |
| `mqttClient` | subscribe | `Any` | `Json` | subscribe 由 `on_deploy` 触发；transform 仅手动 dispatch |
| `canRead` | — | `Any` | `Json` | 输出 `{ can: { id, data, dlc, ... } }`；无帧超时为 `can: null` |
| `canWrite` | — | `Json` (required) | `Json` | 输入 `can_id` / `data` / `is_extended`，输出 `sent` 快照 |
| `timer` / `serialTrigger` / `native` / `barkPush` / `debugConsole` / `template` | — | `Any` | `Any` | 触发器 / 透传 / 格式化类节点保留默认 |

**修改 pin 声明时必须同步：**
- 节点 `input_pins(&self)` / `output_pins(&self)` 实现
- 节点 inline `#[cfg(test)] mod tests` 中的 pin 形状断言
- 本表格
- 兼容矩阵 fixture（`tests/fixtures/pin_compat_matrix.jsonc`）：若引入新 `PinType` 变体，至少补 3 条配对（自反 / `Any` 双向 / 与至少一类不兼容）。`crates/core/tests/pin_compat_contract.rs` 的覆盖纪律测试会拒绝新变体没条目的 PR
- 若声明了 `PinKind::Data` 输出引脚（如 `modbusRead.latest`），同步更新前端 `web/src/components/flowgram/nodes/<node>/index.ts` 的 `flowgram` 块改 `useDynamicPort: true` + 在 `web/src/components/flowgram/nodes/shared.ts` 的 `getLogicNodeBranchDefinitions` 增加分支——否则画布会用 FlowGram 默认渲染，新引脚不可见

**关于 `Custom` 类型推迟到未来 Phase**：Phase 3 故意不引入 `Custom("modbus-register")` / `Custom("sql-row")` 等命名类型。理由是若引入则常见链路（`modbusRead → sqlWriter`）会被部署期校验拒，等于"消费者孤岛"。Custom 推迟到未来配套生产者节点（如 row-formatter）一并引入。

### 阻塞 API 的处理

- `rusqlite` 是**同步** API。`sqlWriter` 在节点内部 `tokio::task::spawn_blocking` 包装，对外是 async-friendly。
- 其他阻塞 API 如果要加，**必须**节点内部自包装或声明 `BLOCKING` 能力标签让 Runner 处理——**不能**在普通 `async fn transform` 里直接调用同步阻塞调用。

### 模板引擎

1. 模板占位符**只读**——`{{ payload.x }}` 不能写回 payload；对 payload 的修改由上游节点完成。
2. 未闭合占位符 `{{ ...` 保留原文，不抛错（防御式）。
3. 超长值自动截断（避免日志爆炸）。

## 依赖约束

- 允许：`nazh-core`、`connections`、`chrono`、`serde_json`、`url`、`tokio`、`uuid`、`tracing`、`thiserror`
- 可选（按 feature 门控，ADR-0018）：`reqwest`、`rumqttc`、`rusqlite`、`tokio-modbus`、`serialport`
- 协议依赖是本 crate 的**职责所在**，但不能传染：
  - **`nodes-flow` 不能依赖 `nodes-io`**
  - **`nazh-core` / `connections` / `scripting` 都不能依赖本 crate**

## Feature 门控（ADR-0018，已实施 2026-04-26）

| Feature | 启用的节点 | 拉入的协议依赖 |
|---------|------------|----------------|
| `io-sql` | `sqlWriter` | `rusqlite`（bundled） |
| `io-http` | `httpClient` | `reqwest` |
| `io-mqtt` | `mqttClient` | `rumqttc` |
| `io-modbus` | `modbusRead` | `tokio-modbus` |
| `io-serial` | `serialTrigger` | `serialport` |
| `io-notify` | `barkPush` | `reqwest`（与 `io-http` 共享） |
| `io-can` | `canRead` / `canWrite` | `serialport`（SLCAN） |
| **元 feature `io-all`** | 全部以上 | 全部以上 |

永远启用（无 feature 门控）：`timer` / `native` / `debugConsole` + `template` 工具——零额外依赖，任何部署都用得到。

构建建议：
- **桌面默认**：facade `nazh-engine` 的 `default = ["io-all"]` 自动包全部
- **嵌入式**：`cargo build -p nazh-engine --no-default-features --features "io-mqtt,io-modbus"` 即可裁剪

## 修改本 crate 时

| 改动 | 必须同步 |
|------|----------|
| 新增 I/O 节点 | 本文件能力表 + `IoPlugin::register`（含 `#[cfg(feature = "io-*")]` 门控）+ `Cargo.toml` 加 io-* feature + 元 feature `io-all` 列入新名 + facade `Cargo.toml` / `src/lib.rs` 转传 + `src/registry.rs` 契约测试 + `NODE_CATEGORY_MAP`（前端）+ 触发器节点可能需要壳层支持 |
| 改节点能力标签 | 本文件能力表 + 契约测试 |
| 改节点 pin 类型 | 节点 `input_pins`/`output_pins` 实现 + 节点 inline pin 形状测试 + 本文件 Pin 声明表 + 兼容矩阵 fixture（若涉及新 `PinType` 变体）+ 反向兼容性集成测试断言（若改的是协议节点） |
| 改元数据键名 | 前端事件显示 + ADR-0008 文档 |
| 新增模板占位符 | 所有使用模板的节点 config 文档 + `template::tests` |

测试：
```bash
cargo test -p nodes-io
cargo test -p nazh-engine --test workflow   # 集成测试
```

## 关联 ADR / RFC

- **ADR-0005** 连接管理器（所有 I/O 节点的连接语义来源）
- **ADR-0008** 节点输出元数据通道
- **ADR-0011** 节点能力标签
- **ADR-0009** 生命周期钩子（已实施）—— `TimerNode` / `SerialTriggerNode` / `MqttClientNode` (subscribe 模式) 在 `on_deploy` 中自持触发器后台任务，撤销时通过 `LifecycleGuard::shutdown` 回收。emit 走 `NodeHandle::emit`，不经过壳层 `dispatch_router` 的 trigger lane，因此 backpressure / DLQ / retry / metrics 等防御能力不生效——引擎级背压补回见 ADR-0014 / ADR-0016
- **ADR-0018** 按协议 feature 门控 — **已实施**（2026-04-26）。详见上"Feature 门控"小节
- **ADR-0010 Phase 3** Pin 声明系统 — **已实施**（2026-04-26）。详见上"Pin 声明（ADR-0010 Phase 3）"小节。前端可视化（Phase 2）独立 plan 启动后跟进
