# ADR-0030: EthercatPdoWrite 执行器接入

- **状态**: 已实施
- **日期**: 2026-06-02
- **决策者**: ssXue
- **关联**: ADR-0028（CapabilityImpl 编码元数据扩展）、ADR-0024（设备信号读取与事件触发节点）

## 背景

ADR-0028 新增了 `CapabilityImpl::EthercatPdoWrite` 变体，`build_capability_from_signal` 已能为 EtherCAT PDO 写信号自动生成能力。但 `capabilityCall` 节点的 `EthercatPdoWrite` 分支当前 fail-fast（"尚未接入 ethercrab 执行器"）。

现有的 `ethercatPdoWrite` 节点（`crates/nodes-io/src/ethercat/pdo_write.rs`）只能写入整块输出 PDO 数据（`data: [0x01, 0x02, ...]`），不支持按单个 PDO entry 精确编码和定位。

### 问题

1. **`EthercatBus` trait 缺少 `read_outputs`**：read-modify-write 模式需要先读取当前输出 PDO，再修改指定 entry 的位，再写回
2. **`SignalSource::EthercatPdo` 没有 `byte_offset`**：ESI 导入时没有计算 PDO entry 在 I/O 缓冲区的物理偏移
3. **执行器未实现**：`CapabilityImplSnapshot::EthercatPdoWrite` 分支返回错误

## 决策

> 我们决定通过三层改动完成接入：ESI 导入时计算 byte_offset、EthercatBus trait 补 read_outputs、capabilityCall 执行器按 byte_offset + bit_len 做 read-modify-write。

### 具体变更

#### 1. ESI 导入时累加计算 byte_offset

`build_signals`（`src-tauri/src/ethercat_esi/utils.rs`）中，同一 PDO 内的 entry 按 ESI 顺序排列。entry 的 byte_offset 通过累加前序 entry 的 `bit_len` 计算（向上取整到字节边界）：

```
byte_offset = sum(ceil(prev_entry.bit_len / 8)) for prev entries in same PDO
```

`SignalSource::EthercatPdo` 新增 `byte_offset: u16` 字段。

#### 2. `EthercatBus` trait 补 `read_outputs`

```rust
async fn read_outputs(&self, slave_address: u16) -> Result<Vec<u8>, EthercatError>;
```

ethercrab 后端和 mock 后端同步实现。

#### 3. `CapabilityImpl` / `CapabilityImplSnapshot` 补 byte_offset

`EthercatPdoWrite` 变体新增 `byte_offset: u16` 字段（`#[serde(default)]`）。`build_capability_from_signal` 从 `SignalSource::EthercatPdo.byte_offset` 传入。

#### 4. capabilityCall 执行器

```
1. 解析 value_template → 得到用户输入值
2. 按 data_type / byte_order / bit_len 编码为字节
3. read_outputs(slave_address) → 读取当前输出 PDO
4. 在 byte_offset 处按 bit_len 覆写编码字节
5. write_outputs(slave_address, modified_data) → 写回
```

## 可选方案

### 方案 A：read-modify-write + byte_offset（本提案）

- **优势**：精确按 PDO entry 编码和定位；与 `data_type`/`byte_order` 编码逻辑统一；ESI 导入时计算 byte_offset 是静态信息，不需要运行时依赖
- **劣势**：涉及面广（dsl-core + ESI import + EthercatBus trait + 两个后端 + executor + 测试）

### 方案 B：复用 ethercatPdoWrite 整块写入

- **优势**：不修改 EthercatBus trait；capabilityCall 构建完整字节块后调用 `write_outputs`
- **劣势**：要求 capabilityCall 知道完整输出 PDO 长度并构建匹配长度的字节块；多个 PDO entry 同时写入时需要协调；与"按 entry 精确编码"的设计意图矛盾

### 方案 C：给 EthercatBus 加 write_pdo_entry 精确方法

- **优势**：后端内部处理偏移计算，调用方更简洁
- **劣势**：后端需要维护 PDO entry 的偏移映射表；与 ethercrab 0.7 的 flat buffer 模型不完全匹配

## 后果

### 正面影响

- `capabilityCall::EthercatPdoWrite` 从 fail-fast 变为可执行——EtherCAT 设备的写能力闭环
- ESI 导入的信号携带 `byte_offset`，提高 DSL 信息密度
- `read_outputs` 为未来的 PDO 监控/诊断提供基础

### 负面影响

- `SignalSource::EthercatPdo` 新增 `byte_offset` 字段——现有 EtherCAT 设备 YAML 需要重新导入（旧 YAML `#[serde(default)]` 兼容，byte_offset 默认 0）
- read-modify-write 在高并发写入同一从站时存在竞态（多次 read → modify → write 可能互相覆盖）

### 风险

- **byte_offset 计算正确性**：ethercrab 的 PDO 映射顺序与 ESI entry 顺序一致，但 edge case（混合位域/字节域 entry）可能导致偏移不匹配。缓解：集成测试使用 mock 后端验证已知 PDO 映射的正确性
- **并发写入竞态**：多个 `capabilityCall` 同时写入同一从站的不同 entry 时，read-modify-write 可能丢失更新。缓解：同一从站的写入通过 `Mutex` 串行化（`SharedEthercatSession::bus` 已返回 `MutexGuard`）

## 备注

- 与 ADR-0028 配合，完成 EtherCAT 设备写能力的完整闭环
- `ethercatPdoWrite` 节点保持不变（整块写入模式），与 capabilityCall 的 read-modify-write 模式互补
