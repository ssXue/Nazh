# Nazh 前端测试体系 + App.tsx 重构设计

## 目标

为 Nazh 前端建立两层测试体系（Vitest 单元 + Playwright E2E），同时将 App.tsx 从 1436 行重构至 ~550 行以提升可测性和可维护性。

## 约束

- 每个新文件目标 ~100 行，不超过 200 行
- E2E 测试运行完整 Tauri 桌面窗口（不做浏览器模式 mock）
- E2E 须能在 GitHub Actions Linux runner 上通过 `xvfb-run` 运行
- 中文注释规范不变

---

## 一、App.tsx 重构

### 1.1 提取 5 个纯函数模块

| 新文件 | 来源行号 | 内容 | 预估行数 |
|--------|---------|------|---------|
| `src/lib/settings.ts` | 83-195 | 6 个 `getInitial*` 函数 + localStorage key 常量 | ~120 |
| `src/lib/demo-data.ts` | 197-370 | `buildIndustrialAlarmExample`、`buildProjectAst`、`buildInitialProjectDrafts` | ~175 |
| `src/lib/sidebar.ts` | 372-423 | `buildSidebarSections` | ~55 |
| `src/lib/workflow-events.ts` | 425-584 | `ParsedWorkflowEvent` 接口、`EMPTY_RUNTIME_STATE`、`pushUnique`/`removeItem`、`createClientEntryId`、`describeUnknownError`、`buildRuntimeLogEntry`、`buildAppErrorRecord`、`parseWorkflowEventPayload`、`reduceRuntimeState` | ~160 |
| `src/lib/workflow-status.ts` | 586-652 | `deriveWorkflowStatus`、`getWorkflowStatusLabel`、`getWorkflowStatusPillClass` | ~70 |

### 1.2 提取 2 个自定义 hooks

| 新文件 | 职责 | 预估行数 |
|--------|------|---------|
| `src/hooks/use-settings.ts` | 主题模式、强调色、UI 密度、动效模式、启动页的 useState + localStorage 写入副作用 + CSS 变量同步 | ~100 |
| `src/hooks/use-workflow-engine.ts` | deploy/dispatch/undeploy 操作；`onWorkflowEvent`/`onWorkflowResult`/`onWorkflowDeployed`/`onWorkflowUndeployed` 事件监听；`runtimeState`/`eventFeed`/`results`/`connectionPreview`/`appErrors` 状态管理 | ~200 |

### 1.3 重构后的 App.tsx（~550 行）

仅保留：
- import 语句
- 项目草稿管理（`activeBoardId`、`projectDrafts`、`astText`/`payloadText` 派生）
- 调用 `useSettings()` 和 `useWorkflowEngine()` 获取状态与 handler
- 面板路由逻辑 + JSX 编排

App.tsx 不再包含任何可独立测试的业务逻辑。

---

## 二、Vitest 单元测试

### 2.1 配置

- 依赖：`vitest`（dev dependency）
- 配置文件：`web/vitest.config.ts`，继承 `vite.config.ts` 的插件和 resolve
- 测试环境：`node`（全部被测函数为纯逻辑，不需要 DOM）
- 命令：`npm run test`（`vitest run`）、`npm run test:watch`（`vitest`）

### 2.2 测试文件

```
src/lib/__tests__/
├── parse-event.test.ts          ~80 行
├── reduce-state.test.ts         ~100 行
├── nazh-to-flowgram.test.ts     ~90 行
├── flowgram-to-nazh.test.ts     ~100 行
├── parse-graph.test.ts          ~60 行
├── layout-graph.test.ts         ~80 行
├── workflow-status.test.ts      ~70 行
└── settings.test.ts             ~60 行
```

### 2.3 覆盖清单

**parse-event.test.ts**：
- Started / Completed / Failed / Output / Finished 五种变体正确解析
- 非法输入（null、非 object、空 object）返回 null

**reduce-state.test.ts**：
- started 事件将 nodeId 加入 activeNodeIds
- completed 事件将 nodeId 从 active 移至 completed
- failed 事件记录 error 并移至 failed
- trace_id 切换时重置状态
- 多节点并发场景

**nazh-to-flowgram.test.ts**：
- 基本转换：nodes/edges 数量一致
- 字段映射：`node_type` → FlowGram `data.nodeType`
- 前端独有字段 `meta.position` 保留到 FlowGram 节点坐标

**flowgram-to-nazh.test.ts**：
- 基本往返：Nazh → FlowGram → Nazh，核心字段不丢失
- `editor_graph` 字段保留在输出中
- previousGraph 的 connections/name 被继承

**parse-graph.test.ts**：
- 合法 JSON 返回 `{ graph, error: null }`
- 非法 JSON 返回 `{ graph: null, error: string }`
- 空字符串返回错误

**layout-graph.test.ts**：
- 线性链（A→B→C）：层级递增
- 分叉 DAG（A→B, A→C）：B 和 C 同层
- 孤立节点：单独一层

**workflow-status.test.ts**：
- 非 Tauri 运行时 → `preview`
- 无活跃看板 → `idle`
- 有部署 + active 节点 → `running`
- 有部署 + failed 节点 → `failed`
- 有部署 + 全部 completed → `completed`
- label 和 pillClass 与 status 一一对应

**settings.test.ts**：
- localStorage 有有效值 → 返回存储值
- localStorage 无值 → 返回默认值
- localStorage 有非法值 → 返回默认值
- `window` undefined → 返回默认值

---

## 三、Playwright E2E

### 3.1 配置

- 依赖：`@playwright/test`（dev dependency）
- 配置文件：`web/e2e/playwright.config.ts`
- 运行前提：Tauri 开发构建已编译（`cargo build --manifest-path src-tauri/Cargo.toml`）
- 启动方式：Playwright 通过 `webServer` 配置启动 `tauri dev`，连接 WebView
- CI：`xvfb-run npx playwright test`

### 3.2 data-testid 清单

需在以下组件上添加 `data-testid` 属性（最小侵入）：

| testid | 组件 | 用途 |
|--------|------|------|
| `sidebar-source` | SidebarNav | 导航到 Source 面板 |
| `sidebar-payload` | SidebarNav | 导航到 Payload 面板 |
| `deploy-button` | SourcePanel / 工具栏 | 触发部署 |
| `undeploy-button` | 工具栏 | 触发卸载 |
| `dispatch-button` | PayloadPanel | 发送 Payload |
| `workflow-status` | 状态标签 | 断言当前状态文本 |
| `ast-editor` | SourcePanel | 输入 AST 文本 |
| `event-feed` | RuntimeDock | 断言事件日志出现 |
| `result-list` | RuntimeDock | 断言结果载荷出现 |
| `error-display` | 错误提示区域 | 断言错误信息 |

### 3.3 测试场景

**deploy-and-dispatch.spec.ts**（~80 行）：
1. 等待应用启动
2. 导航到 Source 面板
3. 确认 AST 文本已填充（默认示例）
4. 点击部署按钮
5. 验证状态变为"已部署"
6. 导航到 Payload 面板
7. 点击发送按钮
8. 验证事件日志出现 Started/Completed 条目
9. 验证结果列表出现载荷

**lifecycle.spec.ts**（~60 行）：
1. 部署工作流
2. 验证状态为"已部署"
3. 点击卸载按钮
4. 验证状态回到"未部署"
5. 重新部署
6. 验证状态恢复"已部署"

**error-handling.spec.ts**（~50 行）：
1. 导航到 Source 面板
2. 清空 AST 文本，输入 `{invalid json`
3. 点击部署
4. 验证错误提示出现

---

## 四、文档同步

以下文档需在实施过程中同步更新：

| 文件 | 更新内容 |
|------|---------|
| `CLAUDE.md` | Build Commands 新增 `npm run test` 和 `npm run test:e2e`；Testing 章节补充前端测试说明；Frontend Key Files 补充 `hooks/`、新增 lib 文件 |
| `AI-Context.md` | 路线图补充前端测试阶段 |
| `README.md` | "当前完成度"补充前端测试条目；"已验证状态"补充 Vitest / Playwright |
| 源码注释 | App.tsx 模块注释更新；新文件遵循中文注释规范 |

---

## 五、不做什么

- 不测 FlowGram 画布内部渲染（第三方库内部，选择器脆弱）
- 不做 React 组件级渲染测试（hooks 和纯函数已单独覆盖）
- 不 mock Tauri API 做浏览器模式 E2E
- demo-data / sidebar 不写测试（纯数据声明，无逻辑分支）
- 不配置 CI workflow 文件（E2E 设计兼容 CI，但 `.github/workflows` 配置留后续）
