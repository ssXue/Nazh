# docs/ — 项目文档

本目录保存 Nazh 的长期文档、架构决策、设计记录和实施计划。若本文档与根目录 `AGENTS.md` 冲突，以 `AGENTS.md` 为准，并在同一个 PR 修正文档漂移。

## 目录结构

```text
docs/
├── README.md              # 本文件
├── conventions.md         # 长期协作、生成物、安全和 Dev Container 约定
├── git.md                 # Git 工作流、commit message、DCO
├── screenshots/           # 界面截图
├── adr/                   # 架构决策记录（ADR）
│   ├── README.md          # ADR 索引与说明
│   ├── template.md        # ADR 模板
│   └── 0001-*.md          # 具体 ADR 条目
├── rfcs/                  # 决策前的较大设计空间
│   ├── README.md          # RFC 索引与说明
│   └── template.md        # RFC 模板
├── specs/                 # 功能或子系统设计文档
│   └── template.md        # Spec 模板
├── plans/                 # 可执行实施计划
│   └── template.md        # Plan 模板
├── blueprints/            # 历史蓝图或评审基准
└── templates/             # 可复制的局部文档模板
    ├── README.md
    └── local-AGENTS.md
```

## 文档类型指南

| 我想... | 应该写... | 位置 |
|---------|----------|------|
| 记录一个已做出的架构决策 | ADR | `docs/adr/` |
| 在决策前探索较大设计方向 | RFC | `docs/rfcs/` |
| 描述功能或子系统设计 | Spec | `docs/specs/` |
| 跟踪多步骤实施工作 | Plan | `docs/plans/` |
| 记录长期协作、生成物、安全、Dev Container 约定 | Conventions | `docs/conventions.md` |
| 记录 Git 和 commit 规则 | Git guide | `docs/git.md` |
| 描述模块职责和局部修改规则 | 局部 `AGENTS.md` | 对应 crate/module 根目录 |
| 记录 API 用法 | Rust doc comments | 源码中的 `///` / `//!` 注释 |

## 新鲜度规则

- 改动导致文档失效时，必须在同一个 PR 更新文档。
- 易过期内容必须带日期，例如 roadmap、已知技术债、状态说明。
- 文件路径、命令或事件通道变化时，更新引用，不保留死路径。
- AI memory 只能作为 point-in-time 线索，引用前必须回到仓库文件验证。

## 生成 API 文档

```bash
cargo doc --no-deps --open
```
