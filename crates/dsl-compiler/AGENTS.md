# dsl-compiler — Workflow DSL 编译器

RFC-0004 Phase 3 核心组件：将 `WorkflowSpec`（状态机 YAML）编译为 `WorkflowGraph` JSON（部署管道输入格式）。

## 对外暴露

- `compile(ctx: &CompilerContext, spec: &WorkflowSpec) -> Result<Value, CompileError>` — 编译入口
- `CompilerContext` — 持有设备/能力资产快照，提供引用校验
- `CompileError` — `Reference` / `StateMachine` / `CapabilityCall` / `OutputBuild`

## 依赖约束

- **可依赖**：`nazh-dsl-core`, `serde`, `serde_json`, `thiserror`
- **禁止依赖**：`nazh-core`, `nazh-graph`, `nazh-engine`（避免循环依赖；一致性由 dev-dependency 测试守护）

## 编译流程

1. `context::CompilerContext::validate_references()` — 设备/能力引用存在性
2. `validate::validate_workflow_spec()` — 状态机语义校验（6 条规则）
3. `validate::determine_initial_state()` — 初始状态选择（idle 优先 → 字典序）
4. `output::GraphBuilder` — 收集 actions → 生成 stateMachine 节点 → capabilityCall 节点 → edges → variables

## 一致性测试

`lib.rs` 中 4 个 conformance test：编译输出经 `serde_json::to_string()` → `WorkflowGraph::from_json()` 验证，守护 schema 契约不漂移。

## 节点 ID / 端口 ID 命名

- stateMachine 节点：`sm_{spec.id}`（经 sanitize）
- capabilityCall 节点：`cap_{target_id}_{port_id}`（经 sanitize）
- entry port：`entry_{state}_{index}`
- exit port：`exit_{state}_{index}`
- transition port：`trans_{from}_{to}`
- sanitize 规则：`.` / `-` / ` ` → `_`

## 变量类型推断

`Value::Number` 整数→`{ "kind": "integer" }`，浮点→`{ "kind": "float" }`，`String`→`{ "kind": "string" }`，`Bool`→`{ "kind": "bool" }`，其余→`{ "kind": "any" }`。

## 修改本 crate 时

- 编译输出格式变更必须同步更新 `WorkflowGraph` serde 格式 + 更新 conformance test
- 新增校验规则须同步 `validate.rs` 的 tests 模块
- `output.rs` 的 `build_*` 方法变更须确认生成的 JSON 仍可被 `stateMachine` / `capabilityCall` 节点反序列化
