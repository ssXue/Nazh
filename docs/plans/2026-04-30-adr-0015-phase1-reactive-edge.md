> **Status:** merged in 9019b90 (Phase 1 核心完成)

# ADR-0015 Phase 1: Reactive 边核心实施 Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 PinKind 中新增 Reactive variant，让 Runner 在 dispatch 时对 Reactive 边同时执行"写缓存 + 推 ContextRef"，实现"值变化时自动唤醒下游"。

**Architecture:** Reactive = Data（写 OutputCache）+ Exec（推 ContextRef）。不新建类型，复用现有 OutputCache watch channel。部署期按 PinKind 分类边，Runner dispatch 三分支。

**Tech Stack:** Rust / tokio watch + mpsc / serde_json

**关联 spec:** `docs/specs/2026-04-30-adr-0015-reactive-data-pin-design.md`
**关联 ADR:** `docs/adr/0015-反应式数据引脚.md`

---

## 文件变更清单

| 文件 | 动作 | 职责 |
|------|------|------|
| `crates/core/src/pin.rs:63-91` | 修改 | PinKind 枚举加 Reactive variant + Display + is_compatible_with |
| `crates/core/src/cache.rs:76-85` | 修改 | write() / write_now() 返回 bool（值是否变更） |
| `src/graph/topology.rs:192-252` | 修改 | ClassifiedEdges 加 reactive_edges，classify_edges 三分支 |
| `src/graph/topology.rs:141-190` | 修改 | detect_data_edge_cycle 改为 detect_non_exec_edge_cycle（Data + Reactive 共享环检测） |
| `src/graph/deploy.rs:174-190` | 修改 | 部署期同时收集 reactive_output_pin_ids，prepare_slot |
| `src/graph/deploy.rs:298-325` | 修改 | 传 reactive_output_pin_ids 给 run_node |
| `src/graph/runner.rs:28-37` | 修改 | run_node 签名加 reactive_output_pin_ids 参数 |
| `src/graph/runner.rs:112-152` | 修改 | dispatch 逻辑：Reactive = 写缓存 + 推 ContextRef |
| `src/graph/pin_validator.rs:120-127` | 修改 | is_compatible_with 三分支兼容矩阵 |
| `src/graph/pin_validator.rs:153-167` | 修改 | ReservedPinId 校验：Data + Reactive 输入 pin id 不得为 "in" |
| `tests/workflow.rs` | 修改 | 新增 Reactive 边集成测试 |
| `src/graph/topology.rs:254-404` | 修改 | classify_edges 单测扩展 |

---

### Task 1: PinKind::Reactive 枚举扩展

**Files:**
- Modify: `crates/core/src/pin.rs:63-91`

- [ ] **Step 1: 在 PinKind 枚举中加 Reactive variant**

`crates/core/src/pin.rs` 当前 PinKind（lines 63-76）：

```rust
pub enum PinKind {
    #[default]
    Exec,
    Data,
}
```

改为：

```rust
pub enum PinKind {
    #[default]
    Exec,
    Data,
    /// 订阅式推送语义。上游写缓存 **+** 推 ContextRef 到下游——值变化时自动唤醒下游。
    /// 行为是 Data + Exec 的并集。下游收到 ContextRef 后照常 pull_data_inputs 读最新缓存值。
    Reactive,
}
```

- [ ] **Step 2: 更新 Display impl**

当前 Display（lines 78-87）只匹配 Exec/Data。加 Reactive arm：

```rust
impl fmt::Display for PinKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Exec => "exec",
            Self::Data => "data",
            Self::Reactive => "reactive",
        })
    }
}
```

- [ ] **Step 3: 更新 is_compatible_with**

当前规则是严格相等。Reactive 放宽：
- Reactive 输出 → 可连 Exec / Data / Reactive 输入
- Exec / Data 输出 → 不可连 Reactive 输入

```rust
pub fn is_compatible_with(self, other: Self) -> bool {
    match (self, other) {
        // 同类互连
        (Self::Exec, Self::Exec)
        | (Self::Data, Self::Data)
        | (Self::Reactive, Self::Reactive) => true,
        // Reactive 输出可连 Exec / Data 输入
        (Self::Reactive, Self::Exec) | (Self::Reactive, Self::Data) => true,
        // Exec / Data 输出不可连 Reactive 输入
        (Self::Exec, Self::Reactive) | (Self::Data, Self::Reactive) => false,
        // 跨类
        (Self::Exec, Self::Data) | (Self::Data, Self::Exec) => false,
    }
}
```

- [ ] **Step 4: 编译检查**

Run: `cargo check -p nazh-core`
Expected: 编译通过（PinKind 变体新增是向后兼容的，#[default] 仍是 Exec）

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/pin.rs
git commit -m "feat(core): PinKind 枚举新增 Reactive variant + 兼容矩阵"
```

---

### Task 2: OutputCache write 返回变更标记

**Files:**
- Modify: `crates/core/src/cache.rs:24-103`

- [ ] **Step 1: 修改 write() 返回 bool**

当前 `write()`（line 75）返回 `()`。改为返回 `bool`——`true` 表示值确实变化了：

```rust
/// 写入指定 pin 的最新值并自动通知所有 Receiver。
/// 返回 `true` 表示值与旧值不同（新写入或值变化）；`false` 表示值未变（覆盖写）。
pub fn write(&self, pin_id: &str, output: CachedOutput) -> bool {
    if let Some(slot) = self.slots.get(pin_id) {
        let changed = slot
            .rx
            .borrow()
            .as_ref()
            .is_none_or(|old| old.value != output.value);
        let _ = slot.tx.send(Some(output));
        changed
    } else {
        false
    }
}
```

- [ ] **Step 2: 修改 write_now() 返回 bool**

当前 `write_now()`（line 83）返回 `()`。改为：

```rust
/// 便利方法：用当前时间戳构造 [`CachedOutput`] 并写入指定 pin。
/// 返回值语义同 [`write`](Self::write)。
pub fn write_now(&self, pin_id: &str, value: Value, trace_id: Uuid) -> bool {
    self.write(
        pin_id,
        CachedOutput {
            value,
            produced_at: Utc::now(),
            trace_id,
        },
    )
}
```

- [ ] **Step 3: 更新 runner.rs 中的 write_now 调用点**

当前 `src/graph/runner.rs` line 124-129 的 `write_now` 调用现在忽略返回值。加 `let _ =` 显式忽略（Data 边不需要变更标记）：

```rust
for pin_id in data_pins_to_write {
    let _ = output_cache.write_now(
        pin_id,
        data_cache_value_for_pin(pin_id, &node_output.payload),
        trace_id,
    );
}
```

注意：返回值在 Task 4 的 Reactive dispatch 中使用。

- [ ] **Step 4: 搜索其他 write / write_now 调用点并适配**

Run: `rg '\.write_now\(' src/ crates/ -t rust -n` 和 `rg 'output_cache\.write\(' src/ crates/ -t rust -n`

所有现有调用点都是 Data 边写入（忽略返回值即可），加 `let _ =` 前缀。

- [ ] **Step 5: 编译 + 测试**

Run: `cargo test --workspace`
Expected: 全通过（返回值签名变了但所有调用点已适配）

- [ ] **Step 6: Commit**

```bash
git add crates/core/src/cache.rs src/graph/runner.rs
git commit -m "refactor(cache): OutputCache write/write_now 返回 bool 标记值是否变更"
```

---

### Task 3: classify_edges 三分支 + 环检测扩展

**Files:**
- Modify: `src/graph/topology.rs:192-252`

- [ ] **Step 1: ClassifiedEdges 加 reactive_edges 字段**

```rust
pub(crate) struct ClassifiedEdges<'a> {
    #[allow(dead_code)]
    pub exec_edges: Vec<&'a super::types::WorkflowEdge>,
    pub data_edges: Vec<&'a super::types::WorkflowEdge>,
    /// ADR-0015 Phase 1：Reactive 边——值变化时自动唤醒下游
    pub reactive_edges: Vec<&'a super::types::WorkflowEdge>,
}
```

- [ ] **Step 2: classify_edges 加 Reactive 分支**

当前 match（lines 242-245）只有 Exec / Data。加 Reactive：

```rust
match from_pin.kind {
    PinKind::Exec => exec_edges.push(edge),
    PinKind::Data => data_edges.push(edge),
    PinKind::Reactive => reactive_edges.push(edge),
}
```

返回值加 `reactive_edges`：

```rust
Ok(ClassifiedEdges {
    exec_edges,
    data_edges,
    reactive_edges,
})
```

初始化加 `let mut reactive_edges = Vec::new();`

- [ ] **Step 3: 环检测扩展——Data + Reactive 共享**

当前 `detect_data_edge_cycle`（lines 141-190）只检查 `data_edges`。改名 + 扩展：

```rust
/// ADR-0014 + ADR-0015：Data 边和 Reactive 边都需要环检测——
/// 两者都依赖缓存拉取语义，环会导致死锁。
pub(crate) fn detect_non_exec_edge_cycle(classified: &ClassifiedEdges<'_>) -> Result<(), crate::EngineError> {
    let combined: Vec<_> = classified.data_edges.iter().chain(classified.reactive_edges.iter()).collect();
    // ... 原有 Kahn 算法逻辑不变，只是输入从 &classified.data_edges 改为 &combined
}
```

- [ ] **Step 4: 更新 deploy.rs 调用点**

`src/graph/deploy.rs` line 169 调用 `detect_data_edge_cycle`。改为：

```rust
detect_non_exec_edge_cycle(&classified)?;
```

- [ ] **Step 5: 更新 topology.rs 内部测试函数名**

搜索 `detect_data_edge_cycle` 所有引用并改为 `detect_non_exec_edge_cycle`。

- [ ] **Step 6: 新增 classify_edges Reactive 单测**

在 `src/graph/topology.rs` 的 `mod tests` 中加：

```rust
#[test]
fn classify_edges_把_reactive_pin_出边归为_reactive() {
    let mut nodes: HashMap<String, Arc<dyn NodeTrait>> = HashMap::new();
    nodes.insert(
        "a".to_owned(),
        make_node(
            "a",
            vec![pin("in", PinDirection::Input, PinKind::Exec)],
            vec![pin("latest", PinDirection::Output, PinKind::Reactive)],
        ),
    );
    nodes.insert(
        "b".to_owned(),
        make_node(
            "b",
            vec![pin("in", PinDirection::Input, PinKind::Reactive)],
            vec![PinDefinition::default_output()],
        ),
    );

    let edges = vec![edge("a", "b", Some("latest"))];
    let classified = classify_edges(&edges, &nodes).unwrap();
    assert_eq!(classified.exec_edges.len(), 0);
    assert_eq!(classified.data_edges.len(), 0);
    assert_eq!(classified.reactive_edges.len(), 1);
    assert_eq!(classified.reactive_edges[0].from, "a");
}
```

- [ ] **Step 7: 编译 + 测试**

Run: `cargo test -p nazh-engine --lib -- topology`
Expected: 全通过（含新 Reactive 分类测试）

- [ ] **Step 8: Commit**

```bash
git add src/graph/topology.rs src/graph/deploy.rs
git commit -m "feat(graph): classify_edges 三分支 + 环检测覆盖 Data 和 Reactive"
```

---

### Task 4: Runner Reactive dispatch 逻辑

**Files:**
- Modify: `src/graph/runner.rs:28-37` (签名)
- Modify: `src/graph/runner.rs:112-152` (dispatch)
- Modify: `src/graph/deploy.rs:174-190` (部署期收集)
- Modify: `src/graph/deploy.rs:298-325` (传参)

- [ ] **Step 1: run_node 签名加 reactive_output_pin_ids**

在 `src/graph/runner.rs` line 37 后加参数：

```rust
    reactive_output_pin_ids: HashSet<String>,
```

- [ ] **Step 2: 部署期收集 reactive_output_pin_ids**

在 `src/graph/deploy.rs` line 177 后加：

```rust
    let mut reactive_output_pin_ids_by_node: HashMap<String, HashSet<String>> =
        HashMap::with_capacity(nodes_by_id.len());
```

在 line 181-189 的 for pin 循环中，同时收集 Reactive pin：

```rust
for (id, node) in &nodes_by_id {
    let cache = OutputCache::new();
    let mut data_pin_ids = HashSet::new();
    let mut reactive_pin_ids = HashSet::new();
    for pin in node.output_pins() {
        match pin.kind {
            PinKind::Data => {
                cache.prepare_slot(&pin.id);
                data_pin_ids.insert(pin.id.clone());
            }
            PinKind::Reactive => {
                cache.prepare_slot(&pin.id);
                reactive_pin_ids.insert(pin.id.clone());
            }
            PinKind::Exec => {}
        }
    }
    output_caches.insert(id.clone(), Arc::new(cache));
    data_output_pin_ids_by_node.insert(id.clone(), data_pin_ids);
    reactive_output_pin_ids_by_node.insert(id.clone(), reactive_pin_ids);
}
```

- [ ] **Step 3: 传参给 run_node**

在 `src/graph/deploy.rs` 的 spawn 调用（~line 308）中，取 reactive_output_pin_ids 并传入：

```rust
let reactive_output_pin_ids = reactive_output_pin_ids_by_node
    .get(node_id)
    .cloned()
    .unwrap_or_default();
```

run_node 调用加参数：

```rust
runtime.spawn(run_node(
    node,
    // ... existing args ...
    Arc::clone(&pure_memo),
    // ADR-0015 Phase 1：Reactive pin ids
    reactive_output_pin_ids,
));
```

- [ ] **Step 4: Runner dispatch 逻辑改为三分支**

核心改动在 `src/graph/runner.rs` lines 112-152。当前逻辑：
1. 写 Data 缓存（line 115-130）
2. Exec 路径过滤匹配 targets（line 132-152）

改为：

```rust
for node_output in output.outputs {
    // 写 Data + Reactive 缓存（不推 ContextRef——Reactive 的 push 在后面单独处理）
    let all_cache_pin_ids: HashSet<&String> = data_output_pin_ids
        .iter()
        .chain(reactive_output_pin_ids.iter())
        .collect();
    if !all_cache_pin_ids.is_empty() {
        let cache_pins_to_write: Vec<(&String, bool)> = match &node_output.dispatch {
            NodeDispatch::Broadcast => all_cache_pin_ids
                .iter()
                .map(|pin_id| (*pin_id, reactive_output_pin_ids.contains(*pin_id)))
                .collect(),
            NodeDispatch::Route(ports) => ports
                .iter()
                .filter(|p| all_cache_pin_ids.contains(p))
                .map(|p| (p, reactive_output_pin_ids.contains(p)))
                .collect(),
        };
        for (pin_id, _is_reactive) in &cache_pins_to_write {
            output_cache.write_now(
                pin_id,
                data_cache_value_for_pin(pin_id, &node_output.payload),
                trace_id,
            );
        }
    }

    // Exec + Reactive 路径：匹配非纯 Data 输出 pin 的下游 sender
    // Reactive pin 的下游 sender 也在此推送 ContextRef
    let matching_targets = match &node_output.dispatch {
        NodeDispatch::Broadcast => downstream_senders
            .iter()
            .filter(|target| {
                target
                    .source_port_id
                    .as_ref()
                    .is_none_or(|port| !data_output_pin_ids.contains(port))
            })
            .collect::<Vec<_>>(),
        NodeDispatch::Route(port_ids) => downstream_senders
            .iter()
            .filter(|target| {
                target.source_port_id.as_ref().is_some_and(|port_id| {
                    !data_output_pin_ids.contains(port_id)
                        && port_ids.iter().any(|candidate| candidate == port_id)
                })
            })
            .collect::<Vec<_>>(),
    };
    // ... 后续 metadata + store.write + send 逻辑不变 ...
```

关键变化：过滤条件从"排除 Data pin"改为"排除纯 Data pin"——Reactive pin 的下游 sender 不被排除，所以 Reactive 边的 ContextRef 会随 Exec 一起推送。

- [ ] **Step 5: 编译检查**

Run: `cargo check --workspace`
Expected: 编译通过

- [ ] **Step 6: Commit**

```bash
git add src/graph/runner.rs src/graph/deploy.rs
git commit -m "feat(graph): Runner 三分支 dispatch——Reactive 边写缓存 + 推 ContextRef"
```

---

### Task 5: pin_validator 兼容矩阵更新

**Files:**
- Modify: `src/graph/pin_validator.rs:120-167`

- [ ] **Step 1: ReservedPinId 校验扩展**

当前 line 153-167 检查 Data 输入 pin id 不得为 "in"。扩展为 Data + Reactive：

```rust
// ADR-0014 Phase 3b + ADR-0015：Data / Reactive 输入引脚 id 不得为 "in"
for (node_id, index) in &indexes {
    for (pin_id, pin) in &index.inputs {
        if matches!(pin.kind, PinKind::Data | PinKind::Reactive)
            && pin_id.as_str() == DEFAULT_INPUT_PIN_ID
        {
            return Err(EngineError::ReservedPinId {
                node: node_id.to_string(),
                pin: pin_id.clone(),
                reason: "Data / Reactive 输入 pin id 不得为 \"in\"——保留给混合输入 payload 合并的 Exec 主输入键".to_owned(),
            });
        }
    }
}
```

- [ ] **Step 2: 编译 + 测试**

Run: `cargo test --workspace`
Expected: 全通过。PinKind::is_compatible_with 已在 Task 1 更新，pin_validator 调用该函数即可。

- [ ] **Step 3: Commit**

```bash
git add src/graph/pin_validator.rs
git commit -m "fix(validator): Data + Reactive 输入 pin id 校验扩展"
```

---

### Task 6: Reactive 边集成测试

**Files:**
- Modify: `tests/workflow.rs`

测试策略：复用 `tests/workflow.rs` 中 `deploy_拒绝_pin_类型不兼容的边` 模式（自定义 NodeRegistry + TypedTestNode）。需要一个新的支持 Reactive pin 的测试节点类型。部署后通过 `deployment.submit()` 注入数据，验证 consumer 被触发并拉到 Reactive pin 的值。

- [ ] **Step 1: 新增支持 Reactive pin 的测试节点类型**

在 `tests/workflow.rs` 的 `TypedTestNode` 定义后加：

```rust
/// 测试用 Reactive 节点：output pin 声明为 Reactive kind。
struct ReactiveTestNode {
    id: String,
    input_pin: PinType,
    input_kind: PinKind,
    output_pin: PinType,
    output_kind: PinKind,
}

#[async_trait]
impl NodeTrait for ReactiveTestNode {
    fn id(&self) -> &str { &self.id }
    fn kind(&self) -> &'static str { "reactiveTest" }
    fn input_pins(&self) -> Vec<PinDefinition> {
        vec![PinDefinition {
            pin_type: self.input_pin.clone(),
            kind: self.input_kind,
            ..PinDefinition::default_input()
        }]
    }
    fn output_pins(&self) -> Vec<PinDefinition> {
        vec![PinDefinition {
            pin_type: self.output_pin.clone(),
            kind: self.output_kind,
            ..PinDefinition::default_output()
        }]
    }
    fn capabilities(&self) -> NodeCapabilities { NodeCapabilities::empty() }
    async fn transform(
        &self,
        _trace_id: Uuid,
        payload: serde_json::Value,
    ) -> Result<NodeExecution, EngineError> {
        // Producer：把 payload 包装为带 "out" key 的值（供 OutputCache 写入）
        // Consumer：直接 passthrough
        Ok(NodeExecution::broadcast(payload))
    }
}
```

- [ ] **Step 2: 写 Reactive 边端到端集成测试**

```rust
/// ADR-0015 Phase 1：Reactive 边——上游写入时自动唤醒下游。
///
/// DAG: src(exec) --exec--> producer(Reactive out) --reactive--> consumer(Reactive in)
/// src submit → producer transform → 写 Reactive 缓存 + 推 ContextRef → consumer 被唤醒
#[tokio::test]
async fn reactive_edge_pushes_downstream_on_write() {
    let mut registry = NodeRegistry::new();

    // Producer：输出 pin 为 Reactive kind
    registry.register_with_capabilities("reactiveProducer", NodeCapabilities::empty(), |def, _res| {
        Ok(Arc::new(ReactiveTestNode {
            id: def.id().to_owned(),
            input_pin: PinType::Any,
            input_kind: PinKind::Exec,
            output_pin: PinType::Any,
            output_kind: PinKind::Reactive,
        }) as Arc<dyn NodeTrait>)
    });

    // Consumer：输入 pin 为 Reactive kind
    registry.register_with_capabilities("reactiveConsumer", NodeCapabilities::empty(), |def, _res| {
        Ok(Arc::new(ReactiveTestNode {
            id: def.id().to_owned(),
            input_pin: PinType::Any,
            input_kind: PinKind::Reactive,
            output_pin: PinType::Any,
            output_kind: PinKind::Exec,
        }) as Arc<dyn NodeTrait>)
    });

    // Src：默认 Exec pin
    registry.register_with_capabilities("execSrc", NodeCapabilities::empty(), |def, _res| {
        Ok(Arc::new(TypedTestNode {
            id: def.id().to_owned(),
            input_pin: PinType::Any,
            output_pin: PinType::Any,
        }) as Arc<dyn NodeTrait>)
    });

    let graph = match WorkflowGraph::from_json(
        &json!({
            "nodes": {
                "src": { "type": "execSrc", "config": {} },
                "producer": { "type": "reactiveProducer", "config": {} },
                "consumer": { "type": "reactiveConsumer", "config": {} },
            },
            "edges": [
                { "from": "src", "to": "producer" },
                { "from": "producer", "to": "consumer" },
            ]
        })
        .to_string(),
    ) {
        Ok(g) => g,
        Err(e) => panic!("graph 应可解析: {e}"),
    };

    let mut deployment = match deploy_workflow(
        graph,
        shared_connection_manager(),
        &registry,
    ).await {
        Ok(d) => d,
        Err(e) => panic!("Reactive DAG 应部署成功: {e}"),
    };

    // 提交数据 → src → producer(Reactive out) → consumer 被 push
    let submit_result = deployment
        .submit(WorkflowContext::new(json!({ "value": 42 })))
        .await;
    assert!(submit_result.is_ok(), "submit 应成功");

    // consumer 是叶节点，结果应通过 next_result 到达
    let result = timeout(Duration::from_secs(2), deployment.next_result()).await;
    match result {
        Ok(Some(ctx_ref)) => {
            // consumer 被触发，说明 Reactive 边的 ContextRef 推送生效
            assert_eq!(ctx_ref.source_node_id.as_deref(), Some("consumer"));
        }
        Ok(None) => panic!("consumer 应有输出（Reactive 边应推送 ContextRef）"),
        Err(_) => panic!("超时：consumer 未被 Reactive 边唤醒（2s）"),
    }
}
```

注意：如果 `ReactiveTestNode` 的 `input_kind: PinKind::Reactive` 导致 `PinDefinition::default_input()` 的 `"in"` id 被_RESERVED 校验拒绝，需将 Reactive 输入 pin id 改为非 `"in"` 的值（如 `"reactive_in"`），并在 DAG edges 中用 `target_port_id` 指定。同 `PinDefinition::default_input()` 的约定。

- [ ] **Step 3: 运行测试验证**

Run: `cargo test -p nazh-engine --test workflow reactive_edge_pushes_downstream_on_write`
Expected: PASS

若因 pin id "in" + Reactive kind 被拒，改 pin id 为 `"reactive_in"` + edges 加 `target_port_id: "reactive_in"`。

- [ ] **Step 4: 写 Reactive 兼容矩阵部署校验测试**

```rust
/// ADR-0015：Exec 输出不可连 Reactive 输入
#[tokio::test]
async fn deploy_拒绝_exec_输出连_reactive_输入() {
    let mut registry = NodeRegistry::new();
    registry.register_with_capabilities("execOnly", NodeCapabilities::empty(), |def, _res| {
        Ok(Arc::new(ReactiveTestNode {
            id: def.id().to_owned(),
            input_pin: PinType::Any,
            input_kind: PinKind::Exec,
            output_pin: PinType::Any,
            output_kind: PinKind::Exec,
        }) as Arc<dyn NodeTrait>)
    });
    registry.register_with_capabilities("reactiveSink", NodeCapabilities::empty(), |def, _res| {
        Ok(Arc::new(ReactiveTestNode {
            id: def.id().to_owned(),
            input_pin: PinType::Any,
            input_kind: PinKind::Reactive,
            output_pin: PinType::Any,
            output_kind: PinKind::Exec,
        }) as Arc<dyn NodeTrait>)
    });

    let graph = match WorkflowGraph::from_json(
        &json!({
            "nodes": {
                "src": { "type": "execOnly", "config": {} },
                "sink": { "type": "reactiveSink", "config": {} },
            },
            "edges": [{ "from": "src", "to": "sink" }],
        })
        .to_string(),
    ) {
        Ok(g) => g,
        Err(e) => panic!("graph 应可解析: {e}"),
    };

    let result = deploy_workflow(graph, shared_connection_manager(), &registry).await;
    match result {
        Ok(_) => panic!("Exec→Reactive 边应被 pin_validator 拒绝"),
        Err(EngineError::IncompatiblePinKinds { from, to, from_kind, to_kind }) => {
            assert_eq!(from, "src.out");
            assert_eq!(to, "sink.in");
            assert_eq!(from_kind, PinKind::Exec);
            assert_eq!(to_kind, PinKind::Reactive);
        }
        Err(e) => panic!("应报 IncompatiblePinKinds，实际: {e}"),
    }
}
```

注意：此测试同样可能因 Reactive 输入 pin id "in" 被 ReservedPinId 校验先拦截。若是，改 pin id 为 `"reactive_in"` + edge 加 `target_port_id: "reactive_in"`。

- [ ] **Step 5: 全量测试回归**

Run: `cargo test --workspace`
Expected: 全通过

- [ ] **Step 6: Commit**

```bash
git add tests/workflow.rs
git commit -m "test(workflow): ADR-0015 Phase 1 Reactive 边集成测试 + 兼容矩阵校验"
```

---

### Task 7: ts-rs 导出 + 前端类型同步

**Files:**
- Modify: `crates/core/src/pin.rs` (PinKind ts-rs)
- Generated: `web/src/generated/` (ts-rs output)

- [ ] **Step 1: 确认 PinKind 已有 ts-rs derive**

检查 `crates/core/src/pin.rs` PinKind 枚举上方已有 `#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]`。若有则无需改动——Reactive variant 自动导出。

- [ ] **Step 2: 重新生成 TypeScript 类型**

Run: `cargo test -p tauri-bindings --features ts-export export_bindings`
Expected: `web/src/generated/` 中的 PinKind 类型更新，包含 `"reactive"` variant。

- [ ] **Step 3: 检查生成 diff**

Run: `git diff web/src/generated/`
确认 `PinKind` 包含 `Reactive` variant。

- [ ] **Step 4: Commit**

```bash
git add web/src/generated/
git commit -m "feat(bindings): PinKind::Reactive ts-rs 导出"
```

---

### Task 8: clippy + fmt + 最终验证

**Files:**
- None (verification only)

- [ ] **Step 1: cargo fmt**

Run: `cargo fmt --all -- --check`
Expected: 无输出（已格式化）。若有差异：`cargo fmt --all`

- [ ] **Step 2: cargo clippy**

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: 0 warnings

- [ ] **Step 3: 全量测试**

Run: `cargo test --workspace`
Expected: 全通过

- [ ] **Step 4: Commit（如有格式修复）**

```bash
git add -A
git commit -m "chore: fmt + clippy 修复"
```

---

### Task 9: 文档同步 + plan 状态更新

**Files:**
- Modify: `docs/plans/2026-04-30-adr-0015-phase1-reactive-edge.md`
- Modify: `docs/plans/2026-04-28-architecture-review.md`
- Modify: `AGENTS.md`

- [ ] **Step 1: 本 plan prepend Status**

本 plan 顶部加：`> **Status:** merged in <SHA>`

- [ ] **Step 2: architecture review plan Phase A checkbox 更新**

`docs/plans/2026-04-28-architecture-review.md` ADR-0015 条目：

```markdown
### ADR-0015 反应式数据引脚

- [x] 新建 plan：`docs/plans/2026-04-30-adr-0015-phase1-reactive-edge.md`
- [x] Phase 1 按 plan 实施（Reactive 边核心）
- [ ] Phase 2 实施（变量 Reactive + IPC）
- [ ] Phase 3 实施（前端 UI）
```

- [ ] **Step 3: AGENTS.md 状态同步**

更新 ADR-0015 状态行（如需）和 ADR Execution Order。

- [ ] **Step 4: Commit**

```bash
git add docs/ AGENTS.md
git commit -m "docs: ADR-0015 Phase 1 plan 状态同步 + AGENTS.md 更新"
```
