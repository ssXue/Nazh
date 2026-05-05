# crates/nodes-io — I/O 节点与模板引擎

> **Ring**: Ring 1
> **对外 crate 名**: `nodes-io`
> **职责**: 所有协议 I/O 节点（16 个） + payload 模板渲染
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
| `ethercatPdoRead` | EtherCAT | 读取从站 PDO 输入数据（ethercrab 真实后端 / Mock 回退） |
| `ethercatPdoWrite` | EtherCAT | 写入从站 PDO 输出数据（ethercrab 真实后端 / Mock 回退） |
| `ethercatStatus` | EtherCAT | 查询所有从站状态与通道信息 |
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
├── ethercat/
├── bark_push.rs
├── sql_writer.rs
├── capability_call.rs
├── human_loop/
└── debug_console.rs
```

Plugin 注册入口：`IoPlugin::register(&mut NodeRegistry)`，在 `lib.rs` 集中声明 16 个节点类型的工厂 + 能力标签。

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
| `ethercatPdoRead` | `DEVICE_IO` | EtherCAT PDO 输入读取 |
| `ethercatPdoWrite` | `DEVICE_IO` | EtherCAT PDO 输出写入 |
| `ethercatStatus` | `DEVICE_IO` | EtherCAT 从站状态查询 |
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

### CAN/SLCAN 连接级共享会话

`canRead` / `canWrite` 共享连接级 CAN 总线会话：

- 同一 `connection_id` 的所有 CAN 节点共享同一个总线实例，存储在 `ConnectionManager::shared_sessions` 缓存中；
- `can::session::CanBusRuntime` 是轻量级操作句柄，按需创建，内部委托给 `ConnectionManager::ensure_shared_session`；
- 部署期（有 `connection_id`）或首帧（无连接 Mock）建立后端，后续 transform 复用；
- SLCAN 只在建连时执行 `C` / `S{bitrate}` / `O` 初始化序列，避免 1M CAN 场景下被串口独占锁、连接限流和初始化延迟拖垮；
- 共享会话不使用 `ConnectionGuard` 的排他借用，改用 `record_connect_success` / `record_connect_failure` 直接报告健康状态；
- 节点级 CAN ID 过滤在接收到帧后本地执行，共享总线不做过滤；
- 撤销部署时 `lifecycle_guard` 清理共享会话；运行错误时 `runtime.shutdown()` 移除会话，所有共享节点下次 `ensure_session` 重建。

修改 CAN 节点时必须保留这个共享会话模型。

### EtherCAT 连接级共享会话

`ethercatPdoRead` / `ethercatPdoWrite` / `ethercatStatus` 共享连接级 EtherCAT 主站实例：

- 同一 `connection_id` 的所有 EtherCAT 节点共享同一个主站后端，存储在 `ConnectionManager::shared_sessions` 缓存中；
- `ethercat::session::EthercatRuntime` 是轻量级操作句柄，按需创建，内部委托给 `ConnectionManager::ensure_shared_session`；
- 部署期通过 `connection_id` 建立后端；开发/测试可绑定 `backend: mock` 的 EtherCAT 连接，后续 transform 复用；
- ethercrab 后端在建连时执行从站发现 + PreOp → OP 状态转换，PDU TX/RX 由后台 tokio task 驱动；
- `PduStorage` 是全局静态单例（`try_split()` 只能调用一次，内部 `is_split` 是 `AtomicBool`，不可复位），因此**同一进程内 EtherCAT 主站只能绑定到第一次成功初始化的网卡**——后续部署若 `interface` 改变，`ensure_maindevice` 会显式报错要求重启 nazh-desktop；
- 共享会话不使用 `ConnectionGuard` 的排他借用，改用 `record_connect_success` / `record_connect_failure` 直接报告健康状态；
- 撤销部署时 `lifecycle_guard` 清理共享会话；运行错误时 `runtime.shutdown()` 移除会话，所有共享节点下次 `ensure_session` 重建。**注意**：进程级 TX/RX 后台任务 + `MainDevice` 不会随 session cleanup 一起销毁——`shutdown()` 只丢 backend 壳，下一次部署若 `interface` 一致则复用。

修改 EtherCAT 节点时必须保留这个共享会话模型。

#### tx_rx_task 接入坑点（删除/重写守护）

`crates/nodes-io/src/ethercat/backends/ethercrab_backend.rs::ensure_maindevice` 的 TX/RX 任务接入是 ethercrab 0.7 API 的反直觉点，**改这段前必读**：

`ethercrab::std::tx_rx_task` 的签名是 `fn(...) -> Result<impl Future<Output = Result<...>>, io::Error>`——**同步函数返回 `Result<Future, io::Error>`**：

- 同步部分：打开 raw socket、读 MAC/MTU。失败必须立即返回，不能继续构造 `MainDevice`，否则会拿到一个 PDU 永远不上线的死主站
- 异步部分：返回的 `Future` 必须被 `tokio::spawn` 持续 poll，PDU 收发循环才会运行

正确写法：

```rust
let task = tx_rx_task(interface, tx, rx).map_err(...)?;       // 同步段失败提前返回
let tx_handle = tokio::spawn(async move { task.await; ... }); // 异步段交给 tokio 驱动
```

**反例**（曾经踩过的坑，2026-05-06 修复）：

```rust
// ❌ 错的：tx_rx_task() 在 Ok 分支返回 Future，被 if let Err 模式整个丢弃
tokio::spawn(async move {
    if let Err(e) = tx_rx_task(&iface, tx, rx) {
        tracing::error!(...);
    }
    // Ok(future) 直接 drop —— PDU 帧永远不上线，init_single_group 一律 timeout: PDU
});
```

`PDU_STATE` 缓存命中时还要校验 `tx_handle.is_finished()`：socket 异常退出后必须给出"任务已死，请重启"的明确错误，不能让后续 `init_single_group` 继续 hang 到超时。

#### `write_outputs` 自动 tx_rx

`EthercatBus::write_outputs` 是 `async fn`，每次写完输出缓冲会立即触发一次 `group.tx_rx(&maindevice).await`，让数据上线。Nazh 没有全局周期 ticker（节点是事件驱动），如果 `write_outputs` 只 stage 不刷帧，写入永远卡在本地缓冲。改成带周期 ticker 的设计前，请保持这个"写即刷帧"的语义。

#### TX/RX 任务死亡后的现场排查（ADR-0023）

部署 EtherCAT 工作流时若撞到下面这条错误，**不是 Nazh bug，是 ethercrab 0.7 API 的硬约束**：

```text
EtherCAT 主站初始化失败: EtherCAT TX/RX 任务已终止（接口 `<iface>`）；
请重启 nazh-desktop 后重试，或检查网卡是否被拔出/链路中断
```

含义：上一次部署期间或之后，进程级后台 TX/RX 任务因 socket 错误（`SendFrame` / `ReceiveFrame` / `PartialSend`）退出。`PduStorage::try_split` 已被消费一次不可复位，且失败路径未归还 `(PduTx, PduRx)`——**当前进程内无法软恢复，必须重启 nazh-desktop**。

诊断与应对路径：

1. **看根因**——重启前在 stderr 找上一次的：
   ```text
   ERROR ethercrab_backend: EtherCAT TX/RX 任务异常终止 error=...
   ```
   `error=...` 是 ethercrab 给出的真实终止原因。开发期建议跑：
   ```bash
   RUST_LOG=info,ethercrab=debug \
     ../web/node_modules/.bin/tauri dev --no-watch
   ```
2. **检查物理链路**——`en8` 这类是 macOS USB-Ethernet 或虚拟网卡；`ifconfig` 确认 UP，必要时拔插一次 USB 适配器重置 BPF。
3. **重启 nazh-desktop**——退出后重启，`PDU_STORAGE` 是进程级 `static`，进程退出即释放。

设计层面的取舍、可选恢复方案（Tauri 重启入口 / vendor patch / 切库）以及重新评估的触发条件见 `docs/adr/0023-ethercat-tx-rx-恢复策略-暂缓.md`。**不要在没看 ADR-0023 的情况下尝试在 `ensure_maindevice` 加重试逻辑**——`tx_rx_task` 失败路径不归还 tx/rx，所谓"重试"不可能跑得通。

### 元数据约定（ADR-0008）

所有节点通过 `NodeExecution::with_metadata()` 返回执行元数据，键名非下划线：
`"timer"`、`"http"`、`"modbus"`、`"serial"`、`"can"`、`"ethercat"`、`"sql_writer"`、`"debug_console"`、`"connection"`、`"bark"`、`"mqtt"`、`"capability_call"`。payload 只保留 `_loop` / `_error` 等路由上下文。

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
| `ethercatPdoRead` | — | `Any` | `Json` | 输出 `{ slave, inputs, traceId }`；Mock 通过 `backend: mock` 连接显式启用 |
| `ethercatPdoWrite` | — | `Json` (required) | `Json` | 输入 `{ data: [u8] }`，输出 `{ slave, data, bytesWritten }` 写入快照 |
| `ethercatStatus` | — | `Any` | `Json` | 输出 `{ slaves, channelInfo }` |
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
- 可选（按 feature 门控，ADR-0018）：`reqwest`、`rumqttc`、`rusqlite`、`tokio-modbus`、`serialport`、`ethercrab`
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
| `io-ethercat` | `ethercatPdoRead` / `ethercatPdoWrite` / `ethercatStatus` | `ethercrab` |
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
