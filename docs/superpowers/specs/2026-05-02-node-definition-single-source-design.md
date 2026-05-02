# NodeDefinition 单一声明源设计

> **状态**: 设计完成（review 修订版）
> **日期**: 2026-05-02
> **动机**: commit `cc59350` 将 AI 编排器节点类型清单集中化到 `workflow-node-capabilities.ts`，但 humanLoop 节点暴露了机制缺口——前端仍有多处分散的 switch/map 需要手动同步。本次重构将 `NodeDefinition` 推为前端节点唯一的声明源，新增普通节点 = 写一个 `index.ts` + 加入 `ALL_DEFS`。
> **关联**: `docs/superpowers/specs/2026-05-02-node-type-contract-design.md`
> **修订原则**: 反过度设计。只把“节点身份、目录、默认配置、端口、palette、AI 提示”收拢到 definition；渲染细节、复杂 settings 表单、连接协议匹配不强行塞进 schema。

## 目标

本设计只处理 `nodeType` 宏观契约中的 **Known Editor NodeType**：也就是前端内置认识、能提供完整编辑体验的节点类型。Runtime NodeType、外部插件 nodeType、未知导入节点的长期语义以关联文档为准。

新增一个普通 Known Editor NodeType 时，前端只需：

1. 创建 `web/src/components/flowgram/nodes/<kind>/index.ts`，实现 `NodeDefinition`
2. 将 definition 加入 `flowgram-node-library.ts` 的 `ALL_DEFS`

以下路径自动响应，避免新增节点时漏同步：

- 类型识别（`normalizeNodeKind` / `isKnownNodeKind`）
- 默认标签（`getFallbackNodeLabel`）
- 配置规范化（`normalizeNodeConfig`）
- FlowGram 动态输出端口（兼容旧名 `getLogicNodeBranchDefinitions`）
- AI sourcePortId 路由提示（从 routing branch 派生，不混入普通 named output）
- 分类目录与 PluginPanel 展示（`getNodeCatalogInfo`）
- 拖拽面板（palette 分组、标题、badge、隐藏规则）
- AI 编排器节点提示（`ai.hint` / `ai.visible` / `ai.editorOnly`）
- 保存/部署侧业务节点白名单（继续从 `getAllNodeDefinitions()` 派生）

不承诺“零代码”的范围：

- 节点需要全新的 settings 表单时，仍需新增对应 `settings.tsx` 并在设置面板接入
- 节点需要全新的 glyph / 颜色时，仍需修改 `FlowgramNodeGlyph.tsx` / `FlowgramCanvas.tsx`
- 节点引入新的连接协议类型时，仍需扩展连接匹配逻辑
- Rust 侧节点注册、能力位图、pin schema、合约测试不在本设计内

## 模块边界

避免 `shared.ts` 反向读取 `DEF_MAP`。模块图固定为单向：

```text
nodes/shared.ts
  ↑
nodes/<kind>/index.ts
  ↑
flowgram-node-library.ts  -- owns ALL_DEFS / DEF_MAP / registry-derived helpers
  ↑
FlowgramCanvas / SettingsPanel / workflow-ai / PluginPanel / tests
```

职责划分：

- `nodes/shared.ts`: 只放类型、常量、无注册表依赖的纯 helper。不得 import `flowgram-node-library.ts` 或任何节点 definition。
- `nodes/<kind>/index.ts`: 只 import `shared.ts`，声明本节点 definition。
- `flowgram-node-library.ts`: 聚合所有 definition，构建 `ALL_DEFS` / `DEF_MAP`，导出所有 registry-derived 函数。
- 业务消费端统一从 `flowgram-node-library.ts` 读取节点能力，不直接维护节点名 switch/map。

## `NodeDefinition` 接口

`shared.ts` 保留泛型基础类型；具体 `NazhNodeKind` 由 `ALL_DEFS` 派生，避免手写联合类型。

```ts
export interface NodeSeed<K extends string = string> {
  idPrefix: string;
  kind: K;
  displayType?: string;
  label: string;
  connectionId?: string | null;
  timeoutMs?: number | null;
  config: {
    [key: string]: unknown;
  };
}

export interface FlowgramOutputPort {
  key: string;
  label: string;
  fixed?: boolean;
}

export type FlowgramLogicBranch = FlowgramOutputPort;

export interface NodeDefinition<K extends string = string> {
  kind: K;
  catalog: { category: NodeCategory; description: string };
  fallbackLabel: string;

  /** palette 只管拖拽面板展示；默认 visible=true、title=fallbackLabel、badge=fallbackLabel。 */
  palette?: {
    visible?: boolean;
    title?: string;
    badge?: string;
  };

  /** AI 编排可见性与提示；默认 visible=true、editorOnly=false。 */
  ai?: {
    visible?: boolean;
    editorOnly?: boolean;
    hint?: string;
  };

  requiresConnection?: boolean;
  fieldValidators?: Partial<Record<keyof SelectedNodeDraft, FieldValidator>>;

  buildDefaultSeed(): NodeSeed<K>;
  normalizeConfig(config: unknown): NodeSeed<K>['config'];

  /** FlowGram 动态输出端口；modbusRead 的 out/latest 属于这里。 */
  getOutputPorts?(config: unknown): FlowgramOutputPort[];

  /** 具有路由语义的 sourcePortId；AI 提示只读这里，不读普通 named output。 */
  getRoutingBranches?(config: unknown): FlowgramLogicBranch[];

  getNodeSize(): { width: number; height: number };
  buildRegistryMeta(): {
    defaultExpanded: boolean;
    isContainer?: boolean;
    size: { width: number; height: number };
    defaultPorts?: Array<{ type: 'input' | 'output' }>;
    useDynamicPort?: boolean;
    deleteDisable?: boolean;
    copyDisable?: boolean;
    padding?: (transform: unknown) => { top: number; bottom: number; left: number; right: number };
    selectable?: (node: unknown, mousePos?: unknown) => boolean;
    wrapperStyle?: Record<string, string>;
  };
  validate(ctx: NodeValidationContext): NodeValidation[];
}
```

节点文件用 `satisfies` 保留字面量类型：

```ts
export const definition = {
  kind: 'timer',
  // ...
} satisfies NodeDefinition<'timer'>;
```

## `NazhNodeKind` 从 `ALL_DEFS` 派生

不把 `NazhNodeKind` 改成裸 `string`。长期看，编译期拼写检查比启动时报错更便宜；启动时校验只做 backstop。

```ts
export const ALL_DEFS = [
  nativeDef,
  codeDef,
  timerDef,
  // ...
] as const;

export type NazhNodeKind = (typeof ALL_DEFS)[number]['kind'];
export type KnownNodeDefinition = (typeof ALL_DEFS)[number];

const DEF_MAP: ReadonlyMap<NazhNodeKind, KnownNodeDefinition> =
  new Map(ALL_DEFS.map((definition) => [definition.kind, definition]));
```

`flowgram-node-library.ts` 导出：

```ts
export function isKnownNodeKind(value: unknown): value is NazhNodeKind {
  return typeof value === 'string' && DEF_MAP.has(value as NazhNodeKind);
}

export function normalizeNodeKind(value: unknown): NazhNodeKind {
  return isKnownNodeKind(value) ? value : 'native';
}

export function getNodeDefinition(kind: NazhNodeKind): KnownNodeDefinition {
  return DEF_MAP.get(kind) ?? nativeDef;
}

export function getNodeCatalogInfo(kind: string): NodeCatalogInfo | null {
  return isKnownNodeKind(kind) ? getNodeDefinition(kind).catalog : null;
}
```

## 启动时 registry 校验

`validateNodeRegistry()` 在 `web/src/main.tsx` 渲染前执行一次；同一逻辑也由 Vitest 覆盖，避免浏览器入口没跑到时漏掉。

校验项：

- `kind` 非空、无重复
- `fallbackLabel` 非空
- `catalog.category` 属于 `NODE_CATEGORIES`
- `catalog.description` 非空
- `buildDefaultSeed().kind === def.kind`
- `palette.visible !== false` 时，palette title / badge 兜底后非空
- `getOutputPorts(defaultConfig)` 的 key 非空且不重复
- `getRoutingBranches(defaultConfig)` 的 key 非空且不重复
- `ai.visible` / `ai.editorOnly` 全部来自 definition，不再维护额外静态 Set
- `NODE_TEMPLATES` 中每个 seed.kind 都是已知节点类型

示意：

```ts
export function validateNodeRegistry(): void {
  const seen = new Set<string>();
  for (const def of ALL_DEFS) {
    if (!def.kind.trim()) throw new Error('NodeDefinition 缺少 kind');
    if (seen.has(def.kind)) throw new Error(`NodeDefinition kind 重复: ${def.kind}`);
    seen.add(def.kind);

    if (!NODE_CATEGORIES.includes(def.catalog.category)) {
      throw new Error(`${def.kind} 使用未知分类: ${def.catalog.category}`);
    }

    const seed = def.buildDefaultSeed();
    if (seed.kind !== def.kind) {
      throw new Error(`${def.kind} 的默认 seed.kind 不一致: ${seed.kind}`);
    }

    assertUniquePortKeys(def.kind, 'output', def.getOutputPorts?.(seed.config) ?? []);
    assertUniquePortKeys(def.kind, 'routing', def.getRoutingBranches?.(seed.config) ?? []);
  }
}
```

## Registry-derived 函数

这些函数放在 `flowgram-node-library.ts`，不放在 `shared.ts`：

```ts
export function getFallbackNodeLabel(kind: NazhNodeKind): string {
  return getNodeDefinition(kind).fallbackLabel;
}

export function normalizeNodeConfig(kind: NazhNodeKind, config: unknown): NodeSeed['config'] {
  return getNodeDefinition(kind).normalizeConfig(config);
}

export function getFlowgramOutputPorts(nodeType: unknown, config: unknown): FlowgramOutputPort[] {
  const kind = normalizeNodeKind(nodeType);
  return getNodeDefinition(kind).getOutputPorts?.(config) ?? [];
}

/** 旧名保留，避免一次性改动所有调用点；语义改为“动态输出端口”。 */
export function getLogicNodeBranchDefinitions(
  nodeType: unknown,
  config: unknown,
): FlowgramOutputPort[] {
  return getFlowgramOutputPorts(nodeType, config);
}
```

`resolveDefaultConnectionId`、`resolveNodeData`、`buildPaletteNodeJson`、`normalizeFlowgramNodeJson` 也迁到 `flowgram-node-library.ts`，因为它们消费 registry-derived normalize 函数。

## 输出端口与路由分支

拆开“画布输出端口”和“AI 路由语义”，避免把 modbusRead 的 `latest` 误当分支节点。

| 节点 | `getOutputPorts(config)` | `getRoutingBranches(config)` |
|------|---------------------------|-------------------------------|
| `if` | `IF_BRANCHES` | `IF_BRANCHES` |
| `switch` | `normalizeSwitchBranches(config.branches) + default` | 同左；AI hint 额外说明 branch key 来自 `config.branches[].key` |
| `tryCatch` | `TRYCATCH_BRANCHES` | `TRYCATCH_BRANCHES` |
| `humanLoop` | approve / reject | approve / reject |
| `modbusRead` | out / latest | `[]` |
| `loop` | `[]`（容器端口由 FlowGram 管理） | `LOOP_BRANCHES`（仅用于 AI / DAG sourcePortId 语义提示） |
| 其余 | `[]` | `[]` |

## Palette 自动生成

`getFlowgramPaletteSections()` 从 `ALL_DEFS` 按 `catalog.category` 分组，顺序使用现有 `NODE_CATEGORIES`，不再引入第二份 `CATEGORY_ORDER`。

隐藏规则：

- `def.palette?.visible === false` 不出现在 palette
- `subgraphInput` / `subgraphOutput` 在各自 definition 中声明 `palette: { visible: false }`

字段映射：

- `title` ← `def.palette?.title ?? def.fallbackLabel`
- `description` ← `def.catalog.description`
- `badge` ← `def.palette?.badge ?? def.fallbackLabel`
- `seed` ← `def.buildDefaultSeed()`

模板节点 section（`NODE_TEMPLATES`）保留为独立 section，但模板 seed.kind 必须通过 registry 校验。

## 目录与 PluginPanel

删除 `NODE_CATEGORY_MAP`，新增 registry-derived 查询：

```ts
export function getNodeCatalogInfo(kind: string): NodeCatalogInfo | null {
  return isKnownNodeKind(kind) ? getNodeDefinition(kind).catalog : null;
}
```

`PluginPanel.tsx` 改为：

- 对前端已知节点：使用 `getNodeCatalogInfo(nodeType.name)`
- 对 Rust-only / 第三方运行时节点：分类为 `其他`，description 为空或来自未来 IPC metadata
- 分组顺序：`NODE_CATEGORIES + ['其他']`

这样删除静态 map 后，PluginPanel 不丢分类；未来 Rust 插件节点也有明确降级路径。

## AI 能力目录免维护

删除 `NODE_AI_USAGE_HINTS`、`AI_HIDDEN_NODE_KINDS`、`AI_EDITOR_ONLY_NODE_KINDS`。这些信息搬入 definition：

```ts
ai: {
  visible: true,
  editorOnly: false,
  hint: 'config 可含 interval_ms, immediate, inject。',
}
```

默认值：

- `ai.visible !== false`
- `ai.editorOnly === true` 才标记编辑期节点
- `ai.hint` 可为空；为空时只输出 category、description、默认配置键、运行时 pin/capability

AI sourcePortId 规则单独生成，不再写死 `switch / if / tryCatch / loop`：

```ts
function buildWorkflowAiSourcePortGuideText(): string {
  return getAllNodeDefinitions()
    .flatMap((def) => {
      const seed = def.buildDefaultSeed();
      const branches = def.getRoutingBranches?.(seed.config) ?? [];
      if (branches.length === 0) return [];
      if (def.kind === 'switch') {
        return ['switch: sourcePortId 使用 config.branches[].key，兜底分支使用 default。'];
      }
      return [`${def.kind}: sourcePortId 只能是 ${branches.map((branch) => branch.key).join(' / ')}。`];
    })
    .join('\n');
}
```

`buildWorkflowAiNodeGuideText()` 继续合并运行时 `listNodeTypes` / `describeNodePins` 信息；definition 只提供前端本地可知的默认 hint。

## 现存硬编码处理

本轮要清掉会影响“新增节点被识别/可拖拽/可保存/AI 可见”的硬编码：

- `shared.ts` 中的 `normalizeNodeKind` / `getFallbackNodeLabel` / `normalizeNodeConfig` / `getLogicNodeBranchDefinitions`
- `flowgram-node-library.ts` 中手写 palette section
- `workflow-node-capabilities.ts` 中 `NODE_AI_USAGE_HINTS` 与 AI hidden/editorOnly Set
- `workflow-orchestrator.ts` 中 sourcePortId 节点名提示
- `PluginPanel.tsx` 中 `NODE_CATEGORY_MAP`
- `FlowgramCanvas.tsx` 中 `isBusinessFlowNode` 节点白名单

本轮保留但不再扩大：

- `FlowgramNodeGlyph.tsx` 的 glyph switch
- `FlowgramCanvas.tsx` 的节点颜色 / preview 文案
- `FlowgramNodeSettingsPanel.tsx` 的节点专属 settings 入口
- `settings-shared.ts` 的连接协议匹配与脚本节点判断

保留理由：这些是展示和编辑行为，不是节点身份声明。强行 schema 化会把设计做厚，后续可在需求变多时按真实重复度再抽象。

## 不改的范围

- **Rust 侧**：`NodeCapabilities` 注册、`NodeRegistry`、IPC 命令、合约测试 `src/registry.rs`
- **`NodeSeed.config` 类型**：保持 `[key: string]: unknown`，不做声明式 schema
- **模板节点**：`NODE_TEMPLATES` 保留为静态列表，但纳入 registry 校验
- **Settings 面板**：各节点 `settings.tsx` 不做 schema 化
- **FlowGram 容器渲染**：`FlowgramContainerCard` 的 subgraph/loop 分支逻辑不动

## 文件变动预估

| 操作 | 文件 | 变动 |
|------|------|------|
| 修改 | `web/src/components/flowgram/nodes/shared.ts` | 保留类型、常量、纯 helper；删除所有 registry-derived 函数 |
| 修改 | `web/src/components/flowgram/flowgram-node-library.ts` | `ALL_DEFS as const`、派生 `NazhNodeKind`、`DEF_MAP`、registry 校验、palette 自动生成、catalog 查询、normalize 包装函数 |
| 修改 | `web/src/components/flowgram/nodes/catalog.ts` | 删除 `NODE_CATEGORY_MAP`，保留 `NODE_CATEGORIES` / `NodeCategory` |
| 修改 | `web/src/lib/workflow-node-capabilities.ts` | 读取 `definition.ai`，删除 AI hint / hidden / editorOnly 静态表 |
| 修改 | `web/src/lib/workflow-orchestrator.ts` | sourcePortId 指南改为读取 `getRoutingBranches` |
| 修改 | `web/src/components/app/PluginPanel.tsx` | 改用 `getNodeCatalogInfo`，未知运行时节点落到 `其他` |
| 修改 | `web/src/components/FlowgramCanvas.tsx` | `isBusinessFlowNode` 改用 `isKnownNodeKind` |
| 修改 | 每个 `web/src/components/flowgram/nodes/*/index.ts` | 搬入 `normalizeConfig` 逻辑，声明 palette / ai / output ports / routing branches |
| 修改 | `web/src/main.tsx` | 调用 `validateNodeRegistry()` |
| 修改 | 受影响测试 | 增加 registry contract test，更新 normalize / palette / AI guide 断言 |

## 实施顺序

1. **收紧模块边界**：把 `NodeSeed` / `NodeDefinition` 改成泛型基础类型；`shared.ts` 仍保留旧函数，先不改行为。
2. **补 definition 字段**：各节点加 `palette` / `ai` / `getOutputPorts` / `getRoutingBranches`，先让测试覆盖新字段。
3. **迁移配置规范化**：逐个节点把 `normalizeConfig` 从全局函数搬回 definition。
4. **建立 registry-derived API**：在 `flowgram-node-library.ts` 派生 `NazhNodeKind`，实现 `isKnownNodeKind`、`normalizeNodeKind`、`normalizeNodeConfig`、`getNodeCatalogInfo`、端口查询。
5. **替换消费端**：更新 palette、PluginPanel、AI catalog、orchestrator sourcePortId 指南、FlowgramCanvas 白名单。
6. **清理旧表**：删除 `NODE_CATEGORY_MAP`、`NODE_AI_USAGE_HINTS`、AI hidden/editorOnly Set、`shared.ts` 中的 registry-derived 函数。
7. **验收**：跑 `npm --prefix web run test`，重点覆盖 registry contract、palette、AI guide、flowgram-to-nazh；前端改动后再跑 `npm --prefix web run build`。

每一步都应保持可编译；不要一次性大搬迁后再调错。
