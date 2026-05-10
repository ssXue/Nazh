# OpenCode LLM 调用架构参考

> **目的**：系统性梳理 [opencode](https://github.com/niccolo-machia/opencode) 项目中 LLM 调用、工具编排、智能体调度和子任务派发的架构设计，作为 Nazh copilot 功能增强的参考蓝本。
>
> **日期**：2026-05-10
> **源码路径**：`/Users/ssxue/code/opencode`

---

## 1. 整体架构分层

OpenCode 的 LLM 系统分四层，自底向上：

```
┌─────────────────────────────────────────────────────────┐
│  Layer 4: Agent Registry（智能体注册表）                   │
│  - 智能体定义、权限、模式、模型偏好                         │
│  - 动态生成自定义智能体                                    │
├─────────────────────────────────────────────────────────┤
│  Layer 3: Session / Processor（会话与事件处理）            │
│  - 消息历史、Parts 系统、流式事件管道                       │
│  - 子任务派发与结果回收                                    │
├─────────────────────────────────────────────────────────┤
│  Layer 2: Tool Runtime（工具运行时）                      │
│  - 递归循环：模型流 → 工具执行 → 后续请求 → 再循环          │
│  - 并发工具调度、停止条件                                  │
├─────────────────────────────────────────────────────────┤
│  Layer 1: LLM Provider（模型提供者）                      │
│  - Stream 抽象、LLMEvent 类型、请求/响应 Schema            │
│  - 多 Provider 路由、模型解析                              │
└─────────────────────────────────────────────────────────┘
```

---

## 2. Layer 1：LLM Provider（模型提供者）

### 2.1 事件流模型

`packages/llm/src/schema.ts` 定义了统一的 LLM 事件类型：

```typescript
type LLMEvent =
  | { type: "text-delta"; text: string; providerMetadata?: ProviderMetadata }
  | { type: "reasoning-delta"; text: string; providerMetadata?: ProviderMetadata }
  | { type: "tool-call"; id: string; name: string; input: unknown; providerExecuted?: boolean }
  | { type: "tool-result"; id: string; name: string; result: ToolResultValue; providerExecuted?: boolean }
  | { type: "tool-error"; id: string; name: string; message: string }
  | { type: "request-finish"; reason: FinishReason }
```

**关键设计**：
- **流即事件序列**：所有模型输出（文本、推理、工具调用、工具结果）统一为事件流
- **`providerExecuted` 标记**：区分"模型请求执行"和"Provider 已执行"的工具调用
- **`request-finish`**：携带 `finishReason`（`"tool-calls"` | `"stop"` | `"length"` 等），驱动循环决策

### 2.2 请求结构

```typescript
interface LLMRequest {
  messages: Message[]       // 对话历史
  tools: ToolDefinition[]   // 可用工具的 JSON Schema
}
```

`LLMRequest.update()` 返回新对象（不可变更新），后续请求在此基础上追加消息。

---

## 3. Layer 2：Tool Runtime（工具运行时）——核心循环

> **文件**：`packages/llm/src/tool-runtime.ts`

这是 OpenCode 最核心的设计：一个**递归循环**处理"模型输出 → 工具执行 → 模型再调用"的多轮交互。

### 3.1 循环结构

```typescript
const stream = <T extends Tools>(options: StreamOptions<T>): Stream<LLMEvent> => {
  const loop = (request: LLMRequest, step: number): Stream<LLMEvent> =>
    Stream.unwrap(Effect.gen(function* () {
      // ── Phase 1: 累积模型输出 ──
      const state: StepState = { assistantContent: [], toolCalls: [], finishReason: undefined }
      const modelStream = options.stream(request)
        .pipe(Stream.tap(event => Effect.sync(() => accumulate(state, event))))

      // ── Phase 2: 判断是否继续 ──
      const continuation = Stream.unwrap(Effect.gen(function* () {
        // 非 tool-calls 结束或无工具调用 → 结束
        if (state.finishReason !== "tool-calls" || state.toolCalls.length === 0) return Stream.empty
        // toolExecution: "none" → 不执行，留给调用方
        if (options.toolExecution === "none") return Stream.empty

        // ── Phase 3: 并发执行工具 ──
        const dispatched = yield* Effect.forEach(
          state.toolCalls,
          call => dispatch(tools, call),
          { concurrency }    // 默认 10 路并发
        )
        const resultStream = Stream.fromIterable(dispatched.flatMap(...emitEvents))

        // ── Phase 4: 停止条件判断 ──
        if (!options.stopWhen) return resultStream
        if (options.stopWhen({ step, request })) return resultStream

        // ── Phase 5: 递归 ──
        return resultStream.pipe(
          Stream.concat(loop(followUpRequest(request, state, dispatched), step + 1))
        )
      }))

      return modelStream.pipe(Stream.concat(continuation))
    }))

  return loop(initialRequest, 0)
}
```

### 3.2 循环五阶段详解

| 阶段 | 职责 | 关键机制 |
|------|------|---------|
| **Phase 1：累积** | 监听模型流事件，累积 assistant 内容和工具调用 | `accumulate()` 将 text-delta/tool-call 等写入 StepState |
| **Phase 2：判断** | 检查 `finishReason` 是否为 `"tool-calls"` | 非工具调用 → 直接结束流 |
| **Phase 3：执行** | 并发执行所有挂起的工具调用 | `Effect.forEach` + `concurrency: 10` |
| **Phase 4：停止** | 检查 `stopWhen({ step, request })` | `stepCountIs(N)` 限制最大步数 |
| **Phase 5：递归** | 构建后续请求并递归调用 `loop()` | `followUpRequest` 追加 assistant + tool messages |

### 3.3 followUpRequest——消息拼接

```typescript
const followUpRequest = (request, state, dispatched) =>
  LLMRequest.update(request, {
    messages: [
      ...request.messages,
      Message.assistant(state.assistantContent),        // 模型输出（含工具调用）
      ...dispatched.map(([call, result]) =>
        Message.tool({ id: call.id, name: call.name, result })  // 工具结果
      ),
    ],
  })
```

**要点**：每轮循环将 **模型输出 + 所有工具结果** 追加到对话历史，形成完整的工具调用轮次。

### 3.4 toolExecution 模式

| 模式 | 行为 |
|------|------|
| `"auto"`（默认） | 模型调用工具 → 运行时自动执行 → 结果喂回模型 → 可能递归 |
| `"none"` | 仅将工具 Schema 发给模型（广告能力），模型输出的工具调用不执行，留给调用方处理 |

---

## 4. Layer 3：Session / Processor（会话与事件处理）

### 4.1 消息 Parts 系统

OpenCode 使用**部件化消息**而非简单文本字符串：

```typescript
type Part =
  | TextPart           // 文本内容
  | ToolPart           // 工具调用与结果
  | ReasoningPart      // 模型推理过程
  | FilePart           // 文件引用
  | AgentPart          // 智能体信息
  | SubtaskPart        // 子任务状态
  | StepStartPart      // 步骤开始标记
  | StepFinishPart     // 步骤完成标记
  | CompactionPart     // 上下文压缩标记
```

**优势**：
- 结构化表示多种内容类型（文本、工具调用、子任务、推理等）
- 每种 Part 可独立更新（流式追加文本、更新工具状态）
- 便于 UI 层精细化渲染

### 4.2 处理器事件管道

`packages/opencode/src/session/processor.ts` 实现事件处理管道：

```
用户输入 → 创建 User 消息（含 system prompt + agent + model 配置）
    ↓
调用 LLM Stream → 接收 LLMEvent
    ↓
事件分发：
  - text-delta → 追加到 TextPart（实时流式）
  - tool-call → 创建 ToolPart（状态: pending → running）
  - tool-result → 更新 ToolPart（状态: completed / error）
  - finish → 标记完成
    ↓
Tool Runtime 循环（自动递归）
    ↓
结果写回消息 Parts → UI 更新
```

### 4.3 子任务派发

```typescript
// 子任务状态
type SubtaskPart = {
  type: "subtask"
  status: "pending" | "running" | "completed" | "error"
  title: string
  sessionId: string
  agentType: string
  output?: string
}
```

子任务通过 `handleSubtask` 函数处理，每个子任务创建独立会话上下文。

---

## 5. Layer 4：Agent Registry（智能体注册表）

### 5.1 智能体类型

> **文件**：`packages/opencode/src/agent/agent.ts`

| 智能体 | 模式 | 可见性 | 职责 |
|--------|------|--------|------|
| `build` | primary | 可见 | 默认主智能体，执行所有工具 |
| `plan` | primary | 可见 | 规划模式，禁用编辑工具 |
| `general` | subagent | 可见 | 通用子智能体，多步研究 |
| `explore` | subagent | 可见 | 快速代码探索，只读 |
| `scout` | subagent | 可见（实验性） | 外部文档/依赖研究 |
| `compaction` | primary | 隐藏 | 对话压缩摘要 |
| `title` | primary | 隐藏 | 会话标题生成 |
| `summary` | primary | 隐藏 | 对话总结 |

**模式说明**：
- `primary`：由用户直接触发，管理主对话流
- `subagent`：由 `task` 工具派发，运行在独立会话中
- `all`：可同时作为 primary 和 subagent

### 5.2 权限系统

每个智能体携带 `Permission.Ruleset`，控制工具访问：

```typescript
// build 智能体权限（最宽松）
permission: Permission.merge(defaults, { question: "allow", plan_enter: "allow" }, user)

// explore 智能体权限（只读）
permission: Permission.merge(defaults, {
  "*": "deny",
  grep: "allow", glob: "allow", list: "allow",
  bash: "allow", webfetch: "allow", websearch: "allow",
  read: "allow",
  external_directory: readonlyExternalDirectory,
}, user)
```

权限级别：`"allow"` | `"ask"` | `"deny"`

### 5.3 智能体配置项

```typescript
interface AgentInfo {
  name: string
  mode: "primary" | "subagent" | "all"
  permission: Permission.Ruleset
  prompt?: string               // 自定义系统提示词
  model?: { providerID, modelID }  // 模型偏好
  temperature?: number
  topP?: number
  steps?: number                // 最大递归步数
  color?: string                // UI 颜色标记
  hidden?: boolean
  native?: boolean              // 内建 vs 用户自定义
  options: Record<string, unknown>  // 扩展选项
}
```

### 5.4 动态智能体生成

`generate()` 方法使用 LLM 根据用户描述自动生成智能体配置：

```
用户描述 → LLM（generateObject + zod schema）→ 输出:
{
  identifier: "code-reviewer",
  whenToUse: "Use this agent when...",
  systemPrompt: "You are..."
}
```

### 5.5 Reference 智能体

Scout 智能体可挂载外部参考源（Git 仓库或本地目录），为每个参考源动态生成只读研究智能体：

```typescript
// 配置 reference 后自动创建：
agents["my-lib"] = {
  name: "my-lib",
  permission: scout 权限 + repo_clone: deny + 特定目录读写,
  prompt: `You are configured reference @my-lib...Cached directory: /path...`,
  mode: "subagent",
}
```

---

## 6. Task 工具——子智能体派发的核心机制

### 6.1 参数 Schema

```typescript
const Parameters = Schema.Struct({
  description: Schema.String,      // 3-5 词任务描述
  prompt: Schema.String,           // 详细任务指令
  subagent_type: Schema.String,    // 目标智能体类型
  task_id: Schema.optional(Schema.String),  // 恢复已有会话
  command: Schema.optional(Schema.String),  // 触发命令
})
```

### 6.2 派发流程

```
1. 参数验证 → 权限检查（ctx.ask）
2. agent.get(subagent_type) → 获取目标智能体配置
3. 创建子会话（独立 sessionID）
4. 权限继承：merge(父会话权限, 子智能体权限)
5. resolvePromptParts(prompt) → 解析模板变量
6. ops.prompt(input) → 执行子智能体 LLM 调用
7. 收集结果 → 包装为结构化输出返回
```

### 6.3 结果结构

```typescript
{
  title: params.description,
  metadata: { sessionId, model },
  output: [
    `task_id: ${nextSession.id}`,
    "",
    "<task_result>",
    lastTextPart,
    "</task_result>",
  ].join("\n")
}
```

### 6.4 会话恢复

通过 `task_id` 参数可恢复之前的子智能体会话，实现增量式多轮子任务。

---

## 7. 系统提示词体系

### 7.1 智能体专属提示词

| 智能体 | 提示词文件 | 角色 |
|--------|-----------|------|
| explore | `prompt/explore.txt` | 标题生成器（注：文件名与内容似乎错位，实际内容为"精英 AI 智能体架构师"的标题生成规则） |
| scout | `prompt/scout.txt` | 外部依赖/文档研究只读智能体 |
| compaction | `prompt/compaction.txt` | PR 描述式对话压缩 |
| summary | `prompt/summary.txt` | 锚定式上下文摘要 |
| title | `prompt/title.txt` | 线程标题生成（≤50字符，严格格式规则） |
| generate | `generate.txt` | 自定义智能体生成的元提示词 |

### 7.2 提示词设计模式

**角色定义**：每个提示词以"You are..."开头，明确定位智能体的专业领域和行为边界。

**约束注入**：
- `scout`："do not modify files or run tools that change the user's workspace"
- `explore`："specify the desired thoroughness level: quick / medium / very thorough"
- `compaction`："2-3 sentences max, describe the changes made, not the process"

**输出格式**：
- `title`：严格的 `<rules>` + `<examples>` 结构化输出
- `generate`：JSON Schema 约束的 `{ identifier, whenToUse, systemPrompt }` 输出
- `compaction`：第一人称 PR 描述格式

---

## 8. 对 Nazh Copilot 的增强启示

### 8.1 当前 Nazh Copilot 的局限

| 层面 | Nazh 现状 | OpenCode 参考 |
|------|----------|--------------|
| **循环模型** | 单轮流式，无自动递归 | Tool Runtime 递归循环 |
| **工具调用** | 无工具系统，仅 JSON Lines 协议 | 完整的工具 Schema + 执行 + 结果回传 |
| **智能体** | 单一 copilot 角色 | 多智能体注册表 + 权限 + 模式 |
| **子任务** | 无子任务派发 | Task 工具 + 独立会话 + 恢复 |
| **消息结构** | 纯文本 content | Parts 系统（文本/工具/推理/子任务） |
| **上下文管理** | 无压缩/摘要 | Compaction + Summary 智能体 |

### 8.2 分阶段增强建议

#### Phase 1：Tool Runtime 循环（优先级最高）

将当前的"单流式 → 解析 JSON Lines → 一次性执行"改为"流式 → 工具调用 → 执行 → 结果回传 → 继续流"。

**具体方案**：

```
当前流程：
  用户 → 流式请求 → AI 输出 JSON Lines → 前端解析 → 执行画布操作 → 结束

增强后流程：
  用户 → 流式请求 → AI 输出（文本 + 工具调用）→ 运行时：
    ├─ 文本 → 直接展示
    ├─ 工具调用（add_node, add_edge, query_device）→ 执行 → 结果回传 AI → AI 继续
    └─ finish → 结束
```

**实现要点**：
- 后端引入 `tool_runtime` 循环（参考 `tool-runtime.ts` 的 `loop` 函数）
- 定义工具 Schema：`add_node`、`add_edge`、`query_device_catalog`、`query_pin_schema`、`validate_workflow` 等
- 工具执行后构建 `followUpRequest`，递归调用模型
- 设置 `maxSteps` 限制防止无限循环

#### Phase 2：结构化工具系统

当前 JSON Lines 协议本质是让 AI "自由发挥"输出格式，不可靠。改为标准 Function Calling：

```rust
// 工具定义示例
ToolDefinition {
    name: "add_node",
    description: "在工作流画布上添加一个节点",
    parameters: json!({
        "type": "object",
        "properties": {
            "ref": { "type": "string", "description": "节点引用 ID" },
            "node_type": { "type": "string", "enum": ["timer", "serialTrigger", "httpServer", ...] },
            "label": { "type": "string" },
            "connection_id": { "type": "string" },
            "config": { "type": "object" }
        },
        "required": ["ref", "node_type"]
    })
}
```

**优势**：
- LLM 的 function calling 能力远优于非结构化 JSON Lines 输出
- 工具参数有 Schema 校验，减少解析失败
- 工具结果可回传，AI 可根据结果调整策略

#### Phase 3：智能体分类与权限

引入智能体概念，区分 copilot 的不同工作模式：

| 智能体 | 职责 | 权限 |
|--------|------|------|
| `workflow-builder` | 构建工作流 | 添加/删除/修改节点和连线 |
| `device-analyzer` | 分析设备文档 | 只读设备资产 + AI 抽取 |
| `debugger` | 诊断运行时问题 | 读取日志/变量/状态 |
| `explainer` | 解释工作流逻辑 | 只读画布 |

#### Phase 4：子任务派发

支持复杂指令的分解与并行执行：

```
用户："为 Modbus 设备创建一个完整的采集工作流"

workflow-builder 智能体：
  1. 解析指令 → 识别子任务
  2. 派发子任务 A：device-analyzer("查询 Modbus 设备型号 X 的信号列表")
  3. 等待结果 → 获取信号列表
  4. 派发子任务 B：基于信号列表，构建采集工作流
  5. 或者并行派发 A 和 B（如果可预先推断信号结构）
```

#### Phase 5：上下文管理

引入 compaction/summary 机制管理长对话的上下文窗口：
- 超过 N 轮对话时自动压缩早期内容
- 保留关键操作记录（已添加的节点/连线）作为锚定上下文
- 工作流快照：每次 AI 操作后记录画布状态，压缩时引用快照而非完整对话

### 8.3 优先级排序

1. **Phase 1（Tool Runtime 循环）**：解决当前"一两个节点就中断"的根本问题
2. **Phase 2（Function Calling 工具）**：提升 AI 输出的可靠性和结构化程度
3. **Phase 4（子任务派发）**：支持复杂工作流的分步构建
4. **Phase 3（智能体分类）**：精细化权限控制
5. **Phase 5（上下文管理）**：长对话支持

---

## 9. 关键代码映射

| OpenCode 概念 | 源码位置 | Nazh 对应 |
|---------------|---------|----------|
| Tool Runtime 循环 | `packages/llm/src/tool-runtime.ts` | 需新建：后端 `tool_runtime` 模块 |
| 智能体注册表 | `packages/opencode/src/agent/agent.ts` | 需新建：copilot agent registry |
| Task 工具 | `packages/opencode/src/tool/task.ts` | 需新建：子任务派发机制 |
| LLM Event 流 | `packages/llm/src/schema.ts` | 对应：`copilot-stream.ts` 的 SSE 解析 |
| 消息 Parts | `packages/opencode/src/session/message-v2.ts` | 对应：`LocalMessage` 类型（需增强） |
| Session Processor | `packages/opencode/src/session/processor.ts` | 对应：`CopilotPanel.tsx` 的 `handleSend` |
| 权限系统 | `packages/opencode/src/permission/` | 需新建：copilot 操作权限 |
| 系统提示词 | `packages/opencode/src/agent/prompt/` | 对应：后端 copilot system prompt |
