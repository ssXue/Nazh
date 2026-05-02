# Human-in-the-Loop 审批节点 Implementation Plan

> **Status:** 已实施（2026-05-03），Task 1-8 核心完成。偏离项与 deferred items 见下方「实施偏离」段。

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 实现 `humanLoop` 审批节点，支持工作流执行暂停等待人工响应、结构化表单、独立超时与默认动作。

**Architecture:** oneshot channel 阻塞模式。节点 `transform()` 创建 `ApprovalSlot`（含 oneshot sender），注册到 `ApprovalRegistry`（DashMap），await receiver。IPC 命令 `respond_human_loop` 通过 sender 唤醒节点。`ApprovalRegistry` 通过 `SharedResources` 注入节点工厂。前端事件转发走 `workflow://human-loop-pending` / `workflow://human-loop-resolved` 独立通道。

**Tech Stack:** Rust（tokio oneshot + DashMap + chrono）、Tauri IPC（invoke/emit）、React 18 + TypeScript、ts-rs 类型导出。

**Design spec:** `docs/superpowers/specs/2026-04-30-human-in-loop-node-design.md`

---

## File Structure

```
crates/core/src/event.rs              # 新增 ExecutionEvent 变体
crates/core/src/node.rs               # 新增 HUMAN_LOOP capability
crates/core/src/lifecycle.rs          # NodeLifecycleContext 增加 workflow_id
crates/core/src/lib.rs                # re-export 更新
crates/core/src/export_bindings.rs    # 新类型 ts-rs 导出

crates/nodes-io/src/human_loop/       # 新模块目录
  mod.rs                              # 导出 + Plugin 注册
  config.rs                           # HumanLoopNodeConfig
  form.rs                             # FormSchemaField 类型
  registry.rs                         # ApprovalRegistry + ApprovalSlot
  node.rs                             # HumanLoopNode 实现
crates/nodes-io/src/lib.rs            # mod human_loop + 注册

crates/graph/src/deploy.rs            # 传 workflow_id 到 NodeLifecycleContext

crates/tauri-bindings/src/lib.rs      # 新增 IPC 类型 + ts-rs 导出
src-tauri/src/commands/human_loop.rs  # 新增 IPC 命令
src-tauri/src/commands/mod.rs         # 注册新模块
src-tauri/src/lib.rs                  # generate_handler! 注册
src-tauri/src/events.rs               # 事件转发扩展
src-tauri/src/state.rs                # DesktopState 持有 ApprovalRegistry

web/src/components/flowgram/nodes/humanLoop/index.ts  # 前端节点定义
web/src/components/flowgram/nodes/shared.ts            # NazhNodeKind 扩展
web/src/components/flowgram/nodes/catalog.ts           # catalog 注册
web/src/components/flowgram/flowgram-node-library.ts   # 节点库注册
web/src/lib/node-capabilities.ts                       # 新 capability flag
web/src/lib/tauri.ts                                   # IPC 包装函数
web/src/components/app/RuntimeDock.tsx                  # 新增 审批 tab
web/src/components/app/ApprovalQueue.tsx               # 审批队列组件
web/src/components/app/ApprovalForm.tsx                # 动态表单组件

src/registry.rs                       # 契约测试更新
```

---

### Task 1: Ring 0 — NodeCapabilities 增加 HUMAN_LOOP + NodeLifecycleContext 增加 workflow_id + ExecutionEvent 新变体

**Files:**
- Modify: `crates/core/src/node.rs:73-143` — NodeCapabilities bitflags
- Modify: `crates/core/src/node.rs:379-389` — 位分配测试
- Modify: `crates/core/src/lifecycle.rs:39-49` — NodeLifecycleContext
- Modify: `crates/core/src/event.rs:26-63` — ExecutionEvent 枚举
- Modify: `crates/core/src/event.rs:126-164` — ExecutionEventSerde
- Modify: `crates/core/src/event.rs:166-267` — From impls
- Modify: `crates/core/src/export_bindings.rs` — ts-rs 导出
- Modify: `crates/graph/src/deploy.rs:235-240` — 传 workflow_id
- Modify: `crates/graph/src/deploy.rs:83-89` — deploy_workflow_with_ai 签名已有 workflow_id

- [ ] **Step 1: 在 `NodeCapabilities` 增加 `HUMAN_LOOP` flag**

在 `crates/core/src/node.rs` 的 bitflags 块中，`BLOCKING` 之后添加：

```rust
        /// **人工交互**：节点执行需要等待外部人工响应（审批、确认、表单填写）。
        ///
        /// **契约**：`transform` 路径上会阻塞等待 IPC 响应，但不占 CPU（oneshot channel await）。
        /// **消费者**：前端画布渲染审批 badge；Runner 识别此类节点不做 spawn_blocking
        /// （内部已是 async await）。
        const HUMAN_LOOP = 0b0001_0000_0000;
```

- [ ] **Step 2: 更新位分配测试**

在 `crates/core/src/node.rs` 测试 `node_capabilities_位分配与_adr_0011_一致` 中添加：

```rust
        assert_eq!(NodeCapabilities::HUMAN_LOOP.bits(), 0b0001_0000_0000);
```

- [ ] **Step 3: `NodeLifecycleContext` 增加 `workflow_id` 字段**

在 `crates/core/src/lifecycle.rs` 的 `NodeLifecycleContext` 结构体中，`variables` 字段后添加：

```rust
    /// 节点所属工作流的唯一标识。
    ///
    /// 由 Runner 在 `deploy_workflow_with_ai` 的阶段 1 传入（从 `workflow_id_for_events` 取值）。
    /// HITL 等需要关联到工作流 scope 的节点使用此字段。
    pub workflow_id: String,
```

- [ ] **Step 4: 更新 `deploy.rs` 传 `workflow_id`**

在 `crates/graph/src/deploy.rs` 的 `NodeLifecycleContext` 构造处（约第 235-240 行），添加 `workflow_id` 字段：

```rust
        let ctx = NodeLifecycleContext {
            resources: shared_resources.clone(),
            handle,
            shutdown: shutdown_token.child_token(),
            variables: Arc::clone(&workflow_variables),
            workflow_id: workflow_id_for_events.clone(),
        };
```

- [ ] **Step 5: 更新所有 `NodeLifecycleContext` 构造点**

搜索所有 `NodeLifecycleContext {` 的构造点，补上 `workflow_id` 字段。已知位置：
- `crates/graph/src/deploy.rs` — 如上
- `crates/core/src/lifecycle.rs` 测试 `lifecycle_context_暴露_variables` — 添加 `workflow_id: "test-wf".to_owned()`

- [ ] **Step 6: 在 `ExecutionEvent` 新增两个变体**

在 `crates/core/src/event.rs` 的 `ExecutionEvent` 枚举中，`BackpressureDetected` 之后添加：

```rust
    /// 人工审批等待中（Human-in-the-Loop）。
    HumanLoopPending {
        stage: String,
        trace_id: Uuid,
        approval_id: Uuid,
        form_schema: serde_json::Value,
        timeout_ms: Option<u64>,
    },
    /// 人工审批已响应。
    HumanLoopResolved {
        stage: String,
        trace_id: Uuid,
        approval_id: Uuid,
        action: String,
        responded_by: Option<String>,
    },
```

- [ ] **Step 7: 更新 `ExecutionEventSerde` + `From` impls**

在 `ExecutionEventSerde` 枚举中添加对应变体：

```rust
    HumanLoopPending {
        stage: String,
        trace_id: Uuid,
        approval_id: Uuid,
        form_schema: serde_json::Value,
        timeout_ms: Option<u64>,
    },
    HumanLoopResolved {
        stage: String,
        trace_id: Uuid,
        approval_id: Uuid,
        action: String,
        responded_by: Option<String>,
    },
```

在两个 `From` impl 的 match 臂中添加对应映射（与其他变体同模式）。

- [ ] **Step 8: 更新 `export_bindings.rs`**

无需额外导出——`ExecutionEvent` 已在 `export_all` 中导出，新变体自动包含。

- [ ] **Step 9: 运行测试确认无回归**

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

- [ ] **Step 10: Commit**

```bash
git add crates/core/src/node.rs crates/core/src/lifecycle.rs crates/core/src/event.rs crates/graph/src/deploy.rs
git commit -m "feat: Ring 0 增加 HUMAN_LOOP capability + NodeLifecycleContext.workflow_id + ExecutionEvent 审批变体"
```

---

### Task 2: Ring 1 — ApprovalRegistry + FormSchema + HumanLoopNodeConfig

**Files:**
- Create: `crates/nodes-io/src/human_loop/mod.rs`
- Create: `crates/nodes-io/src/human_loop/form.rs`
- Create: `crates/nodes-io/src/human_loop/registry.rs`
- Create: `crates/nodes-io/src/human_loop/config.rs`

- [ ] **Step 1: 创建 `form.rs` — 表单 Schema 类型**

```rust
//! HITL 节点表单 schema 定义。

use serde::{Deserialize, Serialize};

/// 表单字段选项。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
}

/// 表单字段定义——简化 JSON Schema 子集。
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
```

- [ ] **Step 2: 创建 `config.rs` — 节点配置**

```rust
//! HITL 节点配置类型。

use serde::{Deserialize, Serialize};

use super::form::FormSchemaField;

/// 超时默认动作。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum DefaultAction {
    /// 注入表单默认值，工作流继续。
    AutoApprove,
    /// 发射 ExecutionEvent::Failed，工作流中断。
    AutoReject,
    /// 路由到指定节点（需 fallback output pin）。
    FallbackNode(String),
}

impl Default for DefaultAction {
    fn default() -> Self {
        Self::AutoReject
    }
}

/// HITL 节点配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanLoopNodeConfig {
    /// 节点显示标题。
    #[serde(default)]
    pub title: Option<String>,
    /// 节点描述 / 审批说明。
    #[serde(default)]
    pub description: Option<String>,
    /// 结构化表单 schema。
    #[serde(default)]
    pub form_schema: Vec<FormSchemaField>,
    /// 审批独立超时（毫秒）。None = 无限等待。
    #[serde(default)]
    pub approval_timeout_ms: Option<u64>,
    /// 超时默认动作。
    #[serde(default)]
    pub default_action: DefaultAction,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn config_默认动作为_auto_reject() {
        let config: HumanLoopNodeConfig =
            serde_json::from_str("{}").unwrap();
        assert_eq!(config.default_action, DefaultAction::AutoReject);
        assert!(config.title.is_none());
        assert!(config.approval_timeout_ms.is_none());
    }

    #[test]
    fn config_完整反序列化() {
        let json = r#"{
            "title": "液压确认",
            "description": "请确认液压操作",
            "form_schema": [
                {"type": "boolean", "name": "confirmed", "label": "确认", "required": true}
            ],
            "approval_timeout_ms": 30000,
            "default_action": "autoApprove"
        }"#;
        let config: HumanLoopNodeConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.title.as_deref(), Some("液压确认"));
        assert_eq!(config.form_schema.len(), 1);
        assert_eq!(config.approval_timeout_ms, Some(30000));
        assert_eq!(config.default_action, DefaultAction::AutoApprove);
    }
}
```

- [ ] **Step 3: 创建 `registry.rs` — 审批注册表**

```rust
//! per-deployment 审批注册表。

use std::sync::Arc;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use uuid::Uuid;

use super::form::FormSchemaField;
use nazh_core::EngineError;

/// 审批 ID = UUID。
pub type ApprovalId = Uuid;

/// 人工响应动作。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ResponseAction {
    Approved,
    Rejected,
}

/// 人工响应。
#[derive(Debug)]
pub struct HumanLoopResponse {
    pub action: ResponseAction,
    pub form_data: serde_json::Value,
    pub comment: Option<String>,
    pub responded_by: Option<String>,
}

/// 审批槽——存储在 DashMap 中，IPC 命令通过 ID 查找。
pub struct ApprovalSlot {
    pub workflow_id: String,
    pub node_id: String,
    pub node_label: String,
    pub form_schema: Vec<FormSchemaField>,
    pub pending_since: DateTime<Utc>,
    pub approval_timeout_ms: Option<u64>,
    pub default_action: super::config::DefaultAction,
    /// oneshot sender——调用 send() 唤醒阻塞的 transform()。
    pub responder: oneshot::Sender<HumanLoopResponse>,
}

/// Pending 审批摘要（IPC 列表用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// per-deployment 审批注册表。
pub struct ApprovalRegistry {
    slots: DashMap<ApprovalId, ApprovalSlot>,
}

impl ApprovalRegistry {
    pub fn new() -> Self {
        Self {
            slots: DashMap::new(),
        }
    }

    /// 注册 pending 审批，返回 approval_id + Receiver。
    pub fn create_slot(
        &self,
        mut slot: ApprovalSlot,
    ) -> (ApprovalId, oneshot::Receiver<HumanLoopResponse>) {
        let approval_id = Uuid::new_v4();
        // 替换 slot 的 dummy sender 为真正的 pair
        let (tx, rx) = oneshot::channel();
        slot.responder = tx;
        self.slots.insert(approval_id, slot);
        (approval_id, rx)
    }

    /// 人工响应——IPC 命令调用。
    pub fn respond(
        &self,
        approval_id: ApprovalId,
        response: HumanLoopResponse,
    ) -> Result<(), EngineError> {
        let (_, slot) = self
            .slots
            .remove(&approval_id)
            .ok_or_else(|| EngineError::invalid_graph(format!("审批 `{approval_id}` 不存在或已响应")))?;
        slot.responder
            .send(response)
            .map_err(|_| EngineError::invalid_graph("审批响应发送失败（receiver 已被 drop）"))?;
        Ok(())
    }

    /// 清理 workflow 的所有 pending 审批（undeploy 时调用）。
    /// 移除 matching slots 后 sender 被 drop，receiver.await 返回 Err。
    pub fn cleanup_workflow(&self, workflow_id: &str) {
        self.slots.retain(|_, slot| slot.workflow_id != workflow_id);
    }

    /// 列出 pending 审批（IPC 命令用）。
    pub fn list_pending(&self, workflow_id: Option<&str>) -> Vec<PendingApprovalSummary> {
        self.slots
            .iter()
            .filter(|entry| {
                workflow_id.is_none_or(|wid| entry.value().workflow_id == wid)
            })
            .map(|entry| {
                let id = entry.key();
                let slot = entry.value();
                PendingApprovalSummary {
                    approval_id: id.to_string(),
                    workflow_id: slot.workflow_id.clone(),
                    node_id: slot.node_id.clone(),
                    node_label: slot.node_label.clone(),
                    pending_since: slot.pending_since.to_rfc3339(),
                    timeout_ms: slot.approval_timeout_ms,
                    form_schema: serde_json::to_value(&slot.form_schema)
                        .unwrap_or(serde_json::Value::Null),
                }
            })
            .collect()
    }
}

impl Default for ApprovalRegistry {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 4: 创建 `mod.rs` — 导出**

```rust
//! Human-in-the-Loop 审批节点模块。

pub mod config;
pub mod form;
pub mod node;
pub mod registry;

pub use config::HumanLoopNodeConfig;
pub use registry::ApprovalRegistry;
```

- [ ] **Step 5: 运行测试**

```bash
cargo test -p nodes-io
```

- [ ] **Step 6: Commit**

```bash
git add crates/nodes-io/src/human_loop/
git commit -m "feat: HITL 审批注册表 + 表单 Schema + 节点配置类型"
```

---

### Task 3: Ring 1 — HumanLoopNode 实现 + IoPlugin 注册

**Files:**
- Create: `crates/nodes-io/src/human_loop/node.rs`
- Modify: `crates/nodes-io/src/lib.rs` — mod + 注册
- Modify: `crates/nodes-io/Cargo.toml` — 添加 chrono + dashmap（检查是否已有）

- [ ] **Step 1: 确认 `nodes-io` Cargo.toml 已有 chrono + dashmap 依赖**

`chrono` 已有。检查 `dashmap`：

```bash
grep dashmap crates/nodes-io/Cargo.toml
```

若没有，在 `[dependencies]` 添加 `dashmap.workspace = true`。

- [ ] **Step 2: 创建 `node.rs` — HumanLoopNode 实现**

```rust
//! HITL 审批节点：暂停工作流等待人工响应。

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Map, Value, json};
use uuid::Uuid;

use nazh_core::{
    EngineError, LifecycleGuard, NodeDispatch, NodeExecution, NodeLifecycleContext, NodeOutput,
    NodeTrait, PinDefinition, PinDirection, PinKind, PinType, ExecutionEvent, event::emit_event,
};

use super::config::{DefaultAction, HumanLoopNodeConfig};
use super::form::FormSchemaField;
use super::registry::{ApprovalRegistry, ApprovalSlot, HumanLoopResponse, ResponseAction};

/// HITL 审批节点。
pub struct HumanLoopNode {
    id: String,
    config: HumanLoopNodeConfig,
    registry: Arc<ApprovalRegistry>,
}

impl HumanLoopNode {
    pub fn new(id: impl Into<String>, config: HumanLoopNodeConfig, registry: Arc<ApprovalRegistry>) -> Self {
        Self {
            id: id.into(),
            config,
            registry,
        }
    }

    /// 从 form_schema 中提取默认值构建 form_data。
    fn build_form_defaults(&self) -> Value {
        let mut map = serde_json::Map::new();
        for field in &self.config.form_schema {
            match field {
                FormSchemaField::Boolean { name, default, .. } => {
                    if let Some(val) = default {
                        map.insert(name.clone(), json!(val));
                    }
                }
                FormSchemaField::Number { name, default, .. } => {
                    if let Some(val) = default {
                        map.insert(name.clone(), json!(val));
                    }
                }
                FormSchemaField::String { name, default, .. } => {
                    if let Some(val) = default {
                        map.insert(name.clone(), json!(val));
                    }
                }
                FormSchemaField::Select { name, default, .. } => {
                    if let Some(val) = default {
                        map.insert(name.clone(), json!(val));
                    }
                }
            }
        }
        Value::Object(map)
    }
}

#[async_trait]
impl NodeTrait for HumanLoopNode {
    nazh_core::impl_node_meta!("humanLoop");

    fn input_pins(&self) -> Vec<PinDefinition> {
        vec![PinDefinition {
            id: "in".to_owned(),
            label: "in".to_owned(),
            pin_type: PinType::Json,
            direction: PinDirection::Input,
            required: true,
            kind: PinKind::Exec,
            description: Some("触发审批的输入数据".to_owned()),
            ..Default::default()
        }]
    }

    fn output_pins(&self) -> Vec<PinDefinition> {
        let mut pins = vec![PinDefinition {
            id: "out".to_owned(),
            label: "out".to_owned(),
            pin_type: PinType::Json,
            direction: PinDirection::Output,
            required: false,
            kind: PinKind::Exec,
            description: Some("审批通过后的输出（含表单数据）".to_owned()),
            ..Default::default()
        }];
        if matches!(&self.config.default_action, DefaultAction::FallbackNode(_)) {
            pins.push(PinDefinition {
                id: "fallback".to_owned(),
                label: "fallback".to_owned(),
                pin_type: PinType::Json,
                direction: PinDirection::Output,
                required: false,
                kind: PinKind::Exec,
                description: Some("超时 fallback 路径".to_owned()),
                ..Default::default()
            });
        }
        pins
    }

    async fn on_deploy(&self, _ctx: NodeLifecycleContext) -> Result<LifecycleGuard, EngineError> {
        Ok(LifecycleGuard::noop())
    }

    async fn transform(&self, trace_id: Uuid, payload: Value) -> Result<NodeExecution, EngineError> {
        // on_deploy 已存储 workflow_id 到节点，通过 registry 的 cleanup_workflow 关联
        let form_defaults = self.build_form_defaults();

        // 创建 dummy sender，由 create_slot 替换为真正的 pair
        let (dummy_tx, _) = tokio::sync::oneshot::channel::<HumanLoopResponse>();
        let slot = ApprovalSlot {
            workflow_id: String::new(), // 将由调用方注入的 registry cleanup 机制关联
            node_id: self.id.clone(),
            node_label: self.config.title.clone().unwrap_or_else(|| self.id.clone()),
            form_schema: self.config.form_schema.clone(),
            pending_since: chrono::Utc::now(),
            approval_timeout_ms: self.config.approval_timeout_ms,
            default_action: self.config.default_action.clone(),
            responder: dummy_tx,
        };

        let (approval_id, rx) = self.registry.create_slot(slot);
        let form_schema_value = serde_json::to_value(&self.config.form_schema)
            .unwrap_or(Value::Null);

        // 注意：ExecutionEvent 通过 Runner 的事件通道发出。
        // 此处节点无法直接发事件——事件由 Runner 在 transform 前后发射。
        // 审批状态事件需要通过另一种机制：NodeOutput metadata 携带审批信息，
        // Runner 识别 HUMAN_LOOP 节点后发出特殊事件。
        //
        // 简化方案：审批 pending/resolved 事件由 registry 的 respond/cleanup 路径
        // 通过一个独立事件通道发出，不走 ExecutionEvent。

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
                let response = rx.await.map_err(|_| EngineError::NodeExecutionFailed {
                    node_id: self.id.clone(),
                    reason: "审批槽被关闭（工作流已卸载）".into(),
                })?;
                self.handle_response(response, payload, approval_id)
            }
        };

        result
    }
}

impl HumanLoopNode {
    fn handle_response(
        &self,
        response: HumanLoopResponse,
        mut payload: Value,
        approval_id: Uuid,
    ) -> Result<NodeExecution, EngineError> {
        match response.action {
            ResponseAction::Approved => {
                // 合并表单数据到 payload
                if let Value::Object(ref mut map) = payload {
                    if let Value::Object(form_data) = response.form_data {
                        for (key, value) in form_data {
                            map.insert(key, value);
                        }
                    }
                }
                let mut metadata = Map::new();
                metadata.insert(
                    "humanLoop".to_owned(),
                    json!({
                        "approval_id": approval_id.to_string(),
                        "action": "approved",
                        "responded_by": response.responded_by,
                    }),
                );
                Ok(NodeExecution::from_outputs(vec![NodeOutput {
                    payload,
                    metadata,
                    dispatch: NodeDispatch::Broadcast,
                }]))
            }
            ResponseAction::Rejected => Err(EngineError::NodeExecutionFailed {
                node_id: self.id.clone(),
                reason: format!(
                    "审批被拒绝 (approval_id: {approval_id}){}",
                    response
                        .comment
                        .as_deref()
                        .map_or(String::new(), |c| format!("：{c}"))
                ),
            }),
        }
    }

    fn handle_timeout(
        &self,
        mut payload: Value,
        approval_id: Uuid,
        form_defaults: &Value,
    ) -> Result<NodeExecution, EngineError> {
        match &self.config.default_action {
            DefaultAction::AutoApprove => {
                if let Value::Object(ref mut map) = payload {
                    if let Value::Object(form_data) = form_defaults {
                        for (key, value) in form_data {
                            map.insert(key.clone(), value.clone());
                        }
                    }
                }
                let mut metadata = Map::new();
                metadata.insert(
                    "humanLoop".to_owned(),
                    json!({
                        "approval_id": approval_id.to_string(),
                        "action": "timeout_auto_approve",
                    }),
                );
                Ok(NodeExecution::from_outputs(vec![NodeOutput {
                    payload,
                    metadata,
                    dispatch: NodeDispatch::Broadcast,
                }]))
            }
            DefaultAction::AutoReject => Err(EngineError::NodeExecutionFailed {
                node_id: self.id.clone(),
                reason: format!("审批超时自动拒绝 (approval_id: {approval_id})"),
            }),
            DefaultAction::FallbackNode(_node_name) => {
                // FallbackNode 路由：通过 Route dispatch 实现
                let mut metadata = Map::new();
                metadata.insert(
                    "humanLoop".to_owned(),
                    json!({
                        "approval_id": approval_id.to_string(),
                        "action": "timeout_fallback",
                    }),
                );
                Ok(NodeExecution::from_outputs(vec![NodeOutput {
                    payload,
                    metadata,
                    dispatch: NodeDispatch::Route(vec!["fallback".to_owned()]),
                }]))
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_config() -> HumanLoopNodeConfig {
        serde_json::from_value(json!({
            "title": "测试审批",
            "form_schema": [
                {"type": "boolean", "name": "confirmed", "label": "确认", "required": true, "default": false}
            ],
            "approval_timeout_ms": 5000,
            "default_action": "autoReject"
        }))
        .unwrap()
    }

    #[test]
    fn form_defaults_提取布尔默认值() {
        let config = test_config();
        let registry = Arc::new(ApprovalRegistry::new());
        let node = HumanLoopNode::new("test-node", config, registry);
        let defaults = node.build_form_defaults();
        assert_eq!(defaults["confirmed"], json!(false));
    }

    #[tokio::test]
    async fn handle_response_approved_合并表单数据() {
        let config = test_config();
        let registry = Arc::new(ApprovalRegistry::new());
        let node = HumanLoopNode::new("test-node", config, Arc::clone(&registry));
        let response = HumanLoopResponse {
            action: ResponseAction::Approved,
            form_data: json!({"confirmed": true}),
            comment: None,
            responded_by: Some("admin".to_owned()),
        };
        let result = node
            .handle_response(response, json!({"sensor_value": 42.5}), Uuid::new_v4())
            .unwrap();
        let output = result.first().unwrap();
        assert_eq!(output.payload["sensor_value"], json!(42.5));
        assert_eq!(output.payload["confirmed"], json!(true));
    }

    #[tokio::test]
    async fn handle_response_rejected_返回错误() {
        let config = test_config();
        let registry = Arc::new(ApprovalRegistry::new());
        let node = HumanLoopNode::new("test-node", config, registry);
        let response = HumanLoopResponse {
            action: ResponseAction::Rejected,
            form_data: json!(null),
            comment: Some("不安全".to_owned()),
            responded_by: None,
        };
        let result = node.handle_response(response, json!({}), Uuid::new_v4());
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("审批被拒绝"));
        assert!(err_msg.contains("不安全"));
    }

    #[tokio::test]
    async fn handle_timeout_auto_approve_注入默认值() {
        let config: HumanLoopNodeConfig = serde_json::from_value(json!({
            "form_schema": [
                {"type": "number", "name": "pressure", "label": "压力", "default": 100.0}
            ],
            "default_action": "autoApprove"
        }))
        .unwrap();
        let registry = Arc::new(ApprovalRegistry::new());
        let node = HumanLoopNode::new("test-node", config, registry);
        let result = node
            .handle_timeout(json!({"raw": 1}), Uuid::new_v4(), &json!({"pressure": 100.0}))
            .unwrap();
        let output = result.first().unwrap();
        assert_eq!(output.payload["pressure"], json!(100.0));
        assert_eq!(output.payload["raw"], json!(1));
    }
}
```

- [ ] **Step 3: 注册到 IoPlugin**

在 `crates/nodes-io/src/lib.rs` 中：

1. 添加 `pub mod human_loop;` 模块声明（永远启用，无协议依赖）
2. 在 `IoPlugin::register` 中添加注册：

```rust
        registry.register_with_capabilities(
            "humanLoop",
            NodeCapabilities::HUMAN_LOOP,
            |def, res| {
                let config: human_loop::HumanLoopNodeConfig = def.parse_config()?;
                let registry = res
                    .get::<Arc<human_loop::ApprovalRegistry>>()
                    .ok_or_else(|| {
                        EngineError::invalid_graph("部署资源中缺少 ApprovalRegistry")
                    })?;
                Ok(Arc::new(human_loop::HumanLoopNode::new(
                    def.id().to_owned(),
                    config,
                    registry,
                )))
            },
        );
```

- [ ] **Step 4: 运行测试**

```bash
cargo test -p nodes-io
```

- [ ] **Step 5: Commit**

```bash
git add crates/nodes-io/src/human_loop/node.rs crates/nodes-io/src/lib.rs crates/nodes-io/Cargo.toml
git commit -m "feat: HumanLoopNode 实现 + IoPlugin 注册"
```

---

### Task 4: IPC 层 — Tauri 命令 + 事件转发 + ApprovalRegistry 注入

**Files:**
- Modify: `crates/tauri-bindings/src/lib.rs` — IPC 类型 + ts-rs 导出
- Create: `src-tauri/src/commands/human_loop.rs` — IPC 命令
- Modify: `src-tauri/src/commands/mod.rs` — 注册新模块
- Modify: `src-tauri/src/lib.rs` — generate_handler! 注册
- Modify: `src-tauri/src/events.rs` — 事件转发扩展
- Modify: `src-tauri/src/state.rs` — DesktopState 持有 ApprovalRegistry
- Modify: `crates/graph/src/deploy.rs` — ApprovalRegistry 注入 SharedResources

- [ ] **Step 1: 在 `tauri-bindings/src/lib.rs` 添加 IPC 类型**

在 `ReactiveUpdatePayload` 之后添加：

```rust
/// `workflow://human-loop-pending` 事件载荷。
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

/// `workflow://human-loop-resolved` 事件载荷。
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

/// `respond_human_loop` 命令请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct RespondHumanLoopRequest {
    pub approval_id: String,
    pub action: String,
    pub form_data: serde_json::Value,
    pub comment: Option<String>,
    pub responded_by: Option<String>,
}
```

在 `export_all()` 函数中添加：

```rust
    HumanLoopPendingPayload::export(&cfg)?;
    HumanLoopResolvedPayload::export(&cfg)?;
    RespondHumanLoopRequest::export(&cfg)?;
```

添加 payload 转换函数（与 `variable_changed_payload` 同模式）：

```rust
/// 将 ExecutionEvent::HumanLoopPending 消耗式转换为 IPC payload。
pub fn human_loop_pending_payload(
    event: nazh_core::ExecutionEvent,
    workflow_id: &str,
) -> Option<HumanLoopPendingPayload> {
    match event {
        nazh_core::ExecutionEvent::HumanLoopPending {
            stage,
            approval_id,
            form_schema,
            timeout_ms,
            ..
        } => Some(HumanLoopPendingPayload {
            workflow_id: workflow_id.to_owned(),
            node_id: stage,
            node_label: String::new(),
            approval_id: approval_id.to_string(),
            form_schema,
            pending_since: chrono::Utc::now().to_rfc3339(),
            timeout_ms,
        }),
        _ => None,
    }
}

/// 将 ExecutionEvent::HumanLoopResolved 消耗式转换为 IPC payload。
pub fn human_loop_resolved_payload(
    event: nazh_core::ExecutionEvent,
    workflow_id: &str,
) -> Option<HumanLoopResolvedPayload> {
    match event {
        nazh_core::ExecutionEvent::HumanLoopResolved {
            stage,
            approval_id,
            action,
            responded_by,
            ..
        } => Some(HumanLoopResolvedPayload {
            workflow_id: workflow_id.to_owned(),
            node_id: stage,
            approval_id: approval_id.to_string(),
            action,
            responded_by,
            comment: None,
        }),
        _ => None,
    }
}
```

- [ ] **Step 2: 在 `state.rs` 的 `DesktopState` 添加 `ApprovalRegistry`**

```rust
use std::sync::Arc;
use nodes_io::ApprovalRegistry;

// 在 DesktopState 结构体中添加：
pub(crate) approval_registry: Arc<ApprovalRegistry>,
```

在 `Default` impl 中初始化：

```rust
            approval_registry: Arc::new(ApprovalRegistry::new()),
```

- [ ] **Step 3: 注入 `ApprovalRegistry` 到 `SharedResources`**

在 `src-tauri/src/commands/workflow.rs` 的 `deploy_workflow` 命令中，调用 `deploy_workflow_with_ai` 前，将 `ApprovalRegistry` 注入到 resource bag。具体位置：找到构造 `RuntimeResources` 或调用 engine deploy 的地方，添加：

```rust
resource_bag.insert(Arc::clone(&state.approval_registry));
```

需要在 deploy 时把 `state.approval_registry` 传到 engine 的 resource bag 中。

- [ ] **Step 4: 创建 `src-tauri/src/commands/human_loop.rs`**

```rust
//! HITL 审批 IPC 命令。

use std::sync::Arc;

use tauri::State;

use crate::state::DesktopState;

/// 人工响应审批。
#[tauri::command]
pub(crate) async fn respond_human_loop(
    state: State<'_, DesktopState>,
    approval_id: String,
    action: String,
    form_data: serde_json::Value,
    comment: Option<String>,
    responded_by: Option<String>,
) -> Result<(), String> {
    let response_action = match action.as_str() {
        "approved" => nodes_io::ResponseAction::Approved,
        "rejected" => nodes_io::ResponseAction::Rejected,
        other => return Err(format!("未知动作: {other}")),
    };
    let response = nodes_io::registry::HumanLoopResponse {
        action: response_action,
        form_data,
        comment,
        responded_by,
    };
    let approval_uuid = uuid::Uuid::parse_str(&approval_id)
        .map_err(|e| format!("无效的 approval_id: {e}"))?;
    state
        .approval_registry
        .respond(approval_uuid, response)
        .map_err(|e| e.to_string())
}

/// 列出 pending 审批。
#[tauri::command]
pub(crate) async fn list_pending_approvals(
    state: State<'_, DesktopState>,
    workflow_id: Option<String>,
) -> Result<Vec<serde_json::Value>, String> {
    let summaries = state
        .approval_registry
        .list_pending(workflow_id.as_deref());
    Ok(summaries
        .into_iter()
        .map(|s| serde_json::to_value(s).unwrap_or(serde_json::Value::Null))
        .collect())
}
```

- [ ] **Step 5: 在 `commands/mod.rs` 注册模块**

添加 `pub mod human_loop;`

- [ ] **Step 6: 在 `lib.rs` 注册命令**

在 `generate_handler!` 列表中添加：

```rust
commands::human_loop::respond_human_loop,
commands::human_loop::list_pending_approvals,
```

- [ ] **Step 7: 在 `events.rs` 添加事件转发**

在 `spawn_execution_event_forwarder` 的 drain loop 中，`VariableDeleted` 分支之后添加：

```rust
            // HITL 审批事件走独立通道。
            if matches!(event, ExecutionEvent::HumanLoopPending { .. }) {
                if let Some(payload) = tauri_bindings::human_loop_pending_payload(event, &workflow_id)
                    && let Err(error) = app.emit("workflow://human-loop-pending", payload)
                {
                    tracing::warn!(?error, "workflow://human-loop-pending 事件转发失败");
                }
                continue;
            }

            if matches!(event, ExecutionEvent::HumanLoopResolved { .. }) {
                if let Some(payload) = tauri_bindings::human_loop_resolved_payload(event, &workflow_id)
                    && let Err(error) = app.emit("workflow://human-loop-resolved", payload)
                {
                    tracing::warn!(?error, "workflow://human-loop-resolved 事件转发失败");
                }
                continue;
            }
```

- [ ] **Step 8: undeploy 时清理审批**

在 `commands/workflow.rs` 的 `undeploy_workflow` 命令中，卸载工作流后添加：

```rust
state.approval_registry.cleanup_workflow(&workflow_id);
```

- [ ] **Step 9: 运行测试**

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- [ ] **Step 10: Regenerate TypeScript types**

```bash
cargo test -p tauri-bindings --features ts-export export_bindings
```

- [ ] **Step 11: Commit**

```bash
git add crates/tauri-bindings/src/lib.rs src-tauri/src/commands/human_loop.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src-tauri/src/events.rs src-tauri/src/state.rs web/src/generated/
git commit -m "feat: HITL IPC 命令 + 事件转发 + ApprovalRegistry 注入"
```

---

### Task 5: 契约测试 + 注册表更新 + 集成测试

**Files:**
- Modify: `src/registry.rs` — 契约测试更新
- Modify: `crates/core/src/node.rs` — 位分配测试
- Modify: `web/src/lib/node-capabilities.ts` — 新 flag
- Add to: `tests/workflow.rs` — HITL 集成测试

- [ ] **Step 1: 更新 `src/registry.rs` 契约测试**

在 `io_plugin_注册全部_io_节点` 测试的 `expected` 数组中添加 `"humanLoop"`。

更新 `两个插件合并后覆盖全部节点类型` 测试的总数（当前 19 → 20）：

```rust
        assert_eq!(
            registry.registered_types().len(),
            20,
            "应注册 20 种节点类型"
        );
```

在 `标准注册表节点能力标签与_adr_0011_契约一致` 测试中添加：

```rust
        expect("humanLoop", NodeCapabilities::HUMAN_LOOP);
```

- [ ] **Step 2: 更新前端 `node-capabilities.ts`**

在 `NODE_CAPABILITY_FLAGS` 中添加：

```typescript
  HUMAN_LOOP: 1 << 8,
```

在 `NODE_CAPABILITY_LABELS` 中添加：

```typescript
  HUMAN_LOOP: '审批',
```

- [ ] **Step 3: 添加 HITL 集成测试到 `tests/workflow.rs`**

添加三个测试用例（需要模拟人工响应）：

```rust
#[tokio::test]
async fn human_loop_审批通过后表单数据注入_payload() {
    // 构建简单 DAG：timer → humanLoop
    // 在 timer 触发前，spawn 一个任务模拟 IPC respond
    // 验证最终 payload 包含表单数据
}

#[tokio::test]
async fn human_loop_超时自动拒绝() {
    // 配置 approval_timeout_ms = 100, default_action = AutoReject
    // 不发送响应，等待超时
    // 验证 ExecutionEvent::Failed
}

#[tokio::test]
async fn human_loop_显式拒绝() {
    // 发送 Rejected 响应
    // 验证 ExecutionEvent::Failed
}
```

- [ ] **Step 4: 运行全部测试**

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- [ ] **Step 5: Commit**

```bash
git add src/registry.rs web/src/lib/node-capabilities.ts tests/workflow.rs
git commit -m "test: HITL 契约测试 + 集成测试 + 前端 capability flag"
```

---

### Task 6: 前端 — 节点定义 + 设置面板 + 节点库注册

**Files:**
- Create: `web/src/components/flowgram/nodes/humanLoop/index.ts`
- Modify: `web/src/components/flowgram/nodes/shared.ts` — NazhNodeKind + normalizeNodeKind + normalizeNodeConfig + getFallbackNodeLabel
- Modify: `web/src/components/flowgram/nodes/catalog.ts` — catalog 注册
- Modify: `web/src/components/flowgram/flowgram-node-library.ts` — 节点库注册

- [ ] **Step 1: 在 `shared.ts` 扩展 `NazhNodeKind`**

在 `NazhNodeKind` 联合类型中添加 `| 'humanLoop'`。

在 `normalizeNodeKind` 函数中添加 case：

```typescript
    case 'humanLoop':
      return 'humanLoop';
```

在 `getFallbackNodeLabel` 中添加：

```typescript
    case 'humanLoop':
      return '审批节点';
```

在 `normalizeNodeConfig` 中添加 HITL 配置规范化（在 `c2f` / `minutesSince` 分支后）：

```typescript
  if (nodeType === 'humanLoop') {
    const formSchema = Array.isArray(rawConfig.form_schema)
      ? rawConfig.form_schema
      : [];
    return {
      ...rawConfig,
      title: typeof rawConfig.title === 'string' ? rawConfig.title : '',
      description: typeof rawConfig.description === 'string' ? rawConfig.description : '',
      form_schema: formSchema,
      approval_timeout_ms:
        typeof rawConfig.approval_timeout_ms === 'number' && Number.isFinite(rawConfig.approval_timeout_ms)
          ? Math.max(0, Math.round(rawConfig.approval_timeout_ms))
          : null,
      default_action:
        rawConfig.default_action === 'autoApprove' ||
        rawConfig.default_action === 'autoReject' ||
        (typeof rawConfig.default_action === 'object' && rawConfig.default_action !== null && 'fallbackNode' in rawConfig.default_action)
          ? rawConfig.default_action
          : 'autoReject',
    };
  }
```

- [ ] **Step 2: 创建 `nodes/humanLoop/index.ts`**

```typescript
import {
  type NodeDefinition,
  type NodeSeed,
  type NodeValidationContext,
  type NodeValidation,
  normalizeNodeConfig,
} from '../shared';

export const definition: NodeDefinition = {
  kind: 'humanLoop',
  catalog: { category: '流程控制', description: '暂停工作流等待人工审批响应' },
  fallbackLabel: '审批节点',

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'human_loop',
      kind: 'humanLoop',
      label: '',
      timeoutMs: null,
      config: {
        title: '',
        description: '',
        form_schema: [],
        approval_timeout_ms: null,
        default_action: 'autoReject',
      },
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    return normalizeNodeConfig('humanLoop', config);
  },

  getNodeSize() {
    return { width: 214, height: 132 };
  },

  buildRegistryMeta() {
    return { defaultExpanded: true, size: this.getNodeSize() };
  },

  validate(_ctx: NodeValidationContext): NodeValidation[] {
    return [];
  },
};
```

- [ ] **Step 3: 在 `catalog.ts` 添加条目**

在 `NODE_CATEGORY_MAP` 中添加：

```typescript
  humanLoop: { category: '流程控制', description: '暂停工作流等待人工审批响应' },
```

- [ ] **Step 4: 在 `flowgram-node-library.ts` 注册**

1. 添加 import：

```typescript
import { definition as humanLoopDef } from './nodes/humanLoop';
```

2. 在 `ALL_DEFS` 数组中添加 `humanLoopDef`。

3. 在 `getFlowgramPaletteSections` 的 `逻辑节点` section 中添加：

```typescript
        { key: 'blank-human-loop', title: '审批节点', description: humanLoopDef.catalog.description, badge: '审批', seed: humanLoopDef.buildDefaultSeed() },
```

- [ ] **Step 5: 运行前端测试**

```bash
npm --prefix web run test
```

- [ ] **Step 6: Commit**

```bash
git add web/src/components/flowgram/nodes/humanLoop/ web/src/components/flowgram/nodes/shared.ts web/src/components/flowgram/nodes/catalog.ts web/src/components/flowgram/flowgram-node-library.ts
git commit -m "feat: HITL 前端节点定义 + 节点库注册"
```

---

### Task 7: 前端 — IPC API + 审批队列 UI

**Files:**
- Modify: `web/src/lib/tauri.ts` — IPC 包装函数
- Create: `web/src/components/app/ApprovalQueue.tsx` — 审批队列
- Create: `web/src/components/app/ApprovalForm.tsx` — 动态表单
- Modify: `web/src/components/app/RuntimeDock.tsx` — 新增审批 tab

- [ ] **Step 1: 在 `tauri.ts` 添加 IPC 包装函数**

在文件末尾（copilot 函数之前）添加：

```typescript
// ── HITL 审批 IPC ────────────────────────────────

export interface HumanLoopPendingPayload {
  workflowId: string;
  nodeId: string;
  nodeLabel: string;
  approvalId: string;
  formSchema: unknown;
  pendingSince: string;
  timeoutMs: number | null;
}

export interface HumanLoopResolvedPayload {
  workflowId: string;
  nodeId: string;
  approvalId: string;
  action: string;
  respondedBy: string | null;
  comment: string | null;
}

export async function respondHumanLoop(params: {
  approvalId: string;
  action: 'approved' | 'rejected';
  formData: Record<string, unknown>;
  comment?: string | null;
  respondedBy?: string | null;
}): Promise<void> {
  return invoke<void>('respond_human_loop', {
    approvalId: params.approvalId,
    action: params.action,
    formData: params.formData,
    comment: params.comment ?? null,
    respondedBy: params.respondedBy ?? null,
  });
}

export async function listPendingApprovals(
  workflowId?: string | null,
): Promise<unknown[]> {
  return invoke<unknown[]>('list_pending_approvals', {
    workflowId: workflowId?.trim() ? workflowId.trim() : null,
  });
}

export async function onHumanLoopPending(
  handler: (payload: HumanLoopPendingPayload) => void,
): Promise<() => void> {
  const unlisten = await listen<HumanLoopPendingPayload>(
    'workflow://human-loop-pending',
    (event) => handler(event.payload),
  );
  return () => { unlisten(); };
}

export async function onHumanLoopResolved(
  handler: (payload: HumanLoopResolvedPayload) => void,
): Promise<() => void> {
  const unlisten = await listen<HumanLoopResolvedPayload>(
    'workflow://human-loop-resolved',
    (event) => handler(event.payload),
  );
  return () => { unlisten(); };
}
```

- [ ] **Step 2: 创建 `ApprovalForm.tsx` — 动态表单渲染器**

根据 `form_schema` 动态渲染 boolean / number / string / select 四种字段。每个字段渲染为 label + input，收集 `formData` state。提交时调用 `respondHumanLoop`。

具体实现：参考 spec 的 `FormSchemaField` 类型，switch on `field.type` 渲染对应输入控件。

- [ ] **Step 3: 创建 `ApprovalQueue.tsx` — 审批队列组件**

- 监听 `onHumanLoopPending` 事件，维护 pending 列表 state
- 监听 `onHumanLoopResolved` 事件，从列表中移除已响应的审批
- 渲染列表：每项显示 nodeLabel + pendingSince + 剩余超时
- 点击展开 `ApprovalForm`

- [ ] **Step 4: 在 `RuntimeDock.tsx` 新增审批 tab**

1. 扩展 `RuntimeDockPanel` 类型：

```typescript
type RuntimeDockPanel = 'events' | 'results' | 'connections' | 'variables' | 'approvals';
```

2. 在 `runtimeDockTabs` 中添加：

```typescript
  { id: 'approvals', label: '审批', title: '待处理审批' },
```

3. 在 dock 内容区域添加 `approvals` panel 分支，渲染 `ApprovalQueue` 组件

- [ ] **Step 5: 运行前端测试 + 构建**

```bash
npm --prefix web run test
npm --prefix web run build
```

- [ ] **Step 6: Commit**

```bash
git add web/src/lib/tauri.ts web/src/components/app/ApprovalQueue.tsx web/src/components/app/ApprovalForm.tsx web/src/components/app/RuntimeDock.tsx
git commit -m "feat: HITL 前端 IPC API + 审批队列 UI + RuntimeDock tab"
```

---

### Task 8: 文档更新 + AGENTS.md 同步

**Files:**
- Modify: `CLAUDE.md`（即 `AGENTS.md`） — IPC surface + NodeCapabilities + workspace layout
- Modify: `crates/nodes-io/AGENTS.md` — 新节点 inventory
- Modify: `crates/core/AGENTS.md` — 新 capability flag

- [ ] **Step 1: 更新根 `AGENTS.md`**

1. 在 "Tauri IPC Surface" 段的事件通道列表添加：`workflow://human-loop-pending` 和 `workflow://human-loop-resolved`

2. 在 IPC 命令列表添加：`respond_human_loop`、`list_pending_approvals`

3. 在 "Current batch of ADRs" 列表中记录 HITL spec 实施状态

- [ ] **Step 2: 更新 `crates/nodes-io/AGENTS.md`**

在节点 inventory 表中添加 `humanLoop` 行。

- [ ] **Step 3: 运行 `cargo doc --no-deps` 确认无警告**

```bash
cargo doc --no-deps 2>&1 | grep warning
```

- [ ] **Step 4: Commit**

```bash
git add AGENTS.md CLAUDE.md crates/nodes-io/AGENTS.md crates/core/AGENTS.md
git commit -m "docs: HITL 实施文档更新"
```

---

## Spec Coverage Check

| Spec 需求 | Task | 实施状态 |
|-----------|------|----------|
| 阻塞等待（oneshot channel） | Task 3 | ✅ 已实施 |
| 结构化表单（4 种类型） | Task 2 | ✅ 已实施 |
| 独立超时 + 默认动作 | Task 2, 3 | ✅ 已实施 |
| Ring 纯净性（Registry 不进 core） | Task 2, 3 | ✅ 已实施 |
| 优雅关闭（undeploy 清理） | Task 4 | ✅ 已实施 |
| 可追踪（ExecutionEvent） | Task 1 | ⚠️ 偏离：无专用事件变体，改用 `Completed` + metadata `"human_loop"`（符合 ADR-0008） |
| NodeLifecycleContext.workflow_id | Task 1 | ⚠️ 偏离：不改 Ring 0 struct，改用 `RuntimeResources` 注入 `WorkflowId` 类型 |
| HUMAN_LOOP capability | Task 1 | ⚠️ 偏离：无专用 bit，使用 `BRANCHING`（节点有 approve/reject 分支，语义合理） |
| IPC 事件通道（human-loop-pending/resolved） | Task 4 | ⚠️ 偏离：无独立事件通道，前端用轮询 `listPendingApprovals` 替代 |
| IPC 命令 | Task 4 | ✅ 已实施（raw 参数，无 typed IPC struct） |
| 前端节点注册 | Task 6 | ✅ 已实施 |
| 审批面板 UI | Task 7 | ⚠️ 部分实施：组件存在但未接入 RuntimeDock tab 系统 |
| DSL 编译器对接 | — | **Deferred**（Phase 5，依赖 ADR-0021 实施） |
| 多级审批 | — | **非目标**（spec 明确排除） |
| 审批历史持久化 | — | **非目标**（Phase 2） |

### Deferred Items（可在后续迭代补回）

- **`ExecutionEvent::HumanLoopPending`/`HumanLoopResolved` 专用变体 + 独立事件通道**：当前用 metadata + 轮询实现，功能等价但实时性较差。补回时需同步修改 `crates/core/src/event.rs` + `src-tauri/src/events.rs` + `web/src/lib/tauri.ts` 事件监听。
- **`NodeCapabilities::HUMAN_LOOP` 专用 bit**：当前用 `BRANCHING`，前端无法区分"分支节点"与"审批节点"。补回时需 `crates/core/src/node.rs` + `web/src/lib/node-capabilities.ts` 同步。
- **RuntimeDock `approvals` tab**：`ApprovalQueue` + `ApprovalForm` 组件已实现，需在 `RuntimeDock.tsx` 的 `RuntimeDockPanel` 类型 + tabs 数组中接入。
- **`tests/workflow.rs` HITL 集成测试**：单元测试已覆盖 config/registry/node 逻辑，但缺少 DAG 端到端集成测试（审批通过 / 超时 / 拒绝三条路径）。
- **typed IPC structs（`tauri-bindings`）+ ts-rs 导出**：当前用 raw `serde_json::Value`，无编译期类型安全。补回时需 `crates/tauri-bindings/src/lib.rs` 新增 `HumanLoopPendingPayload` / `HumanLoopResolvedPayload` / `RespondHumanLoopRequest`。
