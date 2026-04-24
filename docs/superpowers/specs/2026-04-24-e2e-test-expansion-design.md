# E2E 测试全面覆盖扩展设计

## 目标

将 Playwright E2E 测试从 6 个用例扩展至 ~25 个，覆盖全部 8 个主要面板，解决两个 P0 问题：
1. E2E 测试覆盖不足
2. 组件级测试缺失（通过 E2E 补偿）

## 约束

- 测试运行完整 Tauri 桌面窗口（不做浏览器模式 mock）
- 不测试真实外部服务（AI 提供商、串口、MQTT、Modbus），仅验证 UI 行为
- 不测 FlowGram 画布内部渲染（第三方库，选择器脆弱）
- 遵循现有 `data-testid` 选择器模式
- 中文注释规范不变

## 现有测试 ID

| testid | 组件 | 用途 |
|--------|------|------|
| `sidebar-{key}` | SidebarNav | 面板导航 |
| `workflow-status` | SidebarNav | 状态标签 |
| `board-entry` | BoardsPanel | 看板卡片 |
| `board-create` | BoardsPanel | 新建按钮 |
| `deploy-button` / `undeploy-button` | FlowgramCanvas | 部署/卸载 |
| `dispatch-button` | FlowgramCanvas | 手动触发 |
| `event-feed` | RuntimeDock | 事件日志 |
| `result-list` | RuntimeDock | 结果列表 |

## 新增 data-testid 清单

| testid | 组件 | 用途 |
|--------|------|------|
| `board-import` | BoardsPanel | 导入按钮 |
| `board-delete` | BoardsPanel | 删除按钮 |
| `board-delete-confirm` | BoardsPanel | 删除确认 |
| `board-delete-cancel` | BoardsPanel | 取消删除 |
| `board-empty-state` | BoardsPanel | 空状态 |
| `dashboard-navigate-boards` | DashboardPanel | 导航到看板 |
| `connection-add` | ConnectionStudio | 添加连接 |
| `connection-card` | ConnectionStudio | 连接卡片 |
| `connection-empty-state` | ConnectionStudio | 空状态 |
| `connection-id-input` | ConnectionStudio | ID 编辑 |
| `connection-delete` | ConnectionStudio | 删除按钮 |
| `ai-provider-add` | AiConfigPanel | 添加提供商 |
| `ai-provider-card` | AiConfigPanel | 提供商卡片 |
| `ai-provider-empty-state` | AiConfigPanel | 空状态 |
| `ai-provider-preset` | AiConfigPanel | 预设选择 |
| `ai-provider-save` | AiConfigPanel | 保存按钮 |
| `ai-agent-settings` | AiConfigPanel | Agent 设置按钮 |
| `log-level-filter` | LogsPanel | 级别筛选 |
| `log-type-filter` | LogsPanel | 类型筛选 |
| `log-entry` | LogsPanel | 日志条目 |
| `log-inspector` | LogsPanel | 详情面板 |
| `log-empty-state` | LogsPanel | 空状态 |
| `runtime-workflow-item` | RuntimeManagerPanel | 工作流条目 |
| `runtime-dead-letter-item` | RuntimeManagerPanel | 死信条目 |
| `settings-theme-toggle` | SettingsPanel | 主题切换 |
| `settings-accent-preset` | SettingsPanel | 强调色 |
| `settings-workspace-input` | SettingsPanel | 工作区路径 |
| `settings-workspace-apply` | SettingsPanel | 应用路径 |

## 测试文件结构

```
web/e2e/
├── helpers.ts                  共享工具函数
├── playwright.config.ts        (现有，不变)
├── lifecycle.spec.ts           (现有，不变)
├── deploy-and-dispatch.spec.ts (现有，不变)
├── error-handling.spec.ts      (现有，不变)
├── boards.spec.ts              看板管理 (5 cases)
├── settings.spec.ts            设置面板 (3 cases)
├── connections.spec.ts         连接工作台 (4 cases)
├── ai-config.spec.ts           AI 配置 (3 cases)
├── logs.spec.ts                日志面板 (3 cases)
├── runtime-manager.spec.ts     运行管理器 (3 cases)
└── dashboard.spec.ts           仪表盘 (2 cases)
```

## 各 Spec 详细场景

### boards.spec.ts (5 cases)
1. 新建看板 -> 看板卡片出现
2. 打开看板 -> 进入画布视图
3. 删除看板 -> 确认对话框 -> 看板消失
4. 取消删除 -> 看板仍存在
5. 空状态 -> 新建后消失

### settings.spec.ts (3 cases)
1. 主题切换 -> 深色/浅色模式切换
2. 强调色预设 -> 点击切换
3. 工作区路径 -> 输入路径 -> 应用按钮状态

### connections.spec.ts (4 cases)
1. 空状态显示 -> "尚未配置"
2. 添加连接 -> 连接卡片出现
3. 编辑连接 ID -> 失焦保存
4. 删除连接 -> 确认后消失

### ai-config.spec.ts (3 cases)
1. 空状态显示
2. 添加提供商 -> 预设填充 -> 保存调用
3. Agent 设置对话框 -> 参数显示

### logs.spec.ts (3 cases)
1. 无事件时空状态
2. 部署+触发后事件出现
3. 级别筛选交互

### runtime-manager.spec.ts (3 cases)
1. 无运行工作流时空状态
2. 部署后工作流列表更新
3. 停止工作流 -> 列表更新

### dashboard.spec.ts (2 cases)
1. 默认统计数据显示
2. 导航到看板按钮工作

## 实施顺序

1. 添加 data-testid 到组件
2. 创建 helpers.ts
3. 按面板编写 spec 文件
4. 验证全部通过

## 不做什么

- 不做 React 组件级渲染测试
- 不 mock Tauri API 做浏览器模式 E2E
- 不测 FlowGram 画布内部渲染
- 不测真实外部协议驱动
- 不修改 playwright.config.ts
