# crates/ai — `OpenAI` 兼容客户端实现 + 壳层配置层

> **Ring**: Ring 1
> **对外 crate 名**: `ai`（历史曾名 `nazh-ai-core`，2026-04-24 commit `e0bfbeb` 改名）
> **职责**: `OpenAI` 兼容 API 的 HTTP 客户端实现 + AI 壳层私有配置模型
>
> 根 `AGENTS.md` 的约束对本 crate 同样适用。

## 这个 crate 做什么

自 ADR-0019（已实施，2026-04-26）起，`AiService` trait 与请求/响应类型（`AiCompletionRequest` / `AiCompletionResponse` / `AiMessage` / `AiTokenUsage` / `AiGenerationParams` / `AiThinkingConfig` / `AiThinkingMode` / `AiReasoningEffort` / `StreamChunk` / `AiError`）**全部上移到 Ring 0**（`nazh_core::ai`）。本 crate 现在职责瘦身为：

1. **`OpenAiCompatibleService`** — Ring 0 `AiService` 的 OpenAI 兼容实现，跑 `reqwest` + SSE 解析（`src/client.rs`）。
2. **壳层私有配置模型**（`src/config.rs`）：`AiConfigFile` / `AiProviderDraft` / `AiProviderSecretRecord` / `AiProviderUpsert` / `AiProviderView` / `AiSecretInput` / `AiAgentSettings`——只服务于 Tauri 壳层 IPC，不参与运行时类型契约。
3. **`AiTestResult`**（`src/types.rs`）+ **`OpenAiCompatibleService::test_connection`**（inherent 方法，不在 trait 上）——配置态测试连接。
4. **向后兼容 re-export**：`pub use nazh_core::ai::*` 让老代码 `use ai::AiService` 等仍然工作。

**服务于谁**：
1. **Tauri 壳层**（配置态 Copilot + Workflow 部署）：实例化 `OpenAiCompatibleService`，通过 IPC 命令暴露 `copilotComplete` / `test_ai_provider` 等。
2. **前端 AI 配置 UI**：所有 `AiProvider*` 类型通过 `ts-rs` 导出。
3. **运行时（`code` 节点 / 脚本 `ai_complete()`）**：壳层把 `Arc<OpenAiCompatibleService>` 强转为 `Arc<dyn AiService>` 注入到 `deploy_workflow_with_ai`，运行时通过 Ring 0 trait 调用——本 crate 不再被 `scripting` / `nodes-flow` 直接依赖。

## 对外暴露

```text
crates/ai/src/
├── lib.rs       # re-exports（含 nazh_core::ai 类型 pass-through）
├── client.rs    # OpenAiCompatibleService（impl AiService）+ inherent test_connection
├── config.rs    # AiConfigFile / Providers / Agents（壳层私有）
├── error.rs     # re-export AiError from Ring 0
├── service.rs   # re-export AiService from Ring 0
└── types.rs     # re-export 请求/响应类型 from Ring 0 + AiTestResult（壳层私有）
```

关键 API：
- `OpenAiCompatibleService::new(Arc<RwLock<AiConfigFile>>)` 实例化
- `OpenAiCompatibleService::test_connection(AiProviderDraft)` inherent 方法（壳层用）
- 通过 trait（Ring 0）：`complete` / `stream_complete`

## 内部约定

1. **trait 在 Ring 0，实现在本 crate**。未来要加 Anthropic 原生 / 本地 Llama / Qwen，新建 impl crate 实现 `nazh_core::ai::AiService`，**不要**改本 crate。
2. **`test_connection` 不在 trait 上**。它接收 `AiProviderDraft`——壳层配置类型，不属于 Ring 0 关注点。每个 provider 实现把测试连接做成自己的 inherent 方法。
3. **思考模式（`AiThinkingConfig`）是 OpenAI 协议扩展**。目前仅 DeepSeek 等部分厂商支持，`AiReasoningEffort` 是 DeepSeek 推理强度控制。非支持厂商应 no-op 忽略。
4. **密钥管理走独立类型**：`AiSecretInput` / `AiProviderSecretRecord` — 显式区分"用户输入"与"持久化形式"。前端永远不看 `AiProviderSecretRecord`；壳层负责序列化到本地安全存储。
5. **重试策略**：流式重传在上层 orchestrator（`web/src/lib/workflow-orchestrator.ts`），本 crate 不做自动重试——透明把 `AiError` 传上去。
6. **不绑定具体模型**：`AiGenerationParams` 允许任何 model 字符串，由 provider 决定是否接受。

## 依赖约束

- **允许**：`nazh-core`（trait + 类型源头）、`reqwest`（rustls-tls + json + stream）、`serde`、`serde_json`、`futures-util`、`async-trait`、`tokio`、`ts-rs`（optional）
- **禁止**：`connections`、`nodes-*`、`scripting`、`nodes-flow`——本 crate 是叶子节点性质，不向上反向依赖

> ADR-0019 落地后依赖图的预期形态：`nazh-core` ←─ `ai`，`nazh-core` ←─ `scripting`，`nazh-core` ←─ `nodes-flow`。三者互不依赖。

## 修改本 crate 时

| 改动 | 必须同步 |
|------|----------|
| 改 `AiService` trait（trait 在 Ring 0） | 改 `crates/core/src/ai.rs` 的 trait 定义 + 所有实现（目前只有 `OpenAiCompatibleService`）+ 调用方（`scripting::ScriptNodeBase::ai_complete`、Tauri 壳层 `copilotComplete`） |
| 改 `AiProvider*` / `AiAgent*` 配置类型 | ts-rs 重新生成 + 前端 AI 配置面板（`web/src/lib/ai-config.ts` + 组件） |
| 新增 provider（如 Anthropic 原生） | 新建 impl crate 实现 `nazh_core::ai::AiService` + 在 `AiProviderDraft` 里加 type tag + 前端选项 |
| 改思考模式/推理强度模型（在 Ring 0） | `crates/core/src/ai.rs` + 前端 `AiThinkingConfig` 对应 UI + 文档 |
| 改 `OpenAiCompatibleService::test_connection` | 壳层 IPC 命令 `test_ai_provider`（无须改前端契约，仍是 `AiProviderDraft -> AiTestResult`） |

测试：
```bash
cargo test -p ai
```

集成测试依赖真实 API，由壳层"测试连接"功能手动触发，不在 CI。

## 关联 ADR / RFC

- **ADR-0019** AI 能力依赖反转 — **已实施**（2026-04-26），`AiService` trait 已在 `nazh-core::ai`
- **ADR-0002** Rhai 作为脚本引擎（`code` 节点的 AI 能力通过 `scripting` 的 `ai_complete()` 暴露）
