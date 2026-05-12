# crates/graph — DAG 工作流编排层

> **Ring**: Ring 1
> **对外 crate 名**: `nazh-graph`
> **职责**: 解析工作流图、校验 DAG、部署节点任务、维护 Exec/Data/Reactive 边语义与运行时拉路径
>
> 根 `AGENTS.md` 的约束对本 crate 同样适用。

## 这个 crate 做什么

本 crate 是运行时 DAG 编排层。它接收前端/DSL 生成的 `WorkflowGraph`，完成部署期校验与节点任务编排：

1. `WorkflowGraph::from_json()` 解析画布 AST。
2. `topology` 校验节点引用、DAG 环、拓扑顺序与边类型分类。
3. `deploy` 构建节点实例、连接通道、初始化 `OutputCache` / pull path / `WorkflowVariables`。
4. `runner::run_node()` 在 Tokio task 中执行单节点循环，负责 `DataStore` 读写、事件发射、下游分发和结果流。
5. `pull` 支持 Data 输入引脚的运行时拉取、Pure 节点递归求值和 memo。

本 crate 不实现具体节点逻辑；节点行为来自 `nazh_core::NodeTrait` 和 `NodeRegistry`。

## 对外暴露

```text
crates/graph/src/
├── lib.rs              # crate public facade
├── types.rs            # WorkflowGraph / WorkflowEdge / WorkflowDeployment 等类型
├── deploy.rs           # deploy_workflow* 入口与部署编排
├── runner.rs           # 单节点执行循环
├── topology/
│   ├── mod.rs          # DAG 校验与拓扑排序
│   └── classify.rs     # source pin → PinKind 边分类
├── pull/
│   ├── collector.rs    # Data 输入收集与 payload merge
│   ├── index.rs        # pull path 索引
│   └── memo.rs         # Pure 节点 memo
├── pin_validator.rs    # 部署期 pin 类型兼容校验
└── variables_init.rs   # 变量声明初始化
```

关键 API：

- `deploy_workflow(...)`
- `deploy_workflow_and_restore_variables(...)`
- `build_workflow_variables(...)`
- `WorkflowGraph` / `WorkflowEdge`
- `WorkflowDeployment` / `WorkflowIngress` / `WorkflowStreams`

## 内部约定

1. **Graph 只编排，不拥有节点业务语义**。新增节点应放在 `nodes-*` crate，通过 registry 注入；不要在本 crate 硬编码节点类型。
2. **部署期尽早 fail-fast**。节点缺失、边引用缺失、pin 类型不兼容、DAG 环、变量声明错误应在部署阶段返回 `EngineError`，不要等节点运行后才暴露。
3. **Exec/Data/Reactive 三分支语义不可混淆**：
   - `Exec`：推送 `ContextRef` 到下游，不写 `OutputCache`。
   - `Data`：只写 `OutputCache`，不推送下游 `ContextRef`，也不进入 workflow result。
   - `Reactive`：写 `OutputCache`，同时按 Exec 路径推送。
4. **DataStore 引用计数由 runner/ingress 严格配平**。写入 payload 时 consumer 数必须等于真实推送目标数；任一 channel 发送失败都必须释放未交付引用。不要把 Data-only cache 写入伪装成 result entry。
5. **`OutputCache` 的 `None` 语义固定**。`WorkflowDeployment::output_cache(node_id)` 返回 `None` 只表示节点不存在；Exec-only 节点也应有空槽位 cache 句柄。
6. **Pull path 只服务 Data 输入**。`pull::pull_data_inputs` 不应改变 Exec 路径；Pure 节点递归求值必须经过 `guarded_execute` / timeout / `PureMemo`，避免无限递归或重复计算。
7. **事件通道与结果通道分离**。`ExecutionEvent` 承载运行状态与 metadata；workflow result 只承载真实业务输出 payload；变量事件走 `WorkflowVariableEvent` 独立通道。
8. **生命周期由 RAII 管理**。部署时保存 `LifecycleGuard`；shutdown 按部署逆序清理。不要引入手写 close/release 配对。

## 依赖约束

- **允许**：`nazh-core`、`connections`、`serde`、`serde_json`、`tokio`、`chrono`、`tracing`
- **禁止**：协议实现 crate（`reqwest` / `rumqttc` / `rusqlite` / `tokio-modbus` 等）、`nodes-*`、`ai`

`graph` 可以依赖 Ring 0 和连接定义类型，但不应反向依赖具体节点 crate；具体节点由上层 facade/registry 组合。

## 修改本 crate 时

| 改动 | 必须同步 |
|------|----------|
| 改 `WorkflowGraph` / `WorkflowEdge` serde 格式 | `web/src/generated/` ts-rs 导出、`dsl-compiler` conformance test、前端 AST 转换 |
| 改 Exec/Data/Reactive 分发语义 | `runner.rs` tests、`topology/classify.rs` tests、root `AGENTS.md` Data Flow/Design Principles |
| 改 DataStore 引用计数或 channel 失败路径 | 增加 channel closed / partial send / Data-only 输出回归测试 |
| 改 pull path 或 Pure memo | `pull/*` tests，确认 timeout、memo key、递归依赖和错误定位 |
| 改部署生命周期或 shutdown 顺序 | `WorkflowDeployment::shutdown` 相关测试与壳层 runtime 持有逻辑 |
| 新增公共 IPC/TS 类型 | 迁到 `crates/tauri-bindings`，不要把 shell-only 类型塞回本 crate |

## 测试清单

```bash
cargo test -p nazh-graph
cargo test -p dsl-compiler
cargo test -p tauri-bindings --features ts-export export_bindings
```

涉及前端 AST 或 generated type 时，再运行：

```bash
npm --prefix web run build
```

## 关联 ADR / RFC

- RFC-0002 分层内核与插件架构
- ADR-0010 Pin 类型兼容校验
- ADR-0012 工作流变量
- ADR-0014 Data 引脚与 pull path
- ADR-0015 Reactive 引脚
- ADR-0016 边级可观测性
- ADR-0020 graph 编排层长期归属
