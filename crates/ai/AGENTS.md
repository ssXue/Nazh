# crates/ai — AI 公共层

> **Ring**: Ring 1
> **对外 crate 名**: `ai`（历史曾名 `nazh-ai-core`，2026-04-24 commit `e0bfbeb` 改名）
> **职责**: `AiService` trait + OpenAI 兼容客户端 + 配置模型 + 流式与思考模式
>
> 根 `AGENTS.md` 的约束对本 crate 同样适用。

## 这个 crate 做什么

Nazh 把 AI 视为**一等公民**——`code` 节点的脚本生成、运行时 `ai_complete()`、桌面壳层的
Copilot 配置面板，都通过本 crate 调 OpenAI 兼容 API。

核心抽象：
- `AiService` trait（异步流式接口 `complete_stream` + 非流式 `complete`） — `src/service.rs`
- `OpenAiCompatibleService` — 默认实现，跑 `reqwest` + SSE 解析 — `src/client.rs`
- 配置模型（`AiProvider*` / `AiAgent*` / `AiConfigFile` / `AiThinkingConfig` / `AiReasoningEffort` 等） — `src/config.rs`
- 请求/响应类型（`AiCompletionRequest` / `AiMessage` / `AiTokenUsage` / `StreamChunk`） — `src/types.rs`

**服务于谁**：
1. **Tauri 壳层**（配置态 Copilot）：通过 `AiService` 提供 "copilotComplete" IPC 命令给前端。
2. **`scripting` crate**（`ai_complete()` 内建函数）：`code` 节点的 Rhai 脚本可以调 AI。
3. **前端 AI 配置 UI**：所有 `AiProvider*` 类型通过 `ts-rs` 导出。

## 对外暴露

```text
crates/ai/src/
├── lib.rs       # re-exports
├── client.rs    # OpenAiCompatibleService + StreamChunk
├── config.rs    # AiConfigFile / Providers / Agents / Thinking / ReasoningEffort
├── error.rs     # AiError
├── service.rs   # AiService trait
└── types.rs     # AiCompletionRequest / AiMessage / AiTokenUsage / AiTestResult
```

关键 API：`AiService`、`OpenAiCompatibleService::new`、`AiCompletionRequest`、`AiProviderDraft`、`AiTokenUsage`。

## 内部约定

1. **`AiService` 是抽象边界**。未来要加 Anthropic 原生 / 本地 Llama / Qwen 支持，**实现** `AiService`，不要替换调用点。
2. **流式优先**：`complete_stream` 返回 `impl Stream<Item = StreamChunk>`。非流式 `complete` 内部用流式实现再收敛，避免分化。
3. **思考模式（`AiThinkingConfig`）是 OpenAI 协议扩展**。目前仅 DeepSeek 等部分厂商支持，`AiReasoningEffort` 是 DeepSeek 推理强度控制。非支持厂商应 no-op 忽略。
4. **密钥管理走独立类型**：`AiSecretInput` / `AiProviderSecretRecord` — 显式区分"用户输入"与"持久化形式"。前端永远不看 `AiProviderSecretRecord`；壳层负责序列化到本地安全存储。
5. **重试策略**：流式重传在上层 orchestrator（`web/src/lib/workflow-orchestrator.ts`），本 crate 不做自动重试——透明把 `AiError` 传上去。
6. **不绑定具体模型**：`AiGenerationParams` 允许任何 model 字符串，由 provider 决定是否接受。

## 依赖约束

- 允许：`reqwest`（rustls-tls + json）、`serde`、`serde_json`、`futures-util`、`chrono`、`url`、`tracing`、`ts-rs`（optional）
- 禁止：`nazh-core`（本 crate **不**依赖引擎，这是有意为之——见 ADR-0019）、`connections`、`nodes-*`、`scripting`

> 这个"不依赖引擎"的设计让 `ai` 可以被 Tauri 壳层（配置态）独立使用，而不绑定工作流运行时。
> 代价是 `scripting` 要依赖 `ai`（反方向），ADR-0019 提议通过把 `AiService` trait 上移到
> Ring 0 来统一依赖方向。目前保持现状。

## 修改本 crate 时

| 改动 | 必须同步 |
|------|----------|
| 改 `AiService` trait | 所有实现（目前只有 `OpenAiCompatibleService`）+ 调用方（`scripting::ScriptNodeBase::ai_complete`、Tauri 壳层 `copilotComplete`） |
| 改 `AiProvider*` / `AiAgent*` 类型 | ts-rs 重新生成 + 前端 AI 配置面板（`web/src/lib/ai-config.ts` + 组件） |
| 新增 provider（如 Anthropic 原生） | 新实现 `AiService` + 在 `AiProviderDraft` 里加 type tag + 前端选项 |
| 改思考模式/推理强度模型 | 前端 `AiThinkingConfig` 对应 UI + 文档 |

测试：
```bash
cargo test -p ai
```

集成测试依赖真实 API，由壳层"测试连接"功能手动触发，不在 CI。

## 关联 ADR / RFC

- **ADR-0002** Rhai 作为脚本引擎（`code` 节点的 AI 能力通过 `scripting` 的 `ai_complete()` 暴露）
- **（待）ADR-0019** AI 能力依赖反转——提议把 `AiService` 上移到 Ring 0，简化 `scripting` 对本 crate 的依赖
