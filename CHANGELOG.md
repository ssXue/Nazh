# 变更日志

本文件记录 Nazh 项目的所有重要变更。

格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [Unreleased]

## [0.1.0] — 2026-05-18

### 新增

- 项目基础设施：CI 流水线、devcontainer、rust-toolchain、rustfmt、clippy lint、cargo-deny
- 完整的 Rust 文档注释（中文）
- ADR 文档框架及 25 篇 ADR（0001–0025）
- RFC 文档框架及 5 篇 RFC（0001–0005）
- 子模块 README（src/、src-tauri/、web/、tests/、examples/）
- `WorkflowContext` 数据载体（trace_id + timestamp + JSON payload）
- 线性 Pipeline 抽象，支持阶段超时与 panic 隔离
- DAG 工作流图解析、拓扑排序校验与异步部署（`crates/graph/`）
- `NativeNode`（原生节点）与 `RhaiNode`（脚本节点）
- 连接资源池（`ConnectionManager`）+ RAII `ConnectionGuard`
- Tauri IPC 82 条命令，覆盖工作流生命周期、变量、连接、设备、能力、可观测性、AI 配置、copilot 会话
- React 19 + FlowGram.AI 桌面工作台（Dashboard、Boards、Source、Connections、Payload、Canvas、Settings 面板）
- 29 种节点：Flow 8 种（if / switch / loop / tryCatch / code / stateMachine / subgraphInput / subgraphOutput）、IO 18 种（timer / serial / native / modbus / http / mqtt / bark / sql / CAN/SLCAN / EtherCAT 三件套 / debugConsole / capabilityCall / humanLoop / deviceSignalRead / deviceEventTrigger）、Pure 3 种（c2f / minutesSince / lookup）
- RFC-0002 Phases 1–5：分层内核 + 插件架构（Ring 0 / Ring 1 / Facade / Shell）
- RFC-0003 Phase 1–3：边缘存储层（`crates/store/` SQLite crate + 可观测性索引 + 部署审计 + 批量写入器）
- RFC-0004 Phase 0–5：三段式 DSL（Device / Capability / Workflow 编译器 + AI 结构化提取 + 安全编译器 6 条规则）
- RFC-0005：AI 调用前移到前端（Vercel AI SDK + copilot chat + 设备提取 + Rust HTTP 客户端移除）
- ADR-0008：节点输出元数据通道分离
- ADR-0009：节点生命周期钩子（`on_deploy` + `LifecycleGuard`）
- ADR-0010：Pin 声明系统（Ring 0 类型 + 部署期校验 + IPC `describe_node_pins` + 前端连接期校验 + pin tooltip + AI prompt pin schema）
- ADR-0011：节点能力标签（`NodeCapabilities` 位图）
- ADR-0012：工作流变量（Phase 1+2+3：运行时变量 + 持久化 + 全局变量 + 历史曲线 + 删除确认）
- ADR-0013：子图与宏系统（`subgraphInput` / `subgraphOutput` 桥接 + `flattenSubgraphs` 递归展平）
- ADR-0014：引脚求值语义二分（Exec / Data / Reactive + 部署期校验 + 前端端口着色）
- ADR-0015：反应式数据引脚（`subscribe_reactive_pin` IPC + `ReactiveUpdatePayload` 事件推送）
- ADR-0016：边级可观测性（`EdgeTransmitSummary` + `BackpressureDetected` + 100ms 定时窗口 + 前端边热力图）
- ADR-0017：IPC + ts-rs 迁出 Ring 0（`crates/tauri-bindings/`）
- ADR-0018：`nodes-io` 按协议 feature 门控
- ADR-0020：graph 编排层独立为 Ring 1 crate
- ADR-0022：工作流变量持久化（`crates/store/`）
- ADR-0024：设备信号读取与事件触发节点（全协议覆盖）
- ADR-0025：连接资产模型（DSL YAML + Store 私有数据替代 `connections.json`）
- ADR-0023：EtherCAT TX/RX 恢复策略（方案 B 一键重启 + 诊断守卫）
- 设备建模页：卡片式列表 + 搜索过滤 + PDF 说明书导入 + AI 结构化提取
- 连接资源管理器：强类型协议表单 + YAML 资产持久化 + Store secret/local override
- 导航栏折叠/展开动画改用 Apple 风格弹簧缓动
- Windows 兼容性适配（平台感知标题栏、路径、串口与安全校验）
- 多平台 Release workflow（macOS arm64/x64 + Windows + Ubuntu）

### 移除

- DSL 编辑器页面和 AI 编排控制台页面（RFC-0004 Phase 4B/4C）— 设计评估结论：与核心画布创作能力冲突，画布内 AI 编排器已覆盖同等能力。移除 4 个 IPC 命令及前端组件
- 看板页 AI 新建画布入口 — 将创建画布的仪式感交还给使用者
- `connections.json` legacy 路径 — 连接资产已迁移为 DSL YAML（ADR-0025）
- Rust 侧 AI HTTP 客户端和 `AiService` trait — AI 调用已前移到前端（RFC-0005）

### 变更

- 看板页去除 ExpandTransition 展开动画复用，还原为 if/else 直接切换

### 修复

- ExpandTransition 居中浮层遮罩透明度从 80% 降至 0%，修复亮主题下方形白色底
- `native` 节点 payload 键从 `_native_message` 修正为 `native_message`
- `canRead` / `canWrite` 引入显式 `simulation` 开关 + 默认 fail-fast，工业现场漏配时不再静默给出假数据
- `deviceEventTrigger` 部署期协议校验，移除后台静默默认值

<!--
发布新版本时的操作指南：
1. 将 [Unreleased] 下的条目移到新版本号标题下
2. 添加发布日期
3. 在顶部创建新的空 [Unreleased] 区段
4. 更新 Cargo.toml 中的 version 字段
-->
