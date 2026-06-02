# ADR-0029: Copilot 资产操作两阶段确认门控

- **状态**: 已实施
- **日期**: 2026-06-02
- **决策者**: ssXue
- **关联**: RFC-0006（设备建模 Copilot 接管 Phase 3 资产操作工具）

## 背景

RFC-0006 Phase 3 为 copilot 新增了 8 个资产操作工具（`save_device_asset`、`delete_device_asset`、`save_capability_asset`、`add_device_signal`、`remove_device_signal`、`bind_device_connection`、`patch_device_field`）。这些工具封装了已有的 IPC 命令，copilot 调用时**直接执行写入**，立即持久化到磁盘。

当前安全措施仅限于提示层面：

1. 工具 `description` 中注明"此操作会立即持久化到磁盘，调用前应向用户确认"
2. 系统提示要求 AI 保存前输出确认摘要

但这依赖 AI 遵循指令——弱模型可能跳过确认直接执行。工业设备配置写入错误可引发安全事故。

### 需要门控的写入工具

| 工具 | 风险 |
|------|------|
| `save_device_asset` | 覆盖设备配置 |
| `delete_device_asset` | 不可撤销 |
| `save_capability_asset` | 覆盖能力配置 |
| `add_device_signal` | 修改设备信号表 |
| `remove_device_signal` | 删除信号 |
| `bind_device_connection` | 绑定/解绑连接 |
| `patch_device_field` | 修改任意字段 |

只读工具（`validate_device_yaml`、`get_signal_schema_template`、`infer_capabilities_from_signals`、`search_*`、`read_asset_yaml`）不需要门控。

## 决策

> 我们决定采用两阶段确认协议：写入工具首次调用返回 `pending_confirmation` 状态和操作摘要，前端显示确认 UI，用户点击确认后 copilot 带确认令牌二次调用同一工具完成执行。

### 协议设计

#### 阶段 1：请求确认

copilot 调用写入工具时，Rust 侧：

1. 解析参数，校验合法性（YAML 校验、ID 格式等）
2. 生成操作摘要（人类可读的变更描述）
3. 生成一次性确认令牌（UUID，内存中存储，TTL 5 分钟）
4. 返回 `{ "status": "pending_confirmation", "summary": "...", "token": "..." }`

#### 阶段 2：确认执行

copilot 收到 `pending_confirmation` 后：

1. AI 输出操作摘要给用户
2. 前端在 copilot 消息流中渲染确认按钮
3. 用户点击确认 → 前端调用 IPC `copilot_confirm_action(token)`
4. Rust 侧校验令牌有效性和 TTL
5. 执行实际写入操作
6. 返回执行结果，copilot 继续对话

#### 超时与取消

- 确认令牌 5 分钟后自动过期
- 过期后 copilot 需重新发起写入工具调用
- 用户可点击取消按钮跳过确认（令牌作废）

### 数据流

```
AI 输出："将保存设备 sht40_01（2 个信号，Modbus TCP）"
  → 调用 save_device_asset 工具
  → Rust 返回 { status: "pending_confirmation", summary: "保存设备 sht40_01（2 个信号）", token: "abc-123" }
  → 前端渲染确认按钮 [确认] [取消]
  → 用户点击 [确认]
  → 前端调用 copilot_confirm_action("abc-123")
  → Rust 执行 save_device_asset
  → 返回 { status: "confirmed", result: "设备 sht40_01 已保存" }
  → copilot 继续对话
```

### 前端实现

- `CopilotPanel` 中新增确认消息气泡组件（`CopilotConfirmAction`）
- 工具返回 `pending_confirmation` 时，在消息流中插入确认气泡
- 确认/取消按钮通过 IPC 调用 `copilot_confirm_action` / `copilot_cancel_action`
- copilot stream 在 `onToolResult` 回调中检测 `pending_confirmation` 状态，暂停后续工具调用直到确认完成

### Rust 实现

- 新增 `PendingAction` 结构（token + 工具名 + 参数 + 创建时间）
- `CopilotToolCtx` 增加 `pending_actions: Arc<DashMap<String, PendingAction>>`
- 写入工具的 match arm 内：首次调用 → 校验 + 生成摘要 + 返回 pending；`confirmed: true` → 从 pending_actions 取出 → 执行
- 新增 IPC `copilot_confirm_action(token)` 和 `copilot_cancel_action(token)`

## 可选方案

### 方案 A：两阶段确认协议（本提案）

- **优势**：代码层面强制确认，不依赖 AI 遵循指令；确认令牌有 TTL，防止重放；前端可显示结构化操作摘要
- **劣势**：增加 IPC 和前端 UI 复杂度；AI 需要两步完成写入（先请求确认、再获得结果）；多轮工具调用增加延迟

### 方案 B：纯提示层面（当前状态）

- **优势**：零代码变更；工具调用一步完成
- **劣势**：依赖 AI 遵循指令；弱模型可能跳过确认直接执行；无代码层面的安全保障

### 方案 C：前端中间层拦截

- **优势**：Rust 侧不变；前端 `execute` 回调中拦截写入工具，显示确认弹窗
- **劣势**：安全性依赖前端代码（绕过前端可直接调用 IPC）；工具协议需要中断/恢复语义（`execute` 必须返回结果才能继续 stream）；与 ai-sdk 的 tool execute 流程不兼容（`execute` 是同步 await，不能暂停等用户点击）

## 后果

### 正面影响

- 代码层面强制用户确认，不依赖 AI 模型的指令遵循能力
- 操作摘要让用户在确认前清楚了解即将发生的变更
- 确认令牌 TTL 防止过期操作被意外执行
- 为未来的批量操作确认（如 ESI 导入多设备）奠定基础

### 负面影响

- 每次写入操作增加一次 IPC 往返（确认）和一次前端交互（点击按钮）
- copilot 工具调用流程变为异步（stream 暂停等待确认）
- `DashMap` 存储的 pending actions 在应用重启后丢失（未确认的操作不持久化——这是设计意图：重启后不应有悬而未决的写入）

### 风险

- **AI 重复调用**：AI 可能在等待确认时重复调用同一工具，生成多个 pending token。缓解：每个 pending token 独立有效，重复调用返回新 token；前端只渲染最新的确认气泡
- **确认气泡干扰对话流**：多个 pending 确认可能堆积。缓解：每次只允许一个 pending 操作（新操作自动取消旧的）
- **AI 无法处理确认结果**：如果 AI 不理解 `confirmed` 状态的返回值，可能重复操作。缓解：`confirmed` 返回值中包含明确的状态文本和操作结果

## 备注

- 此 ADR 解决 RFC-0006 未解决问题 #3（资产操作的用户确认门控）
- 与 ADR-0028（CapabilityImpl 编码元数据扩展）独立，但共同完善 RFC-0006 的收尾工作
