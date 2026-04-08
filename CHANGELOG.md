# 变更日志

本文件记录 Nazh 项目的所有重要变更。

格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [Unreleased]

### 新增

- 项目基础设施：CI 流水线、devcontainer、rust-toolchain、rustfmt、clippy lint、cargo-deny
- 完整的 Rust 文档注释（中文）
- ADR 文档框架及三篇初始 ADR
- RFC 文档框架及模板
- 子模块 README（src/、src-tauri/、web/、tests/、examples/）

## [0.1.0] — 2025-xx-xx

### 新增

- `WorkflowContext` 数据载体（trace_id + timestamp + JSON payload）
- 线性 Pipeline 抽象，支持阶段超时与 panic 隔离
- DAG 工作流图解析、拓扑排序校验与异步部署
- `NativeNode`（原生节点）与 `RhaiNode`（脚本节点）
- 连接资源池骨架（`ConnectionManager`）
- Tauri IPC：`deploy_workflow`、`dispatch_payload`、`list_connections`
- React + FlowGram.AI 桌面工作台
- Dashboard、Boards、Source、Connections、Payload、Canvas、Settings 面板
- 集成测试（Pipeline + Workflow 端到端）

<!--
发布新版本时的操作指南：
1. 将 [Unreleased] 下的条目移到新版本号标题下
2. 添加发布日期
3. 在顶部创建新的空 [Unreleased] 区段
4. 更新 Cargo.toml 中的 version 字段
-->
