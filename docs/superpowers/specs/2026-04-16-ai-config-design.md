# AI 配置与 API 集成设计

> 日期：2026-04-16
> 状态：已确认，待实现

## 目标

为 Nazh 添加 AI API 配置能力，为后续 AI Copilot（配置态）和 AI 业务节点（运行态）预留统一接口。

## 双态架构

| 维度 | Design-time Copilot | Run-time AI 节点 |
|------|--------------------|--------------------|
| 角色 | 全局副驾驶，辅助设计/生成脚本/排查 | 受限业务节点，参与 DAG 数据流 |
| 调用方 | 前端 UI（设置面板、编辑器侧栏） | 引擎节点 `execute()` |
| 配置作用域 | 全局应用级，用户自用 | 工作流 AST 内声明，受节点 config 约束 |
| 模型/参数 | 用户自由选择，追求效果好 | 由工作流作者指定，追求确定性 |
| 资源限制 | 无特殊限制 | step limit / timeout / token budget |

### 配置分层

```
┌─────────────────────────────────────────────┐
│  AiConfigFile (磁盘层, ai-config.json)        │
│  ├─ providers[] — 提供商凭据（含 api_key）    │
│  ├─ active_provider_id — 当前 Copilot 用的   │
│  └─ copilot_params — Copilot 默认生成参数    │
├─────────────────────────────────────────────┤
│  AiConfigView (IPC 读取层)                    │
│  ├─ providers[] — 前端可见配置（无 api_key）  │
│  └─ has_api_key — 仅暴露是否已保存密钥        │
├─────────────────────────────────────────────┤
│  AiConfigUpdate (IPC 写入层)                  │
│  ├─ providers[] — 可编辑字段 + AiSecretInput │
│  └─ api_key 仅支持 Keep / Clear / Set        │
├─────────────────────────────────────────────┤
│  节点 config 中 (工作流 AST 内)               │
│  ├─ provider_id — 指向全局哪个提供商          │
│  ├─ model — 覆盖默认模型                     │
│  ├─ generation_params — 节点级参数覆盖        │
│  └─ constraints — max_tokens, timeout 等     │
└─────────────────────────────────────────────┘
```

全局配置管"凭据 + Copilot 参数"，节点配置管"运行时约束"。运行时节点通过 `provider_id` 引用全局凭据，不直接持有 api_key。

### 密钥暴露原则

- `api_key` 只存在于后端磁盘层 `AiConfigFile`
- 前端读取配置时只拿到 `AiConfigView`，永远不回传已保存的明文密钥
- 前端写入配置时使用 `AiSecretInput::{Keep, Clear, Set}` 表达密钥变更
- 草稿测试使用 `AiProviderDraft`，允许用户在保存前验证 Base URL / Model / API Key

## Crate 分层

```
Nazh-Workspace/           (Cargo workspace)
├── crates/
│   ├── nazh-core/         # Ring 0：基础类型（不变）
│   └── nazh-ai-core/      # 公共层：AI API HTTP 客户端 + 配置模型
├── src/                   # nazh-engine：后续接入运行时 LLM 节点
└── src-tauri/             # nazh-desktop：本次依赖 ai-core，实现配置时 Copilot 接口
```

本次实现的依赖关系：

```
nazh-core
nazh-ai-core ← src-tauri (nazh-desktop)
```

- `nazh-ai-core` 是纯 AI 通信层，不依赖引擎业务逻辑
- `nazh-ai-core` 有独立的 `AiError`，不引用 `EngineError`
- tauri 层在 IPC 边界转 `String`

后续运行时 AI 节点接入时，再引入：

```
nazh-core ← nazh-ai-core ← nazh-engine
```

届时 engine 层负责错误转换（`AiError → EngineError`）。

## `nazh-ai-core` 模块结构

```
crates/nazh-ai-core/src/
├── lib.rs               # crate 入口，重导出
├── config.rs            # AiConfigFile/View/Update, Provider 相关类型
├── error.rs             # AiError（独立错误类型）
├── types.rs             # AiCompletionRequest/Response, AiMessage, AiTokenUsage 等
├── client.rs            # OpenAiCompatibleClient — reqwest 实现
└── service.rs           # AiService trait 定义
```

### Cargo.toml 依赖

```toml
[dependencies]
async-trait = "0.1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "json"] }
thiserror = "1"
tokio = { version = "1", features = ["sync", "time"] }
tracing = "0.1"
ts-rs = { version = "10", features = ["serde-compat", "serde-json-impl"] }
uuid = { version = "1", features = ["serde", "v4"] }
```

### 核心类型

#### config.rs

```rust
/// 磁盘中的 AI 配置（后端私有，包含密钥）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiConfigFile {
    pub version: u8,
    /// 所有提供商配置列表（含密钥）
    pub providers: Vec<AiProviderSecretRecord>,
    /// 当前激活的提供商 ID（Copilot 使用）
    pub active_provider_id: Option<String>,
    /// Copilot 默认生成参数
    #[serde(default)]
    pub copilot_params: AiGenerationParams,
}

/// 前端读取配置时使用的只读视图（不含明文密钥）
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AiConfigView {
    pub version: u8,
    pub providers: Vec<AiProviderView>,
    pub active_provider_id: Option<String>,
    #[serde(default)]
    pub copilot_params: AiGenerationParams,
}

/// 前端保存配置时使用的写入模型
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AiConfigUpdate {
    pub version: u8,
    pub providers: Vec<AiProviderUpsert>,
    pub active_provider_id: Option<String>,
    #[serde(default)]
    pub copilot_params: AiGenerationParams,
}

/// 磁盘中的单个 AI 提供商记录（含密钥）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderSecretRecord {
    /// 唯一标识（uuid）
    pub id: String,
    /// 显示名称（如 "DeepSeek"、"OpenAI"）
    pub name: String,
    /// OpenAI 兼容 API base URL
    pub base_url: String,
    /// API Key
    pub api_key: String,
    /// 默认模型名（如 "deepseek-chat"、"gpt-4o"）
    pub default_model: String,
    /// 可选：自定义请求头
    #[serde(default)]
    pub extra_headers: HashMap<String, String>,
    /// 是否启用
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// 前端可见的提供商配置视图
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderView {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub default_model: String,
    #[serde(default)]
    pub extra_headers: HashMap<String, String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 是否已经在本地保存过密钥
    #[serde(default)]
    pub has_api_key: bool,
}

/// 前端保存配置时的提供商输入
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderUpsert {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub default_model: String,
    #[serde(default)]
    pub extra_headers: HashMap<String, String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub api_key: AiSecretInput,
}

/// API Key 的写入指令，避免前端回读已保存明文
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[ts(export)]
#[serde(rename_all = "camelCase", tag = "kind", content = "value")]
pub enum AiSecretInput {
    #[default]
    Keep,
    Clear,
    Set(String),
}

/// Copilot 默认生成参数
#[derive(Debug, Clone, Serialize, Deserialize, Default, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AiGenerationParams {
    pub temperature: Option<f32>,      // 缺省 0.7
    pub max_tokens: Option<u32>,       // 缺省 2048
    pub top_p: Option<f32>,            // 缺省 1.0
}
```

#### types.rs

```rust
/// Chat completion 请求（Copilot 和运行时节点共用）
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AiCompletionRequest {
    /// 使用哪个提供商
    pub provider_id: String,
    /// 覆盖默认模型
    pub model: Option<String>,
    /// 消息列表
    pub messages: Vec<AiMessage>,
    /// 生成参数
    pub params: AiGenerationParams,
    /// 超时毫秒（None 使用默认 30s）
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AiMessage {
    pub role: AiMessageRole,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum AiMessageRole {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AiCompletionResponse {
    /// 模型返回的文本内容
    pub content: String,
    /// 本次消耗的 token 数
    pub usage: Option<AiTokenUsage>,
    /// 使用的模型名
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AiTokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AiTestResult {
    pub success: bool,
    pub message: String,
    pub latency_ms: Option<u64>,
}

/// 测试连接时使用的草稿输入
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderDraft {
    /// 可选：已有 provider 的 id；存在时可复用已保存密钥
    pub id: Option<String>,
    pub name: String,
    pub base_url: String,
    /// 草稿态明文密钥；None 表示若 id 命中已有 provider，则复用已保存密钥
    pub api_key: Option<String>,
    pub default_model: String,
    #[serde(default)]
    pub extra_headers: HashMap<String, String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}
```

#### service.rs

```rust
/// AI 服务统一接口，Copilot 和运行时节点共用
#[async_trait]
pub trait AiService: Send + Sync {
    /// Chat completion
    async fn complete(
        &self,
        request: AiCompletionRequest,
    ) -> Result<AiCompletionResponse, AiError>;

    /// 测试提供商连通性（支持草稿配置）
    async fn test_connection(&self, draft: AiProviderDraft) -> Result<AiTestResult, AiError>;
}
```

#### error.rs

```rust
#[derive(Debug, Error)]
pub enum AiError {
    #[error("AI 提供商 `{0}` 不存在")]
    ProviderNotFound(String),

    #[error("AI 提供商 `{0}` 已禁用")]
    ProviderDisabled(String),

    #[error("AI 请求超时（{0} ms）")]
    RequestTimeout(u64),

    #[error("AI API 认证失败: {0}")]
    AuthenticationFailed(String),

    #[error("AI API 请求失败（状态 {status}）: {message}")]
    ApiError { status: u16, message: String },

    #[error("AI 配置无效: {0}")]
    InvalidConfig(String),

    #[error("AI 响应解析失败: {0}")]
    ResponseParseError(String),

    #[error("AI 网络错误: {0}")]
    NetworkError(String),
}
```

#### client.rs

`OpenAiCompatibleService` 实现：

- 持有共享的 `Arc<RwLock<AiConfigFile>>` 和 `reqwest::Client`
- `complete()` → 解析 `provider_id`，从共享配置读取 `base_url + api_key` → POST `/v1/chat/completions` → 解析响应
- `test_connection()` → 优先使用 `AiProviderDraft.api_key`；若为空且 `id` 命中已有 provider，则复用共享配置中的已保存密钥
- 超时通过 `tokio::time::timeout` 包裹

### 配置一致性策略

- 应用启动时先从磁盘加载 `AiConfigFile` 到共享 `Arc<RwLock<_>>`
- `load_ai_config` 直接从共享内存投影出 `AiConfigView`
- `save_ai_config` 执行顺序固定为：
  1. 读取当前 `AiConfigFile`
  2. 将 `AiConfigUpdate` 合并为新的 `AiConfigFile`
  3. 先写入 `ai-config.json.tmp`，再原子 rename 为 `ai-config.json`
  4. 写盘成功后再更新共享 `RwLock`
- 因为 `DesktopState` 与 `OpenAiCompatibleService` 共用同一个 `Arc<RwLock<AiConfigFile>>`，保存后无需重启即可立刻生效

### ts-rs 导出闭环

为避免 AI 类型只停留在设计层，本次同时补齐实际导出入口：

- `crates/nazh-ai-core/src/lib.rs` 新增 `#[cfg(test)] fn export_bindings()`，显式调用所有 AI 相关 `TS` 类型的 `export()`
- `crates/nazh-core/` 保持既有 IPC 类型导出入口；若当前缺失，同步补上同名 `export_bindings()` 测试
- 统一生成命令为：

```bash
TS_RS_EXPORT_DIR=web/src/generated cargo test --workspace --lib export_bindings
```

- `web/src/generated/index.ts` 继续作为前端统一导出入口，AI 类型与现有 IPC 类型一起生成到同一目录

## Engine 层变更

### 新增错误变体

在 `crates/nazh-core/src/error.rs` 中新增：

```rust
#[error("AI 节点 `{node_id}` 调用失败: {message}")]
AiNodeError { node_id: String, message: String },
```

### 运行时 AI 节点（后续实现）

```rust
// src/nodes/ai_node.rs（本次仅预留，不实现）
pub struct AiNodeConfig {
    pub provider_id: String,
    pub model: Option<String>,
    pub system_prompt: String,
    pub temperature: Option<f32>,
    pub max_tokens: u32,
    pub timeout_ms: u64,
}
```

节点 `execute()` 中通过 `Arc<dyn AiService>` 发起调用，受节点 config 约束控制。

## Tauri 层变更

### 新增 IPC 命令

| 命令 | 入参 | 返回 | 职责 |
|------|------|------|------|
| `load_ai_config` | 无 | `AiConfigView` | 读取当前 AI 配置的前端可见视图（不含明文 api_key） |
| `save_ai_config` | `AiConfigUpdate` | `AiConfigView` | 写入全局 ai-config.json，并同步刷新共享内存配置 |
| `test_ai_provider` | `AiProviderDraft` | `AiTestResult` | 测试草稿或已保存提供商的连通性 |
| `copilot_complete` | `AiCompletionRequest` | `AiCompletionResponse` | Copilot 对话补全 |

### 配置文件路径

`app_local_data_dir/ai-config.json`，使用 `app_local_data_dir` 解析（Tauri 的 `Manager::path()` API）。

现有桌面端数据目录约定：

```
app_local_data_dir/
├── ai-config.json               ← AI 凭据（全局，不跟随工作区）
└── workspace/                    ← 默认工程工作区根
    ├── connections.json          ← 连接配置（工作区级）
    ├── deployment-session.json   ← 部署会话（工作区级）
    ├── project-library.json      ← 工程库
    ├── runtime/
    │   └── dead-letters.jsonl
    └── observability/
```

`ai-config.json` 固定存放于 `app_local_data_dir/` 根目录，不跟随 `workspace_path` 参数变化。理由：AI 凭据是用户级全局资源，与工程工作区无关。而 `connections.json`、`deployment-session.json` 等是工作区级数据，通过 `resolve_project_workspace_dir` 解析后存放在对应工作区目录下。两者生命周期独立——切换工作区不影响 AI 配置，清除 AI 配置不影响工程数据。

### Tauri State 扩展

`DesktopState` 新增：

```rust
ai_config: Arc<RwLock<AiConfigFile>>,
ai_service: Arc<dyn AiService>,
```

在应用启动时先加载 `AiConfigFile`，再以同一个共享 `ai_config` 创建 `OpenAiCompatibleService` 并注入。

## 前端变更

### 类型

`web/src/generated/` — 通过 ts-rs 自动生成 `AiConfigView`、`AiConfigUpdate`、`AiProviderView`、`AiProviderUpsert`、`AiProviderDraft`、`AiSecretInput`、`AiGenerationParams`、`AiCompletionRequest`、`AiCompletionResponse`、`AiTestResult`。

`web/src/types.ts` — 从 generated 重导出 AI 类型。

### IPC 包装

`web/src/lib/tauri.ts` — 新增：

```typescript
export async function loadAiConfig(): Promise<AiConfigView> { ... }
export async function saveAiConfig(config: AiConfigUpdate): Promise<AiConfigView> { ... }
export async function testAiProvider(draft: AiProviderDraft): Promise<AiTestResult> { ... }
export async function copilotComplete(request: AiCompletionRequest): Promise<AiCompletionResponse> { ... }
```

### 设置面板

`web/src/components/app/SettingsPanel.tsx` — 新增"AI 配置"区域：

- 提供商列表（添加/编辑/删除）
- 每个提供商：名称、Base URL、API Key、默认模型、启用开关
- API Key 输入框为写入态字段：已保存 provider 不回显明文，只显示"已保存密钥"状态
- "测试连接"按钮使用当前表单草稿调用 `test_ai_provider`
- 当前激活提供商选择
- Copilot 默认参数（temperature、max_tokens、top_p）

### 侧栏

暂不新增 Copilot 入口，但 `copilot_complete` IPC 接口已预留供后续 UI 使用。

## 依赖变更汇总

| crate | 变更 |
|-------|------|
| `Cargo.toml` (workspace) | `members` 新增 `"crates/nazh-ai-core"` |
| `crates/nazh-ai-core/` | 新建，依赖 reqwest/async-trait/serde/thiserror/ts-rs/uuid/tokio/tracing |
| `crates/nazh-core/` | `error.rs` 新增 `AiNodeError` 变体 |
| `src-tauri/` | `Cargo.toml` 新增 `nazh-ai-core` 依赖 |

## 测试

### Rust

- `AiConfigUpdate -> AiConfigFile` 合并测试：覆盖 `AiSecretInput::Keep / Clear / Set`
- `AiConfigFile -> AiConfigView` 投影测试：确保前端视图不包含明文 `api_key`
- `save_ai_config` 测试：写盘成功后共享 `RwLock` 立即可见最新配置
- `save_ai_config` 测试：原子写盘失败时不污染已有 `ai-config.json`
- `test_connection` 测试：草稿明文密钥优先，其次才回退到已保存密钥
- `export_bindings` 测试：AI 类型成功导出到 `web/src/generated/`

### 前端

- `SettingsPanel` 测试：已保存 provider 只显示 `hasApiKey` 状态，不回显真实密钥
- `SettingsPanel` 测试：编辑草稿后点击"测试连接"会提交当前草稿值，而不是旧的已保存值
- `tauri.ts` 包装测试：`loadAiConfig` / `saveAiConfig` / `testAiProvider` 的参数和返回类型契约正确
- 浏览器预览态验证：非 Tauri 环境下 AI 配置区域显示降级提示，不触发 IPC

### 手动验证

- 新增 provider -> 测试连接 -> 保存 -> 重新打开设置页，确认密钥不回显且 `has_api_key=true`
- 修改已保存 provider 的 Base URL / Model，但不改密钥，确认 `Keep` 语义正确
- 清空密钥并保存，确认 `has_api_key=false`，后续调用返回明确配置错误
- 切换激活 provider 后立即调用 `copilot_complete`，确认无需重启即可使用新配置

## 交付范围

### 本次实现

1. 新建 `crates/nazh-ai-core/` — 配置模型、类型、trait、OpenAI 兼容客户端、AiError
2. Workspace `Cargo.toml` 注册新 crate
3. `crates/nazh-core/error.rs` 新增 `AiNodeError` 变体
4. `src-tauri/` 新增 4 个 IPC 命令 + `ai-config.json` 原子读写 + 共享 `ai_config` / `DesktopState` 扩展
5. 前端 `SettingsPanel` 增加 AI 配置区域 + `tauri.ts` IPC 包装 + API Key 写入态交互
6. ts-rs 类型自动生成链路补齐（含 `export_bindings` 导出入口）
7. AI 配置相关单元测试与关键手动验证

### 本次不实现

- Copilot 对话 UI（仅预留 `copilot_complete` IPC 接口）
- 运行时 AI 节点的 DAG 集成（仅预留 `AiNodeError` 错误变体和 `AiNodeConfig` 类型定义）
- `nazh-engine` 对 `nazh-ai-core` 的实际接入与运行时执行链路
