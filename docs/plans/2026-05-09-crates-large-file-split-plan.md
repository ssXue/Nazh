# Crates 大文件拆分计划

> **状态：** 进行中。2026-05-09 已完成第一轮 move-only 拆分、Slice 1 `connections` 续拆、Slice 2 `dsl-compiler::safety` 规则域拆分、Slice 3 `ai::client` 协议/响应/流式解析拆分、Slice 4 `serialTrigger` 帧/循环拆分、Slice 5 `connections::health` 状态机拆分与 Slice 6 `connections::guard` RAII 守卫拆分；本计划用于后续工作确认。

**Goal:** 在不改变运行时语义和 public API 的前提下，逐步降低 `crates/` 中大文件的职责混合风险，让后续修复可以沿清晰模块边界推进。

**Architecture:** 以 crate 现有职责边界为单位做小切片拆分。每个切片优先选择 move-only 的模块外移，保留 crate root re-export 与原有测试覆盖；不做跨 crate 抽象、不顺手改行为、不在一个 PR 中混合多个高风险领域。

**Tech Stack:** Rust / Cargo workspace / ts-rs / Dev Container / `cargo test` / `cargo clippy` / `cargo fmt`。

---

## 确认原则

- 每次只拆一个职责边界，优先 move-only。
- 拆分后保持原有 `crate::{...}` 对外路径稳定。
- 触碰 ts-rs 导出类型时，必须运行 `cargo test -p tauri-bindings --features ts-export export_bindings` 并确认 `web/src/generated/` 无漂移。
- 每个切片同步更新对应 crate `AGENTS.md`。
- `CR-P3-09` 是持续治理项，不以一次性大重构关闭。

## 当前状态

| 文件 | 当前处理 | 状态 |
|------|----------|------|
| `crates/connections/src/lib.rs` | 抽出 `types.rs`、`policy.rs`、`validation.rs`、`health.rs`、`guard.rs` 与 `tests.rs` | Slice 1 + Slice 5 + Slice 6 完成，后续可评估 manager |
| `crates/dsl-compiler/src/safety.rs` | 抽出 `safety/report.rs`、`safety/template.rs` 与 4 个规则域模块 | Slice 2 完成，后续可评估测试外移 |
| `crates/ai/src/client.rs` | 改为 `client/mod.rs` 并抽出 `types.rs`、`protocol.rs`、`provider_policy.rs`、`response.rs`、`stream.rs` 与 `tests.rs` | Slice 3 完成 |
| `crates/nodes-io/src/serial_trigger.rs` | 改为 `serial_trigger/mod.rs` 并抽出 `frame.rs`、`loop.rs` 与 `tests.rs` | Slice 4 完成 |
| `crates/core/src/variables.rs` | 暂缓生产拆分 | 仅建议未来先搬测试 |
| `crates/tauri-bindings/src/lib.rs` | 暂缓拆分 | 作为 IPC / ts-rs 汇总入口保留 |

## 已完成切片

- [x] `connections::types`：抽出 `ConnectionDefinition`、`ConnectionLease`、`ConnectionHealthState`、`ConnectionHealthSnapshot`、`ConnectionRecord` 与 `connection_metadata`。
- [x] `connections::tests`：抽出连接治理回归测试，降低 `lib.rs` review 噪声。
- [x] `connections::policy`：抽出 `ConnectionGovernancePolicy`、governance JSON 读取 helper 与退避窗口计算。
- [x] `connections::validation`：抽出连接类型 allowlist、normalize 与协议字段校验。
- [x] `connections::health`：抽出 Guard Drop 释放、限流、退避、熔断、心跳超时与失败计数状态推进 helper。
- [x] `connections::guard`：抽出 `ConnectionGuard` RAII 守卫、`mark_success` / `mark_failure` API 与 Drop 归还入口，crate root 保持 re-export。
- [x] `dsl-compiler::safety::report`：抽出 `SafetyReport` / `SafetyDiagnostic` 与诊断写入 helper。
- [x] `dsl-compiler::safety::template`：抽出 action 参数模板分类 helper。
- [x] `dsl-compiler::safety::state_graph`：抽出状态可达性、死胡同、循环检测与无条件循环判断。
- [x] `dsl-compiler::safety::action_rules`：抽出单位一致性、量程边界和危险动作审批检查。
- [x] `dsl-compiler::safety::preconditions`：抽出前置条件可达性、表达式标识符提取、信号可读性判断。
- [x] `dsl-compiler::safety::interlock`：抽出机械互锁与寄存器冲突检查。
- [x] `ai::client::types`：抽出 provider 快照、agent settings 快照与 stream request context。
- [x] `ai::client::protocol`：抽出 OpenAI-compatible payload / response / SSE / API-error DTO 与 payload builder。
- [x] `ai::client::provider_policy`：抽出 DeepSeek thinking / reasoning_effort / 轻量 probe 参数策略。
- [x] `ai::client::response`：抽出普通响应解析、HTTP error 解析与响应预览 helper。
- [x] `ai::client::stream`：抽出 SSE event 解析、流式请求发送与 channel 转发。
- [x] `ai::client::tests`：抽出 client 模块回归测试。
- [x] `nodes-io::serial_trigger::frame`：抽出串口帧字段读取、ASCII/HEX 规范化与 payload 构造 helper。
- [x] `nodes-io::serial_trigger::loop`：抽出阻塞串口读循环、delimiter 解析、serialport 参数映射、健康反馈与重连逻辑。
- [x] `nodes-io::serial_trigger::tests`：抽出 serialTrigger 帧规范化与 delimiter 回归测试。
- [x] 同步 `crates/connections/AGENTS.md`、`crates/dsl-compiler/AGENTS.md` 与 remediation 跟踪文档。
- [x] 同步 `crates/ai/AGENTS.md`、`crates/nodes-io/AGENTS.md`。

## 下一步建议

### Slice 1: 继续拆 `connections`

目标：把连接治理策略和校验逻辑从 `lib.rs` 中拆出，继续保持 `ConnectionManager` 外部 API 不变。

- [x] 抽出 `policy.rs`：`ConnectionGovernancePolicy`、governance JSON 读取 helper、退避窗口计算相关纯 helper。
- [x] 抽出 `validation.rs`：`SUPPORTED_CONNECTION_TYPES`、`validate_connection_definition`、连接类型 normalize 与协议字段校验。
- [x] 保留 `ConnectionManager` / `ConnectionGuard` 在 `lib.rs`，等 policy/validation 稳定后再评估是否拆 `manager.rs` / `guard.rs` / `health.rs`。

验证：

```bash
cargo test -p connections
cargo clippy -p connections --all-targets -- -D warnings
```

### Slice 2: 继续拆 `dsl-compiler::safety`

目标：把安全规则按规则域拆开，保留 `run_safety_checks` 作为唯一编排入口。

- [x] 抽出 `state_graph.rs`：状态可达性、死胡同、循环检测与无条件循环判断。
- [x] 抽出 `action_rules.rs`：单位一致性、量程边界和危险动作审批检查。
- [x] 抽出 `preconditions.rs`：前置条件可达性、表达式标识符提取、信号可读性判断。
- [x] 抽出 `interlock.rs`：机械互锁与寄存器冲突检查。

验证：

```bash
cargo test -p dsl-compiler safety
cargo test -p dsl-compiler
cargo clippy -p dsl-compiler --all-targets -- -D warnings
```

### Slice 3: 拆 `ai::client`

目标：隔离 OpenAI-compatible 协议 DTO、provider policy、response/stream parsing，避免 `client.rs` 继续混合配置快照、HTTP 请求和 SSE 解析。

- [x] 将 `client.rs` 改成 `client/mod.rs`，保持 `pub use client::OpenAiCompatibleService` 不变。
- [x] 抽出 `client/types.rs`：`ResolvedProvider`、`ResolvedProviderSnapshot`、`StreamRequestContext`。
- [x] 抽出 `client/protocol.rs`：chat payload / response / API error DTO 与 payload builder。
- [x] 抽出 `client/provider_policy.rs`：DeepSeek thinking 判定和采样参数处理。
- [x] 抽出 `client/response.rs` 与 `client/stream.rs`：普通响应、HTTP error、SSE event 解析和 stream request helper。

验证：

```bash
cargo test -p ai
cargo test -p tauri-bindings --features ts-export export_bindings
git diff --exit-code -- web/src/generated
```

### Slice 4: 视功能改动拆 `serialTrigger`

目标：只有在后续继续改串口触发节点时再拆，避免为拆而拆。

- [x] 抽出 `serial_trigger/frame.rs`：frame 字段读取、HEX/ASCII 规范化、payload 构造。
- [x] 抽出 `serial_trigger/loop.rs`：阻塞串口读取循环、serialport 参数映射、健康反馈与重连。

验证：

```bash
cargo test -p nodes-io serial
cargo test -p nazh-engine --test workflow serial_trigger_node_normalizes_ascii_and_hex_frames
cargo clippy -p nodes-io --all-targets -- -D warnings
```

### Slice 5: 拆 `connections::health`

目标：把连接健康状态机从 `lib.rs` 抽出，保留 `ConnectionManager` / `ConnectionGuard` 对外 API 不变。

- [x] 抽出 `health.rs`：`ConnectionOutcome`、`finalize_release`、配置诊断刷新、限流 / 熔断 / 退避窗口过期处理、失败状态推进与时间 helper。
- [x] 保留 `ConnectionManager` / `ConnectionGuard` 在 `lib.rs`，后续如继续拆分再评估 `manager.rs` / `guard.rs`。

验证：

```bash
cargo test -p connections
cargo clippy -p connections --all-targets -- -D warnings
```

### Slice 6: 拆 `connections::guard`

目标：把 RAII 守卫本体从 `lib.rs` 抽出，保留 `connections::ConnectionGuard` 对外路径与 `ConnectionManager::acquire` 返回类型不变。

- [x] 抽出 `guard.rs`：`ConnectionGuard`、`lease` / `id` / `metadata` getter、`mark_success` / `mark_failure` 与 Drop 归还入口。
- [x] `lib.rs` 通过 `pub use guard::ConnectionGuard` 保持 public API 稳定。

验证：

```bash
cargo test -p connections
cargo clippy -p connections --all-targets -- -D warnings
```

## 暂缓项

- `crates/core/src/variables.rs`：生产职责仍集中，文件大主要来自内联测试。下一次变量功能改动时，优先只搬测试。
- `crates/tauri-bindings/src/lib.rs`：保留为 IPC / ts-rs 单一汇总入口。若未来拆，只按命令域拆内部模块，`lib.rs` 仍保留 re-export 与 `export_all()`。

## 第一轮验证记录

2026-05-09 第一轮拆分后已在 Dev Container `nazh-devcontainer-nzh-main` 内运行：

```bash
cargo test -p connections
cargo test -p dsl-compiler
cargo test -p tauri-bindings --features ts-export export_bindings
cargo fmt --all -- --check
cargo clippy -p connections -p dsl-compiler --all-targets -- -D warnings
git diff --exit-code -- web/src/generated
git diff --check
```

结果：上述命令均通过，`web/src/generated/` 无漂移。

## Slice 1 验证记录

2026-05-09 继续拆分 `connections::policy` / `connections::validation` 后已在 Dev Container `nazh-devcontainer-nzh-main` 内运行：

```bash
cargo test -p connections
cargo clippy -p connections --all-targets -- -D warnings
cargo fmt --all -- --check
git diff --check
```

结果：上述命令均通过。

## Slice 2 验证记录

2026-05-09 拆分 `dsl-compiler::safety` 规则域后已在 Dev Container `nazh-devcontainer-nzh-main` 内运行：

```bash
cargo test -p dsl-compiler safety
cargo test -p dsl-compiler
cargo clippy -p dsl-compiler --all-targets -- -D warnings
cargo fmt --all -- --check
```

结果：上述命令均通过。

## Slice 3 验证记录

2026-05-09 拆分 `ai::client` 协议/响应/流式解析模块后已在 Dev Container `nazh-devcontainer-nzh-main` 内运行：

```bash
cargo test -p ai
cargo test -p tauri-bindings --features ts-export export_bindings
cargo clippy -p ai --all-targets -- -D warnings
cargo fmt --all -- --check
git diff --exit-code -- web/src/generated
git diff --check
```

结果：上述命令均通过，`web/src/generated/` 无漂移。

## Slice 4 验证记录

2026-05-09 拆分 `serialTrigger` 帧规范化与同步读循环后已在 Dev Container `nazh-devcontainer-nzh-main` 内运行：

```bash
cargo test -p nodes-io serial
cargo test -p nazh-engine --test workflow serial_trigger_node_normalizes_ascii_and_hex_frames
cargo clippy -p nodes-io --all-targets -- -D warnings
cargo fmt --all -- --check
git diff --check
```

结果：上述命令均通过。

## Slice 5 验证记录

2026-05-09 拆分 `connections::health` 状态机 helper 后已在 Dev Container `nazh-devcontainer-nzh-main` 内运行：

```bash
cargo test -p connections
cargo clippy -p connections --all-targets -- -D warnings
```

结果：上述命令均通过。

## Slice 6 验证记录

2026-05-09 拆分 `connections::guard` RAII 守卫后已在 Dev Container `nazh-devcontainer-nzh-main` 内运行：

```bash
cargo test -p connections
cargo clippy -p connections --all-targets -- -D warnings
```

结果：上述命令均通过。
