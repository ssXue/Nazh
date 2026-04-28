# `crates/nodes-pure` — 纯计算节点（Ring 1）

## 这是什么

Nazh 的"无副作用纯函数"节点集合。所有节点声明仅 `PinKind::Data` 引脚，部署
时由 [`is_pure_form`](nazh_core::is_pure_form) 判定为 pure-form，被下游
Data 输入拉取时即时求值（不进 Tokio task spawn 列表）。

## 当前节点目录（2026-04-28）

| 节点 kind | 输入 | 输出 | capability |
|-----------|------|------|------------|
| `c2f` | `value: Float` (Data) | `out: Float` (Data) | `PURE` |
| `minutesSince` | `since: String` (Data, RFC3339) | `out: Integer` (Data) | 空（读取系统时钟，非确定性） |

## 内部约定

- 节点必须无外部 IO：不发起网络/文件/设备访问、不读 `WorkflowVariables`、不调
  `AiService`。若节点读取系统时钟（如 `minutesSince`），可以是 pure-form，
  但**不能**声明 `NodeCapabilities::PURE`。
- 节点必须线程安全（`Send + Sync`）——递归 pull 求值在不同 task 上下文里
- 错误返回 `EngineError::PayloadConversion { node_id, message }`，
  携带节点 ID 与具体描述（类型不匹配 / 解析失败 / 数学溢出等）

## 修改本 crate 时

- 加新节点：在 `lib.rs` 的 `PurePlugin::register` 加 `register_with_capabilities(...)` +
  写 `mod xxx; pub use xxx::XxxNode;` + 同步更新本 AGENTS.md 节点目录表 +
  根 `src/registry.rs` 的 `pure_plugin_注册全部纯计算节点` 集成测试断言列表
- 节点必须有单元测试覆盖：（a）正常输入产出预期值（b）类型不匹配返回错误
  （c）边界条件（如 c2f 极大极小温度 / minutesSince 非法时间戳）

## 依赖约束

仅依赖 `nazh-core` + `async-trait` + `serde` + `serde_json` + `uuid` + `chrono`。
**不得**依赖 `connections` / `scripting` / `nodes-flow` / `nodes-io` / `ai`——
本 crate 是 Ring 1 中"零协议依赖"的最小子集，体现 pure 节点纯度。
