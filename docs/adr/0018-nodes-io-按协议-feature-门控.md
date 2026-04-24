# ADR-0018: `nodes-io` 按协议拆分 cargo features

- **状态**: 提议中
- **日期**: 2026-04-24
- **决策者**: Niu Zhihong
- **关联**: 回溯评估 Phase 4（`7e7d5af`）的 `nodes-io` 单体打包策略；与 RFC-0002 的 "constrained edge device" 目标一致

## 背景

Phase 4（2026-04-16 `7e7d5af`）把 I/O 节点集中到 `crates/nodes-io/` 单个 crate。当前内容：

| 子模块 | 协议依赖 | 典型部署场景 |
|--------|----------|------------|
| `sql_writer.rs` | `rusqlite` (bundled, ~5MB 编译产物) | 本地持久化的网关 |
| `http_client.rs` | `reqwest` (hyper/tokio/rustls) | 调用上游 REST API |
| `bark_push.rs` | `reqwest` (共享) | iOS 推送告警 |
| `mqtt_client.rs` | `rumqttc` | 工业 MQTT 总线 |
| `modbus_read.rs` | `tokio-modbus` | PLC / 变频器采集 |
| `serial_trigger.rs` | （标准 tokio） | 串口传感器 |
| `timer.rs` | （标准 tokio） | 定时触发 |
| `debug_console.rs` | （无额外依赖） | 调试日志 |
| `native.rs` | （无额外依赖） | 纯内部透传 |
| `template.rs` | （模板引擎工具函数） | 所有节点共享 |

**当前问题**：一个只用 Bark + Timer + Serial 的边缘部署，依然被迫编译全部 4 个重协议栈（SQLite bundled、hyper、tokio-modbus、rumqttc）。对资源受限的边缘设备（ARM、Atom 级嵌入式盒子）来说：

- **编译产物膨胀**：Release 模式下几个 MB 无用代码
- **编译时间**：CI 每次都要构建 SQLite 静态库
- **安全面扩大**：每个协议栈都是潜在 CVE 攻击面，未启用的协议也得跟进升级
- **合规审查成本**：SQLite bundled 含 C 代码，部分行业合规场景下要审

这和 RFC-0002 的目标冲突——原文写着"Need optional compilation for constrained edge devices"。Phase 4 没实施这一项，是一次务实推迟。

## 决策

> 我们决定为 `nodes-io` 引入**按协议划分的 cargo features**。默认启用全部协议（保持当前桌面开发体验），边缘部署可通过 `--no-default-features --features "io-mqtt,io-serial"` 等组合裁剪。`FlowPlugin::register` 按 feature 条件注册对应节点工厂。

### 拟议 feature 划分

```toml
# crates/nodes-io/Cargo.toml

[features]
default = ["io-all"]

# 元 feature
io-all = ["io-sql", "io-http", "io-mqtt", "io-modbus", "io-serial", "io-notify"]
io-minimal = ["io-debug", "io-native", "io-timer"]  # 永远可用的三件套

# 按协议门控
io-sql      = ["dep:rusqlite"]
io-http     = ["dep:reqwest"]
io-mqtt     = ["dep:rumqttc", "dep:reqwest"]  # MQTT over WSS 需要 rustls stack 也会走 reqwest
io-modbus   = ["dep:tokio-modbus"]
io-serial   = []                              # 标准 tokio 即可
io-notify   = ["dep:reqwest"]                 # Bark / 通用 HTTP 推送
io-debug    = []
io-native   = []
io-timer    = []

[dependencies]
# 无条件
nazh-core.workspace = true
connections.workspace = true
async-trait.workspace = true
serde.workspace = true
serde_json.workspace = true
chrono.workspace = true
tokio.workspace = true
uuid.workspace = true

# 可选
rusqlite     = { workspace = true, optional = true }
reqwest      = { workspace = true, optional = true }
rumqttc      = { workspace = true, optional = true }
tokio-modbus = { workspace = true, optional = true }
```

### 源码层的条件编译

```rust
// crates/nodes-io/src/lib.rs

#[cfg(feature = "io-sql")]    mod sql_writer;
#[cfg(feature = "io-http")]   mod http_client;
#[cfg(feature = "io-mqtt")]   mod mqtt_client;
#[cfg(feature = "io-modbus")] mod modbus_read;
#[cfg(feature = "io-serial")] mod serial_trigger;
#[cfg(feature = "io-notify")] mod bark_push;

// 永远编译
mod debug_console;
mod native;
mod timer;
mod template;

impl Plugin for IoPlugin {
    fn register(&self, registry: &mut NodeRegistry) {
        registry.register("timer", ... );
        registry.register("debugConsole", ... );
        registry.register("native", ... );

        #[cfg(feature = "io-sql")]
        registry.register("sqlWriter", ... );

        #[cfg(feature = "io-http")]
        registry.register("httpClient", ... );

        // ...
    }
}
```

### 运行时友好的错误消息

前端 FlowGram 拖入 `sqlWriter` 但部署时发现节点未注册——引擎返回 `EngineError::UnsupportedNodeType` 就不够信息量。建议 `NodeRegistry::create` 在 feature-off 时给出可区分的错误，比如：`UnsupportedNodeType { node_type, reason: "节点类型 `sqlWriter` 在当前构建中未启用（需 `io-sql` feature）" }`。

## 可选方案

### 方案 A: 维持单体（现状）

- 优势：零改动，桌面体验最好
- 劣势：边缘部署被迫带全家桶；与 RFC-0002 目标冲突

### 方案 B: 按协议拆成独立 crate

```
crates/nodes-sql/
crates/nodes-mqtt/
crates/nodes-modbus/
...
```

- 优势：拆分最彻底，每个 crate 独立版本演进
- 劣势：
  - 小 crate 数量爆炸（7+ 个）
  - 每个都要单独的 `Plugin` 实现、manifest、注册逻辑——跨 crate 的共享代码会反复迁移
  - Cargo workspace members 急剧膨胀
  - 和当前 `FlowPlugin` 聚合逻辑的设计哲学冲突（一个插件管一组相关节点）

### 方案 C: Cargo feature 门控（已选）

- 优势：
  - 单 crate 心智模型不变
  - `FlowPlugin::register` 仍是"一个入口注册一批"
  - 边缘部署按需裁剪
  - 桌面默认 `io-all` 体验零回归
- 劣势：
  - 需要写和维护 feature 组合的 CI 矩阵
  - `cargo check --no-default-features --features "io-serial"` 等组合必须都能编译
  - 源码里 `#[cfg(feature = ...)]` 散落——若漏标会让编译偶发失败

### 方案 D: 运行时插件加载（WASM / 动态库）

- 优势：最灵活，运行时决定加载什么
- 劣势：WASM 还在 Phase 7 路线图上；动态库与 "单一二进制部署"（ADR-0003 思路一脉相承）冲突；过度工程

## 后果

### 正面影响

- **边缘构建产物瘦身**：少启用一个协议 → 编译产物减少几 MB + 若干编译时间
- **SQLite bundled 合规问题可规避**：特定行业项目 `--no-default-features --features "io-mqtt,io-modbus"` 就避开
- **CI 矩阵化能发现集成 bug**：至少跑 `io-minimal` 组合，避免"全开"掩盖小问题
- **与 Phase 7 WASM 插件路线图对齐**：未来可以把每个 feature 做成 WASM 模块，feature 体系是中间过渡
- **前端可以查询"本部署启用了哪些节点"**：Tauri 命令 `list_node_types` 已经支持，feature-aware 后天然给出正确列表

### 负面影响

- 首次 feature 拆分需细致：每个节点的依赖树要确认；`mqtt_client` 用到 `reqwest` 是否必须——要逐一 verify
- CI 工作流变长：至少 `default`、`io-minimal`、`--no-default-features` 三个组合
- 贡献者增加一项心智负担："新加协议节点 → feature 名、可选依赖、Plugin::register 条件"

### 风险

- **风险 1：feature 组合漏测**，某个组合无法编译
  - 缓解：CI 矩阵必含 `default` / `io-minimal` / `--no-default-features` / 每个 feature 单独开 四组
- **风险 2：前端 UX 回退**——用户拖 sqlWriter 却不能部署
  - 缓解：`list_node_types` 返回动态结果 + 前端 FlowGram 节点库按返回值过滤；错误消息带明确 feature 名指引
- **风险 3：跨 feature 的共享代码（如 `template.rs`）必须始终可用**
  - 缓解：共享工具代码不走 feature gate；只 gate 节点实现文件
- **风险 4：feature 命名未来想改代价大**
  - 缓解：本 ADR 一次性锁定命名（`io-sql` / `io-http` / `io-mqtt` / `io-modbus` / `io-serial` / `io-notify` / `io-debug` / `io-native` / `io-timer`）

## 备注

- 建议与 ADR-0011（节点能力标签）**解耦但同期实施**：feature 门控是"编译期是否含节点"，capability 标签是"运行时节点特性"，二者正交。可以把"feature 名"存入 capability metadata 让前端一并感知。
- 实施前置条件：ADR-0017 处理了 `ts-rs` 的 feature 化后，本 ADR 的 CI 模式更容易推广。建议先 0017 再 0018。
- 长期愿景：引擎的每个 Ring 1 crate（除了 `nazh-core`）都应走同样模式——feature-able、可选依赖、按需启用。这为 Phase 7 WASM 插件铺路。
