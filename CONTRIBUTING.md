# 贡献指南

**先读 [`AGENTS.md`](./AGENTS.md)**——它是单一事实源。

贡献前请同时阅读：

- `README.md`：安装、运行、测试和文档入口
- `docs/conventions.md`：长期协作、生成物、安全和 Dev Container 约定
- `docs/git.md`：commit message、DCO 和破坏性操作规则

## 快速清单

- 每个 commit 必须 `git commit -s` 带 Signed-off-by
- 代码注释、错误消息、日志消息、commit message 使用中文
- 提交前在 Dev Container 或 CI 等价环境内跑：
  ```bash
  cargo test --workspace
  cargo clippy --workspace --all-targets -- -D warnings
  cargo fmt --all -- --check
  npm --prefix web run test
  npm --prefix web run build
  ```
- 改动 `#[ts(export)]` 类型后跑 `cargo test -p tauri-bindings --features ts-export export_bindings` 并提交生成的 TS
- 重大架构变更先写 ADR（`docs/adr/NNNN-title.md`）

## 开发流程

1. 从最新 `main` 创建短生命周期分支。
2. 按 `README.md` 或 `.devcontainer/README.md` 准备环境。
3. 修改代码、测试和文档；不要混入无关重构。
4. 运行与改动相关的验证命令。
5. 提交 PR，并按 PR 模板说明变更、测试、文档同步、风险与回滚。

## PR 要求

- 一个 PR 只处理一个主要目标。
- 用户可见行为变化必须写入 PR 描述。
- 架构、公开 API、数据模型、存储、IPC、事件通道、安全或平台支持变化必须显式标注。
- UI 变化应提供截图或浏览器验证说明。
- 无法运行的测试必须说明原因。

## 文档同步

| 当你修改... | 必须更新... |
|-------------|-------------|
| 安装、编译、运行、测试、打包、发布命令 | `README.md` |
| 项目级规则、架构不变量、Critical Coding Constraints | `AGENTS.md` |
| crate 局部契约、依赖约束、节点清单 | 最近的 `AGENTS.md` |
| 非显然架构决策 | `docs/adr/` 与 `docs/adr/README.md` |
| 较大功能或子系统设计 | `docs/specs/` |
| 多步骤实施工作 | `docs/plans/` |
| CI、Dev Container 或发布要求 | `README.md`、`AGENTS.md`、`.devcontainer/README.md` |
| 生成代码流程或 ts-rs 类型 | `AGENTS.md`、`crates/tauri-bindings/AGENTS.md`、生成文件 |

## Review 重点

Reviewer 优先检查：

- 是否违反 `AGENTS.md` 的架构边界或 Critical Coding Constraints。
- 是否改变公开 API、数据模型、IPC、事件通道或平台支持。
- 是否补充了合适的 Rust / Vitest / E2E 测试。
- 是否同步更新文档和生成文件。
- 是否引入未说明的安全、兼容性或运维风险。

## License

MIT
