# nodeType 宏观契约设计

> **状态**: 设计完成，待实施
> **日期**: 2026-05-02
> **动机**: `nodeType` 同时出现在 Rust `WorkflowNodeDefinition::type`、`NodeRegistry` 注册键、Tauri `list_node_types` / `describe_node_pins`、FlowGram `data.nodeType`、AI 编排协议 `upsert_node.nodeType`、模板 seed 与前端节点库中。若不先定义它的宏观语义，局部重构会把“运行时身份 / 编辑器形态 / 图标皮肤 / AI 可见性 / pin schema”混成一个概念，长期维护成本会继续上升。

## 一句话定义

`nodeType` 是**节点类型的稳定身份字符串**：它决定一个工作流节点在运行时由哪个 `NodeRegistry` 工厂实例化，也决定持久化图、IPC、前端节点库与 AI 编排协议之间如何指向同一类节点。

`nodeType` 不是 UI 分类、不是图标、不是能力标签、不是 pin schema、不是节点实例 id。

## 设计目标

- **稳定**：保存到项目文件里的 `nodeType` 不能因为 UI 重命名或 crate 重组而失效。
- **分层**：Rust 运行时、前端编辑器、AI 编排各自有事实源，不互相冒充。
- **可扩展**：内置节点、feature-gated 节点、编辑器宏节点、未来外部插件节点都有位置。
- **可降级**：前端未知但运行时已注册的节点可以展示为“未知运行时节点”，而不是被静默改成 `native`。
- **低成本**：只抽象已经反复造成同步问题的维度；图标、颜色、复杂 settings 表单暂不 schema 化。

## 术语

| 名称 | 含义 | 事实源 |
|------|------|--------|
| Runtime NodeType | 可被 Rust `NodeRegistry` 创建的运行时类型，如 `code` / `timer` / `modbusRead` | Rust `NodeRegistry` |
| Editor NodeType | FlowGram 编辑器认识的节点类型；通常等于 Runtime NodeType，也可包含编辑器宏节点 | 前端 `NodeDefinition` registry |
| Known Editor NodeType | 当前前端代码内置认识的 Editor NodeType 字面量联合 | `ALL_DEFS as const` 派生 |
| DisplayType | UI 皮肤/图标选择，默认等于 nodeType，可作为纯展示 fallback | 前端渲染层 |
| Node Instance ID | 图中某个节点实例的 id，如 `timer_trigger` | 工作流图 |
| NodeCapabilities | 类型级能力位图，如 `TRIGGER` / `NETWORK_IO` | Rust `NodeRegistry::capabilities_of` |
| PinDefinition | 实例级输入/输出 pin 契约，可随 config 变化 | `NodeTrait::input_pins/output_pins` |

## 分层事实源

### 1. Rust Runtime Registry

Rust `NodeRegistry` 是**运行时可执行 nodeType 的事实源**。

- `WorkflowNodeDefinition.type` 存储 Runtime NodeType。
- `NodeRegistry::register_with_capabilities(node_type, caps, factory)` 声明可实例化类型。
- `NodeRegistry::create(definition, resources)` 只按 `definition.node_type()` 查找工厂。
- `list_node_types` 只反映当前构建/插件实际注册的 Runtime NodeType。
- `describe_node_pins(nodeType, config)` 通过实例化运行时节点获得 pin schema。

约束：

- Ring 0 不知道任何具体 nodeType。
- Runtime NodeType 是大小写敏感的字符串。
- 未注册 Runtime NodeType 必须部署失败，不能自动 fallback。
- `NodeCapabilities` 是运行时类型级附加信息，不是 nodeType 身份的一部分。

### 2. Frontend Editor Registry

前端 `NodeDefinition` registry 是**编辑器已知 nodeType 的事实源**。

它负责：

- palette 是否展示、标题、badge、分类描述
- 默认 seed 与默认 config 规范化
- FlowGram 动态输出端口
- AI 可见性与本地提示文本
- 前端保存/部署时识别哪些 FlowGram 节点是业务节点

它不负责：

- 证明某 nodeType 在当前 Rust runtime 中一定可执行
- 定义运行时能力位图
- 定义真实 pin schema 的最终真值
- 替代 Rust `NodeRegistry`

因此，前端需要同时区分：

- `KnownEditorNodeType`: 前端内置认识，可得到完整 `NodeDefinition`
- `RuntimeRegisteredNodeType`: 当前桌面 runtime 已注册，由 IPC 返回
- `UnknownNodeType`: 出现在导入图或 runtime list 中，但前端没有 definition

### 3. AI Orchestration Protocol

AI 编排协议里的 `nodeType` 是**编辑器可接受的创建目标**，不是任意 Runtime NodeType。

默认只允许：

- `NodeDefinition.ai.visible !== false`
- 且当前操作协议能表达/编辑的节点

编辑器内部桥接节点（如 `subgraphInput` / `subgraphOutput`）默认不暴露给 AI。编辑器宏节点（如 `subgraph`）可以暴露，但必须标记 `ai.editorOnly=true`，并由操作协议专门处理。

AI 不能通过 `nodeType` 推断 sourcePortId。sourcePortId 约束来自 `NodeDefinition.getRoutingBranches(config)` 或运行时 pin 信息，而不是硬编码节点名。

## nodeType 分类矩阵

| 类别 | 示例 | Runtime 注册 | Editor Definition | Palette | AI 可见 | 保存/部署 |
|------|------|---------------|-------------------|---------|---------|-----------|
| 普通运行时节点 | `code`, `timer`, `httpClient` | 是 | 是 | 通常是 | 通常是 | 直接保存为 `WorkflowNodeDefinition.type` |
| feature-gated 协议节点 | `modbusRead`, `mqttClient` | 取决于构建 feature | 是 | 应按 runtime list 决定可用状态 | 应按 runtime list 决定可用状态 | 未注册时部署失败 |
| 编辑器宏容器 | `subgraph` | 否 | 是 | 是 | 可见但 `editorOnly=true` | 展平后从部署 DAG 消失 |
| 运行时桥接节点 | `subgraphInput`, `subgraphOutput` | 是 | 是 | 否 | 否 | 展平后参与 DAG |
| 复合形态运行时节点 | `loop` | 是 | 是，UI 可渲染为容器 | 是 | 是 | 保存为运行时节点 |
| 外部插件节点 | `opencv/detect` | 是 | 未来可选 | 若无 definition 则不进普通 palette | 默认不可见 | 可展示但编辑能力降级 |
| 兼容别名 | `sql/writer` | 不推荐长期注册 | 否 | 否 | 否 | 只用于旧图迁移 |

## 命名规则

### 内置节点

内置 Runtime NodeType 使用稳定的 lowerCamelCase ASCII 字符串：

```text
code
timer
serialTrigger
modbusRead
mqttClient
httpClient
barkPush
sqlWriter
debugConsole
if
switch
tryCatch
loop
humanLoop
subgraphInput
subgraphOutput
c2f
minutesSince
lookup
```

约束：

- 一经保存到项目文件，视为持久化契约。
- 不因 UI 文案、crate 名、协议实现变化而重命名。
- 新内置节点继续使用 lowerCamelCase；不要引入空格、冒号、点号。

### 外部插件节点

未来外部插件、sidecar、脚本包使用 namespaced nodeType：

```text
opencv/detect
script/temperature
vendor/custom-node
```

约束：

- `/` 只用于 nodeType namespace，不用于节点实例 id 的语义。
- namespace 推荐小写 kebab-case；内置节点不 retroactively 改成带 `/` 的形式。
- `list_node_types` 可以返回 namespaced nodeType；没有前端 definition 时，前端以未知运行时节点降级展示。

### 别名与迁移

别名只服务兼容旧图，不作为新图事实源。

- 新保存的图必须写 canonical nodeType。
- palette / AI / PluginPanel 不展示别名。
- 需要兼容旧别名时，优先在导入/迁移层把旧值改写为 canonical；只有无法迁移时才在 Runtime Registry 临时注册 alias。
- alias 的移除必须有迁移窗口与回归测试。

## 持久化规则

工作流图中的 `WorkflowNodeDefinition.type` 是持久化字段，必须保留原始 nodeType。

前端规则：

- UI 显示可以把未知 nodeType 渲染成 fallback 卡片。
- 保存/部署路径不得把未知 nodeType 静默改成 `native`。
- `normalizeNodeKind(value)` 只适合“已知编辑器节点的 UI fallback”；涉及持久化时应使用 `preserveNodeType(value)` 或显式判断。
- `toNazhWorkflowGraph()` 应保存真实 `data.nodeType ?? node.type`，仅过滤明确的 editor-only 容器。

Rust 规则：

- 部署期对未注册 Runtime NodeType 报 `unsupported_node_type`。
- 不在 Runner 里做模糊匹配或 fallback。
- 路径改写、连接继承等预处理若匹配 nodeType，必须优先匹配 canonical；兼容 alias 时写清迁移理由。

## 与其他维度的关系

### NodeCapabilities

`NodeCapabilities` 是类型级能力标签，来自 Rust registry。

它回答“这一类节点承诺具有什么运行时特征”，不回答“这个节点叫什么”。同一个 nodeType 的所有 config 都必须满足该类型级能力；若能力只在某些 config 下成立，不应声明为类型级能力。

### PinDefinition

PinDefinition 是实例级契约，来自实际节点实例。

它回答“这个 config 下有哪些输入/输出 pin，pin 类型与 Exec/Data 语义是什么”。`switch` 的分支、`modbusRead.latest` 这类输出都属于 pin/port 维度，不应塞进 nodeType 命名。

### NodeDefinition

前端 NodeDefinition 是编辑器声明，不是 runtime authority。

它可以为已知节点提供默认 config、palette、AI hint、动态输出端口，但不能替代 `list_node_types` 和 `describe_node_pins` 的运行时真值。设计上应允许 `NodeDefinition` 超前于 runtime（feature 未启用时置灰/隐藏），也允许 runtime 超前于 `NodeDefinition`（外部插件降级展示）。

### DisplayType

DisplayType 是视觉皮肤。

默认等于 nodeType，但它可以在 UI 层 fallback 到 `native` 图标。这个 fallback 不能回写到工作流图的 `nodeType`。

## 前端类型建议

避免把所有 nodeType 都揉成一个 `string` 或一个手写 union。

推荐分层：

```ts
export type KnownEditorNodeType = (typeof ALL_DEFS)[number]['kind'];

export type RuntimeNodeType = string;

export function isKnownEditorNodeType(value: unknown): value is KnownEditorNodeType {
  return typeof value === 'string' && DEF_MAP.has(value as KnownEditorNodeType);
}

export function toKnownEditorNodeTypeOrNull(value: unknown): KnownEditorNodeType | null {
  return isKnownEditorNodeType(value) ? value : null;
}

export function resolveDisplayType(value: unknown): KnownEditorNodeType | 'unknown' {
  return isKnownEditorNodeType(value) ? value : 'unknown';
}
```

命名建议：

- `normalizeNodeKind` 只作为过渡兼容名保留，语义限定为 UI fallback。
- 新代码优先使用 `isKnownEditorNodeType` / `toKnownEditorNodeTypeOrNull` / `resolveDisplayType`，让调用点表达“是否允许未知类型”。
- 持久化代码不调用会把未知类型变成 `native` 的函数。

## 运行时/前端交汇规则

`list_node_types` 与前端 `NodeDefinition` registry 需要做集合运算：

- `known ∩ registered`: 完整可编辑、可拖拽、可部署
- `known - registered`: 前端知道但当前 runtime 未启用；palette 可隐藏或置灰，部署前必须提示
- `registered - known`: runtime 插件节点；PluginPanel 显示，画布可降级展示，普通 palette 不主动暴露
- `unknown in persisted graph`: 保留 nodeType，UI 降级，部署交给 runtime 判定

这比“前端节点库就是全部节点”更长远，也比现在就为外部插件设计完整 schema 更克制。

## 新增 nodeType checklist

### 新增内置运行时节点

必须同步：

- Rust 插件 crate 中 `register_with_capabilities("<nodeType>", ...)`
- owning crate `AGENTS.md` 节点表
- `src/registry.rs` capability 合约测试
- frontend `NodeDefinition`
- root README 节点目录（若当前目录覆盖该类节点）
- 若有 `#[ts(export)]` 类型变化，重新导出 TS 类型

### 新增编辑器宏节点

必须同步：

- frontend `NodeDefinition`，并声明 `ai.editorOnly` / `palette.visible`
- `toNazhWorkflowGraph()` 的保存/展平策略
- AI 操作协议是否允许创建它
- 说明它是否会出现在 Runtime DAG

### 新增外部插件节点

必须遵守：

- 使用 namespaced nodeType
- runtime 通过 `list_node_types` 暴露
- 前端没有 definition 时只做降级展示，不假装有完整编辑能力
- 若要进入 palette，需要提供后续插件 manifest / schema 设计，而不是塞进内置 `ALL_DEFS`

## 当前设计落点

`docs/superpowers/specs/2026-05-02-node-definition-single-source-design.md` 是本契约在前端内置节点上的第一步落地：

- 从 `ALL_DEFS as const` 派生 Known Editor NodeType
- 把 `NodeDefinition` 定位为编辑器事实源，而非 runtime authority
- 拆开 output ports 与 routing branches
- 删除易漂移的分类、AI 可见性和 palette 静态表
- 保留图标、颜色、复杂 settings 表单的局部硬编码，避免过度 schema 化

后续如果要把本设计升级为强制架构不变量，应补一份 ADR，并把精简版契约写入 root `AGENTS.md` / 相关 crate `AGENTS.md`。
