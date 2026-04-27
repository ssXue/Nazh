# ADR-0013 子图节点实施 Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 基于 FlowGram 子画布插件实现子图节点（含嵌套 + 参数化），部署时前端展平为扁平 DAG，Runner 零改动。

**Architecture:** 新增 3 个前端节点类型（subgraph / subgraphInput / subgraphOutput）+ Rust 侧 PassthroughNode。`flowgram.ts` 在 `toNazhWorkflowGraph` 前递归展平容器节点，内部节点 ID 加 `<subgraph-id>/` 前缀，外部边重写桥接映射。参数 `{{paramName}}` 在展平时替换。

**Tech Stack:** Rust (NodeTrait, FlowPlugin)、TypeScript/React (FlowGram free-container-plugin, SubCanvasRender)、Vitest

---

## File Structure

| 操作 | 路径 | 职责 |
|------|------|------|
| 新建 | `crates/nodes-flow/src/passthrough.rs` | PassthroughNode 实现 |
| 改 | `crates/nodes-flow/src/lib.rs` | 注册 passthrough 模块 + FlowPlugin 加 subgraphInput/subgraphOutput |
| 新建 | `web/src/components/flowgram/nodes/subgraph/index.ts` | 子图节点定义（NodeDefinition） |
| 新建 | `web/src/components/flowgram/nodes/subgraphInput/index.ts` | 桥接入口节点定义 |
| 新建 | `web/src/components/flowgram/nodes/subgraphOutput/index.ts` | 桥接出口节点定义 |
| 改 | `web/src/components/flowgram/flowgram-node-library.ts` | 注册新节点 + 面板分类 |
| 改 | `web/src/components/flowgram/nodes/catalog.ts` | 新增"子图封装"分类 |
| 改 | `web/src/components/flowgram/nodes/shared.ts` | NazhNodeKind 扩展 + normalizeNodeConfig 扩展 |
| 改 | `web/src/lib/flowgram.ts` | 新增 flattenSubgraphs + 参数替换 + 边重写 |
| 新建 | `web/src/lib/__tests__/flowgram-subgraph.test.ts` | 展平逻辑单元测试 |
| 改 | `tests/workflow.rs` | 含 passthrough 节点的集成测试 |
| 改 | `docs/adr/0013-子图与宏系统.md` | 状态更新为已接受 |

---

### Task 1: Rust PassthroughNode 实现

**Files:**
- Create: `crates/nodes-flow/src/passthrough.rs`
- Modify: `crates/nodes-flow/src/lib.rs`

- [ ] **Step 1: 创建 passthrough.rs**

```rust
//! 子图桥接节点的 passthrough 实现——payload 直传，无副作用。

use async_trait::async_trait;
use nazh_core::{NodeCapabilities, NodeExecution, NodeTrait, WorkflowNodeDefinition, SharedResources, EngineError};
use serde_json::Value;
use std::sync::Arc;
use uuid::Uuid;

/// Passthrough 节点：将输入 payload 原样输出。
/// 用于展平后子图桥接节点（subgraphInput / subgraphOutput）。
pub struct PassthroughNode {
    id: String,
}

impl PassthroughNode {
    pub fn new(definition: &WorkflowNodeDefinition) -> Result<Arc<dyn NodeTrait>, EngineError> {
        Ok(Arc::new(Self {
            id: definition.id().to_owned(),
        }))
    }
}

#[async_trait]
impl NodeTrait for PassthroughNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> &'static str {
        "passthrough"
    }

    async fn transform(
        &self,
        _trace_id: Uuid,
        payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        Ok(NodeExecution::broadcast(payload))
    }
}
```

- [ ] **Step 2: 修改 lib.rs 注册模块和节点类型**

在 `crates/nodes-flow/src/lib.rs`：

1. 添加模块声明 `mod passthrough;`
2. 在 `FlowPlugin::register` 中注册 `subgraphInput` 和 `subgraphOutput` 两种类型：

```rust
mod passthrough;

// 在 FlowPlugin::register 方法末尾添加：

registry.register_with_capabilities(
    "subgraphInput",
    NodeCapabilities::PURE,
    passthrough::PassthroughNode::new,
);

registry.register_with_capabilities(
    "subgraphOutput",
    NodeCapabilities::PURE,
    passthrough::PassthroughNode::new,
);
```

注意：`PassthroughNode::new` 签名需要匹配 `register_with_capabilities` 的工厂闭包签名 `Fn(&WorkflowNodeDefinition, SharedResources) -> Result<Arc<dyn NodeTrait>, EngineError>`。上面 `new` 的签名已是如此。

- [ ] **Step 3: 运行 cargo check 验证编译**

Run: `cargo check -p nodes-flow`
Expected: 编译通过，无错误

- [ ] **Step 4: 运行已有测试确认无回归**

Run: `cargo test --workspace`
Expected: 所有测试通过

- [ ] **Step 5: 提交**

```bash
git add crates/nodes-flow/src/passthrough.rs crates/nodes-flow/src/lib.rs
git commit -s -m "feat(nodes-flow): 新增 PassthroughNode 用于子图桥接节点"
```

---

### Task 2: 前端节点类型扩展（shared.ts + catalog.ts）

**Files:**
- Modify: `web/src/components/flowgram/nodes/shared.ts`
- Modify: `web/src/components/flowgram/nodes/catalog.ts`

- [ ] **Step 1: 扩展 NazhNodeKind 联合类型**

在 `web/src/components/flowgram/nodes/shared.ts` 的 `NazhNodeKind` 类型中添加三个新类型：

```typescript
export type NazhNodeKind =
  | 'native'
  | 'code'
  | 'timer'
  | 'serialTrigger'
  | 'modbusRead'
  | 'mqttClient'
  | 'if'
  | 'switch'
  | 'tryCatch'
  | 'loop'
  | 'httpClient'
  | 'barkPush'
  | 'sqlWriter'
  | 'debugConsole'
  | 'subgraph'
  | 'subgraphInput'
  | 'subgraphOutput';
```

- [ ] **Step 2: 扩展 normalizeNodeKind 函数**

在 `normalizeNodeKind` 的 switch 中增加：

```typescript
case 'subgraph':
case 'subgraphInput':
case 'subgraphOutput':
  return value;
```

- [ ] **Step 3: 扩展 getFallbackNodeLabel 函数**

在 `getFallbackNodeLabel` 的 switch 中增加：

```typescript
case 'subgraph':
  return 'Subgraph';
case 'subgraphInput':
  return 'Input';
case 'subgraphOutput':
  return 'Output';
```

- [ ] **Step 4: 更新 catalog.ts**

在 `web/src/components/flowgram/nodes/catalog.ts` 的 `NODE_CATEGORIES` 数组末尾添加 `'子图封装'`。在 `NODE_CATEGORY_MAP` 中添加：

```typescript
subgraph: { category: '子图封装', description: '封装子拓扑为单节点，支持嵌套和参数化' },
```

- [ ] **Step 5: 运行前端类型检查**

Run: `npm --prefix web run build 2>&1 | head -30`
Expected: 类型检查通过（可能有未引用新类型的警告，但无错误）

- [ ] **Step 6: 提交**

```bash
git add web/src/components/flowgram/nodes/shared.ts web/src/components/flowgram/nodes/catalog.ts
git commit -s -m "feat(frontend): 扩展 NazhNodeKind 支持子图节点类型"
```

---

### Task 3: 子图节点定义文件

**Files:**
- Create: `web/src/components/flowgram/nodes/subgraph/index.ts`
- Create: `web/src/components/flowgram/nodes/subgraphInput/index.ts`
- Create: `web/src/components/flowgram/nodes/subgraphOutput/index.ts`

- [ ] **Step 1: 创建 subgraph/index.ts**

```typescript
import {
  type NodeDefinition,
  type NodeSeed,
  type NodeValidationContext,
  type NodeValidation,
  normalizeNodeConfig,
} from '../shared';

export const definition: NodeDefinition = {
  kind: 'subgraph',
  catalog: {
    category: '子图封装',
    description: '封装子拓扑为单节点，支持嵌套和参数化',
  },
  fallbackLabel: 'Subgraph',

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'subgraph',
      kind: 'subgraph',
      label: '',
      timeoutMs: null,
      config: {
        parameterBindings: {},
      },
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    return normalizeNodeConfig('subgraph', config);
  },

  getNodeSize() {
    return { width: 560, height: 400 };
  },

  buildRegistryMeta() {
    return {
      defaultExpanded: true,
      size: this.getNodeSize(),
      defaultPorts: [{ type: 'input' as const }, { type: 'output' as const }],
    };
  },

  validate(_ctx: NodeValidationContext): NodeValidation[] {
    return [];
  },
};
```

- [ ] **Step 2: 创建 subgraphInput/index.ts**

```typescript
import {
  type NodeDefinition,
  type NodeSeed,
  type NodeValidationContext,
  type NodeValidation,
  normalizeNodeConfig,
} from '../shared';

export const definition: NodeDefinition = {
  kind: 'subgraphInput',
  catalog: {
    category: '子图封装',
    description: '子图内部桥接入口',
  },
  fallbackLabel: 'Input',

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'sg_in',
      kind: 'subgraphInput',
      label: '',
      timeoutMs: null,
      config: {},
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    return normalizeNodeConfig('subgraphInput', config);
  },

  getNodeSize() {
    return { width: 120, height: 80 };
  },

  buildRegistryMeta() {
    return {
      defaultExpanded: true,
      size: this.getNodeSize(),
      defaultPorts: [{ type: 'output' as const }],
    };
  },

  validate(_ctx: NodeValidationContext): NodeValidation[] {
    return [];
  },
};
```

- [ ] **Step 3: 创建 subgraphOutput/index.ts**

```typescript
import {
  type NodeDefinition,
  type NodeSeed,
  type NodeValidationContext,
  type NodeValidation,
  normalizeNodeConfig,
} from '../shared';

export const definition: NodeDefinition = {
  kind: 'subgraphOutput',
  catalog: {
    category: '子图封装',
    description: '子图内部桥接出口',
  },
  fallbackLabel: 'Output',

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'sg_out',
      kind: 'subgraphOutput',
      label: '',
      timeoutMs: null,
      config: {},
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    return normalizeNodeConfig('subgraphOutput', config);
  },

  getNodeSize() {
    return { width: 120, height: 80 };
  },

  buildRegistryMeta() {
    return {
      defaultExpanded: true,
      size: this.getNodeSize(),
      defaultPorts: [{ type: 'input' as const }],
    };
  },

  validate(_ctx: NodeValidationContext): NodeValidation[] {
    return [];
  },
};
```

- [ ] **Step 4: 提交**

```bash
git add web/src/components/flowgram/nodes/subgraph/ web/src/components/flowgram/nodes/subgraphInput/ web/src/components/flowgram/nodes/subgraphOutput/
git commit -s -m "feat(frontend): 子图/桥接节点定义文件"
```

---

### Task 4: 节点注册（flowgram-node-library.ts）

**Files:**
- Modify: `web/src/components/flowgram/flowgram-node-library.ts`

- [ ] **Step 1: 导入子图节点定义**

在 `flowgram-node-library.ts` 的 import 区域（第 56-69 行的 `import { definition as ... }` 之后）添加：

```typescript
import { definition as subgraphDef } from './nodes/subgraph';
import { definition as subgraphInputDef } from './nodes/subgraphInput';
import { definition as subgraphOutputDef } from './nodes/subgraphOutput';
```

- [ ] **Step 2: 添加到 ALL_DEFS 数组**

在 `ALL_DEFS` 数组末尾添加（subgraphInput 和 subgraphOutput 不需要出现在面板中，但需要在注册表里）：

```typescript
const ALL_DEFS = [
  nativeDef, codeDef, timerDef, serialTriggerDef, modbusReadDef, mqttClientDef,
  ifDef, switchDef, tryCatchDef, loopDef,
  httpClientDef, barkPushDef, sqlWriterDef, debugConsoleDef,
  subgraphDef, subgraphInputDef, subgraphOutputDef,
];
```

- [ ] **Step 3: 添加面板条目**

在 `getFlowgramPaletteSections()` 的 sections 数组中，在 `'templates'` 之前添加新 section：

```typescript
{
  key: 'subgraph',
  title: '子图封装',
  items: [
    { key: 'blank-subgraph', title: 'Subgraph', description: subgraphDef.catalog.description, badge: 'Subgraph', seed: subgraphDef.buildDefaultSeed() },
  ],
},
```

- [ ] **Step 4: 运行前端构建验证**

Run: `npm --prefix web run build 2>&1 | tail -5`
Expected: 构建成功

- [ ] **Step 5: 提交**

```bash
git add web/src/components/flowgram/flowgram-node-library.ts
git commit -s -m "feat(frontend): 注册子图/桥接节点到节点面板"
```

---

### Task 5: 展平逻辑核心实现（flowgram.ts）

**Files:**
- Modify: `web/src/lib/flowgram.ts`

这是最核心的改动。在 `flowgram.ts` 中新增展平函数，并在 `toNazhWorkflowGraph` 入口调用。

- [ ] **Step 1: 扩展 FLOWGRAM_BUSINESS_NODE_TYPES**

在 `FLOWGRAM_BUSINESS_NODE_TYPES` 集合中添加三个新类型（展平后桥接节点需要被识别为业务节点）：

```typescript
const FLOWGRAM_BUSINESS_NODE_TYPES = new Set([
  'native',
  'code',
  'timer',
  'serialTrigger',
  'modbusRead',
  'if',
  'switch',
  'tryCatch',
  'loop',
  'httpClient',
  'barkPush',
  'sqlWriter',
  'debugConsole',
  'subgraphInput',
  'subgraphOutput',
]);
```

注意：`subgraph` 不加入此集合——展平后它被替换为内部节点，不进入最终 DAG。

- [ ] **Step 2: 添加参数替换函数**

在文件 `flowgram.ts` 中（`FLOWGRAM_BUSINESS_NODE_TYPES` 下方）添加：

```typescript
/**
 * 深度遍历 JSON value，对所有 string 值做 `{{paramName}}` 替换。
 * 未绑定的参数保留原值。
 */
function applyParameterBindings(
  value: unknown,
  params: Record<string, string | number | boolean>,
): unknown {
  if (typeof value === 'string') {
    return value.replace(/\{\{(\w+)\}\}/g, (match, key: string) => {
      if (key in params) {
        return String(params[key]);
      }
      return match;
    });
  }
  if (Array.isArray(value)) {
    return value.map((item) => applyParameterBindings(item, params));
  }
  if (isRecord(value)) {
    const result: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(value)) {
      result[k] = applyParameterBindings(v, params);
    }
    return result;
  }
  return value;
}
```

- [ ] **Step 3: 添加容器节点判断函数**

```typescript
/**
 * 判断 FlowGram 节点是否为子图容器。
 */
function isContainerNode(node: FlowgramWorkflowJSON['nodes'][number]): boolean {
  return node.type === 'subgraph';
}
```

- [ ] **Step 4: 添加查找桥接节点的辅助函数**

```typescript
interface BridgeNodes {
  inputNodes: FlowgramWorkflowJSON['nodes'];
  outputNodes: FlowgramWorkflowJSON['nodes'];
}

/**
 * 从容器内部 blocks 中找出桥接节点。
 */
function findBridgeNodes(blocks: FlowgramWorkflowJSON['nodes']): BridgeNodes {
  const inputNodes = blocks.filter((n) => n.type === 'subgraphInput');
  const outputNodes = blocks.filter((n) => n.type === 'subgraphOutput');
  return { inputNodes, outputNodes };
}
```

- [ ] **Step 5: 添加展平核心函数**

在文件中（`toNazhWorkflowGraph` 函数之前）添加核心展平函数：

```typescript
interface FlatGraph {
  nodes: FlowgramWorkflowJSON['nodes'];
  edges: FlowgramWorkflowJSON['edges'];
}

const MAX_SUBGRAPH_DEPTH = 8;

/**
 * 递归展平子图容器节点，返回纯平的 nodes + edges。
 *
 * 规则：
 * 1. 容器节点被移除，内部 blocks 递归展平后加入
 * 2. 内部节点 ID 加前缀 `<subgraph-id>/`
 * 3. 外部边重写：容器 input port → sg-in，容器 output port → sg-out
 * 4. `{{paramName}}` 被参数绑定替换
 * 5. 嵌套深度超过 8 层报错
 * 6. 检测循环引用（同一路径下 ID 重复）
 */
function flattenSubgraphs(
  flowgramGraph: FlowgramWorkflowJSON,
  depth = 0,
  ancestorIds: Set<string> = new Set(),
): FlatGraph {
  if (depth > MAX_SUBGRAPH_DEPTH) {
    throw new Error(`子图嵌套超过 ${MAX_SUBGRAPH_DEPTH} 层上限`);
  }

  const flatNodes: FlowgramWorkflowJSON['nodes'] = [];
  const flatEdges: FlowgramWorkflowJSON['edges'] = [];

  // 收集容器节点的桥接节点映射，用于重写外部边
  // key = containerNode.id, value = { inputNodeIds, outputNodeIds }
  const containerBridgeMap = new Map<
    string,
    { inputNodeIds: string[]; outputNodeIds: string[] }
  >();

  // 记录所有展平后的节点 ID（含前缀），用于边过滤
  const flatNodeIds = new Set<string>();

  for (const node of flowgramGraph.nodes) {
    if (isContainerNode(node)) {
      // 循环引用检测
      if (ancestorIds.has(node.id)) {
        throw new Error(`子图循环引用：${node.id}`);
      }

      const params = isRecord(node.data)
        ? ((node.data as Record<string, unknown>).parameterBindings ?? {})
        : {};
      const paramMap: Record<string, string | number | boolean> = {};
      if (isRecord(params)) {
        for (const [k, v] of Object.entries(params)) {
          if (typeof v === 'string' || typeof v === 'number' || typeof v === 'boolean') {
            paramMap[k] = v;
          }
        }
      }

      // 递归展平内部
      const innerGraph: FlowgramWorkflowJSON = {
        nodes: node.blocks ?? [],
        edges: node.edges ?? [],
      };
      const nextAncestorIds = new Set(ancestorIds);
      nextAncestorIds.add(node.id);
      const inner = flattenSubgraphs(innerGraph, depth + 1, nextAncestorIds);

      // 找到桥接节点（展平前）
      const { inputNodes, outputNodes } = findBridgeNodes(innerGraph.nodes);
      const prefixedInputIds = inputNodes.map((n) => `${node.id}/${n.id}`);
      const prefixedOutputIds = outputNodes.map((n) => `${node.id}/${n.id}`);

      containerBridgeMap.set(node.id, {
        inputNodeIds: prefixedInputIds,
        outputNodeIds: prefixedOutputIds,
      });

      // 内部节点加前缀 + 参数替换
      for (const innerNode of inner.nodes) {
        const prefixedNode = {
          ...innerNode,
          id: `${node.id}/${innerNode.id}`,
        };
        if (Object.keys(paramMap).length > 0) {
          prefixedNode.data = applyParameterBindings(prefixedNode.data, paramMap) as Record<
            string,
            unknown
          >;
        }
        flatNodes.push(prefixedNode);
        flatNodeIds.add(prefixedNode.id);
      }

      // 内部边加前缀
      for (const innerEdge of inner.edges) {
        flatEdges.push({
          ...innerEdge,
          sourceNodeID: `${node.id}/${innerEdge.sourceNodeID}`,
          targetNodeID: `${node.id}/${innerEdge.targetNodeID}`,
        });
      }
    } else {
      flatNodes.push(node);
      flatNodeIds.add(node.id);
    }
  }

  // 重写外部边
  for (const edge of flowgramGraph.edges) {
    let sourceId = edge.sourceNodeID;
    let targetId = edge.targetNodeID;
    let sourcePortID = edge.sourcePortID;
    let targetPortID = edge.targetPortID;

    // 源节点指向容器 → 改为指向容器的 input 桥接节点
    const sourceBridge = containerBridgeMap.get(sourceId);
    if (sourceBridge) {
      // 这是容器的 output → 外部
      // source 是容器，说明是从容器的 output 出来的
      const outputIds = sourceBridge.outputNodeIds;
      if (outputIds.length > 0) {
        sourceId = outputIds[0] ?? sourceId;
        sourcePortID = undefined;
      }
    }

    // 目标节点指向容器 → 改为指向容器的 input 桥接节点
    const targetBridge = containerBridgeMap.get(targetId);
    if (targetBridge) {
      const inputIds = targetBridge.inputNodeIds;
      if (inputIds.length > 0) {
        targetId = inputIds[0] ?? targetId;
        targetPortID = undefined;
      }
    }

    // 只添加两端都存在的边
    if (flatNodeIds.has(sourceId) && flatNodeIds.has(targetId)) {
      flatEdges.push({
        sourceNodeID: sourceId,
        targetNodeID: targetId,
        sourcePortID,
        targetPortID,
      });
    }
  }

  return { nodes: flatNodes, edges: flatEdges };
}
```

- [ ] **Step 6: 修改 toNazhWorkflowGraph 调用展平**

在 `toNazhWorkflowGraph` 函数体内，替换第 187 行：

原代码：
```typescript
export function toNazhWorkflowGraph(
  flowgramGraph: FlowgramWorkflowJSON,
  previousGraph: WorkflowGraph,
): WorkflowGraph {
  const businessNodes = flowgramGraph.nodes.filter(isBusinessNode);
```

改为：
```typescript
export function toNazhWorkflowGraph(
  flowgramGraph: FlowgramWorkflowJSON,
  previousGraph: WorkflowGraph,
): WorkflowGraph {
  // 展平子图容器节点为扁平 DAG
  const flat = flattenSubgraphs(flowgramGraph);
  const businessNodes = flat.nodes.filter(isBusinessNode);
```

并在 edges 处理处，把 `flowgramGraph.edges` 替换为 `flat.edges`：

原代码（约第 234 行）：
```typescript
    edges: flowgramGraph.edges
      .filter(
        (edge) => businessNodeIds.has(edge.sourceNodeID) && businessNodeIds.has(edge.targetNodeID),
      )
```

改为：
```typescript
    edges: flat.edges
      .filter(
        (edge) => businessNodeIds.has(edge.sourceNodeID) && businessNodeIds.has(edge.targetNodeID),
      )
```

- [ ] **Step 7: 运行已有测试确认不破坏现有功能**

Run: `npm --prefix web run test -- --run src/lib/__tests__/flowgram-to-nazh.test.ts`
Expected: 所有现有测试通过

- [ ] **Step 8: 提交**

```bash
git add web/src/lib/flowgram.ts
git commit -s -m "feat(flowgram): 子图展平逻辑——递归展平容器节点、参数替换、边重写"
```

---

### Task 6: 展平逻辑单元测试

**Files:**
- Create: `web/src/lib/__tests__/flowgram-subgraph.test.ts`

- [ ] **Step 1: 创建测试文件**

```typescript
// 子图展平逻辑单元测试
import { describe, expect, it } from 'vitest';
import { toNazhWorkflowGraph } from '../flowgram';
import type { WorkflowGraph } from '../../types';
import type { WorkflowJSON as FlowgramWorkflowJSON } from '@flowgram.ai/free-layout-editor';

function emptyGraph(): WorkflowGraph {
  return { nodes: {}, edges: [] };
}

describe('flattenSubgraphs（通过 toNazhWorkflowGraph）', () => {
  it('无子图时 pass-through 不变', () => {
    const flowgram: FlowgramWorkflowJSON = {
      nodes: [
        {
          id: 'a',
          type: 'native',
          data: { nodeType: 'native', config: { message: 'hello' } },
        },
        {
          id: 'b',
          type: 'code',
          data: { nodeType: 'code', config: { script: 'payload' } },
        },
      ],
      edges: [{ sourceNodeID: 'a', targetNodeID: 'b' }],
    };

    const result = toNazhWorkflowGraph(flowgram, emptyGraph());
    expect(Object.keys(result.nodes)).toHaveLength(2);
    expect(result.edges).toHaveLength(1);
  });

  it('单层子图展平——内部节点加前缀', () => {
    const flowgram: FlowgramWorkflowJSON = {
      nodes: [
        {
          id: 'timer-1',
          type: 'timer',
          data: {
            nodeType: 'timer',
            config: { interval_ms: 5000 },
          },
        },
        {
          id: 'sub-1',
          type: 'subgraph',
          data: { label: 'Test Subgraph', parameterBindings: {} },
          blocks: [
            {
              id: 'sg-in',
              type: 'subgraphInput',
              data: { nodeType: 'subgraphInput', config: {} },
            },
            {
              id: 'code-1',
              type: 'code',
              data: { nodeType: 'code', config: { script: 'payload' } },
            },
            {
              id: 'sg-out',
              type: 'subgraphOutput',
              data: { nodeType: 'subgraphOutput', config: {} },
            },
          ],
          edges: [
            { sourceNodeID: 'sg-in', targetNodeID: 'code-1' },
            { sourceNodeID: 'code-1', targetNodeID: 'sg-out' },
          ],
        },
        {
          id: 'sql-1',
          type: 'sqlWriter',
          data: {
            nodeType: 'sqlWriter',
            config: { database_path: './test.db', table: 'logs' },
          },
        },
      ],
      edges: [
        { sourceNodeID: 'timer-1', targetNodeID: 'sub-1' },
        { sourceNodeID: 'sub-1', targetNodeID: 'sql-1' },
      ],
    };

    const result = toNazhWorkflowGraph(flowgram, emptyGraph());

    // timer-1, sub-1/sg-in, sub-1/code-1, sub-1/sg-out, sql-1 = 5 节点
    expect(Object.keys(result.nodes)).toHaveLength(5);
    expect(result.nodes['timer-1']).toBeDefined();
    expect(result.nodes['sub-1/sg-in']).toBeDefined();
    expect(result.nodes['sub-1/code-1']).toBeDefined();
    expect(result.nodes['sub-1/sg-out']).toBeDefined();
    expect(result.nodes['sql-1']).toBeDefined();

    // timer → sg-in, sg-in → code-1, code-1 → sg-out, sg-out → sql = 4 边
    expect(result.edges).toHaveLength(4);

    // 验证外部边重写
    const edgeFromTimer = result.edges.find((e) => e.from === 'timer-1');
    expect(edgeFromTimer).toBeDefined();
    expect(edgeFromTimer!.to).toBe('sub-1/sg-in');

    const edgeToSql = result.edges.find((e) => e.to === 'sql-1');
    expect(edgeToSql).toBeDefined();
    expect(edgeToSql!.from).toBe('sub-1/sg-out');
  });

  it('嵌套子图（2 层）展平正确', () => {
    const flowgram: FlowgramWorkflowJSON = {
      nodes: [
        {
          id: 'outer',
          type: 'subgraph',
          data: { label: 'Outer', parameterBindings: {} },
          blocks: [
            {
              id: 'sg-in',
              type: 'subgraphInput',
              data: { nodeType: 'subgraphInput', config: {} },
            },
            {
              id: 'inner',
              type: 'subgraph',
              data: { label: 'Inner', parameterBindings: {} },
              blocks: [
                {
                  id: 'sg-in',
                  type: 'subgraphInput',
                  data: { nodeType: 'subgraphInput', config: {} },
                },
                {
                  id: 'native-1',
                  type: 'native',
                  data: { nodeType: 'native', config: { message: 'inner' } },
                },
                {
                  id: 'sg-out',
                  type: 'subgraphOutput',
                  data: { nodeType: 'subgraphOutput', config: {} },
                },
              ],
              edges: [
                { sourceNodeID: 'sg-in', targetNodeID: 'native-1' },
                { sourceNodeID: 'native-1', targetNodeID: 'sg-out' },
              ],
            },
            {
              id: 'sg-out',
              type: 'subgraphOutput',
              data: { nodeType: 'subgraphOutput', config: {} },
            },
          ],
          edges: [
            { sourceNodeID: 'sg-in', targetNodeID: 'inner' },
            { sourceNodeID: 'inner', targetNodeID: 'sg-out' },
          ],
        },
      ],
      edges: [],
    };

    const result = toNazhWorkflowGraph(flowgram, emptyGraph());

    // outer/sg-in, outer/inner/sg-in, outer/inner/native-1, outer/inner/sg-out, outer/sg-out = 5
    expect(Object.keys(result.nodes)).toHaveLength(5);
    expect(result.nodes['outer/inner/native-1']).toBeDefined();

    // 内部边保持（带前缀）
    const innerEdge = result.edges.find(
      (e) => e.from === 'outer/inner/sg-in' && e.to === 'outer/inner/native-1',
    );
    expect(innerEdge).toBeDefined();
  });

  it('嵌套超过 8 层报错', () => {
    // 构造 9 层嵌套
    let deepest: FlowgramWorkflowJSON = {
      nodes: [
        {
          id: 'sg-in',
          type: 'subgraphInput',
          data: { nodeType: 'subgraphInput', config: {} },
        },
      ],
      edges: [],
    };

    for (let i = 9; i >= 1; i--) {
      deepest = {
        nodes: [
          {
            id: `level-${i}`,
            type: 'subgraph',
            data: { label: `Level ${i}`, parameterBindings: {} },
            blocks: deepest.nodes,
            edges: deepest.edges,
          },
        ],
        edges: [],
      };
    }

    expect(() => toNazhWorkflowGraph(deepest, emptyGraph())).toThrow(
      '子图嵌套超过 8 层上限',
    );
  });

  it('参数替换——{{param}} 被绑定值替换', () => {
    const flowgram: FlowgramWorkflowJSON = {
      nodes: [
        {
          id: 'sub-1',
          type: 'subgraph',
          data: {
            label: 'Param Test',
            parameterBindings: { host: '192.168.1.10', port: 502 },
          },
          blocks: [
            {
              id: 'sg-in',
              type: 'subgraphInput',
              data: { nodeType: 'subgraphInput', config: {} },
            },
            {
              id: 'modbus-1',
              type: 'modbusRead',
              data: {
                nodeType: 'modbusRead',
                config: { host: '{{host}}', port: '{{port}}' },
              },
            },
            {
              id: 'sg-out',
              type: 'subgraphOutput',
              data: { nodeType: 'subgraphOutput', config: {} },
            },
          ],
          edges: [
            { sourceNodeID: 'sg-in', targetNodeID: 'modbus-1' },
            { sourceNodeID: 'modbus-1', targetNodeID: 'sg-out' },
          ],
        },
      ],
      edges: [],
    };

    const result = toNazhWorkflowGraph(flowgram, emptyGraph());
    const modbusConfig = result.nodes['sub-1/modbus-1']?.config as Record<string, unknown>;
    expect(modbusConfig.host).toBe('192.168.1.10');
    expect(modbusConfig.port).toBe('502');
  });

  it('未绑定参数保留原值', () => {
    const flowgram: FlowgramWorkflowJSON = {
      nodes: [
        {
          id: 'sub-1',
          type: 'subgraph',
          data: { label: 'Partial', parameterBindings: {} },
          blocks: [
            {
              id: 'code-1',
              type: 'code',
              data: {
                nodeType: 'code',
                config: { script: 'payload["x"] = "{{unbound}}"; payload' },
              },
            },
          ],
          edges: [],
        },
      ],
      edges: [],
    };

    const result = toNazhWorkflowGraph(flowgram, emptyGraph());
    const codeConfig = result.nodes['sub-1/code-1']?.config as Record<string, unknown>;
    expect(codeConfig.script).toBe('payload["x"] = "{{unbound}}"; payload');
  });

  it('空子图（无 blocks）不崩溃', () => {
    const flowgram: FlowgramWorkflowJSON = {
      nodes: [
        {
          id: 'sub-empty',
          type: 'subgraph',
          data: { label: 'Empty', parameterBindings: {} },
          blocks: [],
          edges: [],
        },
      ],
      edges: [],
    };

    const result = toNazhWorkflowGraph(flowgram, emptyGraph());
    // 空子图 = 没有内部节点产出
    expect(Object.keys(result.nodes)).toHaveLength(0);
  });

  it('子图无桥接节点时外部边无法重写但仍不崩溃', () => {
    const flowgram: FlowgramWorkflowJSON = {
      nodes: [
        {
          id: 'timer-1',
          type: 'timer',
          data: { nodeType: 'timer', config: { interval_ms: 1000 } },
        },
        {
          id: 'sub-no-bridge',
          type: 'subgraph',
          data: { label: 'No Bridge', parameterBindings: {} },
          blocks: [
            {
              id: 'code-1',
              type: 'code',
              data: { nodeType: 'code', config: { script: 'payload' } },
            },
          ],
          edges: [],
        },
      ],
      edges: [
        { sourceNodeID: 'timer-1', targetNodeID: 'sub-no-bridge' },
      ],
    };

    // 不应崩溃，但外部边可能被丢弃（没有桥接节点映射）
    const result = toNazhWorkflowGraph(flowgram, emptyGraph());
    expect(result.nodes['sub-no-bridge/code-1']).toBeDefined();
    // 外部边 timer → sub-no-bridge 无法重写（无桥接），会被过滤掉
    expect(result.edges).toHaveLength(0);
  });
});
```

- [ ] **Step 2: 运行测试**

Run: `npm --prefix web run test -- --run src/lib/__tests__/flowgram-subgraph.test.ts`
Expected: 所有测试通过

- [ ] **Step 3: 提交**

```bash
git add web/src/lib/__tests__/flowgram-subgraph.test.ts
git commit -s -m "test(flowgram): 子图展平逻辑单元测试——单层/嵌套/参数/边界"
```

---

### Task 7: Rust 集成测试

**Files:**
- Modify: `tests/workflow.rs`

- [ ] **Step 1: 添加含 PassthroughNode 的集成测试**

在 `tests/workflow.rs` 中添加测试函数（在已有测试之后）。需要先阅读文件了解测试 pattern。

测试逻辑：构建一个包含 `subgraphInput` 和 `subgraphOutput` 节点的扁平 DAG，验证它们能正确 passthrough payload。

```rust
#[tokio::test]
async fn passthrough_nodes_forward_payload() {
    let mut registry = nazh_engine::standard_registry();
    // subgraphInput / subgraphOutput 已在 standard_registry() 中通过 FlowPlugin 注册

    let graph_json = serde_json::json!({
        "nodes": {
            "native-1": {
                "id": "native-1",
                "type": "native",
                "config": { "message": "test-passthrough" },
                "buffer": 1
            },
            "sg-in": {
                "id": "sg-in",
                "type": "subgraphInput",
                "config": {},
                "buffer": 1
            },
            "sg-out": {
                "id": "sg-out",
                "type": "subgraphOutput",
                "config": {},
                "buffer": 1
            }
        },
        "edges": [
            { "from": "native-1", "to": "sg-in" },
            { "from": "sg-in", "to": "sg-out" }
        ]
    });

    // 解析并部署
    let ast = serde_json::to_string(&graph_json).unwrap();
    let graph = nazh_engine::WorkflowGraph::from_json(&ast).unwrap();
    let deployment = nazh_engine::deploy_workflow_with_ai(
        "test-passthrough",
        &graph,
        &registry,
        None,
        &[],
    ).await;

    assert!(deployment.is_ok(), "部署应成功: {:?}", deployment.err());
}
```

注意：实际测试代码需要根据 `tests/workflow.rs` 中已有的 helper 函数和 import pattern 来调整。

- [ ] **Step 2: 运行测试**

Run: `cargo test --test workflow passthrough`
Expected: 测试通过

- [ ] **Step 3: 提交**

```bash
git add tests/workflow.rs
git commit -s -m "test(workflow): PassthroughNode 集成测试"
```

---

### Task 8: ADR 状态更新 + 最终验证

**Files:**
- Modify: `docs/adr/0013-子图与宏系统.md`

- [ ] **Step 1: 更新 ADR 状态**

在 `docs/adr/0013-子图与宏系统.md` 中将状态从 `提议中` 改为 `已接受`：

```
- **状态**: 已接受
```

- [ ] **Step 2: 运行全量检查**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`
Expected: 全部通过

Run: `npm --prefix web run test -- --run`
Expected: 全部通过

- [ ] **Step 3: 提交**

```bash
git add docs/adr/0013-子图与宏系统.md
git commit -s -m "docs(adr-0013): 状态更新为已接受"
```

---

## Self-Review Checklist

**1. Spec coverage:**
- ✅ 数据模型（3 新节点类型）→ Task 2, 3
- ✅ 展平逻辑（递归 + ID 前缀 + 边重写）→ Task 5
- ✅ 参数替换 → Task 5
- ✅ 嵌套上限 8 层 → Task 5
- ✅ 循环引用检测 → Task 5
- ✅ PassthroughNode → Task 1
- ✅ 节点注册 → Task 4
- ✅ 测试 → Task 6, 7
- ✅ ADR 状态更新 → Task 8
- ✅ 模板库延后（不在 plan 中）

**2. Placeholder scan:** 无 TBD/TODO。

**3. Type consistency:**
- `NazhNodeKind` 扩展了 `subgraph`/`subgraphInput`/`subgraphOutput` → 所有引用处一致
- Rust `PassthroughNode::new` 签名匹配 `register_with_capabilities` 工厂闭包
- `FLOWGRAM_BUSINESS_NODE_TYPES` 包含 `subgraphInput`/`subgraphOutput` 但不包含 `subgraph`
- 测试中的 JSON 构造使用 `nodeType` 字段（匹配 FlowgramNodeData 接口）
