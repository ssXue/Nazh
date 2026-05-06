# 2026-04-29 Facade 编排层审计 findings

**范围**：root facade crate `src/`，重点审计 `src/graph/` 职责边界、`standard_registry()` 注册路径、ADR-0020 触发条件。

**结论**：`standard_registry()` 的插件组合路径清晰，`src/graph/` 未直接硬耦合 `nodes-*` 具体节点类型；但 ADR-0020 的两个触发条件已经实际满足：`src/graph/` 总行数超过 1500，且 ADR-0009/0014 等已显著改造 deploy/runner。Phase C 至少需要拆文件；是否独立 `crates/graph` 应进入解冻后的 ADR 讨论。

## 行数与模块职责

取证命令：`wc -l src/graph.rs src/graph/*.rs`。

| 文件 | 行数 | 当前职责 | 评估 |
|------|------|----------|------|
| `src/graph.rs` | 36 | 模块索引 + re-export + 默认 pin 常量 | 合理 |
| `src/graph/types.rs` | 422 | WorkflowGraph / Edge / Deployment / Ingress / Streams / handle 方法 | 超过 ADR-0020 单文件 300 行卫生线 |
| `src/graph/topology.rs` | 473 | DAG 校验、Kahn 拓扑、Data 边环检测、边分类 | 超过卫生线；拓扑与 pin-kind 分类混在一起 |
| `src/graph/deploy.rs` | 360 | 四阶段部署、资源注入、pin 校验、cache 构造、on_deploy、spawn | 超过卫生线；仍可读但已接近拆分点 |
| `src/graph/runner.rs` | 245 | 单节点运行循环、guarded transform、metadata/event、Data cache 写入 | 合理 |
| `src/graph/pin_validator.rs` | 513 | pin 索引、类型 / kind / required 校验 + 大量测试 | 超过卫生线；测试占比高但仍建议拆 test helper |
| `src/graph/pull.rs` | 251 | ADR-0014 Data 输入拉路径 | 合理 |
| `src/graph/variables_init.rs` | 89 | 变量声明初始化 | 合理 |
| **合计** | **2389** | graph 编排层 | 超过 ADR-0020 “1500 行重评”触发条件 |

## 主要 findings

| ID | 优先级 | 位置 | 发现 | 建议动作 |
|----|--------|------|------|----------|
| B3-FAC-01 | P1 | `docs/adr/0020-*.md` 触发条件 | `src/graph/` 当前 2389 行，超过 ADR-0020 的 1500 行重评线；且 ADR-0009 生命周期、ADR-0014 PinKind/Data pull 已落地并改造 `deploy.rs` / `runner.rs`。 | Phase E 汇总为 P1：解冻后新 ADR 决定是否实施 `crates/graph` 拆分；Phase C 先做文件级拆分。 |
| B3-FAC-02 | P1 | `src/graph/types.rs:25` | `types.rs` 同时承载序列化 schema、部署句柄、Ingress/Streams 方法、Deployment shutdown / resources / output_cache API，职责偏宽。 | Phase C 可拆 `schema.rs` / `deployment.rs` / `streams.rs`，保持 public re-export 不变。 |
| B3-FAC-03 | P1 | `src/graph/topology.rs:113` | `topology.rs` 包含 Data 边环检测和 `classify_edges`，后两者更像 ADR-0014 edge semantics，而非基础 DAG topology。 | Phase C 可拆 `edge_semantics.rs` 或并入 `pin_validator` 附近，减少 topology 模块概念负担。 |
| B3-FAC-04 | P2 | `src/graph/pin_validator.rs:1` | `pin_validator.rs` 生产逻辑不大，但测试和 stub helper 使文件 513 行。 | 可先拆 `#[cfg(test)] mod tests` 到子模块，或保留到 Phase C 行数普查统一处理。 |
| B3-FAC-05 | P1 | `src/lib.rs:8` | facade rustdoc 架构表缺 `nodes-pure` / `ai`，`nodes-io` 节点描述也漏 `mqttClient`；与 `standard_registry()` 实际加载 `PurePlugin` 不一致。 | 文档修复 PR：同步 root facade rustdoc、root AGENTS、README crate 表。 |
| B3-FAC-06 | P2 | `src/registry.rs:43` | `两个插件合并后覆盖全部_18_种节点类型` 测试名已过期：实际 `standard_registry()` 加载 Flow + Io + Pure 三个插件。 | 改测试名与断言说明为 “标准注册表覆盖全部 18 种节点类型”。 |

## `standard_registry()` 注册路径

`src/lib.rs:74` 当前：

```rust
pub fn standard_registry() -> NodeRegistry {
    let mut host = PluginHost::new();
    host.load(&FlowPlugin);
    host.load(&IoPlugin);
    host.load(&PurePlugin);
    host.into_registry()
}
```

评估：
- 路径清晰，所有节点仍通过 Plugin 注册，没有在 graph 编排层硬编码节点类型。
- `src/graph` 只依赖 `NodeRegistry` / `NodeTrait` / `SharedResources` 等 Ring 0 trait；未发现 `use nodes_io::*` 之类硬耦合。
- facade re-export 负责让桌面默认带全节点集合，这个定位仍成立。

## ADR-0020 触发条件评估

| 触发条件 | 当前状态 |
|----------|----------|
| `src/graph/` 总行数超过 1500 | 已触发：2389 行。 |
| ADR-0014 / ADR-0013 / ADR-0009 等重大改造 deploy/runner | 已触发部分：ADR-0009 和 ADR-0014 已落地并新增 lifecycle / OutputCache / pull path。 |
| 第二种面向调用方编排模式 | 未见。 |
| 集成测试数量 / runtime 阈值 | 本轮未测。 |

结论：ADR-0020 应在 Phase E findings 中列为 P1。Phase B 不直接实施拆 crate，因为 architecture freeze 明确不新增 crate；Phase C 可先做文件级拆分并更新文档。
