# crates/ai — 壳层 AI 配置模型

> **Ring**: Ring 1
> **对外 crate 名**: `ai`
> **职责**: AI provider 配置管理（密钥、参数、提供商切换），服务于 Tauri 壳层 IPC
>
> 根 `AGENTS.md` 的约束对本 crate 同样适用。

## 定位

RFC-0005（2026-05-12）将所有 AI HTTP 调用前移到前端（Vercel AI SDK）。本 crate 已移除 HTTP 客户端（`client/` 目录已删除），仅保留：

1. **壳层私有配置模型**（`src/config.rs`）：`AiConfigFile`（磁盘层）/ `AiConfigView`（前端读取）/ `AiConfigUpdate`（前端写入）/ `AiProviderSecretRecord`（含密钥）/ `AiProviderView` / `AiProviderUpsert` / `AiSecretInput` / `AiAgentSettings`——只服务于 Tauri 壳层 IPC，不参与运行时类型契约。
2. **`AiTestResult`**（`src/types.rs`）——前端直调 AI SDK 测试连接后的结果类型。

**服务于谁**：
- Tauri 壳层——`load_ai_config` / `save_ai_config` / `load_ai_api_key` IPC 命令。
- 前端 AI 配置 UI——所有 `AiProvider*` / `AiConfig*` 类型通过 ts-rs 导出。

## 对外暴露

```text
crates/ai/src/
├── lib.rs        # re-exports + ts-rs export_bindings 入口
├── config.rs     # AiConfigFile / AiConfigView / AiConfigUpdate / AiProvider* / AiSecretInput / AiAgentSettings
└── types.rs      # AiTestResult（壳层私有）
```

关键 API：
- `AiConfigFile::to_view()` / `AiConfigFile::merge_update()` / `AiConfigFile::normalize()`
- `AiProviderSecretRecord::find_by_id()` / `find_active_by_id()`

ts-rs 导出：`AiAgentSettings` / `AiConfigView` / `AiConfigUpdate` / `AiProviderView` / `AiProviderUpsert` / `AiProviderDraft` / `AiSecretInput` / `AiTestResult`（由 `ts-export` feature 门控）。

## 内部约定

1. **密钥管理三层分离**：`AiProviderSecretRecord`（磁盘，含 api_key）→ `AiProviderView`（前端只读，仅 `has_api_key` bool）→ `AiSecretInput`（前端写入指令：Keep / Clear / Set）。前端永远不接触已保存的明文密钥。
2. **`extra_headers` 只能保存非敏感 header**：`Authorization`、`Proxy-Authorization`、`X-Api-Key` 等敏感 header 不得通过 `extra_headers` 成为密钥旁路。
3. **`AiConfigFile::normalize()` 保证任意时刻最多一个 enabled provider**。
4. **不绑定具体模型**：`AiGenerationParams` 允许任何字符串作为 temperature / max_tokens 等参数。

## 依赖约束

- **允许**：`nazh-core`（`AiError` / `AiGenerationParams`）、`serde`、`serde_json`、`ts-rs`（optional）
- **禁止**：`async-openai`、`reqwest`、`tokio`、`async-trait`、`futures-util`、`connections`、`nodes-*`、`scripting`

## 修改本 crate 时

| 改动 | 必须同步 |
|------|----------|
| 改 `AiProvider*` / `AiConfig*` / `AiAgent*` 配置类型 | ts-rs 重新生成 + 前端 AI 配置面板 |
| 改 `AiGenerationParams` / `AiError`（在 Ring 0） | `crates/core/src/ai.rs` + 所有引用方 |
| 改密钥管理模型 | 壳层 `load_ai_api_key` + 前端 `ai/api-key.ts` |

测试：
```bash
cargo test -p ai
```

## 关联 ADR / RFC

- **RFC-0005** AI 调用前移到前端 — 已实施（2026-05-12），本 crate HTTP 客户端已移除
- **ADR-0019** AI 能力依赖反转 — 已由 RFC-0005 取代
