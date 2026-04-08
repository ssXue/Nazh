# examples/ — 示例程序

本目录包含独立可运行的示例，演示引擎核心功能的用法。

## 示例列表

| 示例 | 说明 |
|------|------|
| `phase1_demo.rs` | Phase 1 线性流水线演示：温度归一化（C→F）+ 元数据标记 + 阶段超时 |
| `graph_demo.rs` | DAG 图执行演示 |

## 运行

```bash
# 运行指定示例
cargo run --example phase1_demo
cargo run --example graph_demo
```
