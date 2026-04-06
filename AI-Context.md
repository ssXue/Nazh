# Nazh: 工业级边缘计算与数据编排引擎 (Rust + Tauri 版)

## 1. 项目背景与核心约束 (Context & Constraints)
Nazh 是一款专为工业自动化边缘计算场景设计的数据流转与逻辑编排平台。
* **核心定位**：数据网关、协议转换、OT-IT 融合与非标业务逻辑编排。
* **非硬实时**：侧重高吞吐、高并发、低延迟的数据流转，而非微秒级硬件中断控制。
* **绝对可靠**：引擎必须具备极高的防爆性，单节点执行失败或网络超时**绝不能**导致主线程（Tokio Runtime）Panic 或崩溃。

## 2. 核心技术栈 (Tech Stack)
* **后端核心 (Engine)**: Rust (Edition 2024 / 2021) + Tokio 异步运行时。
* **桌面端壳层 (App Shell)**: Tauri v2 (提供轻量级跨平台桌面能力与本地 IPC 通信)。
* **前端画布 (UI)**: React + TypeScript + `FlowGram.AI` (字节跳动开源节点编辑器)。
* **脚本引擎 (Scripting)**: `Rhai` (纯 Rust 编写的嵌入式脚本引擎，用于动态业务逻辑)。
* **通信机制**: 前后端通信严格使用 Tauri `invoke` IPC (不用 HTTP/gRPC)；引擎内部调度使用 `tokio::sync::mpsc` 或 `async-channel`。

## 3. 核心领域模型 (Domain Models)
开发时请严格遵循以下 Rust 数据结构抽象：
* **`WorkflowContext`**: 在无锁通道中流转的数据载体。必须包含 `trace_id` (Uuid)、`timestamp` 以及动态的 `payload` (`serde_json::Value`，用于承载工业 TQV 数据)。必须实现 `Clone` 和 `Send`。
* **`NodeTrait`**: 所有节点的统一异步 Trait。定义 `async fn execute(&self, ctx: WorkflowContext) -> Result<WorkflowContext, Error>`。
* **`WorkflowGraph`**: 后端用于解析的前端 AST（抽象语法树）。包含节点字典和连线定义，必须能够通过 `serde_json` 完美反序列化。

## 4. 混合节点执行系统 (The Hybrid Node System)
这是本项目的核心架构，AI 在实现节点时必须区分以下两类：
* **静态核心节点 (Native Nodes)**：直接用纯 Rust 硬编码在后端的节点。凡是涉及底层网络 I/O、协议解析（HTTP, MQTT, Modbus, TCP, SQLite）的动作，全部作为静态核心节点。享用原生性能，无沙箱开销。
* **动态脚本节点 (Code Node / Rhai Node)**：唯一的“动态”节点。内部嵌入 `rhai::Engine`。用户通过前端 FlowGram 传入 Rhai/JS 风格的脚本文本，该节点在执行时将 `WorkflowContext.payload` 转入 Rhai 环境，执行动态逻辑后返回。注意：需设置 Rhai 执行步数上限防止死循环。

## 5. 硬件解耦与单向数据流 (Hardware & Dataflow)
* **全局连接池 (Connection Manager)**：绝对禁止节点直连物理硬件。串口、Modbus 等物理连接必须由一个全局的 `Arc<RwLock<ConnectionManager>>` 统一管理。
* **动作节点机制**：画布上的节点（如“读取 PLC”）仅包含业务参数和关联的 `connection_id`。节点在 `execute` 时，从全局 Manager 中借出连接，完成通信后归还。
* **统一数据流驱动**：不区分“时序线”和“数据线”。数据的到达即代表触发执行。利用 Tokio 的 MPSC Channel 构建 DAG 的流水线（Pipeline）。

## 6. 前端与 Tauri 协作 (Frontend & Tauri IPC)
* **前端职责**：React + FlowGram 仅作为视图层。负责提供友好的拖拽界面，以及将用户的配置导出为 JSON 格式的 AST（抽象语法树）。
* **通信机制**：通过 Tauri `#[tauri::command]` 暴露 Rust 函数。
  * `deploy_workflow(ast: String)`: 接收前端 JSON，在后端反序列化并构建 Tokio 流水线。
  * 状态同步：利用 Tauri 的 `Window::emit` 向前端主动推送节点执行状态（如当前执行节点 ID、高亮日志），触发 FlowGram 画布的连线动画。

## 7. AI Copilot 原生接口 (AI-Native)
系统从底层支持 LLM 调用，提供真正的“一句话开发”体验：
* **元数据描述**：为每个 Native Node 维护一份 `ai_description` 字符串。
* **代码生成定位**：LLM 的核心作用是**生成 Rhai 脚本**。用户在前端输入自然语言，后台组装 Prompt 并调用外部 LLM API，生成一段短小精悍的 Rhai 脚本，直接填充至 `Code Node` 中执行。

## 8. AI 增量开发路线图 (Roadmap for AI Assistant)
请 AI 助手严格按照以下 Phase 顺序进行开发，未要求的阶段请勿提前实现：
* **Phase 1: 基础设施 (Rust Base)** -> 定义 `WorkflowContext`，创建基于 Tokio MPSC Channel 的简易流水线调度原型。
* **Phase 2: 混合节点抽象 (Node System)** -> 定义 `NodeTrait`，实现一个 `NativeNode` (如打印/HTTP请求) 和一个 `RhaiNode` (嵌入 rhai 引擎处理 JSON)。
* **Phase 3: 图结构反序列化 (AST Parsing)** -> 定义可序列化的 DAG 结构体，实现从 JSON 转换为实际 Tokio 执行流的过程。
* **Phase 4: 全局资源管理 (ConnMgr)** -> 实现跨任务共享的硬件连接池骨架。
* **Phase 5: Tauri 桥接与前端** -> 编写 Tauri `invoke` 接口，用 React 搭建带有 FlowGram.AI 的画板，跑通全链路。

## 9. Rust 编码规范提醒 (Rust Coding Standards)
* 彻底杜绝使用 `unwrap()` 或 `expect()`。所有错误必须通过自定义 `Error` 枚举（推荐使用 `thiserror` 库）向上传递，确保运行时绝不 Panic。
* 在多任务并发时，正确使用 `Arc` 和 `tokio::sync::Mutex` / `RwLock`，注意避免死锁。尽量通过 Channel 传递所有权（Message Passing）而不是共享内存。
* 对于 JSON 操作，大量使用 `serde_json` 宏与动态 `Value`。