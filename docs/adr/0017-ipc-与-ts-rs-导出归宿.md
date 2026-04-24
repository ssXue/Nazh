# ADR-0017: IPC 响应类型与 ts-rs 导出从 Ring 0 迁出

- **状态**: 已实施
- **日期**: 2026-04-24
- **决策者**: Niu Zhihong
- **关联**: 回溯评估 Phase 1（`5cc9e9b`）与 Phase 4（`7e7d5af`）的 ts-rs 集中策略；影响 ADR-0007（ts-rs 类型契约守卫）

## 背景

Phase 1（2026-04-16 `5cc9e9b`）把 `src/ipc.rs` 上移到 `crates/nazh-core/src/ipc.rs`，Phase 4（`7e7d5af`）保留此决策。文件头注释写明动机：「从 `src-tauri` 迁移至引擎 crate 以实现 ts-rs 统一生成 TypeScript 类型定义」——当时是**工具链便利性**驱动的：一个 `cargo test export_bindings` 就能扫完所有类型。

实际内容（`crates/core/src/ipc.rs` 67 行）：

- `DeployResponse` / `DispatchResponse` / `UndeployResponse` — Tauri IPC 命令响应
- `NodeTypeEntry` / `ListNodeTypesResponse` — `list_node_types` 命令响应

这些类型**只被 Tauri 壳层消费**，但它们住在 Ring 0（引擎内核）。附带问题：`nazh-core` 被迫依赖 `ts-rs`（见 `crates/core/Cargo.toml:20`）——一个本该是"构建期导出工具"的 crate 成了 Ring 0 的运行时依赖。

### 当前痛点

1. **概念漂移**：Ring 0 自称"引擎运行时的基础原语"，但塞满了 Tauri 命令响应——这和定义矛盾
2. **依赖膨胀**：Ring 0 无条件依赖 `ts-rs`；边缘设备部署无法去除
3. **后续扩张风险**：新增 Tauri 命令就倾向于往 `crates/core/src/ipc.rs` 堆——门槛低、反模式强化
4. **文件名误导**：`crates/core/src/ipc.rs` 名字让人误以为是"引擎的 IPC 抽象"，实际是 Tauri 桥
5. **违反 Phase 4 确立的 Ring 分层**：Ring 0 = "zero protocol dependencies"，但 Tauri IPC 就是协议

## 决策

> 我们决定把 IPC 响应类型迁出 Ring 0，同时把 ts-rs 从"强制 Ring 0 依赖"降级为"按 crate 各自声明的导出工具"。具体：
>
> 1. **新建 `crates/tauri-bindings/`** — 集中承载所有 Tauri 命令的请求/响应类型；依赖 `nazh-core`、`connections` 等获取底层类型；被 `src-tauri` 和 `nazh-engine` facade 消费
> 2. **`ts-rs` 通过 feature flag 启用**：每个需要 ts-rs 导出的 crate 声明 `[features] ts-export = ["dep:ts-rs"]`，CI 用 `--features ts-export` 触发 `cargo test export_bindings`
> 3. **导出入口统一到 `tauri-bindings`**：它的 `export_bindings` 测试调用其他 crate 暴露的 `export_all()` 函数；业务 crate 不必单独生成

### 迁移后的层次

```
crates/core/              # Ring 0，不再含 ipc.rs，ts-rs 改为 feature
crates/connections/       # ts-rs 改为 feature
crates/pipeline/
crates/nodes-flow/
crates/nodes-io/
crates/scripting/
crates/ai/
crates/tauri-bindings/    # 新增：集中所有 #[tauri::command] 的 I/O 类型 + ts-rs 导出
src-tauri/                # 依赖 tauri-bindings + nazh-engine
src/                      # nazh-engine facade
```

### `cargo test export_bindings` 的调用形态

```rust
// crates/tauri-bindings/src/lib.rs
#[cfg(feature = "ts-export")]
pub fn export_all() -> Result<(), ts_rs::ExportError> {
    // Ring 0 类型
    nazh_core::export_bindings()?;       // 新的公开函数
    // Ring 1 类型
    connections::export_bindings()?;
    // tauri-bindings 自有类型
    DeployResponse::export()?;
    DispatchResponse::export()?;
    UndeployResponse::export()?;
    NodeTypeEntry::export()?;
    ListNodeTypesResponse::export()?;
    Ok(())
}

#[cfg(all(test, feature = "ts-export"))]
#[test]
fn export_bindings() {
    export_all().expect("ts-rs 导出失败");
}
```

CI 命令变成：`cargo test --features ts-export export_bindings`。

## 可选方案

### 方案 A: 维持现状（IPC 在 Ring 0）

- 优势：零迁移成本；一次 `cargo test export_bindings` 搞定所有类型
- 劣势：概念污染长期累积；Ring 0 定义和内容渐行渐远；`ts-rs` 永远是硬依赖

### 方案 B: IPC 迁回 `src-tauri/src/ipc.rs`，`ts-rs` 保持 Ring 0 硬依赖

- 优势：迁移路径最短；Tauri 类型归位
- 劣势：`ts-rs` 仍困在 Ring 0；每个用到 ts-rs 的 crate 各自管导出，缺乏汇总入口——未来加新类型容易漏

### 方案 C: 新建 `crates/tauri-bindings/` + `ts-rs` 特性门控（已选）

- 优势：
  - **Ring 0 回归纯净**，文字和代码一致
  - **`ts-rs` 真正变为"构建工具"**，生产构建不带它
  - **单一导出入口** 保留了 Phase 1 集中生成的便利性
  - 为未来 WASM/headless 部署（不需要 ts-rs 和 Tauri 类型）打开了门
- 劣势：
  - 新增一个 crate（workspace members 加一条；要写 Cargo.toml 和 lib.rs）
  - CI 要加 `--features ts-export`
  - `src-tauri` 的 use 路径大面积改（10+ 处）
  - 需要把 `nazh-core` 现有的 `export_bindings` test 迁移成可导入的 `pub fn export_bindings`

### 方案 D: 只做 ts-rs 特性门控，不拆 tauri-bindings crate

- 优势：改动更小
- 劣势：解决了 ts-rs 问题，但 IPC 类型仍然在 Ring 0——只解决一半

## 后果

### 正面影响

- **Ring 0 可以真正塞进轻量 edge 部署**（去掉 ts-rs 后减少 ~几十个传递依赖）
- **IPC 类型有专属家**，未来新增 Tauri 命令时不会再思考"放哪？"
- **构建分工清晰**：生产构建不带 ts-rs，CI 负责生成 TS 文件
- **架构文档（CLAUDE.md）和代码一致**：Ring 0 定义"不含协议"重新成立
- **`export_bindings` 仍有单一入口**，Phase 1 的工具链便利性不丢失

### 负面影响

- 一次中等规模迁移：~5-10 文件改动 + 1 新 crate
- CI 脚本调整；贡献者要知道"导出 TS 要加 `--features ts-export`"
- `crates/core/Cargo.toml` 和其他几个 crate 的依赖清单要更新

### 风险

- **风险 1：ts-rs feature 漏写**
  - 缓解：CI 必须跑 `--features ts-export`，失败则门禁；workspace level 可提供一个 `ts-export` meta-feature
- **风险 2：导出顺序/依赖问题**
  - `tauri-bindings::export_all()` 调用链若有循环依赖会死
  - 缓解：`tauri-bindings` 是最下游（所有其他 crate 的消费者），不会形成循环
- **风险 3：`src-tauri` 的 use 路径改动量较大**
  - 缓解：一次 search-replace 能覆盖；CI 编译失败能兜底
- **风险 4：前端生成路径变化**
  - 当前所有 `.ts` 都落在 `web/src/generated/`，路径由各 crate 的 `ts(export_to = ...)` 控制
  - 迁移后保持相同目标路径即可——前端引用零改动

## 备注

- 本 ADR 推翻的是 Phase 1/Phase 4 的"集中便利性"决策。原决策在当时是对的（项目早期，crate 少，ts-rs 集中最省心），现在 crate 数量增加、Ring 分层成熟，便利性已被架构债反噬。
- 实施建议按顺序：(1) 先把 `nazh-core::ipc` 的 `export_bindings` 改为 `pub fn`；(2) 创建 `crates/tauri-bindings/` 把类型移过去；(3) 加 ts-export feature 到各 crate；(4) 删 `crates/core/src/ipc.rs`；(5) CI 脚本调整。
- 与 ADR-0007（ts-rs 类型契约守卫）兼容——该 ADR 确立的是"类型契约在编译期守卫"的原则，本 ADR 只是改变承载位置，不改变守卫机制。

### 实施记录（2026-04-24）

实施时一次性完成全部 5 步：

- 新增 `crates/tauri-bindings/`（`Cargo.toml` + `src/lib.rs`），承载 `DeployResponse`、`DispatchResponse`、`UndeployResponse`、`NodeTypeEntry`、`ListNodeTypesResponse`，并提供 `list_node_types_response(&NodeRegistry) -> ListNodeTypesResponse` helper。
- 删 `crates/core/src/ipc.rs`；同步删除 Ring 0 内污染的 `NodeRegistry::registered_types_list()`（它返回 IPC 类型）——该方法的"排序+包装"逻辑下沉到 `tauri-bindings` 的 helper，Ring 0 只保留 `registered_types() -> Vec<&str>` 原语。
- 各业务 crate（`nazh-core` / `connections` / `ai` / `nazh-engine`）的 `ts-rs` 全部改为 `optional = true`，新增 `ts-export = ["dep:ts-rs"]` feature；`#[derive(.., TS)]` + `#[ts(...)]` 注解一律改写为 `#[cfg_attr(feature = "ts-export", derive(TS), ts(...))]` 形式；`mod export_bindings` 升级为 `pub mod export_bindings { pub fn export_all() -> Result<(), ts_rs::ExportError> }`，由上游 `tauri-bindings::export_all()` 统一调用。
- `tauri-bindings::ts-export` feature 通过 `nazh-engine/ts-export` 等传递依赖，一条命令 `cargo test -p tauri-bindings --features ts-export export_bindings` 即可触发全工作区 33 个 TS 类型导出。
- CI（`.github/workflows/ci.yml`）新增 `rust-ts-export` job：跑上述命令并用 `git diff --exit-code -- web/src/generated/` 校验生成结果与提交一致——防止开发者改了 Rust 类型却忘了重新生成 TS。
- 验证：`cargo test --workspace` 全绿；`cargo fmt --check` 通过；`cargo clippy --workspace --all-targets` 错误数与迁移前一致（5 个 pre-existing `expect_used`，均不在本次改动范围内）；`web/src/generated/*.ts` 33 个文件 md5 与迁移前完全一致——契约零变化。
