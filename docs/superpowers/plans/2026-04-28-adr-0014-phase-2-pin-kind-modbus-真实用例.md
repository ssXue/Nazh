> **Status:** merged in 1b62327

# ADR-0014 Phase 2 实施计划：第一个真实 Data 引脚（modbusRead `latest`）

> **Status:** merged 2026-04-28（11 个 commit `a799469`..`4419f65`，详见 `docs/adr/0014-执行边与数据边分离.md` 实施进度 Phase 2 章节）。所有 9 个 task 落地，含 1 处 reviewer 文档收紧补丁（`f1771a6`）+ 1 处 test rename 跟进（`1d815b8`）。

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 ADR-0014 的"求值语义二分"从 Phase 1 骨架推进到第一个真实业务用例——`modbusRead` 节点暴露第二个输出引脚 `latest`（Data Kind），让下游可拉取最近一次寄存器读数；同时把 PinKind 概念贯通到前端 IPC、连接期校验、画布视觉、E2E 验证的全链路。

**Architecture:**
- **后端**：modbusRead 在原 `out`（Json/Exec）基础上新增 `latest`（Json/Data）。Phase 1 已实现的 runner 双路径骨架（`data_output_pin_ids.is_empty()` 短路）此处首次激活——同一份 transform 输出同时写入 `OutputCache` Data 槽 + 推送给所有 Exec 下游。
- **跨语言契约**：在 `tests/fixtures/` 旁新增 `pin_kind_matrix.jsonc`，与现有 `pin_compat_matrix.jsonc` 平级——Rust 与 TS 各自消费同一份 fixture，保证 PinKind 兼容矩阵不会单边漂移。
- **前端**：`pin-compat.ts` 加 `isKindCompatible`（独立函数，与 `isCompatibleWith` 分开），`pin-validator.ts` `checkConnection` 在 PinType 检查之前先做 PinKind 闸门。FlowGram 引脚节点 CSS 按 PinKind 分形状（Exec=方/三角、Data=圆）和颜色，schema 缓存里已有 `kind` 字段直接读。
- **可观测性**：本 Phase 不引入新事件；缓存写入沿用 Phase 1 的 `OutputCache::write_now`。

**Tech Stack:**
- Rust（`crates/core/src/pin.rs` 工厂方法、`crates/nodes-io/src/modbus_read.rs` 引脚声明、新集成测试 `tests/pin_kind_phase2.rs`）
- TypeScript / React 18 / FlowGram.AI（`web/src/lib/pin-compat.ts`、`web/src/lib/pin-validator.ts`、`web/src/components/FlowgramCanvas.tsx`、CSS）
- Vitest（前端单测 + 合约测试）/ Playwright（E2E）
- ts-rs（已自动覆盖 `kind` 字段，无需新增导出）

---

## File Structure

| 操作 | 路径 | 责任 |
|------|------|------|
| 修改 | `crates/core/src/pin.rs` | 新增 `PinDefinition::output_named_data` 工厂方法（DRY 替代字面量） |
| 修改 | `crates/nodes-io/src/modbus_read.rs` | `output_pins()` 返回两项：`out` (Exec) + `latest` (Data) |
| 创建 | `tests/pin_kind_phase2.rs` | 集成测试：modbusRead 双路径产出（Exec 推送 + Data 缓存写入） |
| 创建 | `tests/fixtures/pin_kind_matrix.jsonc` | PinKind 兼容矩阵权威 fixture（4 条配对） |
| 创建 | `crates/core/tests/pin_kind_contract.rs` | Rust 端 PinKind contract 测试（消费上述 fixture） |
| 修改 | `web/src/lib/pin-compat.ts` | 新增 `isKindCompatible(from: PinKind, to: PinKind): boolean` |
| 创建 | `web/src/lib/__tests__/pin-kind-compat.test.ts` | 前端 contract 测试（消费同一 fixture） |
| 修改 | `web/src/lib/pin-validator.ts` | `checkConnection` 加跨 Kind 拦截 + 新 rejection variant `incompatible-kinds` |
| 修改 | `web/src/lib/__tests__/pin-validator.test.ts` | 加跨 Kind 拒绝用例 |
| 修改 | `web/src/components/FlowgramCanvas.tsx` | 引脚渲染按 `kind` 切换形状 + 颜色 class |
| 创建 | `web/src/components/FlowgramCanvas.css`（或既有样式文件） | 引脚 CSS：`.port-exec` / `.port-data` 形状差异 |
| 创建 | `web/e2e/pin-kind-modbus.spec.ts` | E2E：modbusRead 双输出连接 + 跨 Kind 拒连接视觉验证 |
| 修改 | `docs/adr/0014-执行边与数据边分离.md` | 实施进度章节：Phase 2 已实施（commit SHA） |
| 修改 | `crates/nodes-io/AGENTS.md` | modbusRead 节点条目：双输出引脚 + `latest` Data 引脚说明 |
| 修改 | `CLAUDE.md`（= `AGENTS.md`） | "Current batch of ADRs" / "ADR Execution Order" 中 ADR-0014 状态升级到 Phase 2 |
| 修改 | `~/.claude/projects/-home-zhihongniu-Nazh/memory/MEMORY.md` 与对应 memory 文件 | 工程进度同步 |

---

## Task 1：PinDefinition::output_named_data 工厂方法

**Files:**
- Modify: `crates/core/src/pin.rs`（在 `impl PinDefinition` 块内现有 4 个工厂方法之后追加）

**Why first:** `output_named_data` 是后续 modbusRead 声明 `latest` 引脚的便利函数。提前抽出避免 modbus_read.rs 写一长串 PinDefinition 字面量；同时给未来 Phase 3 PURE 节点复用。

- [ ] **Step 1：写失败的单测**

在 `crates/core/src/pin.rs` 的 `#[cfg(test)] mod tests` 块（约 line 277 之后）找到现有的 PinDefinition 工厂测试附近，追加：

```rust
#[test]
fn output_named_data_工厂方法生成正确字段() {
    let pin = PinDefinition::output_named_data(
        "latest",
        "最近读数",
        PinType::Json,
        "缓存最近一次读取的寄存器值",
    );
    assert_eq!(pin.id, "latest");
    assert_eq!(pin.label, "最近读数");
    assert_eq!(pin.pin_type, PinType::Json);
    assert_eq!(pin.direction, PinDirection::Output);
    assert!(!pin.required, "Data 输出非必需（拉取式）");
    assert_eq!(pin.kind, PinKind::Data);
    assert_eq!(pin.description.as_deref(), Some("缓存最近一次读取的寄存器值"));
}
```

- [ ] **Step 2：跑测试确认失败**

```
cargo test -p nazh-core --lib output_named_data_工厂方法生成正确字段
```

期望：编译失败，`PinDefinition::output_named_data` 未定义。

- [ ] **Step 3：实现工厂方法**

在 `impl PinDefinition` 末尾（line 274 `}` 前）追加：

```rust
    /// 多输出节点的 Data 引脚工厂——`required=false`、`kind=PinKind::Data`。
    ///
    /// 典型用途：节点在主 Exec 输出之外暴露"可拉取的最近态"（如 `modbusRead` 的
    /// `latest`）。下游通过 [`OutputCache`](crate::OutputCache) 槽位拉值，不阻塞
    /// 上游 transform。`id` 与 `label` 由调用方指定（不像 `output()` 默认 `"out"`）。
    pub fn output_named_data(
        id: impl Into<String>,
        label: impl Into<String>,
        pin_type: PinType,
        description: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            pin_type,
            direction: PinDirection::Output,
            required: false,
            kind: PinKind::Data,
            description: Some(description.into()),
        }
    }
```

- [ ] **Step 4：跑测试确认通过 + 全包测试不回归**

```
cargo test -p nazh-core --lib output_named_data_工厂方法生成正确字段
cargo test -p nazh-core
```

- [ ] **Step 5：fmt + clippy**

```
cargo fmt --all
cargo clippy -p nazh-core --all-targets -- -D warnings
```

- [ ] **Step 6：commit**

```bash
git add crates/core/src/pin.rs
git commit -s -m "feat(core): PinDefinition::output_named_data 工厂方法

ADR-0014 Phase 2 准备。多输出节点声明 Data 引脚的 DRY 工厂，
required=false / kind=PinKind::Data 默认值，避免业务节点字面量
铺写 PinDefinition。modbusRead latest 引脚是首个使用方。"
```

---

## Task 2：modbusRead 加 `latest` Data 输出引脚

**Files:**
- Modify: `crates/nodes-io/src/modbus_read.rs:285-290`（`output_pins` 方法）
- Modify: `crates/nodes-io/src/modbus_read.rs:394-398`（现有单测断言"只声明单个输出端口"需更新）

- [ ] **Step 1：更新现有单测期望两个输出引脚**

找到 `crates/nodes-io/src/modbus_read.rs` 测试模块中 `output_pins` 相关断言（约 line 394-398）：

```rust
let pins = node.output_pins();
assert_eq!(pins.len(), 1, "modbusRead 只声明单个输出端口");
assert_eq!(pins[0].id, "out");
assert_eq!(pins[0].pin_type, PinType::Json);
assert!(!pins[0].required, "输出端口默认 required=false");
```

替换为：

```rust
let pins = node.output_pins();
assert_eq!(pins.len(), 2, "modbusRead 声明两个输出端口：out (Exec) + latest (Data)");

let out_pin = pins.iter().find(|p| p.id == "out").expect("缺 out 引脚");
assert_eq!(out_pin.pin_type, PinType::Json);
assert_eq!(out_pin.kind, PinKind::Exec);
assert!(!out_pin.required);

let latest_pin = pins.iter().find(|p| p.id == "latest").expect("缺 latest 引脚");
assert_eq!(latest_pin.pin_type, PinType::Json);
assert_eq!(latest_pin.kind, PinKind::Data);
assert!(!latest_pin.required, "Data 拉取式引脚 required=false");
```

注意：测试模块顶部 `use` 语句若没有 `PinKind`，需要补上：

```rust
use nazh_core::{...existing..., PinKind};
```

`expect` 在测试里被 `#[allow(clippy::expect_used)]` 模块属性覆盖（项目惯例），先确认该模块是否已有该 allow，没有就加：

```rust
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    // ...
}
```

- [ ] **Step 2：跑测试确认失败**

```
cargo test -p nodes-io --lib modbus_read
```

期望：原断言 `assert_eq!(pins.len(), 1, ...)` 失败（`output_pins` 当前只返回 1 项）。

- [ ] **Step 3：修改 output_pins 返回两项**

替换 `crates/nodes-io/src/modbus_read.rs:280-290`：

```rust
    /// 输出引脚：
    /// - `out`（Json/Exec）：每次执行向下游推送的寄存器读取结果。
    /// - `latest`（Json/Data）：拉取式槽位，缓存最近一次读数；下游 PURE 节点或
    ///   独立时钟触发的 transform 可在不重新执行 modbusRead 的情况下读到最新值
    ///   （ADR-0014 Phase 2 引入）。
    ///
    /// 注：[`Self::input_pins`] 保留 trait 默认（单 `Any` 输入）——modbusRead
    /// 常作为根节点或被 `timer`（输出 `Any`）触发，input 形状不重要。
    fn output_pins(&self) -> Vec<PinDefinition> {
        vec![
            PinDefinition::output(
                PinType::Json,
                "寄存器读取结果合并入 input payload 的 JSON 对象",
            ),
            PinDefinition::output_named_data(
                "latest",
                "最近读数",
                PinType::Json,
                "拉取式槽位：缓存最近一次寄存器读数，下游可在不触发 modbusRead 重读的前提下取最新值",
            ),
        ]
    }
```

需要保证文件顶部 `use` 已包含 `PinKind`——但因为 `PinKind` 通过 `output_named_data` 工厂内部使用，调用方实际不需要直接 import；若已有的 `use nazh_core::{...}` 缺 `PinKind` 依然能编译。仅测试模块需要直接 `PinKind` 标识符。

- [ ] **Step 4：跑测试确认通过**

```
cargo test -p nodes-io --lib modbus_read
```

- [ ] **Step 5：跑工作空间全测试，确认无回归**

```
cargo test --workspace
```

特别关注：`tests/workflow.rs` 中包含 modbusRead 的 DAG 测试是否仍通过——Phase 1 的 runner 双路径骨架配合 `data_output_pin_ids` 应让 `latest` 引脚自动走缓存槽，不影响 `out` 推送给现有下游。

- [ ] **Step 6：fmt + clippy**

```
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
```

- [ ] **Step 7：commit**

```bash
git add crates/nodes-io/src/modbus_read.rs
git commit -s -m "feat(nodes-io): modbusRead 新增 latest Data 输出引脚

ADR-0014 Phase 2 第一个真实 Data 引脚用例。原 out (Json/Exec)
保留语义不变（每次 transform 向下游 Exec 推送）；新 latest (Json/Data)
让下游以拉取式拿最近一次读数，不需要重新执行 modbusRead。

Phase 1 已实现的 runner 双路径骨架（data_output_pin_ids 非空时
分裂 Exec 推送 vs Data 缓存写入）此处首次在生产节点激活。"
```

---

## Task 3：集成测试：modbusRead 双路径写入缓存

**Files:**
- Create: `tests/pin_kind_phase2.rs`

**意图：** Phase 1 的 `tests/pin_kind_phase1.rs` 用 stub 节点验证骨架；Phase 2 用真实 modbusRead 节点验证业务路径——modbusRead 在没有真实 Modbus 连接的情况下走 fallback 路径（返回测试值），让 transform 真的执行一次，断言 `OutputCache` 的 `latest` 槽位被写入了正确值，且下游 Exec 节点也通过 `out` 收到了同一份 payload。

- [ ] **Step 1：写失败的集成测试**

新建 `tests/pin_kind_phase2.rs`：

```rust
//! ADR-0014 Phase 2 集成测试：modbusRead `latest` Data 引脚的端到端验证。
//!
//! 场景：单节点工作流 `modbusRead`（无连接配置 → fallback 路径）。
//! 断言：transform 执行后，OutputCache 的 `latest` 槽被写入；Exec `out` 同样
//! 通过结果通道收到同一份 payload（双路径同源）。

use std::sync::Arc;

use nazh_engine::{
    Plugin, ProjectAst, ProjectAstConnection, ProjectAstNode, deploy_workflow,
};
use serde_json::json;
use tokio::time::{Duration, sleep};

#[tokio::test(flavor = "multi_thread")]
async fn modbus_read_的_latest_data_引脚被写入缓存槽() {
    let mut registry = nazh_engine::standard_registry();
    let _ = &mut registry; // 借用避免 lint

    // 单节点工作流：仅 modbusRead，无 connection_id（走 fallback / mock 行为）
    let project = ProjectAst {
        nodes: vec![ProjectAstNode {
            id: "reader".to_owned(),
            node_type: "modbusRead".to_owned(),
            config: json!({
                "register_kind": "holding",
                "address": 0,
                "count": 2
            }),
        }],
        connections: vec![],
    };

    let runtime = deploy_workflow(project, Arc::new(registry))
        .expect("部署单节点 modbusRead 工作流应该成功");

    // 触发一次执行
    runtime
        .submit(json!({}))
        .await
        .expect("submit 应该成功");

    // 等待 transform 完成（runner 异步推进，给 100ms 让事件循环跑完）
    sleep(Duration::from_millis(100)).await;

    // 断言 1：OutputCache 中 reader 节点的 latest 槽有值
    let cache = runtime
        .output_cache_for("reader")
        .expect("reader 节点的 OutputCache 应该存在");
    let cached = cache.read("latest").expect("latest 槽应该被写入");
    assert!(
        cached.value.is_object() || cached.value.is_array(),
        "modbusRead 的 latest 应是 JSON object/array，实际：{}",
        cached.value
    );

    // 断言 2：Exec out 路径也收到了结果（runtime.recv_result）
    let result = runtime
        .try_recv_result()
        .expect("Exec out 应该把结果推到 result 通道");
    assert_eq!(
        result.payload(),
        &cached.value,
        "Exec out 与 Data latest 应是同源 payload"
    );

    runtime.shutdown().await;
}
```

> ⚠️ 此处假设 `WorkflowDeployment` 暴露 `output_cache_for(node_id)` 与 `try_recv_result()` 方法。**实施前**先 grep 确认实际 API；若名字不一致，参考 Phase 1 测试 `tests/pin_kind_phase1.rs` 的对接模式调整为相同形态。如果 runtime 不暴露 `output_cache_for`，新增一个 `pub` 访问器（在 `src/graph/runtime.rs` 或 `src/graph/deploy.rs` 的 `WorkflowDeployment` 结构上加 `pub fn output_cache_for(&self, node_id: &str) -> Option<&Arc<OutputCache>>`）。

- [ ] **Step 2：跑测试确认失败**

```
cargo test --test pin_kind_phase2
```

期望：要么编译失败（API 名不对），要么 Data 槽读取返回 None（runner 双路径骨架在 Phase 1 已实现，但需确认 modbusRead 的 fallback 路径真的让 transform 跑通——若 fallback 抛错，Data 槽不会被写入）。

- [ ] **Step 3：根据失败信息修补**

可能场景：
- **API 不匹配**：参考 `tests/pin_kind_phase1.rs` 找到正确的 runtime 访问器名；如缺则加。
- **modbusRead fallback 抛错**：修复使其在无 connection_id 时返回测试值（已存在还是新增）；查 modbus_read.rs:296+ `transform` 实现。Phase 1 的 mock 路径若已存在，复用即可。
- **OutputCache slot 读取语义差异**：再读 `crates/core/src/cache.rs` 的 `read` 实际签名。

修补后重跑直至通过。

- [ ] **Step 4：跑全测试，确认无回归**

```
cargo test --workspace
```

- [ ] **Step 5：fmt + clippy**

```
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
```

- [ ] **Step 6：commit**

```bash
git add tests/pin_kind_phase2.rs src/  # 若有 runtime 访问器新增
git commit -s -m "test: modbusRead latest Data 引脚双路径集成测试

ADR-0014 Phase 2 集成验证。单节点 modbusRead 工作流，无连接走 fallback；
断言 OutputCache.latest 被写入 + Exec out 推送同源 payload。

补全 WorkflowDeployment::output_cache_for 访问器（如有需要），让测试与
未来 IPC 暴露同一接口。"
```

---

## Task 4：PinKind 兼容矩阵 fixture + Rust contract 测试

**Files:**
- Create: `tests/fixtures/pin_kind_matrix.jsonc`
- Create: `crates/core/tests/pin_kind_contract.rs`

**意图：** PinKind 是封闭枚举，矩阵小到只有 4 条（exec→exec ✓ / data→data ✓ / exec→data ✗ / data→exec ✗），但单独成 fixture 让 Rust + TS 共享同一真值源——避免 `is_compatible_with` 在两端漂移。

- [ ] **Step 1：写 fixture**

新建 `tests/fixtures/pin_kind_matrix.jsonc`：

```jsonc
// PinKind 兼容矩阵的权威合约（ADR-0014）。
//
// Rust: crates/core/src/pin.rs `PinKind::is_compatible_with`
// TS:   web/src/lib/pin-compat.ts `isKindCompatible`
//
// 设计前提（ADR-0014 设计文档 §五）：
//   引脚必须 Kind 完全一致——Exec ↔ Exec / Data ↔ Data。
//   跨 Kind 是部署期与连接期硬错误。
//
// 修改本文件时同步检查上述两份实现，且必须跑：
//   cargo test -p nazh-core --test pin_kind_contract
//   npm --prefix web run test pin-kind-compat
//
// 序列化形态：PinKind 是 `#[serde(rename_all = "lowercase")]` 的简单枚举：
//   "exec"   -> PinKind::Exec
//   "data"   -> PinKind::Data
//
// 覆盖纪律：每个新增 PinKind 变体需补 N+N 条配对（自反 + 与已有变体两两）。

{
  "pairs": [
    { "from": "exec", "to": "exec", "compatible": true  },
    { "from": "data", "to": "data", "compatible": true  },
    { "from": "exec", "to": "data", "compatible": false },
    { "from": "data", "to": "exec", "compatible": false }
  ]
}
```

- [ ] **Step 2：写 Rust contract 测试**

新建 `crates/core/tests/pin_kind_contract.rs`：

```rust
//! PinKind 兼容矩阵合约测试（ADR-0014）。
//!
//! 与 `tests/fixtures/pin_kind_matrix.jsonc` 配对——fixture 是单一真值源，
//! 同一份 fixture 也被前端 `web/src/lib/__tests__/pin-kind-compat.test.ts`
//! 消费。任意一方漂移即 CI 红。

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use nazh_core::PinKind;
use serde::Deserialize;

#[derive(Deserialize)]
struct Matrix {
    pairs: Vec<Pair>,
}

#[derive(Deserialize)]
struct Pair {
    from: PinKind,
    to: PinKind,
    compatible: bool,
}

fn fixture_path() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // crates/core -> 工作空间根
    p.pop();
    p.pop();
    p.push("tests/fixtures/pin_kind_matrix.jsonc");
    p
}

#[test]
fn pin_kind_矩阵每条配对与_is_compatible_with_一致() {
    let path = fixture_path();
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("无法读取 {}: {e}", path.display()));
    // 与 pin_compat_contract 一样用同一注释剥离工具
    let stripped = nazh_core::testing::strip_jsonc_comments(&raw);
    let matrix: Matrix = serde_json::from_str(&stripped)
        .expect("pin_kind_matrix.jsonc 反序列化失败");

    assert!(!matrix.pairs.is_empty(), "矩阵不能为空");

    for pair in matrix.pairs {
        let actual = pair.from.is_compatible_with(pair.to);
        assert_eq!(
            actual, pair.compatible,
            "PinKind 矩阵漂移：{} → {} 期望 {}，实际 {}",
            pair.from, pair.to, pair.compatible, actual
        );
    }
}

#[test]
fn pin_kind_矩阵覆盖所有变体两两配对() {
    let path = fixture_path();
    let raw = std::fs::read_to_string(&path).unwrap();
    let stripped = nazh_core::testing::strip_jsonc_comments(&raw);
    let matrix: Matrix = serde_json::from_str(&stripped).unwrap();

    // 所有变体（PinKind 枚举仅 Exec / Data）
    let variants = [PinKind::Exec, PinKind::Data];
    for from in variants {
        for to in variants {
            assert!(
                matrix.pairs.iter().any(|p| p.from == from && p.to == to),
                "缺 PinKind 配对：{from} → {to}"
            );
        }
    }
}
```

> ⚠️ 假设 `nazh_core::testing::strip_jsonc_comments` 已在 `crates/core` 里以 `pub` 形式暴露给 contract 测试（`pin_compat_contract` 在用）。若未暴露：
>   - **方案 A**：直接 inline 同样的 strip 逻辑（参考 `crates/core/tests/pin_compat_contract.rs` 实现）
>   - **方案 B**：把 strip 工具提到 `crates/core/src/testing.rs` 公开
>
> 实施时确认现状选 A 或 B。**优先 A**——避免改动 Ring 0 公共 API。

- [ ] **Step 3：跑测试确认失败**

```
cargo test -p nazh-core --test pin_kind_contract
```

- [ ] **Step 4：根据 strip 工具方案修补 contract 测试，跑通**

```
cargo test -p nazh-core --test pin_kind_contract
```

- [ ] **Step 5：fmt + clippy**

```
cargo fmt --all
cargo clippy -p nazh-core --all-targets -- -D warnings
```

- [ ] **Step 6：commit**

```bash
git add tests/fixtures/pin_kind_matrix.jsonc crates/core/tests/pin_kind_contract.rs
git commit -s -m "test(core): PinKind 兼容矩阵 fixture + Rust contract 测试

ADR-0014 Phase 2。PinKind 矩阵（4 条配对）独立 fixture，与 PinType
矩阵平级——同一份 jsonc 被 Rust + TS 各自合约测试消费，杜绝单边漂移。"
```

---

## Task 5：前端 isKindCompatible + Vitest contract 测试

**Files:**
- Modify: `web/src/lib/pin-compat.ts`
- Create: `web/src/lib/__tests__/pin-kind-compat.test.ts`

- [ ] **Step 1：在 pin-compat.ts 末尾追加 isKindCompatible**

修改 `web/src/lib/pin-compat.ts`：

```typescript
import type { PinKind, PinType } from '../types';

// ... 现有 isCompatibleWith 不变 ...

/**
 * 判断"上游引脚 Kind `from` → 下游引脚 Kind `to`"是否可连。
 *
 * ADR-0014 求值语义二分：引脚 Kind 必须完全一致——Exec ↔ Exec、Data ↔ Data，
 * 跨 Kind 一律拒绝。理由见 `docs/superpowers/specs/2026-04-28-pin-kind-exec-data-design.md`。
 *
 * 必须与 Rust 端 `PinKind::is_compatible_with`（在 `crates/core/src/pin.rs`）
 * 严格一致，由 `tests/fixtures/pin_kind_matrix.jsonc` 合约保证。
 */
export function isKindCompatible(from: PinKind, to: PinKind): boolean {
  return from === to;
}
```

注意：需要在 `web/src/types.ts` 重导出 `PinKind`（如尚未导出）：

```typescript
export type { PinKind } from './generated/PinKind';
```

如已导出（很可能 ADR-0014 Phase 1 ts-rs 时已加），跳过。

- [ ] **Step 2：写 Vitest contract 测试**

新建 `web/src/lib/__tests__/pin-kind-compat.test.ts`：

```typescript
// PinKind 矩阵合约测试（ADR-0014）。
//
// 消费 tests/fixtures/pin_kind_matrix.jsonc 作为单一真值源——同一份
// fixture 也被 Rust（crates/core/tests/pin_kind_contract.rs）消费。
// 任意一方漂移即 CI 红。

import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { describe, expect, it } from 'vitest';

import { isKindCompatible } from '../pin-compat';
import type { PinKind } from '../../types';

interface Pair {
  from: PinKind;
  to: PinKind;
  compatible: boolean;
}

interface Matrix {
  pairs: Pair[];
}

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const fixturePath = path.resolve(__dirname, '../../../../tests/fixtures/pin_kind_matrix.jsonc');

function stripJsoncComments(raw: string): string {
  // 复用 pin-compat.test.ts 同款的简易剥离（行内 `// ...` + 块注释 `/* ... */`）
  // 若 pin-compat.test.ts 已导出 strip 工具，直接 import。
  return raw
    .replace(/\/\*[\s\S]*?\*\//g, '')
    .replace(/^\s*\/\/.*$/gm, '')
    .trim();
}

const matrix: Matrix = JSON.parse(stripJsoncComments(readFileSync(fixturePath, 'utf8')));

describe('PinKind 矩阵合约（与 Rust 端共享 fixture）', () => {
  it.each(matrix.pairs)(
    '$from → $to 应返回 $compatible',
    ({ from, to, compatible }) => {
      expect(isKindCompatible(from, to)).toBe(compatible);
    },
  );

  it('矩阵覆盖所有 PinKind 变体两两配对', () => {
    const variants: PinKind[] = ['exec', 'data'];
    for (const from of variants) {
      for (const to of variants) {
        expect(matrix.pairs.some((p) => p.from === from && p.to === to)).toBe(
          true,
        );
      }
    }
  });
});
```

> 注：若 `pin-compat.test.ts` 已暴露 `stripJsoncComments`，直接 import 替代本地 inline。

- [ ] **Step 3：跑测试确认通过**

```
npm --prefix web run test pin-kind-compat
```

- [ ] **Step 4：lint**

```
npm --prefix web run lint
```

- [ ] **Step 5：commit**

```bash
git add web/src/lib/pin-compat.ts web/src/lib/__tests__/pin-kind-compat.test.ts web/src/types.ts
git commit -s -m "feat(web): isKindCompatible + Vitest 合约测试

ADR-0014 Phase 2 前端实现。isKindCompatible(from, to) = from === to，
与 Rust PinKind::is_compatible_with 共享 pin_kind_matrix.jsonc。"
```

---

## Task 6：pin-validator.ts 跨 Kind 拦截

**Files:**
- Modify: `web/src/lib/pin-validator.ts`
- Modify: `web/src/lib/__tests__/pin-validator.test.ts`

- [ ] **Step 1：单测加跨 Kind 拒绝用例**

在 `web/src/lib/__tests__/pin-validator.test.ts` 现有 `describe` 块内追加：

```typescript
import { primePinSchemaCache } from '../pin-schema-cache'; // 用真实 cache API；或 mock findPin

describe('PinKind 跨 Kind 连接被拒（ADR-0014 Phase 2）', () => {
  beforeEach(() => {
    // 假设 schema cache 暴露的写入接口：
    // 上游 nodeA.outA: Json/Exec
    // 下游 nodeB.inB: Json/Data
    primePinSchemaCache('nodeA', {
      input_pins: [],
      output_pins: [{
        id: 'outA',
        label: 'outA',
        pin_type: { kind: 'json' },
        direction: 'output',
        required: false,
        kind: 'exec',
      }],
    });
    primePinSchemaCache('nodeB', {
      input_pins: [{
        id: 'inB',
        label: 'inB',
        pin_type: { kind: 'json' },
        direction: 'input',
        required: true,
        kind: 'data',
      }],
      output_pins: [],
    });
  });

  it('Exec → Data 被拒（PinType 兼容也不行）', () => {
    const result = checkConnection('nodeA', 'outA', 'nodeB', 'inB');
    expect(result.allow).toBe(false);
    expect(result.rejection?.kind).toBe('incompatible-kinds');
  });
});
```

> 注：`primePinSchemaCache` 是假设的 cache 写入 API；实际使用 `pin-schema-cache.ts` 现有的写入接口（grep `refreshNodePinSchema` / 已有的测试 helper）。看现状决定用哪个。

- [ ] **Step 2：跑测试确认失败**

```
npm --prefix web run test pin-validator
```

期望：`incompatible-kinds` rejection variant 不存在 / `checkConnection` 没做 Kind 校验。

- [ ] **Step 3：扩展 ConnectionRejection union 与 checkConnection 逻辑**

修改 `web/src/lib/pin-validator.ts`：

```typescript
import type { PinKind, PinType } from '../types';

import { findPin, formatPinType } from './pin-schema-cache';
import { isCompatibleWith, isKindCompatible } from './pin-compat';

export type ConnectionRejection =
  | {
      kind: 'incompatible-types';
      fromNodeId: string;
      fromPortId: string;
      toNodeId: string;
      toPortId: string;
      fromType: PinType;
      toType: PinType;
    }
  | {
      kind: 'incompatible-kinds';
      fromNodeId: string;
      fromPortId: string;
      toNodeId: string;
      toPortId: string;
      fromKind: PinKind;
      toKind: PinKind;
    };

// ... ConnectionCheckResult / ALLOW 不变 ...

export function checkConnection(
  fromNodeId: string,
  fromPortId: string | number,
  toNodeId: string,
  toPortId: string | number,
): ConnectionCheckResult {
  const fromPin = findPin(fromNodeId, fromPortId, 'output');
  const toPin = findPin(toNodeId, toPortId, 'input');

  if (!fromPin || !toPin) {
    return ALLOW;
  }

  // 1) 先看 PinKind——跨 Kind 是硬错误，无须看 PinType
  if (!isKindCompatible(fromPin.kind, toPin.kind)) {
    return {
      allow: false,
      rejection: {
        kind: 'incompatible-kinds',
        fromNodeId,
        fromPortId: String(fromPortId),
        toNodeId,
        toPortId: String(toPortId),
        fromKind: fromPin.kind,
        toKind: toPin.kind,
      },
    };
  }

  // 2) Kind 一致后再看 PinType（沿用现有逻辑）
  if (isCompatibleWith(fromPin.pin_type, toPin.pin_type)) {
    return ALLOW;
  }

  return {
    allow: false,
    rejection: {
      kind: 'incompatible-types',
      fromNodeId,
      fromPortId: String(fromPortId),
      toNodeId,
      toPortId: String(toPortId),
      fromType: fromPin.pin_type,
      toType: toPin.pin_type,
    },
  };
}

// formatRejection 扩展
export function formatRejection(rejection: ConnectionRejection): string {
  if (rejection.kind === 'incompatible-kinds') {
    return `连接不兼容（求值语义不同）：${rejection.fromNodeId}.${rejection.fromPortId} (${rejection.fromKind}) → ${rejection.toNodeId}.${rejection.toPortId} (${rejection.toKind})。Exec 引脚只能连 Exec，Data 引脚只能连 Data。`;
  }
  const { fromNodeId, fromPortId, toNodeId, toPortId, fromType, toType } = rejection;
  return `连接不兼容：${fromNodeId}.${fromPortId} (${formatPinType(fromType)}) → ${toNodeId}.${toPortId} (${formatPinType(toType)})`;
}
```

- [ ] **Step 4：跑测试确认通过**

```
npm --prefix web run test pin-validator
npm --prefix web run test pin-compat
```

- [ ] **Step 5：lint + 全 vitest**

```
npm --prefix web run lint
npm --prefix web run test
```

- [ ] **Step 6：commit**

```bash
git add web/src/lib/pin-validator.ts web/src/lib/__tests__/pin-validator.test.ts
git commit -s -m "feat(web): pin-validator 加跨 PinKind 连接拦截

ADR-0014 Phase 2 前端连接期校验。checkConnection 在 PinType 校验
之前先做 PinKind 闸门——Exec ↔ Exec / Data ↔ Data，跨 Kind 立即拒。
新 incompatible-kinds rejection variant + 中文提示。"
```

---

## Task 7：FlowGram 引脚视觉区分（形状 + 颜色）

**Files:**
- Modify: `web/src/components/FlowgramCanvas.tsx`（或对应 CSS / styled-components 文件）
- Possibly modify: `web/src/components/flowgram/FlowgramNodeGlyph.tsx`（节点引脚渲染主入口，先 grep 确认）

**意图：** Phase 1 已让前端能从 IPC 读到 `kind` 字段；本 Task 让用户**看到**——Exec 引脚保持现有样式（圆点 / 三角，看 FlowGram 默认），Data 引脚视觉区别（建议 Data 引脚用空心圆 ○ / 颜色偏冷色调，与 Exec 实心区分）。

> ⚠️ FlowGram 的引脚渲染机制要先调研——通常通过 `<Field>` / `<Port>` 组件的 props，或全局 CSS 类切换。**实施前**先 grep `Port` / `port-` 在 `web/src/components/flowgram/` 全部用例，确认改造点。如果 FlowGram 不允许 props 自定义形状，回退方案：仅靠 CSS class（按 `data-pin-kind="data"` 属性切换）。

- [ ] **Step 1：调研 FlowGram 引脚渲染入口**

```
grep -rn "port" /home/zhihongniu/Nazh/web/src/components/flowgram --include='*.tsx' --include='*.ts' | head -30
grep -rn "Port" /home/zhihongniu/Nazh/web/src/components/flowgram --include='*.tsx' | head -20
```

确认引脚渲染由 FlowGram 内部组件完成，还是有项目自己的覆写点。

- [ ] **Step 2：写 Vitest 渲染测试（snapshot 或 attribute）**

为 `FlowgramNodeGlyph`（或负责引脚渲染的组件）写一份测试，准备一个含 modbusRead 的节点状态，断言 `latest` 引脚 DOM 有 `data-pin-kind="data"` 属性 / `.port-data` class。

> 此处测试形态视组件结构而定。优先选择"DOM attribute 断言"而非视觉 snapshot，避免 CSS 微调引发 snapshot 抖动。

- [ ] **Step 3：跑测试失败**

```
npm --prefix web run test flowgram-node-glyph
```

- [ ] **Step 4：在引脚渲染处读 PinKind 并加 attribute / class**

视组件不同，类似如下（伪代码——实施时按真实组件改）：

```tsx
// FlowgramNodeGlyph.tsx 或同等位置
<port
  data-pin-kind={pin.kind}
  className={pin.kind === 'data' ? 'port port-data' : 'port port-exec'}
  // ... 其他既有 props
/>
```

CSS（追加到对应 stylesheet）：

```css
.port-exec {
  /* 保持现有样式：实心圆 / 现有颜色——不动 */
}

.port-data {
  /* Data 引脚视觉区分：空心圆 + 冷色边框 */
  background: transparent;
  border: 2px solid var(--port-data-color, #3b82f6); /* 蓝紫，区分于 Exec 默认 */
}

/* tooltip 提示文案兼顾 */
.port-data::after {
  content: 'Data 引脚（拉取式）';
}
```

> ⚠️ 颜色变量与现有主题集成。用 CSS 变量 `--port-data-color` 让暗色主题切换不破。

- [ ] **Step 5：跑测试 + 启动 dev server 视觉验证**

```
npm --prefix web run test
cd src-tauri && ../web/node_modules/.bin/tauri dev --no-watch  # 桌面侧 + 浏览器视觉确认
```

放一个 modbusRead 节点到画布上，确认 `latest` 引脚视觉与 `out` 不同。

- [ ] **Step 6：fmt + lint**

```
npm --prefix web run lint
```

- [ ] **Step 7：commit**

```bash
git add web/src/components/flowgram/ web/src/styles/  # 视实际改动文件
git commit -s -m "feat(web): FlowGram 引脚按 PinKind 视觉区分

ADR-0014 Phase 2 用户首次看到求值语义二分。Data 引脚（modbusRead.latest）
用空心圆 + 冷色边框，与 Exec 实心圆区分；data-pin-kind attribute
+ port-data CSS class 双管道，方便 E2E 选择器命中。"
```

---

## Task 8：E2E：用例 2 跨时钟读取 + 跨 Kind 拒连接

**Files:**
- Create: `web/e2e/pin-kind-modbus.spec.ts`

- [ ] **Step 1：写 E2E 失败用例**

新建 `web/e2e/pin-kind-modbus.spec.ts`：

```typescript
import { test, expect } from '@playwright/test';
import { openCleanWorkspace } from './helpers';

test.describe('ADR-0014 Phase 2：modbusRead Data 引脚视觉与连接校验', () => {
  test('modbusRead 节点同时显示 Exec out 与 Data latest 引脚', async ({ page }) => {
    await openCleanWorkspace(page);

    // 从 NodeAddPanel 拖入 modbusRead
    await page.getByRole('button', { name: /添加节点/ }).click();
    await page.getByRole('menuitem', { name: /modbusRead/i }).click();

    // 等待节点渲染完毕
    const node = page.locator('[data-node-type="modbusRead"]').first();
    await expect(node).toBeVisible();

    // 断言：节点显示两个输出引脚——out (Exec) + latest (Data)
    const execPort = node.locator('[data-pin-kind="exec"]');
    const dataPort = node.locator('[data-pin-kind="data"]');
    await expect(execPort).toBeVisible();
    await expect(dataPort).toBeVisible();
  });

  test('Exec 引脚不能连接到 Data 输入引脚（跨 Kind 拒绝）', async ({ page }) => {
    await openCleanWorkspace(page);

    // 设置：modbusRead → if 节点（其 input 是 Exec）
    // 先尝试把 modbusRead.latest（Data）拖到 if.in（Exec）——应被拒
    // 具体拖拽逻辑用 helpers 提供的 dragLineEnd（参考 deploy-and-dispatch.spec.ts）
    // ...

    // 拒绝表现：toast / console.warn 出现 "连接不兼容（求值语义不同）" 中文文案
    await expect(page.locator('text=连接不兼容（求值语义不同）')).toBeVisible({ timeout: 5000 });
  });
});
```

> 注：第二个用例 "拖拽 + 断言" 的具体步骤需要参考 `web/e2e/helpers.ts` 现有的画布拖拽辅助函数。若 helpers 没有"连接两端口" helper，可在本任务中补一个 `dragPortToPort(page, fromSelector, toSelector)`。

- [ ] **Step 2：跑 E2E 确认失败**

```
cd src-tauri && ../web/node_modules/.bin/tauri build --debug  # 必须先编译可执行文件
npm --prefix web run test:e2e pin-kind-modbus
```

- [ ] **Step 3：根据失败修补**

可能：选择器不匹配 / Toast 文案需要前端 surface（pin-validator 拦截后是否真的展示给用户？检查 `flowgram-line-panel.ts` 是否调用 `formatRejection` 推 toast）。

修补到通过。

- [ ] **Step 4：commit**

```bash
git add web/e2e/pin-kind-modbus.spec.ts web/e2e/helpers.ts  # 若有 helper 新增
git commit -s -m "test(e2e): ADR-0014 Phase 2 modbusRead 双引脚 + 跨 Kind 拒连接

Playwright E2E 用例 2 真实可运行：
- modbusRead 节点显示 out (Exec) + latest (Data) 两个输出引脚视觉
- 跨 Kind 连接被 pin-validator 拦截，前端 toast 提示中文错误"
```

---

## Task 9：文档与状态同步

**Files:**
- Modify: `docs/adr/0014-执行边与数据边分离.md`
- Modify: `crates/nodes-io/AGENTS.md`
- Modify: `CLAUDE.md`（= `AGENTS.md` 根）
- Modify: `~/.claude/projects/-home-zhihongniu-Nazh/memory/MEMORY.md` 与对应 memory 文件

- [ ] **Step 1：ADR-0014 实施进度章节加 Phase 2**

打开 `docs/adr/0014-执行边与数据边分离.md`，找到现有"实施进度"章节（Phase 1 完成项），追加：

```markdown
### Phase 2（已实施 / YYYY-MM-DD）

- modbusRead 节点新增 `latest` Data 输出引脚——Phase 1 runner 双路径骨架在生产节点首次激活
- `PinDefinition::output_named_data` Ring 0 工厂方法
- PinKind 兼容矩阵 fixture（`tests/fixtures/pin_kind_matrix.jsonc`）+ Rust + TS 双侧合约测试
- 前端 `isKindCompatible` + `pin-validator.ts` 跨 Kind 拦截 + `incompatible-kinds` rejection variant
- FlowGram 引脚按 PinKind 视觉区分（Data=空心圆冷色 vs Exec=实心圆）
- Playwright E2E：modbusRead 双引脚视觉 + 跨 Kind 拒连接

涉及 commit：[填写实际 commit SHA 列表]
```

实际 SHA 在 commit 后回填。

- [ ] **Step 2：crates/nodes-io/AGENTS.md 更新 modbusRead 条目**

在节点对照表 / capability 列表中标注：modbusRead 现声明双输出（`out` Json/Exec + `latest` Json/Data）。

- [ ] **Step 3：根 CLAUDE.md 状态升级**

找到 "Current batch of ADRs" 中 ADR-0014 行，把 "Phase 1 已实施" 改为 "Phase 1 + Phase 2 已实施"。
"ADR Execution Order" 第 8 项同步。

- [ ] **Step 4：memory 同步**

更新 `MEMORY.md` 与 `project_system_architecture.md` / `project_architecture_review_2026_04.md`：
- 已实施 ADR 列表中 `0014(P1)` 改为 `0014(P1+P2)`
- "下一候选"提示更新（Phase 3 PURE 节点 / 或 ADR-0013 子图）

- [ ] **Step 5：fmt + clippy + 全测试一遍**

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm --prefix web run test
```

- [ ] **Step 6：commit**

```bash
git add docs/adr/ CLAUDE.md crates/nodes-io/AGENTS.md
# 加上 memory 文件路径
git commit -s -m "docs: ADR-0014 Phase 2 落地后状态同步

- ADR 实施进度章节加 Phase 2
- nodes-io AGENTS.md modbusRead 双输出引脚说明
- 根 CLAUDE.md ADR 执行顺序与 'Current batch of ADRs' 升级
- memory 文件同步进度"
```

---

## Self-Review

**1. Spec coverage check**（对照 `docs/superpowers/specs/2026-04-28-pin-kind-exec-data-design.md` §九 Phase 2）：
- ✅ 选 1 个现有节点扩展 Data 引脚（方向 A：modbusRead `latest`）→ Task 1 + 2
- ✅ IPC `describe_node_pins` 返回 PinKind → 已自动覆盖（PinDefinition.ts 已含 kind 字段，Phase 1 ts-rs 完成；本 plan **不需要** IPC 改动）
- ✅ 前端 ts-rs 类型同步 + `pin-compat.ts` 升级 → Task 5
- ✅ FlowGram 引脚渲染：Exec=三角 / Data=圆 + 颜色 → Task 7
- ✅ FlowGram `canAddLine` PinKind 校验 → Task 6（pin-validator.ts 直接喂 FlowgramCanvas 已接入的 checkConnection）
- ✅ 用例 2（跨时钟读取）真实可运行 → Task 3 集成测试 + Task 8 E2E
- ✅ E2E 测试 → Task 8

**2. Placeholder scan：**
- 所有 Task 都有完整代码 / shell 命令
- Task 7 的 FlowGram 集成点标注了 "实施前先 grep" 调研步骤——这是真实工作要求，不是 TBD
- Task 6 的 `primePinSchemaCache` 标注为"假设"——同样要求实施时确认。**这是合理的**：cache 写入接口名我没有当前的 grep 结果，要求 implementer subagent 在 Task 6 Step 1 之前先验证。
- Task 8 的拖拽辅助函数同上

**3. Type consistency：**
- `PinKind`、`PinDefinition`、`isKindCompatible`、`incompatible-kinds`、`output_named_data`、`OutputCache.read`、`output_cache_for` 在多个 Task 间引用——已确认所有签名一致
- TS 端 `PinKind` 是 `'exec' | 'data'` 字符串字面量联合（PinKind.ts 已生成），与 Rust 序列化形态一致——pin_kind_matrix.jsonc 的 `"from": "exec"` 字符串能反序列化到 Rust `PinKind` 枚举

**4. 风险点：**
- Task 3 集成测试依赖 modbusRead fallback 路径（无连接时能跑出值），若 fallback 抛错则测试要换"插一个 mock connection_manager 资源"路径——已在 Step 3 留出修补空间
- Task 7 FlowGram 自定义引脚渲染可能受框架限制——已留 CSS-only fallback 方案
- Task 4 contract 测试假设 `nazh_core::testing::strip_jsonc_comments` 公开——已留 inline 复制方案 A

---

## 执行交接

完成 Self-Review 后，按 superpowers:subagent-driven-development 派 fresh subagent 逐 task 执行。每 task 后两阶段审验（spec 合规先 + 代码质量后），再进下一 task。
