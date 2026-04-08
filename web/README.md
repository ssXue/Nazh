# web/ — React 前端

本目录是 Nazh 的 Web 前端，基于 React 18 + TypeScript + Vite 构建，使用 FlowGram.AI 作为节点画布编辑器。

## 技术栈

- **框架**: React 18 + TypeScript
- **构建**: Vite 5
- **画布编辑器**: FlowGram.AI（字节跳动开源）
- **桌面集成**: @tauri-apps/api（IPC 通信）

## 目录结构

```text
web/src/
├── App.tsx                      # 主入口：状态管理、面板路由、工作流生命周期
├── types.ts                     # TS 类型定义（镜像 Rust 结构体）+ 示例数据
├── lib/
│   ├── tauri.ts                 # Tauri IPC 封装与事件监听
│   ├── flowgram.ts              # Nazh AST ↔ FlowGram WorkflowJSON 双向转换
│   ├── graph.ts                 # 客户端拓扑排序与布局计算
│   └── theme.ts                 # 主题管理（亮/暗、强调色）
├── components/
│   ├── app/                     # 应用面板（Dashboard、Boards、Source、Settings 等）
│   ├── flowgram/                # FlowGram 画布子组件
│   ├── ConnectionStudio.tsx     # 连接资源管理
│   └── FlowgramCanvas.tsx       # 画布编辑器入口
└── index.css                    # 全局样式
```

## 面板导航

| 面板 | 功能 |
|------|------|
| Dashboard | 工程统计、节点/边数量、状态分布 |
| Boards | 工程看板入口 |
| Source | 直接编辑工作流 AST JSON |
| Connections | 维护连接定义 |
| Payload | 发送测试载荷 |
| Canvas | FlowGram 画布拖拽编辑 |
| Settings | 主题、密度、动画、启动页 |

## 开发

```bash
# 安装依赖
npm install

# 单独启动前端开发服务（端口 1420）
npm run dev

# 构建产物
npm run build
```

> 注意：正常开发流程通过 Tauri 启动，会自动拉起 Vite dev server。
> 单独启动前端时，Tauri IPC 不可用，部分功能会降级。
