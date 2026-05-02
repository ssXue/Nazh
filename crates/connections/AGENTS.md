# crates/connections — 全局连接资源池

> **Ring**: Ring 1
> **对外 crate 名**: `connections`
> **职责**: 统一治理所有协议连接的建连、重连、心跳、超时、限流、熔断与健康诊断
>
> 根 `AGENTS.md` 的约束对本 crate 同样适用。

## 定位

节点绝不直接访问硬件——所有协议连接（Modbus / MQTT / HTTP / 串口 / Bark / ...）都声明为 `ConnectionDefinition`，注册到 `ConnectionManager`，节点通过 `acquire(id)` 借用 `ConnectionGuard`，Drop 时自动归还。

## 对外暴露

全部类型定义在 `src/lib.rs`（单文件模块）。

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

ts-rs 导出：`ConnectionDefinition` / `ConnectionHealthSnapshot` / `ConnectionHealthState` / `ConnectionRecord`（由 `ts-export` feature 门控）。

## 内部约定

1. **RAII 归还是强制约定**。`ConnectionGuard` 的 Drop 在任何退出路径（正常 / 错误 / panic）都自动释放连接；禁止写显式 `release()` / `close()`。
2. **状态反馈是节点义务**。成功走 `guard.mark_success()`；失败路径由 Drop 自动计入失败统计，手动 `mark_failure()` 附加原因。未标记即 Drop 视为异常退出（Pending → Degraded + 诊断日志）。
3. **锁粒度**：`ConnectionManager` 用 `RwLock<HashMap<..., Arc<Mutex<ConnectionRecord>>>>`——外层 RwLock 保护连接表，内层 `Mutex` 保护单连接的治理状态，避免一把大锁。
4. **定义与实例分离**：`ConnectionDefinition` 只描述"怎么连"，实际的 `tokio-modbus` / `rumqttc` / `reqwest::Client` 实例由 `nodes-io` 在 `acquire` 后惰性建立；本 crate 只管治理状态机。
5. **治理策略从 metadata 读取**。`ConnectionGovernancePolicy` 从 `metadata.governance` JSON 中读取可调参数（超时、限流窗口、熔断阈值等），有合理默认值和下限。改熔断算法请同步更新本文件，若改语义走 ADR。
6. **不做节点实现**。本 crate 只提供连接原语。
7. **超时检测**：`acquire` 前调用 `reconcile_timed_state` 处理过期的限流 / 熔断 / 退避窗口；`finalize_release`（Guard Drop 时）检查占用时长是否超过 `operation_timeout_ms`。

## 依赖约束

- 允许：`nazh-core`、`tokio`、`serde`、`serde_json`、`chrono`、`url`、`ts-rs`（optional + `ts-export`）
- **禁止**：协议客户端库（`rumqttc` / `reqwest` / `tokio-modbus` / `rusqlite`）——属于 `nodes-io`

## 修改本 crate 时

| 改动 | 必须同步 |
|------|----------|
| 改 `ConnectionGuard` API / Drop 行为 | 所有 `nodes-io` 中 `acquire` 的调用点；`connection_metadata` 可能需要调整 |
| 改 `ConnectionDefinition` 字段 | ts-rs 重新生成 + 前端连接管理 UI |
| 改熔断/健康判定算法 | 本文件"内部约定"小节 + 相关集成测试 + 若语义变化则开 ADR |
| 加新的连接类型（如 OPC-UA） | `validate_connection_definition` 中的匹配分支 + `ConnectionDefinition` 的 type 字段 + `nodes-io` 的使用方 + 文档 |
| 改 `ConnectionHealthSnapshot` 字段 | ts-rs 重新生成 + 前端连接状态 UI |

测试：
```bash
cargo test -p connections
```

## 关联 ADR / RFC

- **ADR-0005** 连接管理器细粒度锁
- **RFC-0002** Phase 3 — `ConnectionGuard` RAII 从 Ring 0 split 到本 crate
- **ADR-0009** 生命周期钩子——节点在 `on_deploy` 中通过 `ConnectionManager` 借连接
