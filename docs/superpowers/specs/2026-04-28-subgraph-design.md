# ADR-0013 子图节点实施设计

**日期**: 2026-04-28
**ADR**: ADR-0013（子图与宏系统）
**范围**: 子图核心实施（嵌套 + 参数化），模板库延后

## 概述

基于 FlowGram `free-container-plugin` 子画布插件，引入子图节点作为一等工作流组合单元。子图对外表现为单个节点（带 input/output 引脚），内部包裹 `blocks[]` + `edges[]` 子拓扑。部署时前端展平为扁平 DAG，Runner 零改动。

## 决策摘要

- **数据模型对齐 FlowGram** — 不自建 struct，用 `WorkflowNodeJSON` 的 `blocks`/`edges` 递归结构
- **桥接节点补齐 gap** — `subgraphInput`/`subgraphOutput` 连接容器端口与内部子拓扑
- **前端展平，Rust 不感知** — `flowgram.ts` 递归展平，Runner 处理扁平 DAG
- **参数化** — `{{paramName}}` 占位符在展平时替换
- **嵌套上限 8 层** + 循环引用检测
- **AI 编排读取节点能力** — 从节点定义 / 默认 config 键 / 运行时注册表 / pin schema 生成 prompt，支持 `upsert_subgraph`
- **模板库延后** — `definitionRef` 字段预留，不实现存储/加载

## 数据模型

### 新增前端节点类型

| 类型 | 角色 | meta |
|------|------|------|
| `subgraph` | 容器节点，`isContainer: true` | `defaultPorts: [input, output]`，`SubCanvasRender` 内部渲染 |
| `subgraphInput` | 容器内桥接入口 | `inputDisable: true`，仅 output port |
| `subgraphOutput` | 容器内桥接出口 | `outputDisable: true`，仅 input port |

### 子图 JSON 结构

```jsonc
{
  "id": "collect-modbus-1",
  "type": "subgraph",
  "meta": { "isContainer": true },
  "data": {
    "label": "Modbus 采集",
    "parameterBindings": { "mb_host": "192.168.1.10", "register": 40001 }
  },
  "blocks": [
    { "id": "sg-in", "type": "subgraphInput", "meta": {} },
    { "id": "mb-read", "type": "modbusRead", "data": { "config": { "host": "{{mb_host}}" } } },
    { "id": "sg-out", "type": "subgraphOutput", "meta": {} }
  ],
  "edges": [
    { "sourceNodeID": "sg-in", "targetNodeID": "mb-read" },
    { "sourceNodeID": "mb-read", "targetNodeID": "sg-out" }
  ]
}
```

### Rust 侧 PassthroughNode

新增 `crates/nodes-flow/src/passthrough.rs`：

- 实现 `NodeTrait`：`transform` 直接 `Ok(NodeExecution::output(NodeOutput::passthrough(payload)))`
- `capabilities = PURE`
- `input_pins` / `output_pins` 返回默认 `Json` pin
- `FlowPlugin` 注册 `subgraphInput` 和 `subgraphOutput` 两种 type → 同一工厂

## 展平逻辑

### 核心函数 `flattenSubgraphs()`

位置：`web/src/lib/flowgram.ts`

在 `toNazhWorkflowGraph` 入口调用，先展平再走现有转换。

### 展平规则

```
外部 DAG：
  timer ──→ [subgraph "collect-modbus-1"] ──→ sqlWriter

子图内部：
  sg-in ──→ mb-read ──→ sg-out

展平后（传给 Rust）：
  timer ──→ collect-modbus-1/sg-in ──→ collect-modbus-1/mb-read ──→ collect-modbus-1/sg-out ──→ sqlWriter
```

1. 遇 `subgraph` 节点 → 递归处理 `blocks[]` + `edges[]`
2. 内部节点 ID 加前缀 `<subgraph-id>/<inner-node-id>`
3. 外部边 `source → subgraph.input` → `source → <subgraph-id>/sg-in`
4. 外部边 `subgraph.output → target` → `<subgraph-id>/sg-out → target`
5. `{{paramName}}` 在 config JSON string 值中被 `parameterBindings` 替换
6. 嵌套深度 > 8 报错
7. 循环引用检测：ID 路径去重

### 参数替换

```typescript
function applyParameterBindings(
  node: WorkflowNodeDefinition,
  params: Record<string, string | number | boolean>,
): void
```

深度遍历 node 的 `config` JSON，对 string 值做 `{{paramName}}` → 绑定值替换。未绑定参数保留原值。

### FLOWGRAM_BUSINESS_NODE_TYPES

展平阶段在 `toNazhWorkflowGraph` 前执行，原函数看到的已是平的。需确保展平后 `subgraphInput`/`subgraphOutput` 被识别为合法业务节点——加入 `FLOWGRAM_BUSINESS_NODE_TYPES`。

## 前端组件

### 节点注册

`flowgram-node-library.ts`：
- `subgraph` 加入节点目录，新分类"子图封装"
- `subgraphInput` / `subgraphOutput` 不在节点面板显示（拖入子图时自动插入，或编辑器工具栏创建）

### 容器渲染组件

新文件 `SubgraphNode.tsx`（`web/src/components/flowgram/nodes/`）：
- `SubCanvasRender` + `SubCanvasBackground` + `SubCanvasBorder`
- 标题栏显示子图 label + 参数配置入口
- `useNodeSize` 管理尺寸自适应

### Pin 系统接入

- `subgraph` 节点 pin：`describe_node_pins` 返回 `input: [Json]`, `output: [Json]`
- `subgraphInput` / `subgraphOutput` 不走 IPC pin 校验

### 事件路径显示

`node_id` 保留 `/` 前缀路径。前端按 `/` 拆解显示为 `Modbus 采集 › mb-read`。

## Rust 侧改动

最小改动：

1. **`PassthroughNode`** — `crates/nodes-flow/src/passthrough.rs`
2. **`FlowPlugin` 注册** — 加 `subgraphInput` / `subgraphOutput`
3. **IPC 无改动** — `deploy_workflow` 收到的 AST 已展平
4. **`WorkflowNodeDefinition` 无变化** — `id` 已是 `String`，`collect-modbus-1/mb-read` 合法

## 测试策略

### Rust 单元测试

- `PassthroughNode` payload 直传、metadata 为空

### Rust 集成测试

- 含 `subgraphInput`/`subgraphOutput` 的扁平 DAG 端到端执行

### 前端单元测试（Vitest）

- `flattenSubgraphs()` 完整测试：
  - 无子图 pass-through
  - 单层子图展平
  - 嵌套子图（2-3 层）
  - 8 层上限报错
  - 循环引用检测
  - 参数替换（`{{param}}` → 值）
  - 未绑定参数保留
  - 外部边重写（input/output port 映射）

### E2E

- 部署含子图工作流 → 验证节点数/边数 → 验证执行结果

## 文件变更预估

| 文件 | 改动类型 |
|------|----------|
| `web/src/lib/flowgram.ts` | 新增 `flattenSubgraphs` + 参数替换 + 边重写 |
| `web/src/components/flowgram/nodes/SubgraphNode.tsx` | 新建 |
| `web/src/components/flowgram/nodes/SubgraphInputNode.tsx` | 新建 |
| `web/src/components/flowgram/nodes/SubgraphOutputNode.tsx` | 新建 |
| `web/src/components/flowgram/flowgram-node-library.ts` | 注册新节点 |
| `web/src/lib/workflow-node-capabilities.ts` | AI 节点能力目录 |
| `web/src/lib/workflow-orchestrator.ts` | `upsert_subgraph` 操作 + 自动节点能力 prompt |
| `web/src/lib/__tests__/flowgram-subgraph.test.ts` | 新建 |
| `crates/nodes-flow/src/passthrough.rs` | 新建 |
| `crates/nodes-flow/src/lib.rs` | 注册 passthrough |
| `crates/nodes-flow/src/plugin.rs` | FlowPlugin 加 subgraphInput/subgraphOutput |
| `tests/workflow.rs` | 集成测试 |
| `docs/adr/0013-子图与宏系统.md` | 状态更新 → 已实施（子图核心） |

## 不在本范围

- 模板库存储（本地文件 / 项目库 / 远程）— 独立 ADR
- `definitionRef` 版本对比 / 升级提示
- 变量作用域 `<subgraph-id>::` 命名空间（依赖 ADR-0012 变量系统扩展）
- 跨工程模板复用
