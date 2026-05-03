# ADR-0021：三段式 DSL 编译与 AI 编排入口

| 字段     | 值                                         |
|----------|--------------------------------------------|
| 状态     | 已接受，部分实施（Phase 3 编译器 + Phase 4A/4B/4C/4D AI 编排已实施，Phase 5 Safety Compiler 已实施，画布导入闭环已实施；DSL 编辑器 gutter diagnostics 待后续）                |
| 日期     | 2026-04-30                                 |
| 决策者   | ssXue                                      |
| 关联     | RFC-0004（三段式 DSL）、ADR-0010（Pin 声明）、ADR-0012（工作流变量）、ADR-0013（子图）、ADR-0014（PinKind）、ADR-0019（AI 依赖反转）、ADR-0020（`src/graph/` 归属） |

## 背景

RFC-0004 提议三段式 DSL（Device / Capability / Workflow），以 YAML 为载体定义设备语义、能力边界和工作流状态机。现有系统的编辑入口是 FlowGram 画布拖拽，部署路径是画布导出编辑态 JSON → 前端转换为 Nazh `WorkflowGraph` JSON → Tauri IPC → `WorkflowGraph::from_json()` → `deploy_workflow()`。

Nazh 的产品定位是 AI-native industrial edge workflow。DSL 不是"另一种导入格式"或"平行通道"，而是 **AI 编排的结构化语言**：用户通过 AI 交互生成 / 修订 DSL，AI 以 DSL 作为可审查、可 diff、可校验的结构化输出格式，设备动作必须经 DSL / 编译器 / Safety Compiler / `WorkflowGraph` 部署管道落地，不能由运行时 LLM 直接决定或发起工业动作。用户也可以绕过 AI 直接手写/编辑 DSL——手动编辑是兜底能力，但产品心智模型是"通过 AI 下达结构化编排指令"。

RFC-0004 未锁定以下设计决策：

1. DSL 编译产物以什么格式进入画布（直接输出 `WorkflowGraph` JSON vs FlowGram 编辑态 JSON vs 引入新 IR）
2. DSL 编译产物进入画布后，画布与 DSL 源的关系（只读锁定 vs 可编辑独立副本 vs 双向同步）
3. 状态机运行时模型（直接展开成 DAG 边、修改 DAG Runner 支持环，还是引入状态机子运行时）
4. Device/Capability 注册表的生命周期（全局 vs per-workflow，部署/卸载时清理）
5. DSL 编译器与 `src/graph/` 的 crate 归属关系
6. AI 编排的输出边界：AI 直接生成运行时动作，还是生成可审查 / 可校验的 DSL patch，并由编译器派生部署产物
7. 产品入口：AI 编排控制台应嵌入画布页，还是拥有独立前端页面

### 2026-04-30 复评结论

本次复评把原先"DSL 文本通道与画布通道汇合"的表述收敛为"AI 编排控制台生成 DSL，编译后导入画布"。结论是：**方向可接受，但实施边界需要收紧**。

复评依据：

- 前端已有 `WorkflowGraph → FlowGram` 的基础导入能力：`web/src/lib/flowgram.ts` 的 `toFlowgramWorkflowJson()` 会把裸 `WorkflowGraph` 补齐为 FlowGram 编辑态 JSON，`web/src/lib/graph.ts` 的 `layoutGraph()` 会做简单层级布局。因此 ADR-0021 不应把"生成 FlowGram 内部 AST"作为编译器职责；真正缺的是产品化的跨页面导入、诊断状态和复杂图布局质量。
- 现有 `web/src/lib/workflow-orchestrator.ts` 直接让 AI 输出 JSON Lines 操作流并修改 `WorkflowGraph`，这是 DSL 落地前的过渡能力。按本 ADR，正式 AI 编排入口必须输出 DSL patch proposal；直接生成 `WorkflowGraph` 只能作为调试/预览派生产物，不能成为可部署源级契约。
- `WorkflowGraph` Rust 类型当前仍在 facade crate 的 `src/graph/types.rs`，不是独立 schema crate。为了避免 `dsl-compiler` 反向依赖 `nazh-engine`，Phase 1 编译器应输出符合 `WorkflowGraph` JSON 契约的 DTO / `serde_json::Value`，并用 `WorkflowGraph::from_json()` conformance 测试守住漂移。若 ADR-0020 后续拆出 `crates/graph/` 或 `workflow-schema` crate，实施 ADR 可再切换为共享 Rust 类型。
- RFC-0004 中"双通道输入"是探索期措辞，本 ADR 复评后把产品入口改为单通道：所有最终部署都从画布页发起；DSL 只在 AI 编排控制台内作为源级结构化语言存在。

### 2026-05-03 复评结论

本次复评进一步把"三段式 DSL"从"一次性编排输入"改为**分层资产生命周期**：

1. **说明书 → 设备** 是独立产品入口。新增 **设备建模控制台**：上传说明书、点表、寄存器表、协议文档，由 AI 抽取 `DeviceSpec` / `SignalSpec` / 初步 `CapabilitySpec`，人工审查、修正、测试连接后固化为项目 / 环境级设备资产。
2. **设备 / 能力资产是 AI 编排的上下文**。AI 编排控制台不再每次重新解释说明书；它读取已审查、已版本化的设备模型和能力目录，只生成 / 修订 Workflow DSL。
3. **三段式 DSL 的职责分化**：Device DSL 是设备知识固化层；Capability DSL 是安全能力封装层；Workflow DSL 是面向业务目标的编排层。三者仍同属 DSL 体系，但生命周期不同。

因此，ADR-0021 的产品模型从"一个 AI 编排控制台承载三段 DSL"调整为"设备建模控制台 + AI 编排控制台"：

- 设备建模控制台负责说明书理解、来源追溯、字段置信度、人工确认、版本化保存。
- AI 编排控制台负责选择设备 / 能力快照、生成 Workflow DSL、解释编译 / Safety 错误，并导入画布。
- 若用户在 AI 编排时上传新说明书，该输入必须先进入设备建模流程，不能作为未审查上下文直接影响可部署动作。

## 决策

> 我们决定：
> 1. **DSL 是 AI 编排与设备建模的结构化语言，不是独立部署通道。** 画布是唯一的编辑/部署真值源。DSL 编译产物输出 `WorkflowGraph` JSON（Nazh 自控契约），通过画布导入路径进入画布（auto-layout 补齐画布元数据），下游复用现有前端转换 + DAG 校验 + 部署管道。用户主要通过 AI 交互生成/修订 DSL，也支持手动编辑作为兜底。
> 2. **画布载入后 DSL 源变为历史快照。** DSL 编译产物载入画布后，画布是独立副本，用户可在画布上继续编辑（拖拽、连线、改配置）。DSL 源与画布状态不再双向同步——DSL 是 AI 的结构化工作语言，画布是运行时真值。若用户需从画布回到 DSL，视为重新生成（新的 AI 编排会话）。
> 3. Device/Capability 注册表的**源级真值是持久项目 / 环境资产**，不是 per-deployment 临时产物。设备建模控制台产出版本化的 Device DSL / Capability DSL；Workflow DSL 编译时读取选定版本的设备 / 能力快照。部署产物是引用快照还是烘焙必要字段，留到实施 ADR 决定。
> 4. DSL 编译器位于独立 crate `crates/dsl-compiler/`，依赖 `dsl-core`（Spec 类型 + YAML parser），**不依赖** `nazh-engine` / `src/graph/`（避免 facade 反向依赖）。在 `WorkflowGraph` 类型尚未独立成 schema crate 前，编译器输出符合 Nazh `WorkflowGraph` JSON 契约的 DTO / `serde_json::Value`，经画布导入管道消费，并通过 `WorkflowGraph::from_json()` conformance 测试守护 schema 漂移。
> 5. **DSL 是 AI 的源级结构化契约**：设备建模 AI 可以生成 / 修订 / 解释 Device DSL、Capability DSL；编排 AI 只能基于已审查设备 / 能力快照生成 / 修订 Workflow DSL。编译产物（`WorkflowGraph` JSON）只作为编译后的派生产物导入画布，不作为 AI 绕过 DSL 的源级输出。AI 不直接绕过 DSL / 编译器 / Safety Compiler / 画布部署管道发起设备动作。
> 6. **前端新增两个独立页面：`设备建模控制台` 与 `AI 编排控制台`**。设备建模控制台承载说明书 → 设备资产流程；AI 编排控制台承载自然语言目标 → Workflow DSL → 编译 / Safety 反馈 → 导入画布。画布页继续作为唯一部署入口，不承载 DSL 源级审查。
> 7. **Workflow DSL 状态机编译为 `stateMachine` 节点 + 无环 action/capability DAG**。状态循环、当前状态、guard、timeout、fault fallback 留在 `stateMachine` 节点内部；外层 `WorkflowGraph.edges` 继续保持 DAG，不修改现有 Kahn 校验，也不让 action 节点回连状态机。

### DSL 资产生命周期

```
设备建模控制台:
  说明书 / 点表 / 寄存器表 / 协议文档
                              ↓
                    AiService 抽取 / 对齐 / 标注不确定项
                              ↓
                    Device DSL + Signal DSL + Capability DSL 草稿
                              ↓
                    人工审查 / 来源追溯 / 连接测试 / Safety 预检
                              ↓
                    版本化设备资产库
                    (DeviceSpec / SignalSpec / CapabilitySpec)
                              ↓

AI 编排控制台:
  设备资产快照 + 能力目录 + 自然语言业务目标
                              ↓
                    AiService 生成 / 修订 Workflow DSL
                              ↓
                    Workflow DSL（引用 device_id / capability_id）
                              ↓
                    dsl-compiler: 快照 + WorkflowSpec → Nazh WorkflowGraph JSON
                              ↓
                    画布导入（auto-layout 补齐画布元数据）
                              ↓
                        ┌─────────────────────┐
                        │  FlowGram 画布       │  ← 唯一编辑/部署真值源
                        │  渲染 + 确认 + 部署  │
                        └─────────┬───────────┘
                                  ↓
                    前端转换 → Nazh WorkflowGraph JSON
                                  ↓
                    src/graph/ 解析 → WorkflowGraph  ← 唯一 DAG 校验层
                                  ↓
                    deploy_workflow()
```

### AI 编排入口：单通道多输入

```
AI 编排控制台:
  设备资产快照 / 能力目录 / 自然语言目标
                              ↓
                    AiService 辅助生成 / 修订 / 解释
                              ↓
                    Workflow DSL（AI 的结构化工作语言，可审查、可 diff、可校验）
                              ↓
                    用户可手动编辑 Workflow DSL（兜底）
                              ↓
                    dsl-core 解析 WorkflowSpec
                              ↓
                    dsl-compiler:
                      设备/能力快照 + WorkflowSpec → Nazh WorkflowGraph JSON
                              ↓
                    画布导入（auto-layout 补齐画布元数据）
                              ↓
                        ┌─────────────────────┐
                        │  FlowGram 画布       │  ← 唯一编辑/部署真值源
                        │  渲染 + 确认 + 部署  │
                        └─────────┬───────────┘
                                  ↓
                    前端转换 → Nazh WorkflowGraph JSON
                                  ↓
                    src/graph/ 解析 → WorkflowGraph  ← 唯一 DAG 校验层
                                  ↓
                    deploy_workflow()

手动拖拽（同一画布，同一入口）:
  FlowGram 画布拖拽 → 编辑态 JSON → 前端转换 → DAG 校验 → deploy
```

**核心收益：单通道 + 编译器耦合自控 schema。** `WorkflowGraph` JSON 是 Nazh 自己定义的部署契约（前端有 ts-rs 导出类型，Rust 端有完整解析 / 校验），编译器输出这个已有格式，画布端 auto-layout 补齐画布元数据后渲染。编译器不需要了解 FlowGram 内部数据结构，耦合方向从"第三方 SDK 内部格式"回到"自己控制的部署契约"。

当前前端已经有基础的 `WorkflowGraph → FlowGram` 转换和简单层级布局；ADR-0021 要求把它产品化为 AI 编排控制台到画布页的导入能力，而不是要求 DSL 编译器直接生成 FlowGram 编辑态 JSON。

### AI 编排边界

AI 编排分为五层，只有前三层参与本 ADR 的部署汇合决策：

1. **Device Modeling AI**：从说明书、点表、寄存器表、协议文档生成 / 修订 Device DSL、Signal DSL、Capability DSL 草稿；必须经人工审查与版本化保存后，才能被编排引用。
2. **Workflow Authoring AI**：从已审查设备 / 能力快照和自然语言目标生成 / 修订 Workflow DSL。
3. **Compiler AI 辅助**：解释 DSL 编译错误、Safety Compiler 拒绝原因，并生成 DSL patch proposal。
4. **Runtime AI 节点**：通过现有 `AiService` trait 在节点内部做局部推理（分类、诊断、摘要、脚本辅助等），仍受 `NodeTrait`、Pin、metadata、timeout、panic isolation 约束。
5. **Autonomous Orchestration AI**：运行时自主决定下一步工业动作。本 ADR 明确不接受该层绕过 DSL / 编译 / Safety 管道直接执行动作；若未来引入，也必须输出 DSL patch，经同一部署闸门重新编译。

因此，本 ADR 的约束是：

- AI 生成的源级可部署产物必须落到 DSL；`WorkflowGraph` proposal 只允许作为编译后的派生产物或预览产物。
- AI 编排不能直接消费未审查说明书；说明书 / 点表 / 协议文档必须先进入设备建模控制台，产出版本化设备 / 能力资产。
- Capability DSL 是 AI 可见的设备能力目录，包含输入、前置条件、副作用、fallback、安全等级；AI 不直接操作寄存器 / topic / serial command。
- Workflow DSL 是 AI 编排 plan 的主落点；自然语言 plan 不可直接部署。
- Safety Compiler 拒绝的产物不得自动降级部署；AI 只能解释原因并提出修订。
- 所有最终部署仍必须经过画布页 → 前端转换 → `WorkflowGraph::from_json()` → `src/graph/` DAG 校验 → Tauri IPC 运行时。AI 编排控制台只产生 `WorkflowGraph` JSON，不直接触发部署。

### 前端页面：设备建模控制台

设备建模控制台是 **说明书 to 设备** 的产品入口。它把非结构化设备资料转成可版本化、可审查、可被 AI 编排引用的设备资产。

核心区域：

- **资料栏**：说明书、点表、寄存器表、协议文档、现场补充说明。每份资料保留文件名、版本、导入时间和来源。
- **AI 抽取栏**：AI 生成 Device DSL、Signal DSL、Capability DSL 草稿，并列出不确定项、缺失字段、冲突字段。
- **设备模型编辑区**：结构化编辑设备、信号、单位、量程、寄存器 / topic / 串口命令、缩放表达式和连接引用。
- **能力目录编辑区**：把底层信号 / 写操作封装为 capability，补齐输入参数、前置条件、副作用、fallback、安全等级、是否需要人工审批。
- **来源追溯 / 置信度栏**：每个 signal / capability 字段可追溯到说明书页码、表格行或人工输入；无法追溯的字段必须标记为人工确认。
- **连接测试与保存**：可用现有连接配置做读写 dry-run / 只读探测；保存时生成设备资产版本。

设备建模控制台的状态机：

```
Imported
  → AiExtracting
  → ReviewRequired
  → Validating
  → ReadyAsAsset | ValidationFailed
```

`ReadyAsAsset` 后的设备 / 能力快照才能进入 AI 编排控制台。

### 前端页面：AI 编排控制台

ADR-0021 的编排入口是新增一个主导航页面，暂命名为 **AI 编排控制台**。它消费设备建模控制台产出的设备 / 能力资产快照——用户通过 AI 对话生成/修订 Workflow DSL，AI 以 DSL 作为结构化交付物，编译通过后导入画布确认部署。也支持用户直接手写/编辑 Workflow DSL 作为兜底。目标用户是需要快速编排设备工作流的工程人员。

页面采用操作台布局，避免营销式 hero 或说明页：

```
左侧 AI 对话栏    中间 DSL 编辑区                       右侧审查栏
AI 对话历史       ┌ workflow.yaml + 只读设备/能力快照              ┐
AI 生成/修订入口  │ YAML editor + diagnostics gutter               │
                  └────────────────────────────────────────────────┘

顶部上下文栏     设备资产版本 / 能力目录版本 / 连接环境
底部状态带       编译状态 / Safety 状态 / 派生 WorkflowGraph 摘要 / "导入画布"按钮
```

核心区域：

- **AI 对话栏**：与 AI 的交互界面。用户以自然语言描述编排目标，AI 通过 `AiService` 读取设备 / 能力快照并生成/修订 Workflow DSL。AI 输出以 DSL patch proposal 形式呈现，用户逐 hunks 接受/拒绝。AI 对话历史保留在同一会话内，方便回溯。
- **DSL 编辑区**：YAML 编辑器（优先复用现有编辑器组件），默认展示 Workflow DSL；Device / Capability DSL 以只读快照方式可查看，不在本页直接改源级设备资产。用户可以直接手动编辑 Workflow DSL（兜底）。错误行号、警告、AI patch hunk 都在 gutter 标记。
- **审查栏**：编译诊断、Safety 校验结果、AI uncertainties/warnings 摘要。每条错误必须能回跳到 DSL 文件位置，无法定位时落到全局诊断。
- **编译与 Safety 面板**：按 `Parse → Semantic → Safety → WorkflowGraph JSON` 阶段展示结果。
- **"导入画布"按钮**：编译通过 + Safety 通过后激活。点击后将 `WorkflowGraph` JSON 导入画布页（通过现有项目库 IPC 或画布导入 API），画布页 auto-layout 渲染，由用户在画布上可视化确认后部署。AI 编排控制台不设自己的部署闸门。

页面状态机：

```
Draft
  → AiProposed
  → Compiling
  → SafetyRejected | CompileFailed | ReadyToCanvas
```

状态含义：

- `Draft`：用户手写或导入 DSL，尚未编译。
- `AiProposed`：存在 AI patch proposal，未全部接受 / 拒绝。
- `Compiling`：DSL parser、semantic check、Safety Compiler、`WorkflowGraph` JSON 派生进行中。
- `CompileFailed`：语法 / 引用 / schema / `WorkflowGraph` JSON 派生失败。
- `SafetyRejected`：DSL 能编译，但 Safety Compiler 拒绝。
- `ReadyToCanvas`：已生成 `WorkflowGraph` JSON，且 Safety 通过；"导入画布"按钮激活。

`Deployed` / `Previewed` 状态不出现在 AI 编排控制台——部署由画布页负责，AI 编排控制台只负责到 `ReadyToCanvas`。

页面与现有系统边界：

- 不替代 FlowGram 画布页；画布页是唯一编辑/部署真值源。AI 编排控制台是 AI 结构化编排的入口。
- 不直接编辑运行时 `WorkflowGraph`；源级编辑真值是 DSL，运行时真值是画布。
- 不新增运行时 AI 执行动作入口；AI 只提交 DSL patch proposal。
- 不在编排页重新解释说明书；未建模资料必须先进入设备建模控制台。
- 不在编排页直接修改 Device / Capability 源级资产；需要修改设备模型时跳回设备建模控制台并生成新版本。
- 既有画布侧 AI 编排能力若继续保留，只能作为过渡 / 实验性 `WorkflowGraph` 草图生成器；正式 AI 编排入口迁到本控制台，并以 DSL patch proposal 作为源级交付物。
- 不绕过项目库；DSL 文件应归属当前 Project / Environment，保存与快照机制复用现有项目库概念。
- 不把连接密钥写入 DSL；Device DSL 引用现有 `ConnectionDefinition` 的 `connection_id`。
- 复用现有 `AiService` trait（ADR-0019）进行 AI 对话与 DSL 生成，不新增 provider 抽象。

### 状态机运行时模型

```
事件 / tick / 传感器输入 / 人工触发
        ↓
  stateMachine 节点（有状态决策子运行时）
        ↓ NodeDispatch::Route(action_port)
  action / capability DAG（普通无环节点）
        ↓
  result / observability / 下游业务节点
```

状态机不是一组互相回连的 DAG 节点，而是 DAG 中的一个有状态决策节点。外层 DAG 只表达"事件进入状态机"和"状态机输出动作"；`idle → running → idle` 这类循环只存在于 `stateMachine` 的内部状态表，不进入 `WorkflowGraph.edges`。

`stateMachine` 的职责：

- 读取当前状态、事件 payload、WorkflowVariables 和可用的 Data pin 输入。
- 按 `transitions` 顺序评估 guard / condition，选择下一状态。
- 更新当前状态、`entered_at`、最近 transition 等控制面状态。
- 根据 entry / exit / transition action 通过 `NodeDispatch::Route([...])` 发出 action port。
- 输出 `state_machine` metadata，包含 `from_state`、`to_state`、`event`、`matched_transition`、`guard_result`、`action_routes`、`entered_at` 等观测字段。

`stateMachine` 明确不做的事：

- 不直接访问硬件、协议连接、寄存器、topic 或串口命令；所有 I/O 仍由下游 action / capability 节点经 `ConnectionManager` / `ConnectionGuard` 执行。
- 不把 `_state`、`_transition` 等控制信息塞进业务 payload；状态与转移观测走 `WorkflowVariables` + metadata。
- 不要求 action / capability 节点回连到 `stateMachine`；动作完成后的下一次转移由后续设备事件、timer tick、人工 dispatch 或其他根输入再次触发状态机。
- 不修改 `src/graph/` 的 DAG 校验，不引入有环调度。

状态存储：

- `current_state`、`entered_at`、`last_transition` 写入 `WorkflowVariables`，例如 `state_machine.<node_id>.current_state`。
- Phase 1 只支持单活跃状态（Sequential）；同一个 `stateMachine` 节点由单个 Runner task 串行处理输入，避免并发 transition 竞争。
- 若未来 Runner 支持同节点并行 transform，`stateMachine` 必须引入 per-node 状态锁或 CAS 语义，届时需独立 ADR。

timeout 模型：

- timeout 是 `stateMachine` 内部规则，不展开为状态回边。
- 进入 state 时记录 `entered_at`；每次事件或 tick 到达时检查 timeout。
- tick 可由普通 `timer` 节点单向触发 `stateMachine`，但 `stateMachine` 不回连 timer。

action / capability 编译模型：

- Workflow DSL 中的 capability 调用编译为下游普通节点或 `capabilityCall` adapter（具体二选一留给实施 ADR）。
- `stateMachine` 的输出 port 与 action group 绑定，例如 `enter_pressing`、`fault_stop`、`return_home`。
- 多个 entry action 可编译为该 port 下游的一段无环 action DAG；状态机只负责触发，不等待下游 action 回连。

并发状态（同一 workflow 多个 state 同时活跃）暂不支持——Phase 1 状态机严格线性，每次只有一个活跃 state。并发状态机留作后续 ADR。

### 编译流水线

```
设备资产版本      → dsl-core 加载 → DeviceSpec / SignalSpec
能力目录版本      → dsl-core 加载 → CapabilitySpec
                                    ↓
                          Capability 注册表构建
                          (HashMap<CapabilityId, CapabilitySpec>)
                                    ↓
workflow.yaml     → dsl-core 解析 → WorkflowSpec
                                    ↓
	                    dsl-compiler:
	                      1. 校验引用完整性（device/capability 存在性、信号类型匹配）
	                      2. states / transitions / timeout → stateMachine 节点 config
	                      3. entry / exit / transition actions → action route ports
	                      4. capability 调用 → 下游普通节点或 capabilityCall adapter
	                      5. 可选 tick → timer → stateMachine 单向边
	                      6. 输出符合 Nazh WorkflowGraph JSON 契约的 DTO / serde_json::Value
                                    ↓
                    画布导入（auto-layout 补齐画布元数据）→ FlowGram 渲染
                                    ↓
                    src/graph/ 现有解析管道
                      → WorkflowGraph
                      → deploy_workflow()
```

### Device/Capability 注册表作用域

```rust
/// 项目 / 环境级设备资产注册表，源级真值由设备建模控制台维护。
pub struct DeviceAssetRegistry {
    pub version: AssetVersion,
    pub devices: HashMap<DeviceId, DeviceSpec>,
    pub capabilities: HashMap<CapabilityId, CapabilitySpec>,
    pub signals: HashMap<SignalId, SignalSpec>,
}
```

- `DeviceAssetRegistry` 是源级持久资产，随 Project / Environment 保存和版本化，不随 `deploy_workflow()` 临时创建
- Workflow DSL 编译时读取选定的设备资产版本；编译诊断必须能指出缺失 capability、信号类型不匹配、资产版本不兼容等问题
- 部署产物可选择引用设备资产版本，也可把执行所需字段烘焙进节点 config；这属于实施 ADR 的取舍，但不能改变源级真值在设备资产库中的事实
- Capability 调用节点若采用运行时注册表模型，也必须绑定到部署时确认过的资产快照，不能查"当前最新"导致部署后语义漂移
- 不污染全局 `ConnectionManager`——设备连接仍走现有 `ConnectionDefinition` + `ConnectionGuard` RAII
- 若采用编译期烘焙模型，寄存器地址、协议参数等必要字段在编译阶段写入 `WorkflowGraph` 节点 config，运行时无需再查注册表；若采用运行时快照模型，部署生命周期内显式持有不可变快照

## 可选方案

### 方案 A：编译到 `WorkflowGraph` JSON，经画布导入路径部署（本 ADR 推荐）

YAML → `WorkflowGraph` JSON → 画布导入（auto-layout）→ 前端转换 → `WorkflowGraph` → `deploy_workflow()`。画布是唯一部署入口，编译器耦合 Nazh 自控 schema。

- **优势**：编译器耦合 `WorkflowGraph` JSON（Nazh 自己的部署契约，Rust 端已有解析 / 校验，前端有 ts-rs 导出类型），不依赖第三方 SDK 内部格式；画布导入能力（auto-layout + `WorkflowGraph` JSON 加载）是通用能力；`deploy_workflow()` 零改动；未来 DAG 校验改进自动覆盖；编译器职责单一（DSL 语义校验 + 语义映射到 `WorkflowGraph` JSON）
- **劣势**：在 `WorkflowGraph` Rust 类型拆出独立 schema crate 前，编译器只能用 DTO / JSON 契约 + conformance 测试守护漂移；`WorkflowGraph` JSON 缺少画布布局信息，auto-layout 质量影响首次渲染体验

### 方案 B：直接构造 WorkflowGraph Rust 类型

YAML → `WorkflowGraph` Rust 类型，绕过 JSON 序列化。

- **优势**：类型约束更强；可少一层 JSON 序列化 / 反序列化；编译器在 Rust 端直接复用 `WorkflowGraph` 类型
- **劣势**：`dsl-compiler` 需要依赖导出 `WorkflowGraph` 的 crate，可能受 ADR-0020 的 `src/graph/` 归属影响；仍需序列化为 JSON 传递到前端画布；若绕过 `WorkflowGraph::from_json()`，仍需确认不会跳过 normalize / validate 逻辑

### 方案 C：引入独立 IR

新增 `WorkflowIR` crate，DSL → IR → `WorkflowGraph` JSON / `WorkflowGraph` Rust 类型。

- **优势**：IR 层可做状态机特有校验（不可达状态、循环触发）；编译错误可映射回 YAML 行号
- **劣势**：Phase 1 状态机简单（线性），IR 收益有限；额外 crate + 额外类型维护；增加实施周期

### 方案 C2：状态机作为 `stateMachine` 子运行时（本 ADR 采纳）

Workflow DSL 的状态表编译为一个 `stateMachine` 节点 config，状态循环在节点内部处理；该节点通过 `NodeDispatch::Route` 触发下游 action / capability DAG。

- **优势**：外层 `WorkflowGraph` 仍是 DAG，复用现有 Kahn 校验、Runner、Pin 校验、observability 和部署管道；状态机可表达循环、timeout、fault fallback；硬件动作仍由普通节点经 `ConnectionGuard` 执行
- **劣势**：`stateMachine` 成为一个小型子运行时，需要额外测试状态转移、timeout、并发输入顺序和 metadata；action 完成不会自动回连状态机，业务必须用设备事件 / tick / 人工 dispatch 再次触发下一次转移

### 方案 C3：把状态机直接展开成有环 DAG（拒绝）

每个 state 编译为子图，transition 编译为边，允许 `idle → running → idle` 这类回边进入 `WorkflowGraph.edges`。

- **优势**：画布上看起来直观，每个状态 / 转移都可视化成节点和边
- **劣势**：直接违反当前 `WorkflowGraph::validate()` 的 DAG 不变量；需要重写 Runner 的循环调度、背压、死信、timeout 和可观测语义；也会让 Data / Exec / Reactive 边的环检测复杂化。本 ADR 明确拒绝该方案

### 方案 C4：修改 DAG Runner 原生支持循环（拒绝）

保留节点级展开，但让 `src/graph/` 支持有环图调度。

- **优势**：理论上可以让状态机、反馈控制、循环数据流共享同一图模型
- **劣势**：破坏现有 DAG 编排的简单性；需要引入迭代边界、队列背压、重复触发去重、终止条件、cycle observability 等大量新语义；风险远超 ADR-0021 的 AI 编排入口范围。本 ADR 拒绝把状态机需求升级为 Runner 级循环图能力

### 方案 D：允许两通道混用

同一 workflow 可同时包含画布拖拽节点和 DSL 声明节点，运行时合并。

- **优势**：灵活——简单部分画布拖，复杂状态机用 DSL
- **劣势**：合并冲突难检测（节点 ID 碰撞、边语义冲突）；调试困难（错误发生在哪个通道？）；版本管理混乱（YAML diff + JSON diff 混合）；Phase 1 不值得

### 方案 E：运行时 AI 直接编排设备动作

LLM 在运行时读取状态、选择 capability / 寄存器操作并立即执行，DSL 只作为可选提示上下文。

- **优势**：演示效果强；自然语言交互最直接；可快速探索未知流程
- **劣势**：绕开 DSL / Safety Compiler / DAG 校验；动作不可稳定复现，审计链弱；违反工业边缘系统"可解释、可回放、可拒绝"要求；与 Capability DSL 的安全边界冲突。本 ADR 明确拒绝该方案作为可部署路径

### 方案 F：不新增页面，AI 编排嵌入画布页

在现有 FlowGram 画布页增加一个 AI 编排 drawer 或弹窗，作为 AI 编排入口。

- **优势**：改动范围小；用户仍停留在已有画布工作区；可复用现有 AI composer 入口
- **劣势**：AI 对话、DSL 审查、Safety 诊断、编译反馈会挤在侧栏里，难以形成工程审查闭环；也容易把 AI 编排会话与画布编辑态混在一起。本 ADR 选择独立 AI 编排控制台 + 画布统一部署入口

## 后果

### 正面影响

- **消除双通道概念，简化架构**——DSL 是 AI 编排的结构化输入语言，不是平行通道；画布是唯一真值源，心智模型简单
- **编译器耦合自控 schema**——输出 `WorkflowGraph` JSON（Nazh 自己的部署契约），不依赖第三方 SDK 内部格式
- **DAG 校验层唯一**——`src/graph/` 是唯一 DAG 结构校验入口，维护成本最小
- **状态机循环不污染 DAG**——状态转移、timeout、fault fallback 留在 `stateMachine` 子运行时内部，外层图继续无环
- **AI 编排有明确闸门**——AI 输出先落 DSL patch，再经编译与 Safety 校验派生 `WorkflowGraph` JSON 导入画布，避免运行时黑盒动作
- **产品路径完整**——设备建模控制台（说明书 to 设备资产）+ AI 编排控制台（设备能力 to Workflow DSL）+ 画布（可视化确认/部署），职责清晰
- Capability DSL 可作为 AI tool catalog 的安全来源，但不会让 AI 直接接触协议细节
- DSL 编译器独立 crate，不侵入 Ring 0 / 现有 `src/graph/`
- `deploy_workflow()` 零改动
- 画布导入能力（`WorkflowGraph` JSON + auto-layout）是独立有价值的通用功能

### 负面影响

- 画布端已有基础 `WorkflowGraph` JSON 导入 / auto-layout 函数，但需要产品化为跨页面导入、诊断联动、复杂图布局与项目保存流程
- 线性状态机限制表达能力——工业场景常见并发（多轴联动、多工位并行），Phase 1 无法覆盖
- `stateMachine` 引入子运行时复杂度，需要额外处理 transition 顺序、timeout tick、metadata、变量写入和 action route 的一致性
- 设备资产持久化和版本管理会增加项目库复杂度，需要处理资产迁移、快照兼容、回滚和"部署使用旧资产版本"的 UX
- 部分编译错误信息停在 `WorkflowGraph` / DAG 层，无法直接指向 YAML 行号（需额外 source-map 机制，留后续迭代）
- AI 编排不能直接"边想边执行"，交互链路变长；需要用高质量编译反馈和 patch proposal 补偿体验
- 新增页面增加前端导航、项目库、保存/快照的产品复杂度
- AI 编排控制台到画布页的导入需要跨页面通信机制（`WorkflowGraph` JSON 传递 + 画布加载）

### 风险

| 风险 | 缓解 |
|------|------|
| `WorkflowGraph` schema 变更导致编译器断裂 | `WorkflowGraph` 是 Nazh 自控的部署契约；Phase 1 用 DTO / JSON snapshot + `WorkflowGraph::from_json()` conformance 测试同步暴露漂移，若后续拆出 schema crate 再改为共享 Rust 类型 |
| Capability 调用节点运行时找不到注册表信息 | 编译时必须绑定设备资产版本；若选择编译期模型，将寄存器地址/协议参数烘焙进节点 config；若选择运行时模型，部署生命周期内持有不可变资产快照 |
| `stateMachine` config 过大或难以审查 | AI 编排控制台显示状态 / transition / action 摘要；Phase 1 限制 state / transition 数量上限，超限需拆分 workflow |
| action 完成后无法自动回到状态机 | Phase 1 明确采用事件驱动模型：下一次转移由设备事件、timer tick 或人工 dispatch 再次触发；需要同步写入 DSL 指南 |
| 状态转移并发竞争 | Phase 1 依赖单个 Runner task 串行处理同一 `stateMachine` 输入；若未来同节点并行 transform，必须先补 per-node 状态锁 / CAS ADR |
| 并发状态机需求提前到来 | 预留 `WorkflowSpec` 的 `concurrency` 字段（默认 `Sequential`），后续 ADR 扩展 |
| AI 生成 DSL 看似合理但缺少现场约束 | Safety Compiler 必须校验单位、量程、权限、前置条件、fallback；AI 输出保留 uncertainties / warnings 供人工审查 |
| 说明书抽取错误被编排放大 | 设备建模控制台必须保留来源追溯、置信度、人工确认状态；未确认字段不得进入 `ReadyAsAsset` |
| 用户期望 AI 一键部署危险动作 | AI 编排控制台只提供"导入画布"按钮，部署由画布页统一管控；危险 capability 必须走审批或显式确认 |
| 画布编辑后与 DSL 源分叉 | 设计意图：DSL 源载入画布后变为独立副本，画布是运行时真值。若需回到 DSL，视为新的 AI 编排会话（重新生成） |
| 编译诊断无法定位到 YAML 行 | dsl-core parser 需保留 source span；无法定位的错误进入全局诊断区，不允许吞掉或只显示泛化失败 |
| auto-layout 质量不佳导致画布首次渲染混乱 | Phase 1 可沿用现有层级布局；复杂图达到阈值后引入 dagre / elkjs / ELK 等布局引擎。`stateMachine` 节点内部状态不需要画布布局（子运行时模型） |

## 备注

- 本 ADR 仅覆盖编译策略、设备建模 / AI 编排入口、资产生命周期和画布导入边界。Device / Capability / Workflow 三段 Spec 的字段设计由 RFC-0004 Phase 0 锁定。
- DSL parser crate 命名 `dsl-core` / 编译器 crate 命名 `dsl-compiler` 遵循 RFC-0004 提议，可在实施 ADR 中调整。
- Safety Compiler（单位校验、量程校验、可达性校验）不在本 ADR 范围——属于 RFC-0004 Phase 5，需独立 ADR。
- Crate 归属与 ADR-0020（`src/graph/` 长期归属）的关系：在 `WorkflowGraph` 类型仍位于 facade 时，`dsl-compiler` 不依赖 `nazh-engine` / `src/graph/`，只输出 JSON 契约并用 conformance 测试验证；若后续拆出 `crates/graph/` 或 `workflow-schema`，实施 ADR 可改为依赖共享 schema crate。
- `stateMachine` 节点实现候选放在 `crates/nodes-flow/` 或后续独立 `crates/nodes-state/`。不进入 `crates/core/`；Ring 0 只保留 `WorkflowVariables`、`NodeDispatch`、metadata 等通用抽象。
- **编译器直接输出 `WorkflowGraph` JSON 契约**——复用 Nazh 自控的部署契约作为编译器输出格式，画布端 auto-layout 补齐画布元数据。编译器不耦合 FlowGram 内部数据结构。
- AI 相关能力复用 ADR-0019 的 `AiService` trait；本 ADR 不新增 provider 抽象。AI 编排控制台的对话功能复用现有 `copilot_complete` / `copilot_complete_stream` IPC 管道，新增 DSL 结构化输出模式。
- 既有 `web/src/lib/workflow-orchestrator.ts` 属于前 DSL 过渡方案；ADR-0021 被接受后，应迁移为 DSL patch proposal 生成器，或明确降级为实验性 WorkflowGraph 草图工具。
