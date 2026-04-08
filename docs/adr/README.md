# 架构决策记录（ADR）

本目录记录 Nazh 项目中做出的重要架构决策。

## 什么是 ADR？

ADR（Architecture Decision Record）是一种轻量级文档格式，用于记录在项目中做出的重要架构决策。
每条记录包含决策的上下文、可选方案、最终选择及其后果，帮助后来者理解"为什么这样做"。

参考：[Michael Nygard 的 ADR 规范](https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions)

## 索引

| 编号 | 标题 | 状态 | 日期 |
|------|------|------|------|
| [0001](0001-tokio-mpsc-dag-调度.md) | 使用 Tokio MPSC Channel 实现 DAG 节点调度 | 已接受 | 2025-01-01 |
| [0002](0002-rhai-作为脚本引擎.md) | 选择 Rhai 作为嵌入式脚本引擎 | 已接受 | 2025-01-01 |
| [0003](0003-tauri-ipc-不用-http.md) | 前后端通信使用 Tauri IPC 而非 HTTP | 已接受 | 2025-01-01 |

## 如何新增 ADR

1. 复制 `template.md` 为 `NNNN-简短标题.md`（编号递增）
2. 填写各个章节
3. 更新本文件的索引表
4. 提交 PR

## 状态说明

- **提议中** — 正在讨论，尚未达成共识
- **已接受** — 已采纳并在实施中
- **已废弃** — 曾经接受但已被后续决策替代
- **已取代** — 被新的 ADR 替代（注明替代编号）
