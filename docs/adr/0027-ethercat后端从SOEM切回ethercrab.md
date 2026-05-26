# ADR-0027: EtherCAT 后端从 SOEM 切回 ethercrab

- **状态**: 提议中
- **日期**: 2026-05-26
- **决策者**: ssXue
- **关联**: ADR-0023（EtherCAT TX/RX 恢复策略，暂缓）、commit `48287d1`（ethercrab→SOEM 迁移，无 ADR）

## 背景

2026-05-23，EtherCAT 后端从 ethercrab 0.7（纯 Rust）一次性迁移到 SOEM（C 库 + soem-sys FFI）。迁移未记录 ADR，commit message 列出了变更清单但未说明决策原因。

从旧 ethercrab 后端代码（`ethercrab_backend.rs`，546 行）可以推断迁移动机：

1. **PduStorage 进程级单例**：`PDU_STORAGE.try_split()` 内部用 `AtomicBool` 一次性开关，不可复位。首次部署后 `MainDevice` 绑死在该 interface 上，换网卡必须重启进程。SOEM 的 `ecx_context` 可以 close 再 init，无此限制。
2. **AL 状态诊断缺失**：旧 ethercrab 后端的 `get_slave_states()` 返回硬编码 `al_status: 0x08`、`"运行"`，无法读取从站真实 AL status code。SOEM 通过 `ec_slave.ALstatuscode` 直接读寄存器，配套 35 条翻译表。
3. **DC 手动配置**：ethercrab 要求显式传入 `DcConfiguration { start_delay, sync0_period, sync0_shift }`；SOEM 的 `ecx_configdc` 从从站 ESI 自动发现并配置 DC。

迁移后，soem-sys 引入了一系列构建问题：

- macOS OSAL 需要本地补丁（`clock_nanosleep` → `nanosleep`），补丁每次 `fs::copy` 覆盖 submodule 文件导致 mtime 变更
- `generate_ec_options()` 每次编译都 `fs::write` 更新 `OUT_DIR` 中的生成文件 mtime
- `rerun-if-changed` 声明了不存在的文件路径（`include/ethercattype.h`）
- 三者叠加导致 Cargo fingerprint 永远判定 dirty → build.rs 无限重编译（issue: 568↔569 循环）
- 虽已修复（比对内容后跳过写入、修正 rerun-if-changed 路径），但 C FFI 的本质决定了类似问题随时可能复现（上游 SOEM 变更、平台 OSAL 差异、bindgen 版本兼容、pcap SDK 路径等）

## 决策

> 我们决定将 EtherCAT 后端从 SOEM 切回 ethercrab，因为 soem-sys 的 C FFI 构建复杂度已构成工业边缘软件的交付风险，而切回的损失项在 Nazh 桌面应用场景下可接受或可缓解。

## 可选方案

### 方案 A: 切回 ethercrab（推荐）

- 优势：
  - 彻底消除 C FFI 构建问题：无 submodule、无 cc/bindgen、无 macOS OSAL 补丁、无 pcap 依赖
  - 纯 Rust，与 `unsafe_code = "forbid"` 一致。当前 `nodes-io` 禁止 unsafe，但 soem-sys 是 unsafe 的，切回后全链路 safe Rust
  - CI/CD 简化：无 Linux raw socket 权限 / macOS pcap / Windows WinPcap 环境问题
  - `cargo check` / `cargo build` 稳定，无 fingerprint 循环风险
  - ethercrab 0.7 已改进 AL status code 读取和 DC 32 位时钟兼容性
- 劣势：
  - PduStorage 不可重置：首次部署后绑定 interface，换网卡需重启进程
  - DC 配置变回手动：需显式传入 period / shift / delay
  - 周期循环从 OS 线程降级为 tokio task：亚毫秒确定性时序丢失
  - AL status code 翻译表需从 SOEM 后端搬入
  - SM（Sync Manager）配置读回诊断丢失

### 方案 B: 保留 SOEM，持续修构建问题

- 优势：
  - 保持现有诊断能力（AL 状态翻译表、SM 配置日志、working counter 精确比对）
  - DC 自动发现（`ecx_configdc`）
  - OS 线程周期循环提供微秒级确定性
  - `ecx_context` 可 close 再 init，无进程级绑定限制
- 劣势：
  - C FFI 构建问题本质不可消除，只能逐个修（今天修了 568↔569，明天可能出新问题）
  - macOS OSAL 不是上游主线，需持续维护本地补丁
  - bindgen 版本升级可能引入 API 变更
  - vendor/soem-sys/soem-src submodule 增加 CI 复杂度（checkout depth、平台差异）
  - 违反 `unsafe_code = "forbid"` 精神——soem-sys 整个 crate 是 unsafe

### 方案 C: 双后端共存（soem + ethercrab）

- 优势：
  - 用户可按场景选择后端（开发/调试用 ethercrab，现场部署用 SOEM）
  - 渐进迁移，不需要一次性切换
- 劣势：
  - 维护两套后端的测试和维护成本翻倍
  - DSL 枚举、前端表单、连接校验都需要支持两个真实后端
  - 不消除 SOEM 的构建问题，反而增加 CI 矩阵
  - 违反项目"一条主路径"原则

## 后果

### 正面影响

- 构建稳定性根本性提升：纯 Rust 依赖链，`cargo check` 结果可复现
- 开发体验改善：无 submodule、无 C 编译、无平台特殊处理
- `unsafe_code = "forbid"` 全链路生效，安全审计覆盖完整
- 依赖树减小：移除 `cc`、`bindgen`、`pcap` 等构建依赖
- CI 时间缩短：无 SOEM C 源码编译步骤

### 负面影响

- PduStorage 进程级单例限制仍然存在：同一进程生命周期内只能绑定一个 EtherCAT 网卡。在 Nazh 桌面应用的实际场景中（一个 EtherCAT 网段 → 一个 interface → 进程级绑定），此限制可接受
- DC 配置无法自动发现：需用户在连接 DSL 中显式填写 `dc_sync0_period_us` / `dc_sync0_shift_us` / `dc_start_delay_us`。当前 DSL 已将这些字段定义为必填，不影响
- AL status code 诊断需自建翻译表：从 SOEM 后端搬入 35 条翻译表到 ethercrab 后端
- 无 SM 配置读回诊断：进入 OP 失败时只能依赖 AL status code，无法打印 SM start/length/flags/type
- 周期循环由 OS 线程降为 tokio task：Nazh 是事件驱动架构（节点触发 → 读写 PDO），不是 PLC 式周期驱动，tokio task 精度足够

### 风险

| 风险 | 可能性 | 影响 | 缓解 |
|------|--------|------|------|
| ethercrab 在特定从站上进入 OP 失败 | 中 | 高 | 保留 mock 后端作为 fallback；现场调试时可换回 SOEM（方案 C 作为应急路径） |
| PduStorage 单例导致用户困惑（换网卡报错） | 低 | 中 | 错误信息已引导"请重启 nazh-desktop"；可在前端网卡选择界面提示 |
| ethercrab 0.7 的 AL status code 读取不完整 | 低 | 中 | 实测验证；必要时提 upstream issue 或本地 patch |
| 亚毫秒周期场景 tokio task 精度不足 | 低 | 低 | Nazh 不面向 PLC 场景；若未来需要可引入 dedicated thread 方案 |
| ethercrab 上游维护停滞 | 低 | 高 | ethercrab 0.7 已发布，社区活跃（Matrix、GitHub Sponsors）；最坏情况 fork 维护 |

## 备注

### 实施范围

1. **恢复 `ethercrab_backend.rs`**：基于 `48287d1^` 的旧版本，加入 SOEM 迁移期间积累的改进（AL status code 翻译表、SM 诊断日志、working counter 校验）
2. **删除 `vendor/soem-sys/`**：整个目录移除，包括 submodule、patch、build.rs
3. **Cargo.toml**：`soem-sys` 替换为 `ethercrab = { version = "0.7", default-features = false, features = ["std", "log"] }`
4. **DSL / 连接校验**：`EthercatBackend` 枚举从 `Soem` 改回 `Ethercrab`；validation 白名单同步更新
5. **前端**：连接表单 backend 选项从 `soem` 改为 `ethercrab`
6. **EthercatConfig**：DC 参数从可选改回必填（`dc_sync0_period_us`、`dc_sync0_shift_us`、`dc_start_delay_us`）
7. **Feature gate**：`io-ethercat` feature 依赖从 `soem-sys` 改为 `ethercrab`

### 保留的 SOEM 时期改进

以下改进应搬入 ethercrab 后端，不随 SOEM 删除而丢失：

- AL status code 翻译表（35 条，`al_status_to_text`）
- `format_diagnostic` 从站诊断格式化（address、state、ALstatus、PDO 大小）
- `resolve_slave_index` 从站选择器兼容逻辑（精确地址匹配 → 位置编号 fallback）
- write_outputs 写后立即刷帧的语义（"写即刷帧"）
- `format_cycle_duration` 周期格式化工具
- 后台周期循环的错误计数 + 限速日志（首条 + 每 100 条）
- `safe_shutdown` OP → SAFE-OP 安全关闭流程

### ADR-0023 关联

ADR-0023（EtherCAT TX/RX 软恢复策略，暂缓）在 SOEM 后端下讨论的是 OS 线程异常后的进程级恢复。切回 ethercrab 后，TX/RX 由 tokio task 驱动，恢复策略变为 `JoinHandle::abort` + 重新 spawn，与 ADR-0023 的上下文不同。若 ADR-0023 未来重启，需在 ethercrab 后端下重新评估。

### 历史记录

- 2026-05-06：首次接入 ethercrab 0.7（`1800e3b`）
- 2026-05-14~22：DC SYNC0 支持、安全关闭、网卡扫描、微秒周期
- 2026-05-23：切到 SOEM（`48287d1`），无 ADR
- 2026-05-23~25：SOEM 构建修复（submodule、macOS OSAL 补丁、CI、fingerprint 循环）
- 2026-05-26：本 ADR，提议切回 ethercrab
