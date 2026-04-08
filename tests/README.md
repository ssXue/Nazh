# tests/ — 集成测试

本目录包含 `nazh_engine` 的集成测试，验证引擎核心功能的端到端行为。

## 测试文件

| 文件 | 覆盖范围 |
|------|----------|
| `pipeline.rs` | 线性流水线：payload 变换、错误恢复、panic 隔离、超时保护 |
| `workflow.rs` | DAG 工作流：端到端执行、Rhai 脚本集成、连接池借还、环检测 |

## 运行

```bash
# 运行全部测试
cargo test

# 运行单个测试
cargo test workflow_graph_executes_end_to_end

# 运行某个测试文件的所有测试
cargo test --test pipeline

# 显示 println 输出
cargo test -- --nocapture
```

## 编写新测试的注意事项

- 测试中可以使用 `.unwrap()` / `.expect()`（clippy deny 仅作用于 lib 代码）
- 异步测试使用 `#[tokio::test]` 宏
- 涉及超时的测试建议使用 `tokio::time::timeout` 设置合理上限，避免 CI 卡死
