# Git 与 Commit 规范

根目录 `AGENTS.md` 是最高真值源；本文件是 Git 工作流的集中说明。

## Git 规则

- 从最新 `main` 创建短生命周期分支。
- 一个 commit 只处理一个关注点。
- 一个 PR 只处理一个主要目标。
- 不把无关重构混入功能、bugfix 或文档修复。
- 不重写共享历史；已推送 commit 不使用 `--amend`。
- 破坏性操作需要人工确认，包括 force push、reset、批量删除、生产数据修改、分支删除。

## Commit 规范

每个 commit 必须使用 DCO sign-off：

```bash
git commit -s
```

格式采用 Conventional Commits：

```text
<type>(<scope>)!: <subject>

<body>

<footer>
```

`type` 使用英文小写：

| type | 使用场景 |
|------|----------|
| `feat` | 新功能或能力 |
| `fix` | bug 修复 |
| `refactor` | 不改变外部行为的重构 |
| `perf` | 性能优化 |
| `test` | 测试新增或调整 |
| `docs` | 文档变更 |
| `build` | 构建系统、依赖、工具链 |
| `ci` | CI/CD 配置 |
| `chore` | 维护性杂项 |
| `style` | 格式、空白、排序等不改变语义的变更 |
| `revert` | 回退先前提交 |

规则：

- `scope` 可选，使用英文，例如 `core`、`graph`、`tauri`、`web`、`docs`、`ci`、`devcontainer`。
- `subject` 使用中文优先，保留必要英文术语，不加句号。
- 存在破坏性变化时使用 `!`，并在 body 或 footer 写迁移方式。
- 禁止无信息量提交信息，例如 `update`、`fix bug`、`misc`、`wip`。

可启用仓库提交模板：

```bash
git config commit.template .gitmessage
```

示例：

```text
feat(graph): 支持 Reactive 数据引脚订阅
fix(tauri): 约束 sqlWriter 数据库路径到工作目录
docs(agents): 更新解冻后的项目状态
ci(web): 使用 Node 24 LTS 跑前端测试
refactor(core)!: 拆分执行事件控制平面
```
