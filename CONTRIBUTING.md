# 贡献指南

**先读 [`AGENTS.md`](./AGENTS.md)**——它是单一事实源。

## 快速清单

- 每个 commit 必须 `git commit -s` 带 Signed-off-by
- 代码注释、错误消息、日志消息、commit message 使用中文
- 提交前跑：
  ```bash
  cargo test --workspace
  cargo clippy --workspace --all-targets -- -D warnings
  cargo fmt --all --check
  ```
- 改动 `#[ts(export)]` 类型后跑 `cargo test -p tauri-bindings --features ts-export export_bindings` 并提交生成的 TS
- 重大架构变更先写 ADR（`docs/adr/NNNN-title.md`）

## License

MIT
