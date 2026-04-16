# 插件管理页设计

## 概述

在 Nazh 桌面应用中新增"插件管理"侧栏页面，以分组卡片列表的形式只读展示当前引擎中已注册的所有节点类型。数据通过新增的 Rust IPC 命令从 `NodeRegistry` 动态获取，分类标签由前端静态映射表维护。

## 需求

- 只读展示所有已注册的节点类型插件
- 每个插件卡片展示：名称、别名、分类标签
- 按分类分组排列（流程控制、脚本执行、数据注入、硬件接口、外部通信、持久化、调试工具）
- 通过 Rust IPC 动态查询注册表，不硬编码节点列表

## 方案选择

采用**轻量 IPC + 前端分类映射**方案：

- Rust 侧只暴露节点名称和别名列表
- 分类标签在前端静态映射，不污染引擎核心
- 新增节点类型时需同步更新前端映射表

备选方案（已否决）：在 `NodeRegistry` 注册接口中扩展元数据字段——变更面大，对只读展示页过度设计。

## 架构变更

### Rust 引擎层（`src/`）

**`src/ipc.rs`** — 新增两个 ts-rs 导出类型：

```rust
#[derive(TS, Serialize)]
#[ts(export)]
pub struct NodeTypeEntry {
    pub name: String,
    pub aliases: Vec<String>,
}

#[derive(TS, Serialize)]
#[ts(export)]
pub struct ListNodeTypesResponse {
    pub types: Vec<NodeTypeEntry>,
}
```

**`src/registry.rs`** — 新增方法 `registered_types_with_aliases()`：
- 遍历 `factories` HashMap
- 按 `Arc<FactoryFn>` 指针地址去重（同一个工厂的多个注册名归为一组）
- 每组选最短名称作为主名称，其余为别名
- 返回 `Vec<NodeTypeEntry>`

### Tauri Shell 层（`src-tauri/`）

**`src-tauri/src/lib.rs`** — 新增 IPC 命令：

```rust
#[tauri::command]
fn list_node_types(state: State<AppState>) -> Result<ListNodeTypesResponse, String>
```

从 `AppState` 中持有的 `NodeRegistry` 调用 `registered_types_with_aliases()`。

### 前端层（`web/src/`）

**类型生成**：`web/src/generated/` 中自动生成 `NodeTypeEntry` 和 `ListNodeTypesResponse` 的 TypeScript 类型（ts-rs）。

**分类映射** — 新建 `web/src/lib/node-catalog.ts`：

```typescript
export const NODE_CATEGORIES = [
  '流程控制', '脚本执行', '数据注入',
  '硬件接口', '外部通信', '持久化', '调试工具',
] as const;

export const NODE_CATEGORY_MAP: Record<string, { category: string; description: string }> = {
  if:           { category: '流程控制', description: '布尔条件分支路由' },
  switch:       { category: '流程控制', description: '多路分支路由' },
  tryCatch:     { category: '流程控制', description: '脚本异常捕获路由' },
  loop:         { category: '流程控制', description: '循环迭代与逐项分发' },
  rhai:         { category: '脚本执行', description: '沙箱化 Rhai 脚本执行' },
  native:       { category: '数据注入', description: '打印 payload 元数据，可选附加连接上下文' },
  timer:        { category: '硬件接口', description: '按固定间隔触发工作流并注入计时元数据' },
  serialTrigger:{ category: '硬件接口', description: '接收串口外设数据流并触发工作流' },
  modbusRead:   { category: '硬件接口', description: '读取 Modbus 寄存器并将遥测数据写入 payload' },
  httpClient:   { category: '外部通信', description: '将 payload 发送到 HTTP 端点' },
  sqlWriter:    { category: '持久化',   description: '将当前 payload 持久化到本地 SQLite 表' },
  debugConsole: { category: '调试工具', description: '将 payload 打印到调试控制台以供检查' },
};
```

**Tauri 通信** — `web/src/lib/tauri.ts` 新增：

```typescript
export async function listNodeTypes(): Promise<ListNodeTypesResponse> { ... }
```

**新增组件** — `web/src/components/app/PluginPanel.tsx`：

Props：
```typescript
interface PluginPanelProps {
  isTauriRuntime: boolean;
}
```

行为：
- 页面加载时调用 `listNodeTypes()`
- 将返回数据与 `NODE_CATEGORY_MAP` 合并
- 未在映射表中的节点归入"其他"分类
- 按 `NODE_CATEGORIES` 顺序分组渲染
- 每组：标题 + 2 列网格卡片
- 每张卡片：名称、别名标签（如有）、描述
- 非 Tauri 运行时显示预览态提示

**路由注册**：
- `types.ts`：`SidebarSection` 联合类型新增 `'plugins'`
- `sidebar.ts`：在 `connections` 条目后插入 `{ key: 'plugins', group: 'main', label: '插件管理', badge: '...' }`
- `App.tsx`：`renderStudioContent` switch 新增 `case 'plugins'`

## UI 布局

分组卡片列表（方案 A）：

```
┌─────────────────────────────────┐
│ 插件管理  共 12 个节点类型 · 7 分类 │
├─────────────────────────────────┤
│ 流程控制                         │
│ ┌──────────┐ ┌──────────┐      │
│ │ if       │ │ switch   │      │
│ │ 条件分支  │ │ 多路分支  │      │
│ └──────────┘ └──────────┘      │
│ ┌──────────┐ ┌──────────┐      │
│ │ tryCatch │ │ loop     │      │
│ │ 异常捕获  │ │ 循环分发  │      │
│ └──────────┘ └──────────┘      │
│                                 │
│ 脚本执行                         │
│ ┌──────────────────────┐       │
│ │ rhai  别名: code      │       │
│ │ 沙箱化脚本执行         │       │
│ └──────────────────────┘       │
│ ...                             │
└─────────────────────────────────┘
```

## 文件变更清单

| 文件 | 操作 |
|------|------|
| `src/ipc.rs` | 新增 `NodeTypeEntry`、`ListNodeTypesResponse` |
| `src/registry.rs` | 新增 `registered_types_with_aliases()` 方法 |
| `src-tauri/src/lib.rs` | 新增 `list_node_types` 命令 |
| `web/src/lib/node-catalog.ts` | 新建，分类映射表 |
| `web/src/lib/tauri.ts` | 新增 `listNodeTypes()` |
| `web/src/components/app/PluginPanel.tsx` | 新建，页面组件 |
| `web/src/components/app/types.ts` | `SidebarSection` 新增 `'plugins'` |
| `web/src/lib/sidebar.ts` | 新增 plugins 条目 |
| `web/src/App.tsx` | 新增 case 'plugins' 路由 |
| `web/src/styles.css` | 新增插件管理页样式 |

## 测试

- Rust 单元测试：`registered_types_with_aliases()` 返回正确的分组和别名
- Rust 单元测试：`list_node_types` IPC 命令序列化正确
- 前端：预览模式下（非 Tauri）显示降级提示
- 手动验证：侧栏导航、分组渲染、别名展示
