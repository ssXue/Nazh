# ADR-0020: `src/graph/` 编排层是否独立成 crate（评估备案）

- **状态**: 提议中（**评估性 ADR**，重评已触发：2026-04-30）
- **日期**: 2026-04-24
- **决策者**: Niu Zhihong
- **关联**: 回溯评估 Phase 4（`7e7d5af`）的"facade + 编排合并"决策

## 背景

Phase 4（2026-04-16 `7e7d5af`）拆分 Ring 1 时，commit message 原文：

> nazh-engine 降级为 **Facade，仅保留 DAG 编排和 NodeRegistry**

当前根 crate `nazh-engine`（`src/`）内容：

| 文件 | 行数 | 职责 |
|------|------|------|
| `src/lib.rs` | ~80 | Facade re-export + `standard_registry()` |
| `src/registry.rs` | - | 节点工厂聚合 |
| `src/graph.rs` | 24 | 子模块索引（刚从 `mod.rs` 重命名） |
| `src/graph/types.rs` | ~60 | `WorkflowGraph` / `WorkflowEdge` 数据结构 |
| `src/graph/topology.rs` | 123 | Kahn 算法、拓扑排序、环检测 |
| `src/graph/deploy.rs` | 148 | 工作流部署编排（DataStore 初始化、通道创建、任务派发） |
| `src/graph/runner.rs` | 179 | 单节点异步执行循环、panic 隔离、超时、事件发射 |

### 观察

facade crate **不等于** 纯 re-export crate。它承担了三项职责：

1. **Facade 再导出**（`src/lib.rs`）——标准的 facade 作用
2. **`standard_registry()` 工具函数**——便利入口，组装所有标准插件
3. **整套 DAG 编排运算代码**（`src/graph/` 的 4 个运算文件，~510 行）

前两项符合"facade"定义。第三项是**实质性的运行时逻辑**——它不是简单的 re-export，而是引擎的"中枢调度"代码。

### 这是不是问题？

**不全是。** 理由：

- Phase 4 commit 明确说了"保留 DAG 编排"——是**显式决策**，不是疏忽
- 编排层处在"所有 Ring 1 crate 的共同下游" 位置：`graph` 需要 `nodes-*` 来实例化节点、`connections` 来管理资源、`scripting` 来执行脚本——如果拆出去，该 crate 要依赖所有 Ring 1，层次结构会是"Ring 1 nodes → Ring X graph → app"的多一层
- 当前规模（~510 行运算代码）放在 facade 里是可控的

### 但有隐忧

- **规模上限不明**：加 ADR-0009 生命周期钩子后 `deploy.rs` 会膨胀（+100 行）；加 ADR-0014 边分离后 `runner.rs` 大改（+几百行）；加 ADR-0013 子图后 `deploy.rs` 要展开逻辑（+大量代码）
- **"Facade + 运算"不符合一般分层惯例**：新入项目的贡献者会以为 `src/` 只是 re-export；看到 179 行的 runner.rs 会困惑"这是什么"
- **测试组织模糊**：根 crate 的 `tests/` 目录放的是引擎级集成测试，但运算代码在这里——很难区分"单元测试属于哪个 crate"

## 决策

> **当前不实施任何拆分**。但通过本 ADR 明确：
>
> 1. **设立拆分触发条件**——满足任一即启动拆分讨论
> 2. **拆分方案预研**——真要拆时按预研方案执行，不再临时设计
> 3. **过渡期的卫生要求**——避免 `src/graph/` 失控扩张

### 拆分触发条件（满足任一）

- [x] `src/graph/` 总行数超过 **1500 行**（2026-04-30 复核：3295 行）
- [x] 入参 ADR 之一落地且涉及 `deploy.rs` / `runner.rs` 的**重大改造**（ADR-0009 生命周期钩子、ADR-0013 子图、ADR-0014 引脚二分均已落地）
- [ ] 出现第二种"面向调用方的编排模式"（如 Web 后端也想嵌入工作流，但不要 Tauri 壳层）——此时需要"编排 crate + 不同 facade"
- [ ] 集成测试数量 > 50 条，但 `tests/` 目录单仓测试运行时 > 60s——考虑把编排单测沉到专门 crate 加速

### 重评触发记录（2026-04-30）

本 ADR 的前两个触发条件已经满足，但本 ADR 本身仍是"评估备案"，不直接实施拆分。后续应新开一个 ADR 或计划 PR 来决定：

- 是否按下方预研方案拆出 `crates/graph/`。
- 若暂不拆，至少先把 `src/graph/{types,topology,pin_validator,pull}.rs` 中超过 300 行的文件继续拆子模块，并更新本 ADR 的卫生线。
- 拆分前必须保留 `graph` 不直接依赖 `nodes-flow` / `nodes-io` 的约束，只通过 `NodeRegistry` 与 `dyn NodeTrait` 交互。

### 拆分方案预研（仅记录，不实施）

若触发，按以下方案：

```
crates/graph/                            # 新 Ring 1 crate
  src/lib.rs                             # types.rs / topology.rs / deploy.rs / runner.rs 从 src/graph/ 搬来
  [dependencies]
  nazh-core.workspace = true             # 基础类型
  connections.workspace = true           # 部署时注入资源
  scripting.workspace = true             # AI trait 引用（如 ADR-0019 落地后则不再需要）
  # 不直接依赖 nodes-flow/nodes-io ——节点是通过 NodeRegistry 注册的，graph 只看 dyn NodeTrait

src/lib.rs                               # 瘦身：仅 re-export + standard_registry
```

关键约束：**`graph` crate 不依赖 `nodes-flow` / `nodes-io`**。二者的关系是"graph 执行 NodeTrait，nodes-* 注册到 NodeRegistry"——通过 Ring 0 的 trait 解耦，不形成跨 Ring 1 依赖。

### 过渡期卫生要求

在未拆分时，`src/graph/` 应当遵守：

- 不新增"对单一 Ring 1 crate 的硬耦合"（如直接 `use nodes_io::HttpClientNode`）——所有节点相关操作走 `NodeRegistry`
- 模块边界清晰：types / topology / deploy / runner 四分法保持
- 单文件不超过 **300 行**——超过要先拆子模块
- 每次 PR 动 `src/graph/` 的，review 时明确思考"这个功能是否在催促独立 crate"

## 可选方案

### 方案 A: 立即拆出 `crates/graph/`

- 优势：架构更整齐；根 crate 真正成为 facade
- 劣势：
  - **推翻 Phase 4 主动决策**，需要更硬的动机（目前没有）
  - 10+ 文件的 use 路径改动
  - 增加一个 crate 的 maintenance 负担
  - 当前规模（~510 行）不足以证明收益

### 方案 B: 维持现状不讨论

- 优势：无成本
- 劣势：`src/graph/` 悄悄膨胀，等发现问题时已是大规模 refactor

### 方案 C: 设立触发条件 + 预研方案，暂不实施（已选）

- 优势：
  - 尊重 Phase 4 的决策，不做"理论洁癖 refactor"
  - 给未来的自己/贡献者一个客观判据
  - 避免了"每次加功能都要讨论要不要拆"的讨论消耗
- 劣势：
  - 触发条件是主观判断，执行时仍需裁量
  - 若贡献者不知道本 ADR 存在，触发条件形同虚设——要在 CLAUDE.md 里指路

### 方案 D: 把 facade 和 `standard_registry` 也一起拆，根 crate 变成纯 app shell

- 优势：彻底解耦
- 劣势：`nazh-engine` 作为 workspace 的"主 crate"变成空壳——概念上不自然

## 后果

### 正面影响（本 ADR 作为决策文件的价值）

- **防止慢性膨胀**：有明文触发条件后，每次改动 `src/graph/` 有个"还能不能继续长胖"的检查点
- **保留 Phase 4 决策尊严**：不为了"整齐"而推翻实用选择
- **给 C2 这条审计发现留了归档位置**：审计中提到的问题不会被"无视"，而是被"延后到有数据再谈"
- **贡献者指引**：CLAUDE.md 可以引用本 ADR，新人知道"`src/graph/` 的边界是什么、什么时候该重新讨论"

### 负面影响

- **仍是一份"空头决策"**——没有立即代码产出
- 触发条件依赖人工判断——CI 不会提醒"行数超 1500"

### 风险

- **风险 1：触发条件被遗忘**
  - 缓解：在 `CLAUDE.md` 的"Architecture"章节加一行引用本 ADR
- **风险 2：时间一长，拆分方案本身过时**
  - 缓解：本 ADR 每 6 个月 revisit 一次；若 Ring 结构大变，方案同步更新

## 备注

- 本 ADR 是项目里第一条**评估性 ADR**——结论是"不做"。这类 ADR 对防止"因为能改就改"的冲动很有价值。
- 若未来真的触发拆分，应当**提交新 ADR**（ADR-00XX），形如"实施 ADR-0020 预研方案——`src/graph/` 拆分为 `crates/graph/`"，明确当时的触发依据。
- 与其他 ADR 的关系：
  - ADR-0017（IPC 迁出）之后 Ring 0 瘦身了，但不影响 `src/graph/` 的判断——graph 的依赖是"所有 Ring 1 的交集"，不涉及 Ring 0 重构
  - ADR-0014（边类型分离）如果实施，会显著扩张 `runner.rs`——届时可能触发本 ADR 的第二个条件
  - ADR-0013（子图）如果实施，会扩张 `deploy.rs`——届时同样可能触发
