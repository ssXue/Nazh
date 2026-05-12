# ADR-0023: EtherCAT TX/RX 任务终止后的恢复策略（评估备案）

- **状态**: 已部分实施（方案 B 已落地，方案 C/D 保留预研）
- **日期**: 2026-05-06（方案 B 实施于 2026-05-13）
- **决策者**: Niu Zhihong
- **关联**: 提交 1800e3b（接入 EtherCAT 三件套节点 + 修复 ethercrab 后端关键 Bug）、`crates/nodes-io/src/ethercat/backends/ethercrab_backend.rs`、ethercrab 0.7.1（`std::tx_rx_task` / `PduStorage::try_split`）

## 背景

2026-05-06 在真实使用中复现：用户配置 EtherCAT 工作流，部署后撞上以下错误：

```text
节点 `ecat_pdo_write_1` 配置无效: EtherCAT 主站初始化失败:
  EtherCAT TX/RX 任务已终止（接口 `en8`）；请重启 nazh-desktop
  后重试，或检查网卡是否被拔出/链路中断
```

这条提示是 1800e3b 主动加的诊断守卫（`ensure_maindevice` 检测 `tx_handle.is_finished()`）。**报错本身是正确行为**——它没有让用户陷入"`init_single_group` 一律 timeout: PDU"的迷雾，而是直接告诉用户："上一次 TX/RX 已死，进程恢复不了，请重启"。但它暴露的是一个**架构级硬约束**——一旦 ethercrab 的 TX/RX 后台任务因 socket 错误退出，当前进程内无法软恢复。

### ethercrab 0.7.1 的两个硬约束

经源码确认（`/Users/ssxue/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/ethercrab-0.7.1/src/`）：

1. **`PduStorage::try_split()` 一次性消费**（`pdu_loop/storage.rs:141`）

   ```rust
   self.is_split.compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
       .map_err(|_| ())?;
   ```

   `is_split: AtomicBool` 一旦置 true 不可复位。`PduStorage` 是 `static` 单例，因此**整个进程生命周期内只能 split 一次**——拿到一组 `(PduTx, PduRx, PduLoop)` 之后，没有 API 能再要一组。

2. **`TxRxFut` 在错误分支不归还 tx/rx**（`std/unix/mod.rs:22-115`）

   ```rust
   struct TxRxFut<'a> {
       socket: Async<RawSocketDesc>,
       mtu: usize,
       tx: Option<PduTx<'a>>,
       rx: Option<PduRx<'a>>,
   }
   impl Future for TxRxFut<'_> {
       type Output = Result<(PduTx, PduRx), Error>;
       // should_exit() → Ok((tx.take().release(), rx.take().release()))
       // send/receive 错误 → Poll::Ready(Err(e))，tx/rx 留在 self 内随 future drop
   }
   ```

   只有 `should_exit()` 路径（`PduTx::mark_shutdown()` 主动触发）会归还 `(tx, rx)`；socket 致命错误（`SendFrame` / `ReceiveFrame` / `PartialSend`）走 `Poll::Ready(Err(_))`，`tx`/`rx` 仍是 `Some`，随 future struct drop 一起销毁。**调用方拿不到 tx/rx 重新启动新一轮 `tx_rx_task`**。

两条叠加 ⇒ 一旦 TX/RX 任务因网卡链路或 socket 错误死亡：
- 旧 tx/rx 已 drop，无法复用；
- `try_split` 用过一次，无法再发一组；
- 当前进程内**没有任何代码路径**能让 EtherCAT 主站重新上线。

### 触发场景观察

`en8` 是 macOS 上的 USB-Ethernet / 虚拟网卡（`en0`/`en1` 通常是内置）。常见触发：

- USB-Ethernet 适配器拔插
- macOS 睡眠唤醒（BPF 子设备失效）
- 网线短暂断开导致一次 `PartialSend`（`std/unix/mod.rs:54-71`）
- 链路抖动让 raw socket 写入返回的字节数小于请求长度

只要 `tracing::error!(?error, "EtherCAT TX/RX 任务异常终止")` 命中过一次，PDU_STATE 缓存就被污染，本进程后续所有 EtherCAT 部署都会被守卫拒绝。

## 决策

> **暂不在 Nazh 侧实施进程内软恢复机制**（方案 C/D 保留预研）。1800e3b 加的诊断守卫保留——它已经做对了"不让用户陷入迷雾"的关键事。
>
> 通过本 ADR 明确：
> 1. 这是 ethercrab 0.7 API 约束，不是 Nazh 侧 bug；
> 2. 设立**重访触发条件**——满足任一即重新评估方案 C/D；
> 3. 三种可行的根治方案预研归档，等到有依据时再选。

## 实施记录

### 2026-05-13：方案 B 已实施

- **新增 IPC 命令** `restart_app`（`src-tauri/src/commands/system.rs`）：Tauri v2 `AppHandle::restart()` 封装，进程级重启。
- **前端对话框** `EthercatRestartDialog`（`web/src/components/app/EthercatRestartDialog.tsx`）：参考 `RestoreDeploymentDialog` 样式，标题 + 错误详情 + "取消"/"重启应用" 双按钮。
- **错误检测**（`web/src/hooks/use-deployment-restore.ts`）：`runDeploymentSnapshot` catch 块检测 `message.includes('EtherCAT TX/RX 任务已终止') || message.includes('请重启 nazh-desktop')`，命中时通过 `onEthercatFatalError` 回调触发对话框。
- **状态集成**（`web/src/App.tsx`）：`useState<string | null>` 管理对话框显隐，`useDeploymentRestore` 传入回调，`renderEthercatRestartDialog()` 条件渲染于 `main` 根节点。

方案 B 将用户体验从"手动关闭并重新打开应用"提升到"点击确认按钮一键重启"，但底层仍是进程重启（非进程内软恢复）。

### 重访触发条件（满足任一即评估方案 C/D）

- [ ] **现场反复**：单一用户/部署在一周内 ≥ 5 次（方案 B 已将门槛从 3 次提升到 5 次）撞上重启流程，且用户明确抱怨"重启太慢/丢状态"
- [ ] **ethercrab 升级**：上游发布 ≥ 0.8 版本且修改了 `tx_rx_task` 的错误归还语义、或开放了 `PduStorage::reset` 类 API
- [ ] **第二个网卡相关需求**：例如运行时切换网卡、热插拔自动重连成为产品需求（不再只是"少数人睡眠唤醒"）
- [ ] **生产部署上线**：Nazh 进入工厂现场长期 7×24 运行场景，重启代价从"开发不便"升级为"产线停机"

### 重启路径（当前应对）

用户撞上此错误时：
1. 前端弹出确认对话框，点击"重启应用"一键重启 nazh-desktop（Tauri `AppHandle::restart()`）
2. 或手动关闭 nazh-desktop（dock 退出）后重新打开
3. 检查 `en8` 在 `ifconfig` 里仍存在并标记 `UP`；若是 USB 适配器，重新插拔一次以重置 BPF
4. 重启后重新部署工作流

无需清理 SQLite、无需重置工程文件——`PDU_STORAGE` 是进程级 `static`，进程退出即释放。

## 可选方案

### 方案 A: 接受现状 + 重访条件（基础层已落地）

诊断守卫已落地，方案 B 已实施。本方案保留作为兜底约束声明。

- 优势：
  - **不绑死技术路径**：不在 Nazh 侧投资上游 API 的 workaround，等 ethercrab 升级或现场数据自然推动决策
  - **尊重 ethercrab 的设计意图**：上游故意把 PDU 存储设计为进程级 `static`，避免 PDU 帧 ID/缓冲区生命周期混乱
- 劣势：
  - **依赖现场触发**：触发条件主观，缺乏 CI 自动提醒
- 风险：
  - **风险 1**：触发条件被遗忘。缓解：`docs/project-status.md` 的"已知约束"栏列入本 ADR

### 方案 B: 在 nazh-desktop 加"一键重启"按钮（**已实施，2026-05-13**）

- 优势：
  - **最小代价 UX 改善**：Tauri `AppHandle::restart()`，实现成本 < 1 天 ✅ 已验证
  - **不动 ethercrab**：纯壳层补丁 ✅ 未动 ethercrab 一行代码
  - **对其他场景也有用**：未来其他全局 `static` 资源撞同类问题时复用
- 劣势：
  - **仍是重启**：进程级资源释放，会话状态丢失（ADR-0022 变量持久化已缓解）
  - **遮掩问题**：让用户更容易接受"撞错就重启"，弱化推动上游修复的动力
- 风险：用户养成"撞错就点重启"习惯，掩盖真正的网卡硬件问题（如 USB 适配器损坏）
- **实施详情**：见 `## 实施记录`

### 方案 C: vendor 或 patch ethercrab，让 Err 分支归还 (tx, rx)

- 优势：
  - **真正软恢复**：监督任务感知 TX/RX 死亡 → 自动重新 spawn → 用户无感知
  - **工业级正确性**：工厂 7×24 不应因为一次帧错误就要求重启进程
- 劣势：
  - **fork 维护负担**：跟随上游升级要持续 rebase
  - **改动 PDU 安全模型**：`PduStorage` 单例假设"tx/rx 只存在一份"，要让 tx/rx 重新进入同一 storage 必须仔细审计 frame ID 生命周期；`PduStorage::try_split` 的 `AtomicBool` 也得加 reset 路径，可能引入悬挂引用
  - **测试代价高**：需要构造网卡断开/恢复的回归测试，CI 难以覆盖
- 风险：
  - **风险 1**：patch 引入安全 bug——并发 send/recv 与 reset 路径竞争导致 frame buffer 错乱，工业现场的损失远高于"重启即可"
  - **风险 2**：上游 0.8+ 改了相关 API，patch 失效

### 方案 D: 切换 EtherCAT 库（如 SOEM 绑定、`canopen-rs` 衍生方案）

- 优势：彻底脱离 ethercrab 0.7 约束
- 劣势：
  - 重做 EtherCAT 三件套节点 + ESI 导入 + 连接配置
  - SOEM 是 C 库，要 FFI + unsafe，违反根 `AGENTS.md` 的 `unsafe_code = "forbid"`（需要专门 vendor crate 隔离 unsafe）
  - 时间窗口不合理——EtherCAT 三件套刚于 2026-05-06 接入，还没有现场数据证明 ethercrab 不可接受
- 风险：替换库引入新约束，可能换汤不换药

## 后果

### 正面影响

- **诊断信号清晰**：1800e3b 的明确错误 + 本 ADR 的归档让贡献者/未来的自己看到这条错误时知道"不是 bug，是约束"
- **决策可追溯**：本 ADR 留下"为什么暂不动方案 C/D"的证据，未来重评时不必从零讨论
- **避免过度工程**：在没有现场数据前，方案 C/D 是"理论上更好"，本 ADR 拒绝在没数据时下手；方案 B 作为 UX 补丁先落地
- **用户体验已改善**：方案 B 实施后，从"手动关应用"提升到"一键重启"，操作路径明确

### 负面影响

- **仍是进程重启**：方案 B 没有解决"进程内软恢复"的本质问题，重启仍会丢失未持久化的运行时状态
- **工业现场长期运行场景下仍有缺口**：7×24 场景下即使一键重启也是中断，本 ADR 明确把这点列为"已知"
- **重启成本随产品阶段递增**：开发期可接受，预生产期需要观察，生产部署期可能成为 P0

### 风险

- **风险 1：触发条件被遗忘**——缓解：`docs/project-status.md` 已知约束栏 + `crates/nodes-io/AGENTS.md` EtherCAT 节点排查路径都引用本 ADR
- **风险 2：用户在不知约束的情况下报告"bug"**——缓解：错误信息已含操作指引 + 前端对话框明确说明原因；产品文档面向终端用户时也要复述
- **风险 3：方案 B 遮掩真正问题**——用户可能养成"撞错就点重启"习惯，掩盖网卡硬件故障。缓解：对话框保留"取消"按钮，用户仍可手动排查；日志中仍保留 `tracing::error!` 供诊断

## 备注

- 本 ADR 是 Nazh 第二条"评估性 ADR"——延续 ADR-0020 设立的范式：决议是"不动重工程"，但留下触发条件让未来的自己有客观依据。
- 文档同步：
  - `crates/nodes-io/AGENTS.md` —— EtherCAT 共享会话小节"TX/RX 死亡的现场排查路径"已更新一键重启操作
  - `docs/project-status.md` —— "评估性 ADR"小节已更新方案 B 实施状态
  - `docs/adr/README.md` —— 索引表状态已更新
- 方案 B 在本 ADR 内实施，无需新 ADR。若未来实施方案 C（patch ethercrab），按"重大架构改造"标准新开 ADR-00XX 引用本 ADR。
- 与 ADR-0009（生命周期钩子）的边界：EtherCAT 共享会话的 `lifecycle_guard` 已经在撤销部署时清理 backend 壳，但**故意不动进程级 `PDU_STATE` / `MainDevice` / TX/RX 任务**——这是为了让"同接口重新部署"复用进程级单例。本 ADR 接受这个权衡。
