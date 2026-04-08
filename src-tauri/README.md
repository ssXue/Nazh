# src-tauri/ — Tauri 桌面壳层

本目录是 `nazh-desktop` 二进制 crate，基于 Tauri v2 将 Rust 引擎和 React 前端打包为桌面应用。

## 结构

```text
src-tauri/
├── Cargo.toml        # 二进制 crate 配置，依赖 nazh-engine（本地路径）
├── tauri.conf.json   # Tauri 应用配置（窗口尺寸、构建命令、端口等）
├── build.rs          # Tauri 构建脚本
├── src/
│   ├── main.rs       # 入口（仅调用 run()）
│   └── lib.rs        # 核心逻辑：IPC 命令 + 状态管理
└── icons/            # 应用图标
```

## IPC 命令

Tauri 通过 `#[tauri::command]` 向前端暴露三个函数：

| 命令 | 参数 | 说明 |
|------|------|------|
| `deploy_workflow` | `ast: String` | 解析 JSON AST → 校验 DAG → 部署 → 启动事件/结果转发 |
| `dispatch_payload` | `payload: Value` | 向当前工作流的根节点发送测试数据 |
| `list_connections` | 无 | 返回连接池快照 |

## 事件推送

引擎事件通过 `Window::emit` 主动推送给前端：

- `workflow://deployed` — 部署完成，携带节点/边数量
- `workflow://node-status` — 节点执行状态变更
- `workflow://result` — 叶节点输出结果

## 开发

```bash
# 启动桌面开发模式（自动拉起 Vite 前端）
cd src-tauri && ../web/node_modules/.bin/tauri dev --no-watch

# 仅检查编译
cargo check --manifest-path src-tauri/Cargo.toml
```
