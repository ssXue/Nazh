# Code 节点 AI 脚本生成功能设计

## 概述

在 code/rhai 节点的设置面板中，增加「AI 生成」按钮，允许用户通过自然语言需求描述，调用全局 AI 配置自动生成 Rhai 脚本代码并填入脚本编辑区。

## 方案选型

选择**方案 A：前端直调 `copilotComplete`**。复用已有 IPC 通道和 AI 基础设施，无需新增 Rust 端命令，改动最小。

其他备选方案：
- B：新增 Rust 端 `generate_script` 命令 — 上下文获取链路长，当前 Rust 端不存储前端画布状态
- C：流式生成 + WebSocket — 复杂度高，用户选择加载指示器方案，流式非必需

## 交互流程

1. 脚本编辑区右上角显示「AI 生成」按钮（仅 `code`/`rhai` 节点可见，且全局 AI 已配置时可用）
2. 用户点击 → 弹出需求输入弹窗（textarea + 取消/生成按钮）
3. 点击「生成」→ 按钮变为加载态，textarea 禁用
4. 前端调用 `copilotComplete()`，发送构建好的 prompt
5. 成功 → 弹窗关闭，生成代码直接替换脚本编辑区内容（用户可 Ctrl+Z 撤销）
6. 失败 → 弹窗内显示错误提示，用户可重试或关闭

### 前置检查

- 全局无 AI 提供商 → 按钮灰显 + tooltip "请先在 AI 配置中添加提供商"
- 正在生成中 → 按钮禁用，防止重复调用

## Prompt 构建

独立函数 `buildScriptGenerationPrompt(requirement, nodeContext)` 构建 `AiMessage[]`。

### System Prompt（固定）

```
你是工业边缘计算工作流的脚本编写助手。根据用户需求生成 Rhai 脚本代码。
规则：
- 只输出可执行的 Rhai 脚本，不要输出解释文字
- 脚本可通过 ctx.payload() 获取输入数据
- 脚本可通过 ctx.set_output(value) 设置输出
- 如需调用 AI，使用 ai_complete("prompt") 函数
- 不要使用 print() 等调试语句
- 保持简洁，专注于数据处理和转换逻辑
```

### User Message（动态拼接）

```
节点类型：{nodeType}
节点名称：{label}
节点描述：{aiDescription || "无"}

上下游信息：
- 上游节点：{upstreamList}（每个含 nodeType + label + aiDescription）
- 下游节点：{downstreamList}

用户需求：
{requirement}
```

### 上下文获取

从 FlowGram 画布数据中读取当前节点的上下游节点信息（类型、名称、描述），构建 `nodeContext` 对象。

## AI 配置来源

使用全局 AI 配置（从 `aiProviders` + `activeAiProviderId` 读取），不依赖节点级 AI 能力配置。若全局未配置 AI 提供商，按钮不可用。

## 组件结构与代码改动

### 新增文件

1. **`web/src/components/flowgram/AiScriptGenerator.tsx`** — 需求输入弹窗组件
   - Props: `open`, `onClose`, `onGenerate(requirement: string)`, `loading`, `error`
   - 内部状态: `requirement` textarea
   - 渲染: 模态弹窗（textarea + 取消/生成按钮 + 加载/错误态）

2. **`web/src/lib/script-generation.ts`** — Prompt 构建与生成逻辑
   - `buildScriptGenerationPrompt(requirement, nodeContext)` → `AiMessage[]`
   - `generateScript(requirement, nodeContext)` → 调用 `copilotComplete()` → 返回生成代码
   - `getNodeContext(flowData, nodeId)` → 从画布数据提取上下游信息

### 修改文件

1. **`web/src/components/flowgram/FlowgramNodeSettingsPanel.tsx`**
   - 脚本 textarea 区右上角添加「AI 生成」按钮（仅 `isScriptNode` 且非只读时显示）
   - 按钮可用性：全局 AI 配置存在且未在生成中
   - 新增状态: `aiGenerating`, `aiGeneratorOpen`, `aiGenerateError`
   - 生成成功后更新 `draft.script` + 调用 `handleFieldChange` 同步到 FlowGram

2. **`web/src/components/FlowgramCanvas.tsx`** — 确认全局 AI 配置状态已传递给设置面板（`aiProviders`/`activeAiProviderId` 已有传递路径）

### 不改动的部分

- Rust 端零改动
- 不新增 IPC 命令
- 不新增 ts-rs 类型
- 弹窗样式复用项目现有 CSS 类

## 生成状态反馈

按钮显示加载旋转图标 + 简短文字"生成中..."，完成（成功或失败）后自动恢复。生成失败时弹窗内显示错误信息。

## 错误处理

- AI 服务不可用（无提供商/连接失败）→ 弹窗内展示错误，保留需求文本，可重试
- 生成结果为空 → 提示"AI 未返回有效代码"
- 生成超时 → 复用 `copilotComplete` 的超时机制，超时提示"生成超时，请重试"

## 测试策略

- `script-generation.ts` 的单元测试（Vitest）：
  - `buildScriptGenerationPrompt` 输出格式验证
  - `getNodeContext` 上下游信息提取
  - 边界情况：无上下游、无 aiDescription
- `AiScriptGenerator.tsx` 组件测试：
  - 弹窗开关、加载态、错误态渲染
- 手动集成测试：完整流程（点击→输入→生成→替换）
