# crates/connections — 全局连接资源池

> **Ring**: Ring 1
> **对外 crate 名**: `connections`
> **职责**: 统一治理所有协议连接的建连、重连、心跳、超时、限流、熔断与健康诊断
>
> 根 `AGENTS.md` 的约束对本 crate 同样适用。

## 定位

节点绝不直接访问硬件——所有协议连接（Modbus / MQTT / HTTP / 串口 / CAN/SLCAN / Bark / ...）都声明为 `ConnectionDefinition`，注册到 `ConnectionManager`，节点通过 `acquire(id)` 借用 `ConnectionGuard`，Drop 时自动归还。

## 对外暴露

当前结构：

```text
crates/connections/src/
├── lib.rs        # ConnectionGuard、ConnectionManager 与连接状态机
├── policy.rs     # ConnectionGovernancePolicy 与治理 metadata 读取 / 退避计算
├── validation.rs # 连接类型 allowlist、normalize 与协议字段校验
├── types.rs      # 连接 DTO、健康快照与 connection_metadata
└── tests.rs      # crate 内部回归测试
```

对外 API 仍由 `src/lib.rs` 与 crate root `pub use` 统一承载；内部继续拆分时必须保持 `connections::{...}` 调用路径稳定。

关键类型：
- `ConnectionManager` — 全局连接池，按连接 id 管理 RAII 排他借出
- `SharedConnectionManager = Arc<ConnectionManager>` — 跨线程共享句柄
- `shared_connection_manager()` — 构造空池的工厂函数
- `ConnectionGuard` — RAII Drop 归还 + `mark_success` / `mark_failure`
- `ConnectionDefinition` — 连接声明式定义（id、type、metadata）
- `ConnectionLease` — 借出时生成的租约快照
- `ConnectionRecord` — 内部记录（含健康快照，通过 get/list 暴露给 IPC）
- `ConnectionHealthState` — 连接健康阶段枚举（Idle/Connecting/Healthy/Degraded/Invalid/...）
- `ConnectionHealthSnapshot` — 连接治理运行时快照（计数器、时间戳、诊断文本）
- `connection_metadata()` — 把连接租约序列化为节点 metadata（供 ExecutionEvent 消费）

`ConnectionManager` 主要方法：
- `register_connection` / `upsert_connection` / `upsert_connections` / `replace_connections` — 注册与批量管理
- `acquire` — RAII 借出（校验配置、限流、熔断检查后返回 `ConnectionGuard`）
- `record_connect_success` / `record_connect_failure` / `record_timeout` / `record_heartbeat` — 运行时状态反馈
- `mark_invalid_configuration` / `mark_disconnected` / `mark_all_idle` — 标记生命周期状态
- `get` / `list` — 读取快照（用于 IPC `list_connections`）
- `ensure_shared_session` / `remove_shared_session` / `cleanup_and_remove_shared_session` — 连接级共享会话缓存（CAN/EtherCAT 等长生命周期总线使用）

ts-rs 导出：`ConnectionDefinition` / `ConnectionHealthSnapshot` / `ConnectionHealthState` / `ConnectionRecord`（由 `ts-export` feature 门控）。

## 内部约定

1. **RAII 归还是强制约定**。`ConnectionGuard` 的 Drop 在任何退出路径（正常 / 错误 / panic）都自动释放连接；禁止写显式 `release()` / `close()`。
2. **状态反馈是节点义务**。成功走 `guard.mark_success()`；失败路径由 Drop 自动计入失败统计，手动 `mark_failure()` 附加原因。未标记即 Drop 视为异常退出（Pending → Degraded + 诊断日志）。
3. **锁粒度**：`ConnectionManager` 用 `RwLock<HashMap<..., Arc<Mutex<ConnectionRecord>>>>`——外层 RwLock 保护连接表，内层 `Mutex` 保护单连接的治理状态，避免一把大锁。
4. **共享会话按连接 ID 合流初始化**。`ensure_shared_session` 对同一 `connection_id` 使用 per-key async 锁，首次并发建连只允许一个 factory 运行；成功后写入共享缓存，后续调用复用同一 `Arc<T>`。
5. **运行中连接不静默替换**。`upsert_connection` / `upsert_connections` 遇到正在借出的旧 record 或已有共享会话时保留旧定义；`replace_connections` 遇到任一正在借出的 record 或共享会话时跳过整体替换。未来若需要热切换，应先实现 draining / shutdown 再切换配置。
6. **定义与实例分离**：`ConnectionDefinition` 只描述"怎么连"，实际的 `tokio-modbus` / `rumqttc` / `reqwest::Client` 实例由 `nodes-io` 在 `acquire` 后惰性建立；本 crate 只管治理状态机。
7. **未知连接类型默认拒绝**。`validation.rs` 的 `validate_connection_definition` 只接受显式支持的连接类型（serial / modbus / mqtt / http / bark / can / ethercat 及既有别名），错误消息必须列出支持类型；如果需要 opaque/tool 连接，必须新增 allowlist 分支并限制可用节点。
8. **总线参数必须显式配置**。CAN/SLCAN 连接必须声明 `interface` / `channel` / `baud_rate` / `bitrate`；EtherCAT 连接必须声明 `backend` / `interface` / `cycle_time_ms` / `op_timeout_ms`。测试或 demo 的 mock 连接也要显式写出这些字段，不在连接层静默补默认值。
9. **治理策略从 metadata 读取**。`policy.rs` 的 `ConnectionGovernancePolicy` 从 `metadata.governance` JSON 中读取可调参数（超时、限流窗口、熔断阈值等），有合理默认值和下限。改熔断算法请同步更新本文件，若改语义走 ADR。
10. **失败出口推进治理状态机**。`guard.mark_failure(reason)` 在 Drop 时会更新 failure counters、退避和熔断；若节点已在同一次 lease 内手动调用 `record_connect_failure`，Drop 不重复计数。
11. **不做节点实现**。本 crate 只提供连接原语。
12. **超时检测**：`acquire` 前调用 `reconcile_timed_state` 处理过期的限流 / 熔断 / 退避窗口；`finalize_release`（Guard Drop 时）检查占用时长是否超过 `operation_timeout_ms`。

## 依赖约束

- 允许：`nazh-core`、`tokio`、`serde`、`serde_json`、`chrono`、`url`、`ts-rs`（optional + `ts-export`）
- **禁止**：协议客户端库（`rumqttc` / `reqwest` / `tokio-modbus` / `rusqlite`）——属于 `nodes-io`

## 修改本 crate 时

| 改动 | 必须同步 |
|------|----------|
| 改 `ConnectionGuard` API / Drop 行为 | 所有 `nodes-io` 中 `acquire` 的调用点；`connection_metadata` 可能需要调整 |
| 改 `ConnectionDefinition` 字段 | ts-rs 重新生成 + 前端连接管理 UI |
| 改熔断/健康判定算法 | 本文件"内部约定"小节 + 相关集成测试 + 若语义变化则开 ADR |
| 改共享会话初始化、清理或连接替换策略 | 本文件"内部约定"小节 + `cargo test -p connections` + 相关 `nodes-io` 会话测试 |
| 加新的连接类型（如 OPC-UA / CAN） | `validation.rs` 中的匹配分支 + `ConnectionDefinition` 的 type 字段 + `nodes-io` 的使用方 + 文档 |
| 改 `ConnectionHealthSnapshot` 字段 | ts-rs 重新生成 + 前端连接状态 UI |

测试：
```bash
cargo test -p connections
```

## 关联 ADR / RFC

- **ADR-0005** 连接管理器细粒度锁
- **RFC-0002** Phase 3 — `ConnectionGuard` RAII 从 Ring 0 split 到本 crate
- **ADR-0009** 生命周期钩子——节点在 `on_deploy` 中通过 `ConnectionManager` 借连接
