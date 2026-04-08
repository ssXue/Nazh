# src/ — Nazh 引擎核心

本目录是 `nazh_engine` 库 crate 的源码，实现了工作流 DAG 的解析、校验、部署与异步执行。

## 模块总览

| 模块 | 职责 |
|------|------|
| `lib.rs` | Crate 入口，统一 re-export 所有公开类型 |
| `context.rs` | `WorkflowContext` — 在 DAG 中流转的数据信封（trace_id + timestamp + JSON payload） |
| `error.rs` | `EngineError` — 全局错误枚举，基于 `thiserror`，绝不 panic |
| `nodes.rs` | `NodeTrait` 及两种实现：`NativeNode`（原生 Rust）、`RhaiNode`（沙箱脚本） |
| `connection.rs` | `ConnectionManager` — 全局连接资源池，借出/归还语义 |
| `pipeline.rs` | `build_linear_pipeline()` — 线性流水线抽象，带超时保护和 panic 隔离 |
| `graph.rs` | `WorkflowGraph` + `deploy_workflow()` — DAG 解析、拓扑排序、异步部署（最大模块） |

## 数据流

```text
WorkflowContext
    → 根节点接收（ingress）
    → 节点执行（NativeNode / RhaiNode）
    → MPSC Channel 传递给下游节点
    → 叶节点输出到 result 流
    → 所有节点向 event 流发送状态事件
```

## 关键设计约束

- 所有错误通过 `Result<T, EngineError>` 传播，禁止 `.unwrap()` / `.expect()`
- 每个节点在独立的 Tokio 任务中运行，通过 MPSC 通道通信
- 节点不直连硬件，必须通过 `ConnectionManager` 借出连接
- Rhai 脚本必须设置 `max_operations` 步数上限

## 生成文档

```bash
cargo doc --no-deps --open
```
