# 插件管理页 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 新增"插件管理"侧栏页面，只读展示引擎中已注册的所有节点类型，按分类分组以卡片列表形式呈现。

**Architecture:** Rust 引擎新增 `registered_types_with_aliases()` 方法，Tauri shell 新增 `list_node_types` IPC 命令。前端通过 IPC 获取节点列表，与本地分类映射表合并后按分组卡片列表渲染。

**Tech Stack:** Rust (serde, ts-rs), Tauri v2 IPC, React 18, TypeScript

---

### Task 1: Rust — IPC 响应类型

**Files:**
- Modify: `src/ipc.rs`

- [ ] **Step 1: 在 `src/ipc.rs` 末尾新增 `NodeTypeEntry` 和 `ListNodeTypesResponse`**

```rust
/// 已注册节点类型的信息条目。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct NodeTypeEntry {
    /// 节点类型主名称（如 "rhai"）。
    pub name: String,
    /// 别名列表（如 ["code", "code/rhai"]）。
    #[serde(default)]
    pub aliases: Vec<String>,
}

/// `list_node_types` IPC 命令的响应。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ListNodeTypesResponse {
    pub types: Vec<NodeTypeEntry>,
}
```

- [ ] **Step 2: 在 `src/lib.rs` 中导出新类型**

在 `src/lib.rs` 第 46 行的 `pub use ipc::` 语句中追加 `ListNodeTypesResponse, NodeTypeEntry`：

```rust
pub use ipc::{DeployResponse, DispatchResponse, ListNodeTypesResponse, NodeTypeEntry, UndeployResponse};
```

- [ ] **Step 3: 运行 `cargo check` 验证编译**

Run: `cargo check`
Expected: 编译成功，无错误

---

### Task 2: Rust — NodeRegistry 别名分组方法

**Files:**
- Modify: `src/registry.rs`
- Test: 内联 `#[cfg(test)]` 模块

- [ ] **Step 1: 在 `src/registry.rs` 中新增 `registered_types_with_aliases` 方法**

在 `impl NodeRegistry` 块中（`registered_types` 方法之后）新增：

```rust
    /// 返回已注册节点类型的列表，按工厂函数指针去重合并别名。
    ///
    /// 同一个工厂函数被注册为多个名称时，选择最短名称作为主名称，
    /// 其余名称作为别名。
    pub fn registered_types_with_aliases(&self) -> Vec<crate::ipc::NodeTypeEntry> {
        use std::collections::HashMap as StdMap;
        let mut factory_groups: StdMap<usize, Vec<String>> = StdMap::new();
        for (name, factory) in &self.factories {
            let key = Arc::as_ptr(factory) as usize;
            factory_groups.entry(key).or_default().push(name.clone());
        }

        let mut entries: Vec<crate::ipc::NodeTypeEntry> = factory_groups
            .into_values()
            .map(|mut names| {
                names.sort_by(|a, b| a.len().cmp(&b.len()).then(a.cmp(b)));
                let name = names.remove(0);
                crate::ipc::NodeTypeEntry {
                    name,
                    aliases: names,
                }
            })
            .collect();

        entries.sort_by(|a, b| a.name.cmp(&b.name));
        entries
    }
```

- [ ] **Step 2: 在 `src/registry.rs` 底部添加测试模块**

```rust
#[cfg(test)]
mod tests {
    use super::NodeRegistry;

    #[test]
    fn registered_types_with_aliases_groups_aliases() {
        let mut registry = NodeRegistry::new();
        registry.register("rhai", |_, _| -> Result<_, crate::EngineError> { todo!() });
        let _ = registry.alias("code", "rhai");
        let _ = registry.alias("code/rhai", "rhai");

        registry.register("native", |_, _| -> Result<_, crate::EngineError> { todo!() });
        let _ = registry.alias("log", "native");

        registry.register("timer", |_, _| -> Result<_, crate::EngineError> { todo!() });

        let entries = registry.registered_types_with_aliases();

        assert_eq!(entries.len(), 3);

        let rhai_entry = entries.iter().find(|e| e.name == "rhai").unwrap();
        assert_eq!(rhai_entry.aliases, vec!["code", "code/rhai"]);

        let native_entry = entries.iter().find(|e| e.name == "native").unwrap();
        assert_eq!(native_entry.aliases, vec!["log"]);

        let timer_entry = entries.iter().find(|e| e.name == "timer").unwrap();
        assert!(timer_entry.aliases.is_empty());
    }

    #[test]
    fn registered_types_with_aliases_empty_registry() {
        let registry = NodeRegistry::new();
        let entries = registry.registered_types_with_aliases();
        assert!(entries.is_empty());
    }
}
```

- [ ] **Step 3: 运行测试**

Run: `cargo test registered_types_with_aliases`
Expected: 两个测试均 PASS

---

### Task 3: Rust — Tauri IPC 命令 + 类型导出

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: 在 `src-tauri/src/lib.rs` 导入中追加新类型**

在文件顶部的 `use nazh_engine::` 块（约第 13-18 行）中，追加 `ListNodeTypesResponse`, `NodeTypeEntry`：

```rust
use nazh_engine::{
    deploy_workflow as deploy_workflow_graph, shared_connection_manager, ConnectionDefinition,
    ConnectionRecord, DeployResponse, DispatchResponse, EngineError, ExecutionEvent,
    ListNodeTypesResponse, NodeRegistry, NodeTypeEntry, SerialTriggerNodeConfig, TimerNodeConfig,
    UndeployResponse, WorkflowContext, WorkflowGraph, WorkflowIngress,
};
```

- [ ] **Step 2: 在 `list_connections` 命令之后新增 `list_node_types` 命令**

在 `src-tauri/src/lib.rs` 中 `list_connections` 函数（约第 1281-1285 行）之后添加：

```rust
#[tauri::command]
async fn list_node_types() -> Result<ListNodeTypesResponse, String> {
    let registry = NodeRegistry::with_standard_nodes();
    Ok(ListNodeTypesResponse {
        types: registry.registered_types_with_aliases(),
    })
}
```

- [ ] **Step 3: 在 `generate_handler!` 宏中注册新命令**

在 `src-tauri/src/lib.rs` 第 2751-2773 行的 `tauri::generate_handler![...]` 列表中追加 `list_node_types`（在 `list_connections` 之后）：

```rust
        .invoke_handler(tauri::generate_handler![
            deploy_workflow,
            dispatch_payload,
            undeploy_workflow,
            list_connections,
            list_node_types,
            list_runtime_workflows,
            ...
        ]);
```

- [ ] **Step 4: 运行 `cargo check` 验证编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 编译成功

---

### Task 4: Rust — ts-rs 类型生成

**Files:**
- Generate: `web/src/generated/NodeTypeEntry.ts`
- Generate: `web/src/generated/ListNodeTypesResponse.ts`

- [ ] **Step 1: 运行 ts-rs 导出**

Run: `TS_RS_EXPORT_DIR=web/src/generated cargo test --lib export_bindings`
Expected: 两个新文件生成于 `web/src/generated/`

- [ ] **Step 2: 验证生成的类型文件内容**

Run: `ls web/src/generated/NodeTypeEntry.ts web/src/generated/ListNodeTypesResponse.ts`
Expected: 两个文件存在

- [ ] **Step 3: Commit**

```bash
git add src/ipc.rs src/lib.rs src/registry.rs src-tauri/src/lib.rs web/src/generated/NodeTypeEntry.ts web/src/generated/ListNodeTypesResponse.ts
git commit -s -m "feat: 新增 list_node_types IPC 命令，暴露节点注册表到前端"
```

---

### Task 5: 前端 — 分类映射 + Tauri 通信

**Files:**
- Create: `web/src/lib/node-catalog.ts`
- Modify: `web/src/lib/tauri.ts`
- Modify: `web/src/types.ts`

- [ ] **Step 1: 创建 `web/src/lib/node-catalog.ts`**

```typescript
/** 节点类型分类标签枚举（按展示顺序排列）。 */
export const NODE_CATEGORIES = [
  '流程控制',
  '脚本执行',
  '数据注入',
  '硬件接口',
  '外部通信',
  '持久化',
  '调试工具',
] as const;

export type NodeCategory = (typeof NODE_CATEGORIES)[number];

/** 节点主名称到分类元数据的静态映射。 */
export const NODE_CATEGORY_MAP: Record<
  string,
  { category: NodeCategory; description: string }
> = {
  if: { category: '流程控制', description: '布尔条件分支路由' },
  switch: { category: '流程控制', description: '多路分支路由' },
  tryCatch: { category: '流程控制', description: '脚本异常捕获路由' },
  loop: { category: '流程控制', description: '循环迭代与逐项分发' },
  rhai: { category: '脚本执行', description: '沙箱化 Rhai 脚本执行' },
  native: { category: '数据注入', description: '打印 payload 元数据，可选附加连接上下文' },
  timer: { category: '硬件接口', description: '按固定间隔触发工作流并注入计时元数据' },
  serialTrigger: {
    category: '硬件接口',
    description: '接收串口外设数据流并触发工作流',
  },
  modbusRead: {
    category: '硬件接口',
    description: '读取 Modbus 寄存器并将遥测数据写入 payload',
  },
  httpClient: { category: '外部通信', description: '将 payload 发送到 HTTP 端点' },
  sqlWriter: { category: '持久化', description: '将当前 payload 持久化到本地 SQLite 表' },
  debugConsole: { category: '调试工具', description: '将 payload 打印到调试控制台以供检查' },
};
```

- [ ] **Step 2: 在 `web/src/types.ts` 中重新导出生成类型**

在 `web/src/types.ts` 第 5-14 行的 `export type {}` 块中追加 `ListNodeTypesResponse`, `NodeTypeEntry`：

```typescript
export type {
  ConnectionDefinition,
  DeployResponse,
  DispatchResponse,
  ExecutionEvent,
  JsonValue,
  ListNodeTypesResponse,
  NodeTypeEntry,
  UndeployResponse,
  WorkflowContext,
  WorkflowEdge,
} from './generated';
```

- [ ] **Step 3: 在 `web/src/lib/tauri.ts` 中新增 `listNodeTypes` 函数**

在 `web/src/lib/tauri.ts` 的导入中追加 `ListNodeTypesResponse`，然后在 `listConnections` 函数之后（约第 366 行）添加：

```typescript
export async function listNodeTypes(): Promise<ListNodeTypesResponse> {
  return invoke<ListNodeTypesResponse>('list_node_types');
}
```

- [ ] **Step 4: Commit**

```bash
git add web/src/lib/node-catalog.ts web/src/types.ts web/src/lib/tauri.ts
git commit -s -m "feat: 新增节点分类映射和 listNodeTypes IPC 封装"
```

---

### Task 6: 前端 — 路由注册

**Files:**
- Modify: `web/src/components/app/types.ts`
- Modify: `web/src/lib/sidebar.ts`
- Modify: `web/src/App.tsx`

- [ ] **Step 1: 在 `web/src/components/app/types.ts` 的 `SidebarSection` 类型中追加 `'plugins'`**

将 `SidebarSection` 类型（约第 16-24 行）修改为：

```typescript
export type SidebarSection =
  | 'dashboard'
  | 'boards'
  | 'runtime'
  | 'connections'
  | 'plugins'
  | 'payload'
  | 'logs'
  | 'settings'
  | 'about';
```

- [ ] **Step 2: 在 `web/src/lib/sidebar.ts` 中新增 plugins 条目**

在 `buildSidebarSections` 函数返回的数组中，在 `connections` 条目之后（约第 41 行之后）插入：

```typescript
    {
      key: 'plugins',
      group: 'main',
      label: '插件管理',
      badge: '节点类型',
    },
```

- [ ] **Step 3: 在 `web/src/App.tsx` 中添加 plugins 路由**

首先在 `App.tsx` 顶部 import 中追加：

```typescript
import { PluginPanel } from './components/app/PluginPanel';
```

然后在 `renderStudioContent` 函数的 switch 中，在 `case 'connections':` 之后（约第 1626 行之后）添加：

```typescript
      case 'plugins':
        return (
          <section className="studio-content studio-content--panel">
            <div className="panel studio-content__panel studio-content__panel--scroll">
              <PluginPanel isTauriRuntime={isTauriRuntime} />
            </div>
          </section>
        );
```

- [ ] **Step 4: Commit**

```bash
git add web/src/components/app/types.ts web/src/lib/sidebar.ts web/src/App.tsx
git commit -s -m "feat: 注册插件管理页路由到侧栏导航"
```

---

### Task 7: 前端 — PluginPanel 组件

**Files:**
- Create: `web/src/components/app/PluginPanel.tsx`
- Modify: `web/src/styles.css`

- [ ] **Step 1: 创建 `web/src/components/app/PluginPanel.tsx`**

```tsx
import { useEffect, useMemo, useState } from 'react';

import type { NodeTypeEntry } from '../../types';
import { listNodeTypes, hasTauriRuntime } from '../../lib/tauri';
import {
  NODE_CATEGORIES,
  NODE_CATEGORY_MAP,
  type NodeCategory,
} from '../../lib/node-catalog';

interface PluginPanelProps {
  isTauriRuntime: boolean;
}

interface PluginDisplayEntry {
  name: string;
  aliases: string[];
  category: string;
  description: string;
}

export function PluginPanel({ isTauriRuntime }: PluginPanelProps) {
  const [entries, setEntries] = useState<PluginDisplayEntry[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!hasTauriRuntime()) {
      setIsLoading(false);
      return;
    }

    let cancelled = false;

    const load = async () => {
      try {
        const response = await listNodeTypes();
        if (cancelled) return;

        const displayEntries: PluginDisplayEntry[] = response.types.map(
          (nodeType: NodeTypeEntry) => {
            const meta = NODE_CATEGORY_MAP[nodeType.name];
            return {
              name: nodeType.name,
              aliases: nodeType.aliases,
              category: meta?.category ?? '其他',
              description: meta?.description ?? '',
            };
          },
        );

        setEntries(displayEntries);
        setError(null);
      } catch (err) {
        if (cancelled) return;
        setError(err instanceof Error ? err.message : '加载节点类型失败');
      } finally {
        if (!cancelled) {
          setIsLoading(false);
        }
      }
    };

    void load();
    return () => {
      cancelled = true;
    };
  }, []);

  const grouped = useMemo(() => {
    const groups = new Map<string, PluginDisplayEntry[]>();

    const allCategories: string[] = [
      ...(NODE_CATEGORIES as readonly string[]),
      '其他',
    ];
    for (const cat of allCategories) {
      groups.set(cat, []);
    }

    for (const entry of entries) {
      const list = groups.get(entry.category);
      if (list) {
        list.push(entry);
      } else {
        let other = groups.get('其他');
        if (!other) {
          other = [];
          groups.set('其他', other);
        }
        other.push(entry);
      }
    }

    return allCategories
      .map((cat) => ({ category: cat, items: groups.get(cat) ?? [] }))
      .filter((group) => group.items.length > 0);
  }, [entries]);

  const totalTypes = entries.length;
  const categoryCount = grouped.length;

  if (!isTauriRuntime) {
    return (
      <>
        <div className="panel__header panel__header--desktop window-safe-header" data-window-drag-region>
          <div>
            <h2>插件管理</h2>
          </div>
        </div>
        <div className="plugin-panel__empty">
          <p>浏览器预览模式下无法读取引擎节点注册表。</p>
          <p>请在 Tauri 桌面应用中查看已注册的节点类型插件。</p>
        </div>
      </>
    );
  }

  return (
    <>
      <div className="panel__header panel__header--desktop window-safe-header" data-window-drag-region>
        <div>
          <h2>插件管理</h2>
          <span className="panel__header-badge">
            共 {totalTypes} 个节点类型 · {categoryCount} 个分类
          </span>
        </div>
      </div>

      {isLoading && (
        <div className="plugin-panel__loading">
          <p>正在加载节点类型列表…</p>
        </div>
      )}

      {error && (
        <div className="plugin-panel__error">
          <p>加载失败: {error}</p>
        </div>
      )}

      {!isLoading && !error && (
        <div className="plugin-panel__groups">
          {grouped.map((group) => (
            <div key={group.category} className="plugin-panel__group">
              <h3 className="plugin-panel__group-title">{group.category}</h3>
              <div className="plugin-panel__grid">
                {group.items.map((item) => (
                  <div key={item.name} className="plugin-panel__card">
                    <div className="plugin-panel__card-name">{item.name}</div>
                    {item.aliases.length > 0 && (
                      <div className="plugin-panel__card-aliases">
                        别名: {item.aliases.join(', ')}
                      </div>
                    )}
                    {item.description && (
                      <div className="plugin-panel__card-desc">{item.description}</div>
                    )}
                  </div>
                ))}
              </div>
            </div>
          ))}
        </div>
      )}
    </>
  );
}
```

- [ ] **Step 2: 在 `web/src/styles.css` 末尾追加插件管理页样式**

```css
/* ── 插件管理页 ───────────────────────────────────── */

.plugin-panel__groups {
  display: flex;
  flex-direction: column;
  gap: 18px;
  padding: 4px 2px;
}

.plugin-panel__group-title {
  font-size: var(--font-callout);
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--muted);
  margin: 0 0 8px;
}

.plugin-panel__grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
  gap: 8px;
}

.plugin-panel__card {
  background: var(--surface-muted);
  border: 1px solid var(--line-soft);
  border-radius: var(--button-radius);
  padding: 10px 12px;
  transition: border-color 0.15s;
}

.plugin-panel__card:hover {
  border-color: var(--accent-border);
}

.plugin-panel__card-name {
  font-size: var(--font-title-3);
  font-weight: 600;
  color: var(--text);
}

.plugin-panel__card-aliases {
  font-size: var(--font-subheadline);
  color: var(--muted);
  margin-top: 2px;
}

.plugin-panel__card-desc {
  font-size: var(--font-callout);
  color: var(--text-soft);
  margin-top: 4px;
}

.plugin-panel__loading,
.plugin-panel__error,
.plugin-panel__empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  min-height: 200px;
  color: var(--muted);
  font-size: var(--font-body);
  text-align: center;
  gap: 8px;
}

.plugin-panel__error {
  color: var(--danger-ink);
}
```

- [ ] **Step 3: Commit**

```bash
git add web/src/components/app/PluginPanel.tsx web/src/styles.css
git commit -s -m "feat: 新增插件管理页组件 PluginPanel"
```

---

### Task 8: 验证

- [ ] **Step 1: 运行 Rust lint 和 format 检查**

Run: `cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings`
Expected: 无输出（通过）

- [ ] **Step 2: 运行 Rust 全量测试**

Run: `cargo test`
Expected: 所有测试通过

- [ ] **Step 3: 运行前端构建检查**

Run: `npm --prefix web run build`
Expected: 构建成功，无 TypeScript 错误

- [ ] **Step 4: 运行前端单元测试**

Run: `npm --prefix web run test -- --run`
Expected: 所有测试通过
