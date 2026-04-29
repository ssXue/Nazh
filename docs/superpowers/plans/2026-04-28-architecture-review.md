> **Status:** Phase C/D/E completed 2026-04-29; Phase A remains open, freeze retained

# 2026-04-28 架构 review + 模块拆分 + 规范扫描

**Goal**：在当前所有 in-flight ADR 完成后，对 Ring 0 + Ring 1 + facade + IPC + 前端的接口、数据结构、文件规模、规范符合度做一次系统性 review。输出 (1) 修复 PR 清单 (2) 模块拆分 PR 清单 (3) 规范不符项清单。

**Architecture**：不动现有 ADR 决策；仅做"接口对齐 / 文件拆分 / 规范修复"。重大设计修改若涉及 ADR 必须新立 ADR（freeze 期内不允许）。

**Tech Stack**：现有 Rust workspace 9 crates + Tauri v2 + React。

**关联**：`AGENTS.md` 顶部的 ARCHITECTURE FREEZE 段；解冻条件 = 本 plan 退出标准全勾。

---

## Phase A: ADR Sprint（Day 0..3）

顺序：**ADR-0014 全 Phase → Phase 6 EventBus → ADR-0015/0016**。`loop 升级容器`独立，可任意时机插入。

### ADR-0014 引脚求值语义二分

- [x] **Phase 3** PURE 节点 — `docs/superpowers/plans/2026-04-28-adr-0014-phase-3-pure-nodes.md`（merged in f1f23a2）
- [x] **Phase 3b** lookup + mixed input — `docs/superpowers/plans/2026-04-28-adr-0014-phase-3b-lookup-mixed-input.md`
- [x] **Phase 4** cache lifecycle — `docs/superpowers/plans/2026-04-28-adr-0014-phase-4-cache-lifecycle.md`
- [x] **Phase 5** visual + AI — `docs/superpowers/plans/2026-04-30-adr-0014-phase5-visual-ai.md`

### Phase 6 EventBus + EdgeBackpressure + ConcurrencyPolicy（RFC-0002）

- [x] RFC-0002 Phase 6 已完成（方案修订）。EventBus broadcast 已否决（Lagged 语义冲突），ConcurrencyPolicy / EdgeBackpressure 推迟（无实际场景）。实际修复：`emit_event` 改 `try_send` + 错误日志。详见 `docs/rfcs/0002-分层内核与插件架构.md` Phase 6 段。无需单独 plan。

### ADR-0015 反应式数据引脚

- [x] 设计 spec：`docs/superpowers/specs/2026-04-30-adr-0015-reactive-data-pin-design.md`（watch channel 方案，修订 ADR 原文 broadcast 设计）
- [x] Phase 1 plan + 实施：`docs/superpowers/plans/2026-04-30-adr-0015-phase1-reactive-edge.md`（merged in 9019b90）
- [x] Phase 2 实施（变量 Reactive + IPC）：`docs/superpowers/plans/2026-04-30-adr-0015-phase2-3-ipc-frontend.md`（merged in 9a838b1）
- [x] Phase 3 实施（前端 UI）：同上 plan

### ADR-0016 边级可观测性

- [ ] 新建 plan：`docs/superpowers/plans/2026-04-XX-adr-0016-edge-observability.md`
- [ ] 按 plan 实施

### loop 升级容器（独立）

- [x] 把 origin commit `e35cb43` 的工作带回（merge 68ab709 时丢失；当前 `main` 已包含）

### 每个 ADR 完成时同步

- [x] ADR 状态推进：提议中 → 已接受 → 已实施
- [x] 同步 `docs/adr/README.md` 索引行
- [x] 同步 `crates/*/AGENTS.md` 影响内容
- [x] prepend `> **Status:** merged in <SHA>` 到对应 plan 文件顶部

---

## Phase B: 接口与数据结构 Review（Day 4..8，按 crate 切片，可并行）

每片产出独立 findings 文档：`docs/superpowers/specs/2026-05-XX-review-<topic>-findings.md`。

### B1. Ring 0 接口审计（crates/core）

- [x] 对每个 public trait / struct 产出评估表：
  - cohesion（职责是否单一）
  - 对称性（生产端 vs 消费端形状）
  - 可演化性（加新 variant / 字段是否破 ABI）
  - 命名（动词时态、`is_`/`has_` 前缀、`_` 命名）
  - 建议动作（保留 / 重命名 / 拆 / 删）
- [x] 已识别问题（从前期 mini-review）：
  - [x] `ExecutionEvent::VariableChanged` 是否拆出 `ExecutionEvent`（已识别为接口腐化）
  - [x] `NodeOutput.metadata: Map` vs `CompletedExecutionEvent.metadata: Option<Map>` 不对称
  - [x] `OutputCache` 的 `DashMap` 是否过度（部署期一次性 prepare、运行时仅读）
  - [x] `PinDefinition` 工厂方法数量监控（>= 6 时改 builder）
- [x] `AiService` trait 接口审计（独立子项）
- [x] `WorkflowVariables` API 审计（mutation/snapshot/declarations 三组方法是否清晰）

### B2. Ring 1 横向耦合审计

- [x] 生成 mermaid 真实依赖图 vs ADR 宣称对照
- [x] 5 个 Ring 1 crate 间是否有重复抽象
  - [x] `connections` vs 各协议节点的 `connection_id` 处理
  - [x] `scripting` vs `nodes-flow` 的 Rhai engine 实例化路径
  - [x] `ai` vs `scripting` 的 `ai_complete()` 注入点
- [x] 每个 Ring 1 crate 的 `AGENTS.md` 是否仍准确（与代码对照）

### B3. Facade 编排层审计（src/）

- [x] `src/graph/` 4 模块（`deploy` / `topology` / `pin_validator` / `runner`）职责重叠检查
- [x] `standard_registry()` 与各 plugin 注册路径是否清晰
- [x] 评估 ADR-0020（`src/graph/` 归属）触发条件是否已到

### B4. IPC 边界审计

- [x] `src-tauri/src/lib.rs` 全 IPC 命令 + 事件清单（与 `AGENTS.md` 列表对照）
- [x] 哪些 IPC type 应该挪到 `crates/tauri-bindings/`
- [x] 事件 channel 命名一致性（`workflow://*`）
- [x] `ExecutionEvent::VariableChanged` 拆出与 IPC 事件 channel 是否对齐

### B5. 前端契约审计

- [x] `web/src/generated/` ts-rs 类型与手写 `web/src/types.ts` 的边界
- [x] `web/src/lib/{pin-*,node-*,workflow-*}.ts` 与 Rust 真值源同步状态
- [x] FlowGram 适配层 `flowgram.ts` 的 fallback / hack 清单（特别是 E2E fallback 路径）

---

## Phase C: 模块拆分（Day 4..8，与 Phase B 并行）

按"行数 + 职责重叠"优先级排序。

- [x] **行数普查**
  - [x] Rust：`tokei` 当前环境不可用，改用 `find crates src src-tauri/src -path '*/target/*' -prune -o -name '*.rs' -type f -print0 | xargs -0 wc -l | sort -rn | head -30`
  - [x] TS：`find web/src -type f \( -name '*.ts' -o -name '*.tsx' \) -print0 | xargs -0 wc -l | sort -rn | head -30`
  - [x] 输出 > 500 行清单到 findings 文档
- [x] **`src-tauri/src/lib.rs`** 当前 2,675 行 → 按 IPC 命令域拆分
  - 草案分组：`commands/{connections,ai,observability,runtime,project_library}.rs` + `events.rs`
  - `lib.rs` 只做 register + setup
  - 目标：`lib.rs` < 500 行
  - 同 PR 更新 `AGENTS.md` IPC surface 段
- [x] **其他 > 500 行文件按 review 输出清单逐个评估**
  - 每个文件单独决策：拆 / 保留 / 改架构
- [x] **拆分原则**
  - 不破坏 public API
  - 拆分前后 `cargo test --workspace` 通过率不变
  - 同 PR 更新对应 crate `AGENTS.md` 模块表

---

## Phase D: 规范扫描（Day 4..8，与 Phase B 并行；可全自动化）

对照 `AGENTS.md` "Critical Coding Constraints" + "Design Principles" 全仓 grep + 人工复核。

- [x] **`.unwrap()` / `.expect()` 出现位置**（test 模块除外）
  - 命令：`rg '\.(unwrap|expect)\(' crates/ src/ src-tauri/ -t rust -n | rg -v 'tests?\.rs|#\[cfg\(test\)\]'`
  - 每个命中判定：真违规 → 修；test helper → 加 `#[allow]`；初始化路径 → 评估
- [x] **`unsafe` 出现位置**
  - 命令：`rg 'unsafe\s+(fn|impl|\{)' crates/ src/ src-tauri/ -t rust -n`
  - 期望：0 命中
- [x] **节点 `transform` 内 `DataStore` 读写检测**
  - 命令：`rg 'DataStore|store\.(read|write)' crates/nodes-* -t rust -n`
  - 期望：0 命中（节点不应碰 store，由 Runner 负责）
- [x] **节点是否把 metadata 塞 payload**
  - 人工 review：每个节点 `transform` 返回路径上 payload 字段是否含 `metadata`-语义键
- [x] **Rhai 节点 max_operations**
  - 命令：`rg 'max_operations|set_max_operations' crates/ -t rust`
  - 检查：默认 ≥ 50k 是否仍生效；是否有节点 override 到更大值或关闭限制
- [x] **panic isolation**
  - 所有 `NodeTrait::transform` 调用走 `catch_unwind + timeout` 包装（Runner 路径）
  - `NodeHandle::emit`（触发器路径）是否同样隔离
- [x] **节点直接访问硬件检测**
  - 命令：`rg 'tokio_modbus::|rumqttc::|reqwest::|tokio_serial::' --files-without-match crates/connections/ crates/nodes-io/`
  - 在 `nodes-io` / `connections` 之外不应出现
- [x] **WorkflowNodeDefinition pattern 扩散**
  - 列出所有"应是稳定 type"的 public struct，检查是否仍是 pub 字段
- [x] **CI 三件套**
  - [x] `cargo clippy --workspace --all-targets -- -D warnings` 0 warning
  - [x] `cargo fmt --all -- --check` 通过
  - [x] `cargo deny check` 通过（仅既有 license/duplicate warnings，exit 0）

---

## Phase E: 整合与 PR 清单（Day 8）

- [x] 写 `docs/superpowers/specs/2026-05-XX-architecture-review-findings.md`
  - 按 P0（必修）/ P1（应修）/ P2（可选）排序所有发现
  - 每条标注：来源 phase / 影响面 / 建议 PR 范围
- [x] 派生出修复 PR 列表（不要求 review 期内 merge，列出即可）
- [x] 一次性补 22 个历史 plan 的 `Status` 标记（已 merged 的写 `> **Status:** merged in <SHA>`；deferred 的写明状态）
- [x] 更新 `AGENTS.md`
  - [ ] 删除 freeze 段（Phase A 未完成，按退出标准保留）
  - [x] 同步 Project Status / 已知 tech debt
  - [x] 同步 ADR Execution Order 状态
- [x] 同步 memory 文件（如必要，本轮以 `AGENTS.md` + findings 为真值源，未写本机 memory）
- [x] 关闭本 plan：prepend `> **Status:** completed YYYY-MM-DD`

---

## 退出标准（解冻条件）

全部 5 项勾完即解冻：

- [ ] Phase A 全勾（所有 in-flight ADR 完成 + 同步状态）
- [x] Phase B 全勾（每片产出 findings 文档）
- [x] Phase C 全勾（行数普查 + `lib.rs` 拆分完成；其他文件已决策）
- [x] Phase D 全勾（规范扫描 0 违规，或全部入 P0/P1 清单）
- [ ] Phase E 全勾（findings 整合 + 历史 plan Status 补全 + AGENTS.md freeze 段删除；freeze 删除因 Phase A 未完成而保留）

**findings PR 不阻塞解冻**——按正常 PR 流程后续 merge。

---

## 不在 review 范围

- 重新设计已落地 ADR（如 PinKind 是否在 pin/edge 上）
- ADR-0014 后续 Phase 的范围扩张
- 新功能开发
- UI/UX 改造
- 工具链迁移（构建系统、依赖大版本升级）
