# src/ — facade crate

本目录是 `nazh-engine` facade crate 的源码，只负责对外 re-export Ring 0 / Ring 1
公共类型，并组装标准节点注册表。

## 模块结构

```text
src/
├── lib.rs       # crate 入口，统一 re-export 与 ts-rs facade 导出
└── registry.rs  # 标准节点注册表契约测试
```

实际引擎能力分别位于 `crates/core/`、`crates/graph/`、`crates/nodes-*`、
`crates/connections/`、`crates/scripting/`、`crates/ai/`、`crates/store/` 与
`crates/dsl-*`。
