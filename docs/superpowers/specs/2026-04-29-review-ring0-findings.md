# 2026-04-29 Ring 0 接口与数据结构审计 findings

**范围**：`crates/core/src/*` 的 public trait / struct / enum，重点对照 `AGENTS.md` 的 Ring 0 纯度、控制面 / 数据面分离、metadata 分离、字段可见性约定。

**结论**：Ring 0 依赖纯度仍成立，未发现协议 crate 反向污染；主要风险集中在事件模型继续吸收控制面事件、少数稳定类型仍暴露 public 字段、以及 ADR-0014/0012 引入的缓存/变量事件接口需要在解冻后重新收口。

## 主要 findings

| ID | 优先级 | 位置 | 发现 | 建议动作 |
|----|--------|------|------|----------|
| B1-R0-01 | P1 | `crates/core/src/event.rs:24` | `ExecutionEvent::VariableChanged` 把工作流变量控制面事件塞进执行生命周期事件。Tauri 侧已经把它转成 `workflow://variable-changed`，并显式避免发到 `workflow://node-status`，说明消费端语义已分裂。 | 解冻后新建独立 `VariableEvent` / `RuntimeControlEvent`，或在 IPC 层只导出 `VariableChangedPayload`，逐步从 `ExecutionEvent` 移除该 variant。 |
| B1-R0-02 | P1 | `crates/core/src/node.rs:160`, `crates/core/src/event.rs:60` | `NodeOutput.metadata` 是必填 `Map<String, Value>`，但 `CompletedExecutionEvent.metadata` 是 `Option<Map<...>>`。生产端和消费端空值语义不对称，Runner / NodeHandle 都要手写 `is_empty() -> None`。 | 统一空 metadata 语义：要么生产端也用 `Option<Map>`，要么事件端保留空 map。若保持现状，补一个核心 helper，避免分散转换。 |
| B1-R0-03 | P2 | `crates/core/src/cache.rs:38` | `OutputCache.slots` 使用 `DashMap`，但当前槽位部署期一次性 `prepare_slot`，运行期只读 / 写已存在槽。并发 map 的能力可能超过需要。 | ADR-0014 Phase 4 前暂保留；Phase 4 若不引入动态 slot，评估改成 `HashMap<String, ArcSwap<...>>` 在部署期构造完后只读共享。 |
| B1-R0-04 | P2 | `crates/core/src/pin.rs:185` | `PinDefinition` 工厂方法现有 5 个：`default_input` / `default_output` / `required_input` / `output` / `output_named_data`，尚未达到 plan 的 “>= 6 改 builder” 阈值。 | 暂不改；下一次新增工厂方法时改为 builder 或 `PinDefinitionSpec`，避免构造 API 横向膨胀。 |
| B1-R0-05 | P1 | `crates/core/src/variables.rs:97` | `WorkflowVariables` 的声明 / 读取 / mutation API 分组清晰，但 `set_event_sender` 让存储层直接持有 `ExecutionEvent` sender 和 `workflow_id`，把变量存储与事件投递耦合。 | 解冻后与 B1-R0-01 一起拆：变量存储只返回 changed snapshot，事件桥接由 deploy / runtime adapter 负责。 |
| B1-R0-06 | P2 | `crates/core/src/ai.rs:180` | `AiService` trait 本身简洁，但 `AiGenerationParams` 已包含 `thinking` / `reasoning_effort` 这类 provider 扩展字段。它们在 Ring 0 中是可接受的最小通用契约，但会给未来非 OpenAI 兼容 provider 带来字段膨胀压力。 | 保留；下一次新增 provider 专属字段前先评估 `provider_options: Map<String, Value>` 或 provider-specific config 是否更合适。 |
| B1-R0-07 | P2 | `crates/core/src/node.rs:160`, `crates/core/src/context.rs:22`, `crates/core/src/lifecycle.rs:39` | 部分稳定 core 类型仍是 public 字段，未完全扩散 `WorkflowNodeDefinition` 的 private + getters 模式。当前已被大量构造路径使用，立即收紧会破 API。 | 只对未来新增稳定类型强制 private + getter；既有类型等下一次 breaking window 分批收口。 |

## Public trait / struct 评估表

评分口径：`好` = 当前形状符合职责；`中` = 可继续使用但有演化压力；`风险` = 需派生修复 PR。

| 类型 | 位置 | cohesion | 对称性 | 可演化性 | 命名 | 建议动作 |
|------|------|----------|--------|----------|------|----------|
| `WorkflowContext` | `crates/core/src/context.rs:22` | 好：IPC / 外部边界完整 payload 信封 | 中：与 `ContextRef` 分工清晰，但字段 public | 中：新增字段会影响 ts-rs / 前端 | 好 | 保留；不要再加 metadata 字段。 |
| `ContextRef` | `crates/core/src/context.rs:69` | 好：内部轻量数据引用 | 好：与 `WorkflowContext` 成对 | 中：`source_node` 已扩展为 4 字段，需守住轻量目标 | 好 | 保留；未来字段新增先评估通道成本。 |
| `DataId` | `crates/core/src/data.rs:26` | 好：数据面 opaque id | 好 | 好：内部 tuple private | 好 | 保留。 |
| `DataStore` | `crates/core/src/data.rs:51` | 好：读 / 写 / COW / release 最小面 | 中：`release` 无错误返回，消费端无法得知双 release | 中：持久化后端可能需要 async trait | 好 | 保留；持久化后端出现前不扩 trait。 |
| `ArenaDataStore` | `crates/core/src/data.rs:97` | 好：默认内存实现 | 好 | 好 | 好 | 保留。 |
| `ExecutionEvent` | `crates/core/src/event.rs:24` | 风险：执行生命周期 + 变量控制面混合 | 风险：Tauri 已拆 channel | 中：enum 新 variant 会扩前端 union | 中 | 派生 B1-R0-01。 |
| `CompletedExecutionEvent` | `crates/core/src/event.rs:60` | 好：完成事件载荷 | 中：metadata 与 `NodeOutput` 不对称 | 中 | 好 | 派生 B1-R0-02。 |
| `EngineError` | `crates/core/src/error.rs:16` | 中：全局错误集中，variant 较多 | 好 | 中：继续增长会变成错误垃圾桶 | 好 | 保留；新增错误前优先复用上下文 builder。 |
| `NodeLifecycleContext` | `crates/core/src/lifecycle.rs:39` | 中：resources / handle / shutdown / variables 组合合理但字段 public | 好 | 中：继续加资源会膨胀 | 好 | 保留；新增上下文能力优先走 `RuntimeResources`。 |
| `NodeHandle` | `crates/core/src/lifecycle.rs:61` | 好：触发器 emit 入口 | 好：与 Runner 输出路径对齐 | 中：emit 无背压策略入口 | 好 | 保留；Phase 6 EventBus 再评估。 |
| `LifecycleGuard` | `crates/core/src/lifecycle.rs:166` | 好：RAII lifecycle | 好 | 好 | 好 | 保留。 |
| `NodeCapabilities` | `crates/core/src/node.rs:26` | 好：类型级能力位图 | 好 | 中：bit 分配有限，需 ADR 管控 | 好 | 保留。 |
| `NodeDispatch` | `crates/core/src/node.rs:148` | 好：Broadcast / Route 最小分发 | 好 | 中：边级策略落地后可能需扩展 | 好 | 保留；Phase 6 再评估。 |
| `NodeOutput` | `crates/core/src/node.rs:160` | 好：payload / metadata / dispatch | 中：metadata 空值不对称 | 中：public 字段 | 好 | 派生 B1-R0-02。 |
| `NodeExecution` | `crates/core/src/node.rs:169` | 好：多输出容器 | 好 | 好 | 好 | 保留。 |
| `NodeTrait` | `crates/core/src/node.rs:243` | 好：transform + pins + on_deploy 聚合了节点必要契约 | 中：pin 方法实例级、capability 注册表级，边界清晰 | 中：新增 trait 方法是冻结范围外 breaking change | 好 | 保留；freeze 期不改签名。 |
| `RuntimeResources` | `crates/core/src/plugin.rs:22` | 好：typed Any 资源包 | 中：缺失资源运行时才报错 | 好：可增资源无需改 trait | 好 | 保留。 |
| `SharedResources` | `crates/core/src/plugin.rs:63` | 好 | 好 | 好 | 好 | 保留。 |
| `WorkflowNodeDefinition` | `crates/core/src/plugin.rs:77` | 好：private fields + getters 是参考模式 | 好 | 好 | 好 | 保留。 |
| `NodeRegistry` | `crates/core/src/plugin.rs:214` | 好：工厂 + capability map | 好 | 中：无 unregister / namespace，当前不需要 | 好 | 保留。 |
| `PluginManifest` | `crates/core/src/plugin.rs:286` | 好：name/version | 好 | 中：public fields 可接受 | 好 | 保留。 |
| `Plugin` | `crates/core/src/plugin.rs:295` | 好：manifest + register | 好 | 中：新增生命周期 hook 会破 API | 好 | 保留。 |
| `PluginHost` | `crates/core/src/plugin.rs:301` | 好：顺序加载插件 | 好 | 好 | 好 | 保留。 |
| `PinDirection` | `crates/core/src/pin.rs:37` | 好 | 好 | 好 | 好 | 保留。 |
| `PinKind` | `crates/core/src/pin.rs:66` | 好：Exec/Data 正交语义 | 好 | 中：新增 kind 会影响前后端矩阵 | 好 | 保留。 |
| `PinType` | `crates/core/src/pin.rs:121` | 好：轻 schema 类型 | 好 | 中：新增 variant 要同步矩阵 fixture | 好 | 保留。 |
| `PinDefinition` | `crates/core/src/pin.rs:185` | 中：字段完整但工厂 API 接近膨胀阈值 | 好 | 中：public fields + TS contract | 好 | 派生 B1-R0-04。 |
| `CachedOutput` | `crates/core/src/cache.rs:22` | 好：Data pin 快照 | 好 | 好 | 好 | 保留。 |
| `OutputCache` | `crates/core/src/cache.rs:38` | 中：职责明确，但内部并发结构可能过重 | 好 | 中 | 好 | 派生 B1-R0-03。 |
| `VariableDeclaration` | `crates/core/src/variables.rs:37` | 好：类型 + 初值 | 好 | 中：public fields / TS contract | 好 | 保留。 |
| `TypedVariable` | `crates/core/src/variables.rs:50` | 好：内部状态快照 | 好 | 中：public fields | 好 | 保留；不跨 IPC。 |
| `TypedVariableSnapshot` | `crates/core/src/variables.rs:61` | 好：IPC 快照 | 好 | 中：字段变更需 ts-rs | 好 | 保留。 |
| `WorkflowVariables` | `crates/core/src/variables.rs:97` | 中：存储 + mutation + event sink 混合 | 中 | 中 | 好 | 派生 B1-R0-05。 |
| `AiService` | `crates/core/src/ai.rs:23` | 好：complete / stream_complete 最小运行时能力 | 好 | 中：新增 provider 管理能力不应进 trait | 好 | 保留。 |
| `AiCompletionRequest` | `crates/core/src/ai.rs:71` | 中：运行时请求 + provider 选择 | 好 | 中 | 好 | 保留。 |
| `AiMessage` | `crates/core/src/ai.rs:93` | 好 | 好 | 好 | 好 | 保留。 |
| `AiCompletionResponse` | `crates/core/src/ai.rs:112` | 好 | 好 | 中：未来 tool-call / multimodal 会扩展 | 好 | 保留。 |
| `AiTokenUsage` | `crates/core/src/ai.rs:127` | 好 | 好 | 好 | 好 | 保留。 |
| `StreamChunk` | `crates/core/src/ai.rs:136` | 中：delta/thinking/done 是 OpenAI-like 流抽象 | 好 | 中 | 好 | 保留。 |
| `AiThinkingConfig` | `crates/core/src/ai.rs:162` | 中：provider 扩展上移 Ring 0 | 中 | 中 | 中：`kind` 避免 `type` 关键字合理 | 派生 B1-R0-06。 |
| `AiGenerationParams` | `crates/core/src/ai.rs:180` | 中：通用采样参数 + provider 扩展混合 | 好 | 中 | 好 | 派生 B1-R0-06。 |
| `AiError` | `crates/core/src/ai.rs:41` | 好：协议无关错误 | 好 | 中 | 好 | 保留。 |

## Public enum 补充

| 类型 | 位置 | 评估 | 建议动作 |
|------|------|------|----------|
| `AiMessageRole` | `crates/core/src/ai.rs:102` | 三角色足够当前 chat completion。 | 保留。 |
| `AiThinkingMode` | `crates/core/src/ai.rs:153` | provider 扩展枚举，当前仅 Enabled/Disabled。 | 保留。 |
| `AiReasoningEffort` | `crates/core/src/ai.rs:171` | 目前是 DeepSeek 风格字段，只有 High/Max。 | 后续新增档位前评估是否仍应在 Ring 0。 |

## 子项结论

- `ExecutionEvent::VariableChanged`：确认是接口腐化，建议单独拆事件类型。
- `NodeOutput.metadata` vs `CompletedExecutionEvent.metadata`：确认不对称，建议统一转换 helper 或类型形态。
- `OutputCache` 的 `DashMap`：当前可能过度，留到 ADR-0014 Phase 4 结合缓存策略处理。
- `PinDefinition` 工厂方法数量：当前 5 个，未触发 builder 阈值。
- `AiService`：trait 本身合格；请求参数有 provider 扩展字段压力。
- `WorkflowVariables`：mutation/snapshot/declaration 分组清晰；事件注入耦合需后续拆。
