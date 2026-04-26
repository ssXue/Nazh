# crates/connections — 全局连接资源池

> **Ring**: Ring 1
> **对外 crate 名**: `connections`
> **职责**: 统一治理所有协议连接的建连、重连、心跳、超时、限流、熔断与健康诊断
>
> 根 `AGENTS.md` 的约束对本 crate 同样适用。

## 这个 crate 做什么

这是 Nazh 工业可靠性的基石——**节点绝不直接访问硬件**。所有协议连接（Modbus / MQTT
/ HTTP / 串口 / …）都声明为 `ConnectionDefinition`，在运行时注册到全局
`ConnectionManager`，节点通过 `acquire(id)` 借用 `ConnectionGuard`，节点 Drop 时自动归还。

核心抽象：
- `ConnectionManager` — 全局池，按连接 id 管理状态机
- `SharedConnectionManager = Arc<ConnectionManager>` — 跨线程共享句柄
- `ConnectionGuard` — RAII Drop 归还 + `mark_success` / `mark_failure` 状态反馈
- `ConnectionDefinition` — 连接的声明式定义（id、type、url、timeout、circuit-breaker 参数等）
- `connection_metadata()` — 把连接健康快照序列化为节点 metadata（供事件流消费）

**为什么要做连接池**：工业场景常见"串口 COM3 被两个节点抢"、"MQTT 重连风暴"、
"熔断/降级"这些问题；统一在这个 crate 管掉，节点只管借还。

## 对外暴露

```text
crates/connections/src/
├── lib.rs            # ConnectionManager + ConnectionDefinition（核心）
├── guard.rs          # ConnectionGuard RAII
├── health.rs         # 连接状态机（Healthy / Degraded / CircuitOpen / ...）
├── circuit_breaker.rs
├── pool.rs
└── metadata.rs       # connection_metadata() 序列化
```

关键类型：`ConnectionManager`、`ConnectionGuard`、`ConnectionDefinition`、`SharedConnectionManager`、`connection_metadata`。

## 内部约定

1. **RAII 归还是强制约定**。任何 `acquire` 得到的 `ConnectionGuard` 必须在作用域结束时 Drop 归还；禁止写显式 `release()` / `close()`。
2. **状态反馈是节点义务**。节点用完成功走 `guard.mark_success()`；失败路径由 Drop 自动计入失败统计，必要时手动 `mark_failure()` 附加原因。
3. **锁粒度细**（ADR-0005）：`ConnectionManager` 内部每连接独立 `RwLock<ConnectionState>`，而非一把大锁。新增字段/方法时保留这个粒度。
4. **定义与实例分离**：`ConnectionDefinition` 只描述「怎么连」，实际的 `tokio-modbus` / `rumqttc` / `reqwest::Client` 实例由 `ConnectionManager` 在首次 `acquire` 时惰性建立。
5. **熔断策略硬编码在 crate 内**。触发条件（连续失败次数、恢复窗口）作为配置而非 trait 扩展点——改熔断算法请同步更新本文件，若改语义走 ADR。
6. **不做节点实现**。本 crate 只提供连接原语。具体"怎么用这个连接读 Modbus"是 `nodes-io` 的事。

## 依赖约束

- 允许：`nazh-core`、`tokio`、`serde`、`chrono`、`url`、`ts-rs`（optional + `ts-export`）
- **禁止**：协议客户端库（`rumqttc` / `reqwest` / `tokio-modbus` / `rusqlite`）——它们属于具体使用方 `nodes-io`，本 crate 只定义抽象。

> 这一点值得警惕：若某天要加"连接级别的 MQTT ping 预热"，**不要**把 `rumqttc` 拉进来，改用依赖注入（由 `nodes-io` 注册回调）。

## 修改本 crate 时

| 改动 | 必须同步 |
|------|----------|
| 改 `ConnectionGuard` API / Drop 行为 | 所有 `nodes-io` 中 `acquire` 的调用点；`connection_metadata` 可能需要调整 |
| 改 `ConnectionDefinition` 字段 | ts-rs 重新生成（`cargo test -p tauri-bindings --features ts-export export_bindings`）+ 前端连接管理 UI |
| 改熔断/健康判定算法 | 本文件"内部约定"小节 + 相关集成测试 + 若语义变化则开 ADR |
| 加新的连接类型（如 OPC-UA） | `ConnectionType` 枚举、`ConnectionDefinition` 的 type 字段、`nodes-io` 的使用方、文档 |

测试：
```bash
cargo test -p connections
```

## 关联 ADR / RFC

- **ADR-0005** 连接管理器细粒度锁
- **RFC-0002** Phase 3 — `ConnectionGuard` RAII 从 Ring 0 split 到本 crate
- **ADR-0009** 生命周期钩子（已实施）—— Ring 1 节点在 `on_deploy` 中借连接：先 `acquire` 校验类型/metadata、`mark_failure/success`，再 spawn 后台任务时通过 `runtime.block_on(connection_manager.acquire(...))`（同步循环）或直接 `.await`（async 循环）持续重连。撤销时 `LifecycleGuard::shutdown` 等待后台任务退出，`mark_disconnected` 由节点自身负责
