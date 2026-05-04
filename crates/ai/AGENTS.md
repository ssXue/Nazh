# crates/ai — OpenAI 兼容客户端实现 + 壳层配置层

> **Ring**: Ring 1
> **对外 crate 名**: `ai`
> **职责**: OpenAI 兼容 API 的 HTTP 客户端实现 + AI 壳层私有配置模型
>
> 根 `AGENTS.md` 的约束对本 crate 同样适用。

## 定位

自 ADR-0019（已实施）起，`AiService` trait 与请求/响应类型全部上移到 Ring 0（`nazh_core::ai`）。本 crate 职责瘦身为：

1. **`OpenAiCompatibleService`** — Ring 0 `AiService` 的 OpenAI 兼容实现，跑 `reqwest` + SSE 解析（`src/client.rs`）。含 DeepSeek 思考模式 / 推理强度的特殊处理。
2. **壳层私有配置模型**（`src/config.rs`）：`AiConfigFile`（磁盘层）/ `AiConfigView`（前端读取）/ `AiConfigUpdate`（前端写入）/ `AiProviderSecretRecord`（含密钥）/ `AiProviderView` / `AiProviderUpsert` / `AiSecretInput` / `AiAgentSettings`——只服务于 Tauri 壳层 IPC，不参与运行时类型契约。
3. **`AiTestResult`**（`src/types.rs`）+ **`OpenAiCompatibleService::test_connection`**（inherent 方法）——配置态测试连接。
4. **向后兼容 re-export**：`lib.rs` 重新导出 `OpenAiCompatibleService` 与配置类型，老代码 `use ai::...` 仍可工作。

**服务于谁**：
- Tauri 壳层（配置态 Copilot + Workflow 部署）——实例化 `OpenAiCompatibleService`，通过 IPC 暴露。
- 前端 AI 配置 UI——所有 `AiProvider*` / `AiConfig*` 类型通过 ts-rs 导出。
- 运行时（`code` 节点 / 脚本 `ai_complete()`）——壳层把 `Arc<OpenAiCompatibleService>` 强转为 `Arc<dyn AiService>` 注入，运行时通过 Ring 0 trait 调用；本 crate 不再被 `scripting` / `nodes-flow` 直接依赖。

## 对外暴露

```text
crates/ai/src/
├── lib.rs       # re-exports（含 nazh_core::ai 类型 pass-through）+ ts-rs export_bindings 入口
├── client.rs    # OpenAiCompatibleService（impl AiService）+ inherent test_connection
├── config.rs    # AiConfigFile / AiConfigView / AiConfigUpdate / AiProvider* / AiSecretInput / AiAgentSettings
└── types.rs     # AiTestResult（壳层私有）
```

关键 API：
- `OpenAiCompatibleService::new(Arc<RwLock<AiConfigFile>>)` — 实例化
- `OpenAiCompatibleService::test_connection(AiProviderDraft)` — inherent 方法（壳层用）
- 通过 trait（Ring 0）：`complete` / `stream_complete`
- 配置层：`AiConfigFile::to_view()` / `AiConfigFile::merge_update()` / `AiConfigFile::normalize()`

ts-rs 导出：`AiAgentSettings` / `AiConfigView` / `AiConfigUpdate` / `AiProviderView` / `AiProviderUpsert` / `AiProviderDraft` / `AiSecretInput` / `AiTestResult`（由 `ts-export` feature 门控）。

## 内部约定

1. **trait 在 Ring 0，实现在本 crate**。新 provider（Anthropic 原生 / 本地 Llama / Qwen）应新建 impl crate 实现 `nazh_core::ai::AiService`，不改建 crate。
2. **`test_connection` 不在 trait 上**。它接收 `AiProviderDraft`——壳层配置类型，不属于 Ring 0。
3. **思考模式是 DeepSeek 协议扩展**。`provider_accepts_deepseek_options` 根据 base_url 或 model 名判断是否注入 `thinking` / `reasoning_effort` 字段；启用思考模式时省略 `temperature` / `top_p`（DeepSeek API 限制）。非 DeepSeek 厂商的思考配置被静默忽略。
4. **密钥管理三层分离**：`AiProviderSecretRecord`（磁盘，含 api_key）→ `AiProviderView`（前端只读，仅 `has_api_key` bool）→ `AiSecretInput`（前端写入指令：Keep / Clear / Set）。前端永远不接触已保存的明文密钥。
5. **重试策略不在本 crate**——流式重传在上层 orchestrator。本 crate 透明传 `AiError`。
6. **不绑定具体模型**：`AiGenerationParams.model` 允许任何字符串。
7. **`AiConfigFile::normalize()` 保证任意时刻最多一个 enabled provider**。

## 依赖约束

- **允许**：`nazh-core`（trait + 类型源头）、`reqwest`（stream feature）、`serde`、`serde_json`、`futures-util`、`async-trait`、`tokio`、`tracing`、`ts-rs`（optional）
- **禁止**：`connections`、`nodes-*`、`scripting`、`nodes-flow`——本 crate 是叶子节点，不向上反向依赖

依赖图预期：`nazh-core` ← `ai`，`nazh-core` ← `scripting`，`nazh-core` ← `nodes-flow`。三者互不依赖。

## 修改本 crate 时

| 改动 | 必须同步 |
|------|----------|
| 改 `AiService` trait（在 Ring 0） | `crates/core/src/ai.rs` + 所有实现 + 调用方（`scripting::ai_complete`、壳层 `copilotComplete`） |
| 改 `AiProvider*` / `AiConfig*` / `AiAgent*` 配置类型 | ts-rs 重新生成 + 前端 AI 配置面板（`web/src/lib/ai-config.ts` + 组件） |
| 新增 provider（如 Anthropic 原生） | 新建 impl crate 实现 `nazh_core::ai::AiService` + `AiProviderDraft` 加 type tag + 前端选项 |
| 改思考模式/推理强度模型（在 Ring 0） | `crates/core/src/ai.rs` + 前端 `AiThinkingConfig` 对应 UI + 文档 |
| 改 `OpenAiCompatibleService::test_connection` | 壳层 IPC `test_ai_provider`（无须改前端契约） |

测试：
```bash
cargo test -p ai
```

集成测试依赖真实 API，由壳层"测试连接"功能手动触发，不在 CI。

## 关联 ADR / RFC

- **ADR-0019** AI 能力依赖反转 — 已实施（2026-04-26），`AiService` trait 在 `nazh-core::ai`
- **ADR-0002** Rhai 作为脚本引擎（`code` 节点的 AI 能力通过 `scripting` 的 `ai_complete()` 暴露）
