# src/ — Nazh 引擎核心

本目录是 `nazh_engine` 库 crate 的源码，实现了工作流 DAG 的解析、校验、部署与异步执行。

## 模块结构

```
src/
├── lib.rs               # crate 入口，统一 re-export
├── context.rs           # WorkflowContext 数据信封
├── error.rs             # EngineError 统一错误类型
├── connection.rs        # 全局连接资源池（借出/归还语义）
│
├── nodes/               # 节点系统（每种节点一个文件）
│   ├── mod.rs           # NodeTrait、NodeDispatch、NodeExecution 核心抽象
│   ├── helpers.rs       # 共享基础设施：RhaiNodeBase 脚本基座、with_connection
│   ├── native.rs        # NativeNode — 原生 Rust 逻辑节点
│   ├── rhai.rs          # RhaiNode — 沙箱化 Rhai 脚本节点
│   ├── timer.rs         # TimerNode — 定时触发节点
│   ├── modbus_read.rs   # ModbusReadNode — Modbus 寄存器读取（模拟）
│   ├── if_node.rs       # IfNode — 布尔条件分支
│   ├── switch_node.rs   # SwitchNode — 多路分支
│   ├── try_catch.rs     # TryCatchNode — 异常捕获路由
│   ├── http_client.rs   # HttpClientNode — HTTP 请求
│   ├── sql_writer.rs    # SqlWriterNode — SQLite 持久化写入
│   ├── debug_console.rs # DebugConsoleNode — 调试输出
│   └── loop_node.rs     # LoopNode — 循环迭代
│
├── graph/               # DAG 工作流图
│   ├── mod.rs           # 模块入口与 re-export
│   ├── types.rs         # 数据结构定义与句柄方法
│   ├── topology.rs      # Kahn 算法拓扑排序与环检测
│   ├── deploy.rs        # deploy_workflow() 部署编排
│   ├── instantiate.rs   # 节点工厂：按 node_type 创建实例
│   └── runner.rs        # 单节点异步执行循环与事件发射
│
└── pipeline/            # 线性流水线
    ├── mod.rs           # 模块入口与 re-export
    ├── types.rs         # 类型定义与 build_linear_pipeline()
    └── runner.rs        # 单阶段异步执行循环与事件发射
```

## 数据流

```text
WorkflowContext
    → 根节点接收（ingress）
    → 节点执行（NativeNode / RhaiNode / ...）
    → MPSC Channel 传递给下游节点
    → 叶节点输出到 result 流
    → 所有节点向 event 流发送状态事件
```

## 关键抽象

- **`NodeTrait`**（`nodes/mod.rs`）— 所有节点的统一异步接口，新节点只需实现此 Trait
- **`RhaiNodeBase`**（`nodes/helpers.rs`）— 脚本节点的组合基座，封装 Rhai 引擎初始化、编译、求值
- **`with_connection`**（`nodes/helpers.rs`）— 连接借出-释放的异步生命周期辅助

## 添加新节点类型

1. 在 `nodes/` 下创建新文件（如 `mqtt_publish.rs`）
2. 定义 `XxxNodeConfig`（带 `#[derive(Serialize, Deserialize)]`）和节点结构体
3. 若为脚本节点，嵌入 `helpers::RhaiNodeBase`；若需连接，使用 `helpers::with_connection`
4. 实现 `NodeTrait`（`id`、`kind`、`ai_description`、`execute`）
5. 在 `nodes/mod.rs` 中添加 `mod xxx;` 和 `pub use xxx::{...};`
6. 在 `graph/instantiate.rs` 的 `instantiate_node()` 中添加匹配分支
7. 在 `lib.rs` 的 `pub use nodes::{...}` 中添加导出

## 关键设计约束

- 所有错误通过 `Result<T, EngineError>` 传播，禁止 `.unwrap()` / `.expect()`
- 每个节点在独立的 Tokio 任务中运行，通过 MPSC 通道通信
- 节点不直连硬件，必须通过 `ConnectionManager` 借出连接
- Rhai 脚本必须设置 `max_operations` 步数上限

## 生成文档

```bash
cargo doc --no-deps --open
```
