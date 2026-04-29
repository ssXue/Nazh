# 2026-04-29 前端契约审计 findings

**范围**：`web/src/generated/`、`web/src/types.ts`、`web/src/lib/{pin-*,node-*,workflow-*}.ts`、`web/src/lib/flowgram.ts`、FlowGram node registry。

**结论**：PinType / PinKind / NodeCapabilities 的跨语言契约有 fixture 和单测兜底，状态较好；但 FlowGram → Nazh AST 转换仍有硬编码节点白名单，已经漏掉 `mqttClient` 与新增 pure-form 节点，属于直接部署路径风险。

## 主要 findings

| ID | 优先级 | 位置 | 发现 | 建议动作 |
|----|--------|------|------|----------|
| B5-FE-01 | P0（已修复） | `web/src/lib/flowgram.ts:31` | `FLOWGRAM_BUSINESS_NODE_TYPES` 漏掉 `mqttClient`、`c2f`、`minutesSince`。这些节点已在 FlowGram node library / catalog 中存在，但 `toNazhWorkflowGraph()` 会把不在白名单里的节点过滤掉，导致保存/部署 AST 丢节点。 | 本轮 Phase E 已修复：白名单改为从 `getAllNodeDefinitions()` 派生，并补 `flowgram-to-nazh` 回归测试。 |
| B5-FE-02 | P1 | `web/src/types.ts:8` | `types.ts` 没有 re-export `SetWorkflowVariable*` / `SnapshotWorkflowVariables*` / `VariableChangedPayload` 等新 generated 类型，`workflow-variables.ts` 直接从 `../generated` 引用。当前可工作，但“generated → types.ts → app”边界不一致。 | 统一规则：跨 IPC 消费默认从 `types.ts` 取；generated 仅在类型扩展文件内部使用。 |
| B5-FE-03 | P2 | `web/src/generated/index.ts:2` | generated index 注释仍写 `cargo test --workspace --lib export_bindings`，与 root AGENTS 当前命令 `cargo test -p tauri-bindings --features ts-export export_bindings` 不一致。 | 下次生成或手工修正生成模板/后处理注释。 |
| B5-FE-04 | P2 | `web/src/lib/pin-schema-cache.ts:24` | `describe_node_pins` IPC 失败时 fallback `Any/Any` + `Exec/Exec` 是有意 UX 降级；但在浏览器 E2E 模式下会长期走 fallback，不能断言后端真实 pin。 | 保留；在 E2E 新用例中继续只断 wiring presence，不断后端真值。 |
| B5-FE-05 | P2 | `web/src/lib/flowgram.ts:337` | 子图容器外部边重写时多入口 / 多出口桥接节点只取 `inputIds[0]` / `outputIds[0]`，未显式报错。 | 若子图多桥接是非法配置，应在保存前 validation 报错；若合法，需要 port mapping 设计。 |
| B5-FE-06 | P2 | `web/src/generated/ExecutionEvent.ts` | `ExecutionEvent` generated union 含 `VariableChanged`，但 `workflow://node-status` 不会发送该 variant；`workflow-variables.ts` 改订独立 `workflow://variable-changed`。 | 与 B4-IPC-04 / B1-R0-01 同步处理；短期补注释避免误用。 |

## ts-rs generated 与手写类型边界

| 文件 | 当前角色 | 评估 |
|------|----------|------|
| `web/src/generated/*` | Rust ts-rs 输出，包含 IPC / engine / AI / connection 类型 | 类型完整；注释里的生成命令过期。 |
| `web/src/types.ts` | re-export generated + 前端扩展类型 | 边界有用，但没有 re-export 新变量 IPC 类型。 |
| `web/src/lib/workflow-variables.ts` | 变量 IPC wrapper | 直接引用 generated，绕过 `types.ts`。 |
| `web/src/lib/tauri.ts` | 大部分 IPC wrapper + 手写 shell 类型 | 与 `src-tauri` 私有 struct 镜像多，漂移风险见 B4。 |

建议：`types.ts` 保持 “应用层唯一类型入口”。只有在扩展 generated 类型时，内部 import generated base；业务 wrapper 不直接 import `../generated`。

## Pin / node / workflow 契约状态

| 契约 | Rust 真值源 | TS 实现 | 同步状态 |
|------|-------------|---------|----------|
| PinType 兼容矩阵 | `crates/core/src/pin.rs` | `web/src/lib/pin-compat.ts` | 好：共享 `tests/fixtures/pin_compat_matrix.jsonc`。 |
| PinKind 兼容矩阵 | `crates/core/src/pin.rs` | `web/src/lib/pin-compat.ts` | 好：共享 `tests/fixtures/pin_kind_matrix.jsonc`。 |
| pure-form 判定 | `nazh_core::is_pure_form` | `web/src/lib/pin-compat.ts` | 好：共享 pure form fixture。 |
| NodeCapabilities 位图 | `crates/core/src/node.rs` | `web/src/lib/node-capabilities.ts` | 好：位值一致；root AGENTS 旧状态描述需修。 |
| 节点 inventory | `standard_registry()` + Flow/I/O/Pure plugin | `flowgram-node-library.ts` / `nodes/catalog.ts` / `flowgram.ts` | 风险：`flowgram.ts` 白名单漏节点。 |
| Workflow AST | `WorkflowGraph` / `WorkflowEdge` generated | `toNazhWorkflowGraph()` | 中：转换逻辑含子图 flatten hack 和节点白名单。 |

## FlowGram fallback / hack 清单

| 位置 | 说明 | 风险 |
|------|------|------|
| `pin-schema-cache.ts` fallback Any/Any | IPC 失败或浏览器 E2E 无 Tauri runtime 时放行连接，部署期兜底 | UX 友好；E2E 不能断真实 pin。 |
| `flowgram.ts` `SUBGRAPH_ID_SEPARATOR = "/"` | 通过 `/` 判断展平副本，用户自定义 id 含 `/` 会误判 | 需 UI validation 禁止 `/`。 |
| `flowgram.ts` 多桥接取第一个 | 子图外部边只映射第一个 input/output bridge | 多桥接语义未定义。 |
| `flowgram.ts` 硬编码 business node set | 转 AST 时过滤节点 | 已漏节点，P0。 |

## 回归测试建议

- `flowgram-to-nazh.test.ts` 增加 `mqttClient`、`c2f`、`minutesSince` 三节点 round-trip，断言 `toNazhWorkflowGraph(...).nodes` 保留它们。
- 对新增 node kind 建一个单测：`NODE_DEFINITIONS.map(kind)` 必须全部被 `toNazhWorkflowGraph` 识别为 business node。
