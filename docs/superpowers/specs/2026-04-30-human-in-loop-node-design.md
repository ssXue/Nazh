# Human-in-the-Loop 节点设计 spec

**日期：** 2026-04-30
**状态：** 设计中
**关联：** RFC-0004（三段式 DSL）、ADR-0009（生命周期钩子）、ADR-0010（Pin 声明）、ADR-0011（节点能力）、ADR-0012（工作流变量）、ADR-0014（PinKind）、ADR-0016（边级可观测性）、ADR-0021（AI 编排入口）

## 动机

工业场景中安全操作（液压控制、高压操作、机器人动作）需要人工确认后才能继续。蓝图文档 §7 Capability DSL 的 `requires_approval` 字段、蓝图评审 spec 中 Safety Compiler 的"危险动作审批"校验项，都依赖一个基础能力：**工作流执行暂停，等待外部人工响应**。

当前引擎无此能力——所有节点在 `transform()` 内同步完成，无阻塞等待模式。

### 目标

设计一个 `humanLoop` 节点，支持：

1. 暂停工作流执行，向 UI 推送审批请求（含结构化表单）
2. 等待人工填写表单并提交响应
3. 响应数据注入 payload，工作流继续
4. 独立超时 + 默认动作（自动通过 / 自动拒绝 / 路由到 fallback 节点）
5. 可被 DSL 编译器引用（Capability DSL 的 `requires_approval` 编译为 `humanLoop` 节点）

### 非目标

- 审批流程编排（多级审批、会签、或签）——留作后续 ADR
- 审批历史持久化——依赖 ADR-0012 Phase 3 的变量持久化
- 权限控制（谁能审批）——属于 Safety Compiler 范围
- 审批撤回——Phase 1 不支持

## 需求与约束

### 硬需求

1. **阻塞等待**：节点 `transform()` 能暂停执行，等待外部信号，不占用 CPU
2. **结构化表单**：节点 config 定义表单 schema，前端动态渲染，响应数据经验证后注入 payload
3. **独立超时**：`approval_timeout_ms` 与节点 `timeout_ms` 分离，超时执行默认动作
4. **默认动作枚举**：`AutoApprove`（注入表单默认值）/ `AutoReject`（Failed）/ `FallbackNode(String)`（路由到指定节点）
5. **Ring 纯净性**：Registry 类型不进入 `crates/core/`
6. **优雅关闭**：undeploy 时 pending 审批被清理，节点收到 cancellation 信号
7. **可追踪**：每次审批请求 / 响应 / 超时产生 `ExecutionEvent`

### 可协商

- 表单 schema 的具体子集（JSON Schema 全集 vs 自定义简化格式）
- FallbackNode 路由是否需要新 Pin 声明（`fallback` output pin）
- `NodeLifecycleContext` 是否增加 `workflow_id` 字段

## 设计

### 核心机制：oneshot channel 阻塞

```
ContextRef 到达 → transform() 被调用
  → 创建 (oneshot::Sender, oneshot::Receiver) 对
  → Sender 存入 ApprovalRegistry
  → 发射 ExecutionEvent::HumanLoopPending（含表单 schema）
  → Tauri emit → 前端渲染审批面板
  → await Receiver（阻塞，不占 CPU）
  → IPC respond_human_loop → Sender.send(response)
  → Receiver 收到 response → 注入 payload → 返回 NodeExecution
```

节点每次 `transform()` 调用创建独立的 approval slot。上游连续发来的 ContextRef 在 channel buffer 排队，形成天然背压——前一个审批完成前不会处理下一个。

### 类型定义

#### ApprovalRegistry（`crates/nodes-io/src/human_loop/registry.rs`）

```rust
/// 审批 ID = UUID，每次 transform() 生成唯一 ID
pub type ApprovalId = Uuid;

/// 审批槽——存储在 DashMap 中，IPC 命令通过 ID 查找
pub struct ApprovalSlot {
    pub workflow_id: String,
    pub node_id: String,
    pub node_label: String,           // 节点显示名
    pub form_schema: Vec<FormSchemaField>,
    pub pending_since: DateTime<Utc>,
    pub approval_timeout_ms: Option<u64>,
    pub default_action: DefaultAction,
    /// oneshot sender——调用 send() 唤醒阻塞的 transform()
    pub responder: Mutex<Option<oneshot::Sender<HumanLoopResponse>>>,
}

/// 人工响应
pub struct HumanLoopResponse {
    pub action: ResponseAction,
    pub form_data: serde_json::Value,  // 表单数据（键值对）
    pub comment: Option<String>,       // 审批意见
    pub responded_by: Option<String>,  // 审批人标识（Phase 1 可选）
}

/// 响应动作
pub enum ResponseAction {
    Approved,
    Rejected,
}

/// 超时默认动作
pub enum DefaultAction {
    /// 注入表单默认值，工作流继续
    AutoApprove,
    /// 发射 ExecutionEvent::Failed，工作流中断
    AutoReject,
    /// 路由到指定节点（需 fallback output pin）
    FallbackNode(String),
}

/// per-deployment 审批注册表
pub struct ApprovalRegistry {
    slots: DashMap<ApprovalId, ApprovalSlot>,
}

impl ApprovalRegistry {
    pub fn new() -> Self { /* ... */ }

    /// 注册 pending 审批，返回 approval_id + Receiver
    pub fn create_slot(&self, slot: ApprovalSlot) -> (ApprovalId, oneshot::Receiver<HumanLoopResponse>) {
        let approval_id = Uuid::new_v4();
        let (tx, rx) = oneshot::channel();
        // store slot with tx wrapped in Mutex<Option<>>
        // ...
        (approval_id, rx)
    }

    /// 人工响应——IPC 命令调用
    pub fn respond(&self, approval_id: ApprovalId, response: HumanLoopResponse) -> Result<(), EngineError> {
        // 取出 slot，通过 oneshot sender 发送 response
        // ...
    }

    /// 清理 workflow 的所有 pending 审批（undeploy 时调用）
    pub fn cleanup_workflow(&self, workflow_id: &str) {
        // 移除所有 matching slots，drop sender（唤醒 receiver 时返回 Err）
    }

    /// 列出 pending 审批（IPC 命令用）
    pub fn list_pending(&self, workflow_id: Option<&str>) -> Vec<PendingApprovalSummary> {
        // ...
    }
}
```

#### 表单 Schema（`crates/nodes-io/src/human_loop/form.rs`）

```rust
/// 表单字段定义——简化 JSON Schema 子集
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum FormSchemaField {
    #[serde(rename = "boolean")]
    Boolean {
        name: String,
        label: String,
        #[serde(default)]
        required: bool,
        #[serde(default)]
        default: Option<bool>,
    },
    #[serde(rename = "number")]
    Number {
        name: String,
        label: String,
        #[serde(default)]
        required: bool,
        #[serde(default)]
        default: Option<f64>,
        #[serde(default)]
        min: Option<f64>,
        #[serde(default)]
        max: Option<f64>,
        #[serde(default)]
        unit: Option<String>,
    },
    #[serde(rename = "string")]
    String {
        name: String,
        label: String,
        #[serde(default)]
        required: bool,
        #[serde(default)]
        default: Option<String>,
        #[serde(default)]
        multiline: bool,
        #[serde(default)]
        max_length: Option<usize>,
    },
    #[serde(rename = "select")]
    Select {
        name: String,
        label: String,
        #[serde(default)]
        required: bool,
        options: Vec<SelectOption>,
        #[serde(default)]
        default: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
}
```

#### 节点 Config（`crates/nodes-io/src/human_loop/config.rs`）

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanLoopNodeConfig {
    /// 节点显示标题
    #[serde(default)]
    pub title: Option<String>,

    /// 节点描述 / 审批说明
    #[serde(default)]
    pub description: Option<String>,

    /// 结构化表单 schema
    #[serde(default)]
    pub form_schema: Vec<FormSchemaField>,

    /// 审批独立超时（毫秒）。None = 无限等待
    #[serde(default)]
    pub approval_timeout_ms: Option<u64>,

    /// 超时默认动作
    #[serde(default = "DefaultAction::AutoReject")]
    pub default_action: DefaultAction,
}
```

#### 节点实现（`crates/nodes-io/src/human_loop/node.rs`）

```rust
pub struct HumanLoopNode {
    id: String,
    config: HumanLoopNodeConfig,
    registry: Arc<ApprovalRegistry>,
    workflow_id: Mutex<Option<String>>,  // 在 on_deploy 中填充
}

#[async_trait]
impl NodeTrait for HumanLoopNode {
    nazh_core::impl_node_meta!("humanLoop");

    fn capabilities(&self) -> NodeCapabilities {
        NodeCapabilities::HUMAN_LOOP
    }

    fn input_pins(&self) -> Vec<PinDefinition> {
        vec![PinDefinition::required_input(PinType::Json, "触发审批的输入数据")]
    }

    fn output_pins(&self) -> Vec<PinDefinition> {
        let mut pins = vec![PinDefinition::named_output(
            "out", PinType::Json, "审批通过后的输出（含表单数据）"
        )];
        if matches!(&self.config.default_action, DefaultAction::FallbackNode(_)) {
            pins.push(PinDefinition::named_output(
                "fallback", PinType::Json, "超时 fallback 路径"
            ));
        }
        pins
    }

    async fn on_deploy(&self, ctx: NodeLifecycleContext) -> Result<LifecycleGuard, EngineError> {
        // 存储 workflow_id（需要 NodeLifecycleContext 扩展，见下文）
        *self.workflow_id.lock().await = Some(ctx.workflow_id.clone());
        Ok(LifecycleGuard::noop())
    }

    async fn transform(&self, trace_id: Uuid, payload: Value)
        -> Result<NodeExecution, EngineError>
    {
        let workflow_id = self.workflow_id.lock().await
            .clone()
            .ok_or_else(|| EngineError::NodeExecutionFailed {
                node_id: self.id.clone(),
                reason: "节点未部署（缺少 workflow_id）".into(),
            })?;

        let form_defaults = self.build_form_defaults();
        let slot = ApprovalSlot {
            workflow_id,
            node_id: self.id.clone(),
            node_label: self.config.title.clone().unwrap_or_else(|| self.id.clone()),
            form_schema: self.config.form_schema.clone(),
            pending_since: Utc::now(),
            approval_timeout_ms: self.config.approval_timeout_ms,
            default_action: self.config.default_action.clone(),
            responder: Mutex::new(None), // 由 create_slot 填充
        };

        let (approval_id, rx) = self.registry.create_slot(slot);

        // 注意：ExecutionEvent 扩展见下文

        // 等待响应或超时
        let result = match self.config.approval_timeout_ms {
            Some(timeout_ms) => {
                tokio::select! {
                    response = rx => {
                        self.handle_response(response?, payload, approval_id)
                    }
                    _ = tokio::time::sleep(Duration::from_millis(timeout_ms)) => {
                        self.handle_timeout(payload, approval_id, &form_defaults)
                    }
                }
            }
            None => {
                let response = rx.await
                    .map_err(|_| EngineError::NodeExecutionFailed {
                        node_id: self.id.clone(),
                        reason: "审批槽被关闭（工作流已卸载）".into(),
                    })?;
                self.handle_response(response, payload, approval_id)
            }
        };

        result
    }
}
```

### ExecutionEvent 扩展

在 `crates/core/src/event.rs` 新增变体：

```rust
pub enum ExecutionEvent {
    // ... 现有变体 ...

    /// 人工审批等待中（Human-in-the-Loop）
    HumanLoopPending {
        stage: String,          // node_id
        trace_id: Uuid,
        approval_id: Uuid,
        form_schema: Value,     // 序列化的表单 schema
        timeout_ms: Option<u64>,
    },

    /// 人工审批已响应
    HumanLoopResolved {
        stage: String,
        trace_id: Uuid,
        approval_id: Uuid,
        action: String,         // "approved" | "rejected" | "timeout"
        responded_by: Option<String>,
    },
}
```

### Ring 0 微调：NodeLifecycleContext

```rust
// crates/core/src/lifecycle.rs
pub struct NodeLifecycleContext {
    pub resources: SharedResources,
    pub handle: NodeHandle,
    pub shutdown: CancellationToken,
    pub variables: Arc<WorkflowVariables>,
    pub workflow_id: String,  // 新增：节点所属工作流 ID
}
```

Runner 在调用 `on_deploy` 时传入 `workflow_id`。变更范围小——仅 `NodeLifecycleContext` 构造处。

### 新 Capability Flag

```rust
// crates/core/src/node.rs
bitflags! {
    pub struct NodeCapabilities: u32 {
        // ... 现有 flags ...
        const HUMAN_LOOP = 0b0000_0001_0000_0000;  // 1 << 8
    }
}
```

前端对应更新 `web/src/lib/node-capabilities.ts`。

### IPC 接口

#### 新事件通道

| 事件 | 方向 | Payload |
|------|------|---------|
| `workflow://human-loop-pending` | Rust → 前端 | `HumanLoopPendingPayload` |
| `workflow://human-loop-resolved` | Rust → 前端 | `HumanLoopResolvedPayload` |

#### IPC 类型（`crates/tauri-bindings/src/lib.rs`）

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct HumanLoopPendingPayload {
    pub workflow_id: String,
    pub node_id: String,
    pub node_label: String,
    pub approval_id: String,
    pub form_schema: serde_json::Value,
    pub pending_since: String,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct HumanLoopResolvedPayload {
    pub workflow_id: String,
    pub node_id: String,
    pub approval_id: String,
    pub action: String,
    pub responded_by: Option<String>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct PendingApprovalSummary {
    pub approval_id: String,
    pub workflow_id: String,
    pub node_id: String,
    pub node_label: String,
    pub pending_since: String,
    pub timeout_ms: Option<u64>,
    pub form_schema: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct RespondHumanLoopRequest {
    pub workflow_id: String,
    pub approval_id: String,
    pub action: String,         // "approved" | "rejected"
    pub form_data: serde_json::Value,
    pub comment: Option<String>,
    pub responded_by: Option<String>,
}
```

#### 新 IPC 命令（`src-tauri/src/commands/human_loop.rs`）

```rust
/// 人工响应审批
#[tauri::command]
pub(crate) async fn respond_human_loop(
    state: State<'_, DesktopState>,
    request: RespondHumanLoopRequest,
) -> Result<(), String> {
    let registry = state.approval_registry.lock().await;
    let response = HumanLoopResponse {
        action: match request.action.as_str() {
            "approved" => ResponseAction::Approved,
            "rejected" => ResponseAction::Rejected,
            other => return Err(format!("未知动作: {other}")),
        },
        form_data: request.form_data,
        comment: request.comment,
        responded_by: request.responded_by,
    };
    registry.respond(Uuid::parse_str(&request.approval_id).map_err(|e| e.to_string())?, response)
        .map_err(|e| e.to_string())
}

/// 列出 pending 审批
#[tauri::command]
pub(crate) async fn list_pending_approvals(
    state: State<'_, DesktopState>,
    workflow_id: Option<String>,
) -> Result<Vec<PendingApprovalSummary>, String> {
    let registry = state.approval_registry.lock().await;
    Ok(registry.list_pending(workflow_id.as_deref()))
}
```

#### 事件转发（`src-tauri/src/events.rs`）

在 `spawn_execution_event_forwarder` 中增加对 `HumanLoopPending` / `HumanLoopResolved` 的特殊处理（与 `VariableChanged` 同模式）：

```rust
// 新增分支
ExecutionEvent::HumanLoopPending { .. } => {
    let payload = tauri_bindings::human_loop_pending_payload(workflow_id, event);
    let _ = app.emit("workflow://human-loop-pending", payload);
    continue;
}
ExecutionEvent::HumanLoopResolved { .. } => {
    let payload = tauri_bindings::human_loop_resolved_payload(workflow_id, event);
    let _ = app.emit("workflow://human-loop-resolved", payload);
    continue;  // 不要再转发到 node-status
}
```

### 前端

#### 节点注册（`web/src/components/flowgram/`）

- 新增 `nodes/humanLoop/` 目录：definition + settings + form renderer
- 注册到 `flowgram-node-library.ts`
- Catalog section: `流程控制`，badge: `审批`

#### 审批面板（`web/src/components/runtime/`）

- `ApprovalQueue` 组件——显示 pending 审批列表
- `ApprovalForm` 组件——根据 `form_schema` 动态渲染表单字段
- 集成到 Runtime dock 中（与 Variables tab 同级，新增 `审批` tab）

#### 事件监听

```typescript
// web/src/lib/tauri.ts 新增
export async function onHumanLoopPending(
  handler: (payload: HumanLoopPendingPayload) => void,
): Promise<() => void> { /* ... */ }

export async function onHumanLoopResolved(
  handler: (payload: HumanLoopResolvedPayload) => void,
): Promise<() => void> { /* ... */ }

export async function respondHumanLoop(
  request: RespondHumanLoopRequest,
): Promise<void> { /* ... */ }

export async function listPendingApprovals(
  workflowId?: string,
): Promise<PendingApprovalSummary[]> { /* ... */ }
```

### Crate 归属

```
crates/
  nodes-io/src/human_loop/      # 新模块
    mod.rs                       # 导出 + Plugin 注册
    node.rs                      # HumanLoopNode 实现
    config.rs                    # HumanLoopNodeConfig
    form.rs                      # FormSchemaField 类型
    registry.rs                  # ApprovalRegistry + ApprovalSlot

  crates/core/src/event.rs       # 新增 ExecutionEvent 变体
  crates/core/src/lifecycle.rs   # NodeLifecycleContext 增加 workflow_id
  crates/core/src/node.rs        # 新增 HUMAN_LOOP capability
  crates/tauri-bindings/         # 新增 IPC 类型
src-tauri/src/commands/human_loop.rs  # 新增 IPC 命令
src-tauri/src/events.rs          # 事件转发扩展
web/src/components/flowgram/nodes/humanLoop/  # 前端节点
web/src/components/runtime/ApprovalQueue.tsx   # 审批面板
web/src/components/runtime/ApprovalForm.tsx    # 动态表单
web/src/lib/tauri.ts             # IPC API 扩展
```

### 与现有系统的关系

| 现有系统 | HITL 节点如何复用 / 扩展 |
|----------|--------------------------|
| ADR-0009 生命周期 | `on_deploy` 存储 `workflow_id`；`CancellationToken` 处理 undeploy 时清理 |
| ADR-0010 Pin 声明 | output pins 按 `default_action` 动态声明（fallback pin）；`PinType::Json` |
| ADR-0011 节点能力 | 新 `HUMAN_LOOP` flag；前端 badge 渲染 |
| ADR-0012 工作流变量 | 审批表单默认值可引用变量（`$var.target_pressure`）——Phase 2 |
| ADR-0013 子图 | 状态机 fault state 可内嵌 `humanLoop` 节点 |
| ADR-0014 PinKind | Exec pin 触发审批；Data pin 传递响应数据 |
| ADR-0016 边级可观测 | 审批等待时长可被 `EdgeTransmitSummary` 覆盖 |
| ADR-0021 DSL 编译 | Capability DSL `requires_approval: true` 编译为 `humanLoop` 节点 |
| ConnectionManager | HITL 节点不需要连接——纯逻辑节点 |
| SharedResources | `ApprovalRegistry` 通过 `SharedResources` 传递给节点工厂 |

## 实施拆解

### Phase 0：Ring 0 微调（~1 天）

- `NodeLifecycleContext` 增加 `workflow_id` 字段
- `NodeCapabilities` 增加 `HUMAN_LOOP` flag
- `ExecutionEvent` 增加 `HumanLoopPending` / `HumanLoopResolved` 变体
- Runner 传 `workflow_id` 到 `on_deploy` context
- `cargo test --workspace` 确保无回归

### Phase 1：节点核心（~3 天）

- `crates/nodes-io/src/human_loop/` 模块：config / form / registry / node
- Plugin 注册
- 单元测试：表单验证 / 超时处理 / 响应注入
- 集成测试：`tests/workflow.rs` 新增 `human_loop_approval` / `human_loop_timeout` / `human_loop_rejection`

### Phase 2：IPC 接入（~2 天）

- `crates/tauri-bindings/` 新增 IPC 类型 + ts-rs 导出
- `src-tauri/src/commands/human_loop.rs` 新增命令
- `src-tauri/src/events.rs` 事件转发扩展
- `DesktopState` 持有 `Arc<ApprovalRegistry>`
- `cargo test -p tauri-bindings --features ts-export export_bindings`

### Phase 3：前端节点（~3 天）

- `web/src/components/flowgram/nodes/humanLoop/` 节点定义 + 设置面板
- `flowgram-node-library.ts` 注册
- `web/src/lib/tauri.ts` IPC API
- `web/src/lib/__tests__/` 单元测试

### Phase 4：审批面板 UI（~3 天）

- `ApprovalQueue` 组件
- `ApprovalForm` 动态表单渲染器
- 集成到 Runtime dock
- 事件监听 + 状态管理

### Phase 5：DSL 编译器对接（~2 天，依赖 Phase 1-4 + ADR-0021 实施）

- `dsl-compiler` 将 Capability DSL `requires_approval: true` 编译为 `humanLoop` 节点
- 表单 schema 从 Capability inputs 自动生成

## 风险与未知

| 风险 | 缓解 |
|------|------|
| `transform()` 长时间阻塞占用 runner task | 每个节点独立 Tokio task，阻塞不影响的节点；task 本身 await 不占 CPU |
| undeploy 时 pending 审批丢失 | `CancellationToken` 触发 → receiver Err → `ExecutionEvent::Failed`；前端清理 pending 列表 |
| FallbackNode 路由需要动态 output pin | `output_pins()` 按 config 条件声明；部署期 pin 校验验证 fallback 节点存在 |
| 表单 schema 过于复杂 | Phase 1 仅支持 boolean / number / string / select 四种类型；后续按需扩展 |
| 多级审批需求提前到来 | `humanLoop` 节点可串联（A 审批 → B 审批）；复杂流程需新 ADR |
| `NodeLifecycleContext` 扩展影响面 | `workflow_id` 是 `String`，零新依赖；所有现有 `on_deploy` 实现不受影响 |
