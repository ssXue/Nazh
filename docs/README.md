# docs/ — 项目文档

## 目录结构

```text
docs/
├── README.md          # 本文件
├── screenshots/       # 界面截图
├── adr/               # 架构决策记录（ADR）
│   ├── README.md      # ADR 索引与说明
│   ├── template.md    # ADR 模板
│   └── 0001-*.md      # 具体 ADR 条目
└── rfcs/              # 功能提案（RFC）
    ├── README.md      # RFC 索引与说明
    └── template.md    # RFC 模板
```

## 文档类型指南

| 我想... | 应该写... | 位置 |
|---------|----------|------|
| 记录一个已做出的架构决策 | ADR | `docs/adr/` |
| 提出一个新功能的设计方案 | RFC | `docs/rfcs/` |
| 记录版本变更 | CHANGELOG | 项目根目录 `CHANGELOG.md` |
| 记录 API 用法 | Rust doc comments | 源码中的 `///` 注释 |
| 描述一个模块的职责和结构 | 子模块 README | 对应目录下的 `README.md` |

## 生成 API 文档

```bash
cargo doc --no-deps --open
```
