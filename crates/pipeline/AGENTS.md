# crates/pipeline — 线性流水线

> **Ring**: Ring 1
> **对外 crate 名**: `pipeline`
> **职责**: 顺序阶段执行（Sequential Pipeline），与 DAG 并列的简化调度器
>
> 根 `AGENTS.md` 的约束对本 crate 同样适用。

## 这个 crate 做什么

Nazh 有两种调度器：DAG（由 facade crate `nazh-engine` 的 `src/graph/` 承担）和**线性流水线**（本 crate）。
DAG 表达工作流，线性流水线面向简单的"A → B → C"顺序处理场景——例如测试固定
多阶段管道、示例程序、将来可能的 CLI 工具。

核心抽象：
- `PipelineStage` — 单个阶段（`Arc<dyn Fn(Value) -> Pin<Box<dyn Future>>>`）
- `build_linear_pipeline(stages) -> PipelineHandle` — 串联阶段，启动独立 Tokio 任务
- 每阶段有独立 timeout + `catch_unwind` panic 隔离，失败事件走统一的 `ExecutionEvent`

**什么时候用 pipeline 而不是 graph**：只有纯线性顺序依赖、不需要分支/循环、不需要分 Fan-in/Fan-out 时。绝大多数工作流用 DAG。

## 对外暴露

```text
crates/pipeline/src/
├── lib.rs      # 仅 re-exports
├── types.rs    # PipelineStage / PipelineHandle / build_linear_pipeline
└── runner.rs   # 单阶段异步循环 + 事件发射
```

核心 API 集中在 `types.rs`：`PipelineStage`、`PipelineHandle`、`StageFuture`、`build_linear_pipeline`。

## 内部约定

1. **阶段之间通过 MPSC 串联**。上游阶段完成后 `send` 到下游阶段的输入 channel，不共享可变状态。
2. **每阶段隔离**：panic + timeout 使用 `nazh_core::guard`；某一阶段挂掉不应影响上下游事件流。
3. **事件语义对齐 DAG**。所有 `ExecutionEvent` 变体必须与 `nazh-core` 定义一致，前端无需区分"流水线事件"和"图事件"。见 ADR-0004。
4. **不负责业务逻辑**。本 crate 不实现具体计算——调用方提供 `PipelineStage` 闭包。

## 依赖约束

- 允许：`nazh-core`、`tokio`、`futures-util`、`tracing`
- 禁止：任何协议 / 脚本 / AI crate，也不依赖 `connections` / `nodes-*` / `scripting` / `ai`

本 crate 与 DAG 编排层是**平行选择**，不是其子组件。DAG 不通过本 crate 实现线性优化；线性场景由调用方直接用。

## 修改本 crate 时

| 改动 | 必须同步 |
|------|----------|
| 改 `PipelineStage` / `PipelineHandle` 公共 API | `tests/pipeline.rs`（集成测试在 facade）+ 调用方 |
| 改事件发射时机 | `nazh-core` 的 `ExecutionEvent` 契约 + 前端事件解析器（若用在生产路径） |
| 增加阶段间并发/背压语义 | 先开 ADR（本 crate 目前只做"顺序+隔离"最小核） |

测试：
```bash
cargo test -p pipeline
cargo test -p nazh-engine --test pipeline       # facade 层集成测试
```

## 关联 ADR / RFC

- **ADR-0001** Tokio MPSC DAG 调度（本 crate 共享相同的 MPSC + 隔离原语）
- **ADR-0004** 统一执行事件模型（pipeline 事件与 DAG 事件同形）
- **RFC-0002** Phase 4 — 线性流水线最初与 `nazh-core` 一起 split 出来
