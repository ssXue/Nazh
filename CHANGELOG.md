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
- RFC-0004 Phase 3：Workflow DSL 编译器（`crates/dsl-compiler/`）、`stateMachine` 节点（`crates/nodes-flow/`）、`capabilityCall` 节点（`crates/nodes-io/`）、4 个一致性测试 + 集成测试
- ADR-0012 Phase 3：变量重置/删除/历史查询 IPC + 变量持久化 `crates/store/`（ADR-0022）+ 历史曲线 + 全局变量 CRUD + 删除确认弹窗
- ADR-0022：`crates/store/` Ring 1 SQLite crate（变量快照 / 变更历史 / 全局变量 / schema 版本管理）
- P1/P2 技术债批量偿还：变量事件独立通道（`WorkflowVariableEvent`）、`NodeOutput.metadata` 改 `Option<Map>`、Rhai `default_max_operations` 统一、`workflow.rs` 拆为三模块、前端大文件拆分、17 IPC 类型迁入 `tauri-bindings`
- RFC-0004 Phase 4A：设备/能力 AI 结构化提取提案（`extract_device_proposal` / `extract_device_proposal_stream`）— JSON 输出含 uncertainties + warnings
- RFC-0004 Phase 4B：DSL 编译器 IPC（`compile_workflow_dsl` / `load_compiler_asset_snapshot`）+ 前端 DSL 编辑器（YAML textarea + 编译反馈 + 资产快照）
- RFC-0004 Phase 4C：AI 编排控制台（`ai_generate_workflow_dsl` / `ai_generate_workflow_dsl_stream`）— 从自然语言目标生成 Workflow DSL + 自动编译 + 不确定项标记
- RFC-0004 Phase 5：安全编译器 6 条规则（`compile_with_safety()`）— 单位一致性、量程边界、前置条件可达性、状态机完整性、危险动作审批、机械互锁 + IPC 透传安全诊断
- ADR-0021 画布导入闭环：AI 编排控制台编译成功后可点击"导入画布"按钮，自动创建工程并导航到画布页渲染 WorkflowGraph

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
