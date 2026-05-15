# 2026-05-16 项目架构 Review

> **Status:** 当前架构检查记录。本文记录 2026-05-16 对 Nazh 当前代码库的静态架构 review 结果，后续修复应落到对应代码、ADR/RFC、根 `AGENTS.md` 或 crate `AGENTS.md`。

## 范围

本次 review 以根 `AGENTS.md` 的架构契约为基准，抽查以下边界：

- Ring 0 / Ring 1 依赖纯度
- Tauri IPC 暴露面与文档一致性
- AI 调用前移与 API key 管理路径
- `NodeTrait` / Runner / DataStore / metadata 分离
- 节点注册、能力标签、Pin 声明与 crate 文档一致性
- 运行时配置 fail-fast 与隐式 fallback

本次未运行完整测试套件；最终合并前仍需在 Dev Container 或 CI 等价环境执行 `cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`，以及相关前端测试。

## 总体结论

当前核心分层方向基本成立：Ring 0 未发现直接协议依赖；AI HTTP 调用主体位于前端；DAG runner 的 panic / timeout 隔离覆盖普通执行路径和 pure pull 路径；`nodes-io` 的协议 feature gating 基本符合 ADR-0018。

主要风险集中在三类：

1. 文档契约与实现已经出现明显漂移。
2. 部署期 fail-fast 未覆盖所有运行时配置路径。
3. AI key 存储安全实现与根架构说明不一致。

## 发现

### A1：AI key 存储实现与架构契约冲突

**严重度：高**

根 `AGENTS.md` 声明 API key 从 Rust encrypted storage 读取：

- `AGENTS.md:13`

当前实现中，`AiConfigFile` 包含明文 `api_key`，并通过 `save_ai_config` 序列化到 `app_local_data_dir()/ai-config.json`：

- `crates/ai/src/config.rs:20`
- `crates/ai/src/config.rs:221`
- `src-tauri/src/commands/ai.rs:65`
- `src-tauri/src/commands/ai.rs:80`
- `src-tauri/src/state.rs:75`

这不是单纯文档措辞问题：根文档把 key storage 作为 RFC-0005 后 AI 架构的一部分，外部审阅者会据此判断安全边界。当前实现等价于本地明文配置文件，和“encrypted storage”不一致。

**建议动作：**

- 短期二选一：
  - 落地 OS keychain / Tauri secure storage，只在 `ai-config.json` 保存 provider 非敏感字段与 key 引用。
  - 或立即更新 `AGENTS.md` / README / 相关文档，明确当前为明文本地配置，并登记安全债务。
- `load_ai_api_key` 保持按需读取，但应从 keychain 或等价加密存储读取。
- 为“配置视图不泄漏 key”保留现有 `AiSecretInput::{Keep, Clear, Set}` 模型，这部分方向是正确的。

### A2：部署期 fail-fast 未覆盖批量连接定义与部分节点配置

**严重度：中高**

`ConnectionManager::register_connection()` 会调用 `validate_connection_definition()`，但批量路径 `upsert_connections()` / `replace_connections()` 不做同等校验：

- `crates/connections/src/manager/mod.rs:49`
- `crates/connections/src/manager/mod.rs:93`
- `crates/connections/src/manager/mod.rs:125`

`deploy_workflow()` 使用的是 `upsert_connections(graph.connections)`：

- `crates/graph/src/deploy.rs:203`

这意味着 workflow JSON 中带入的连接定义可能绕过注册期校验，直到节点运行时才失败。与根 `AGENTS.md` 的“运行时配置必须显式声明并 fail fast”不完全一致。

同时，部分节点仍存在运行期 fallback：

- `deviceEventTrigger::resolve_connection()` 对缺失 host 使用空字符串、缺失 port 使用 1883：
  - `crates/nodes-io/src/device_event_trigger/mod.rs:135`
- `modbus_loop::extract_unit_id()` 对缺失 unit 使用 1：
  - `crates/nodes-io/src/device_event_trigger/modbus_loop.rs:169`
- `serial_loop::parse_serial_metadata()` 对缺失 baud_rate / delimiter 使用 9600 / `\n`：
  - `crates/nodes-io/src/device_event_trigger/serial_loop.rs:147`

其中部分默认值可能是协议常量或 UI 便利，但目前边界没有在代码中区分“协议默认”与“现场配置缺失”。对工业现场连接参数而言，静默 fallback 容易掩盖部署错误。

**建议动作：**

- 让 `upsert_connections()` / `replace_connections()` 返回 `Result`，并复用 `validate_connection_definition()`。
- `deploy_workflow()` 在进入 `on_deploy` 前完成连接定义校验，错误中包含 connection id、kind、缺失字段。
- 对 `deviceEventTrigger` 各协议 listener 增加部署期校验：
  - MQTT：host / port / topics
  - Modbus：host / port / unit 策略
  - Serial：port_path / baud_rate / delimiter 策略
  - CAN：interface / channel / bitrate
- 如果某个默认值确实是领域不变量，应在对应 crate `AGENTS.md` 或 rustdoc 中说明，不要让它看起来像现场配置 fallback。

### A3：Tauri IPC surface 文档与实现漂移

**严重度：中**

根 `AGENTS.md` 写 Tauri IPC surface 为 77 commands：

- `AGENTS.md:130`

当前 `src-tauri/src/lib.rs` 的 `tauri::generate_handler!` 实际注册 82 个 handler：

- `src-tauri/src/lib.rs:261`

文档中还列出了部分当前 handler 未注册的命令，例如：

- `extract_device_from_text`
- `extract_device_from_text_stream`
- `extract_device_from_pdf`
- `extract_device_proposal`
- `extract_device_proposal_stream`
- `copilot_clear_embeddings`
- `copilot_store_embeddings`

实现侧则有文档未列出的命令，例如：

- `reset_connection_circuit_breaker`
  - `src-tauri/src/commands/connections.rs:49`
- `bind_device_connection`
  - `src-tauri/src/commands/devices.rs:513`
- copilot conversation 系列命令
  - `src-tauri/src/lib.rs:302`
- `restart_app` / `list_network_interfaces`
  - `src-tauri/src/lib.rs:342`

IPC surface 是前后端、测试、权限审计和用户数据边界的核心清单。该清单继续手工维护会持续腐化。

**建议动作：**

- 短期更新根 `AGENTS.md` 与 README 的 IPC 表。
- 增加一个轻量脚本或测试，从 `generate_handler!` / `#[tauri::command]` 提取命令清单，并和文档清单比对。
- 对已删除或迁移到前端的 AI extraction / embedding 命令，明确在文档中标注“已移除 / 前端实现 / 不再是 IPC”。

### A4：架构文档多处滞后

**严重度：低到中**

发现的文档漂移包括：

- `standard_registry()` 已加载 `PurePlugin`，根 `AGENTS.md` 仍写 baseline set 为 `FlowPlugin`, `IoPlugin`：
  - `AGENTS.md:175`
  - `src/lib.rs:100`
- 根 `AGENTS.md` 写 React 18，`web/package.json` 实际为 React 19：
  - `AGENTS.md:13`
  - `web/package.json:47`
- `nodes-flow/AGENTS.md` 仍提 `AiService` 注入路径，但当前 `CodeNodeConfig` 已不包含 AI 服务：
  - `crates/nodes-flow/AGENTS.md:83`
  - `crates/nodes-flow/src/code_node.rs:17`
- `modbusRead` 模块头注释仍说无连接时模拟回退，但实现已要求显式 `simulation=true`：
  - `crates/nodes-io/src/modbus_read.rs:1`
  - `crates/nodes-io/src/modbus_read.rs:375`

这些单点不一定直接造成运行错误，但会降低 `AGENTS.md` 作为 single source of truth 的可信度。

**建议动作：**

- 建一个 “docs freshness” 修复 PR，集中修正根 `AGENTS.md`、README、相关 crate `AGENTS.md` 与模块 rustdoc。
- 对 `React 18 -> 19` 这类事实性升级，更新所有架构总览。
- 对 `PurePlugin`、`deviceSignalRead`、`deviceEventTrigger` 等新增主路径节点，补齐根 README 节点清单与 crate 文档的对应关系。

## 正向观察

- Ring 0 `crates/core` 未发现直接依赖协议 crate；`ts-rs` 仍是 feature-gated，符合 ADR-0017。
- `guarded_execute()` 在 DAG runner 和 pure pull collector 中均被使用，panic / timeout 隔离路径清晰：
  - `crates/core/src/guard.rs:19`
  - `crates/graph/src/runner.rs:293`
  - `crates/graph/src/pull/collector.rs:160`
- metadata 与 payload 分离路径仍然明确，runner 将 `NodeOutput::metadata` 合并到 `ExecutionEvent::Completed`：
  - `crates/graph/src/runner.rs:359`
  - `crates/graph/src/runner.rs:463`
- `nodes-io` 的 `simulation: bool` 显式模拟模式已在 `modbusRead`、`canRead`、`canWrite`、`deviceSignalRead` 等节点中落实，方向符合工业现场 fail-fast 要求：
  - `crates/nodes-io/src/modbus_read.rs:89`
  - `crates/nodes-io/src/can/can_read.rs:27`
  - `crates/nodes-io/src/can/can_write.rs:26`
  - `crates/nodes-io/src/device_signal_read.rs:28`
- 节点注册和能力标签已有 `src/registry.rs` 测试守护，这比纯文档表更可靠：
  - `src/registry.rs:72`

## 建议修复顺序

1. **先处理 AI key 存储表述 / 实现不一致。** 这是对外安全声明级别的问题。
2. **补齐连接定义批量路径校验。** 这能把很多运行期故障前移到部署期。
3. **同步 IPC surface 文档并加防漂移检查。** 避免后续命令增删继续扩大偏差。
4. **集中修正文档滞后点。** 包括 React 版本、`PurePlugin`、`nodes-flow` AI 说明、`modbusRead` 注释。

## 后续验证建议

修复后至少补充以下验证：

- `ConnectionManager::upsert_connections` / `replace_connections` 对无效连接定义返回错误。
- `deploy_workflow` 带无效 `graph.connections` 时在部署期失败。
- IPC 命令清单与文档清单一致性测试。
- AI config 持久化不再包含明文 `api_key`，或文档明确声明当前安全边界。
