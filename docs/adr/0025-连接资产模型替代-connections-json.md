# ADR-0025: 连接资产模型替代 `connections.json`

- **状态**: 已实施
- **日期**: 2026-05-17
- **决策者**: ssxue
- **关联**: RFC-0003, RFC-0004, ADR-0005, ADR-0021, ADR-0024

## 背景

Nazh 当前通过工作区根目录下的 `connections.json` 保存连接定义。该文件由前端连接管理 UI 整体读写，经 Tauri IPC 加载为 `Vec<ConnectionDefinition>`，再注册到 `ConnectionManager`。`ConnectionDefinition` 当前只有 `id` / `type` / `metadata` 三个字段，`metadata` 同时承载协议拓扑、治理参数、可能的密钥或本机现场差异。

随着 Device / Capability / Workflow 三段式 DSL 落地，设备资产已经以工程工作路径下的 YAML 文件作为唯一真值源；Device DSL 只引用 `ConnectionDefinition.connection_id`，不拥有连接参数。连接配置继续留在单个 JSON 文件中，会形成一条与 DSL 资产体系平行的配置路径，长期增加以下成本：

- `connections.json` 会逐渐混入工程拓扑、本机私密配置、环境覆盖、运行态健康信息等不同生命周期的数据。
- `metadata: serde_json::Value` 使协议配置缺乏强类型边界，前端表单、后端校验、节点读取都需要重复维护字符串键。
- 密钥类字段容易跟随工程文件、部署会话或执行 metadata 被导出、审计或暴露给 AI 上下文。
- Store RFC 中“连接配置从 `connections.json` 迁入 Store”的旧路线，与 DSL 资产“YAML 是可审查真值源”的路线存在心智冲突。

本应用尚未发布，不需要保留 `connections.json` 作为用户兼容格式。因此应优先降低长期架构成本，而不是围绕旧文件做渐进迁移。

## 决策

> 决定废弃 `connections.json`，将连接提升为一等工程资产：以 `dsl/connections/*.connection.yaml` 作为连接清单的唯一工程真值源；Store 只保存连接密钥、本机私有覆盖与运行态历史；`ConnectionManager` 只消费部署前解析完成的运行时连接定义。

目标结构：

```text
dsl/
  connections/
    plc-line-a.connection.yaml
    mqtt-edge.connection.yaml
  devices/
  capabilities/
```

连接模型分为三层：

1. **ConnectionSpec（工程资产）**：YAML 文件，保存可审查、可 diff 的连接拓扑和协议参数，例如连接 ID、协议类型、endpoint、治理策略、标签、说明、适用环境。不得保存明文密钥。
2. **Secret / LocalOverride（本机私有）**：SQLite Store 保存密钥、本机串口路径覆盖、网卡名覆盖、开发机与现场差异等不应进入工程资产的内容。工程资产只保存 `secret_ref` 或 override slot。
3. **RuntimeState（运行态）**：`ConnectionManager` 内存状态和 observability / Store 历史保存健康、借出、熔断、延迟、失败次数。运行态不得回写 ConnectionSpec。

`ConnectionDefinition { id, kind, metadata }` 保留为过渡期运行时 DTO，最终应收敛为更强类型的 `ResolvedConnectionDefinition`。工程层新增强类型模型：

```rust
pub struct ConnectionSpec {
    pub id: String,
    pub protocol: ConnectionProtocol,
    pub governance: ConnectionGovernanceSpec,
    pub secrets: ConnectionSecretRefs,
    pub labels: Vec<String>,
    pub description: Option<String>,
}

pub enum ConnectionProtocol {
    ModbusTcp { host: String, port: u16, unit_id: Option<u8> },
    Serial { port_path: String, baud_rate: u32, data_bits: u8, parity: SerialParity, stop_bits: u8 },
    Mqtt { host: String, port: u16, topic: String, client_id: Option<String> },
    Http { url: String, method: HttpMethod, headers: Vec<HeaderSpec> },
    Bark { server_url: String },
    CanSlcan { channel: String, baud_rate: u32, bitrate: u32 },
    Ethercat { backend: EthercatBackend, interface: String, cycle_time_ms: u64, op_timeout_ms: u64 },
}
```

部署链路改为由 Tauri 壳层统一解析：

```text
ConnectionSpec YAML
  + Project environment overlay
  + Store secret / local override
  -> ResolvedConnectionDefinition
  -> ConnectionManager
```

前端不再把完整连接数组作为 `deploy_workflow` 参数传入。部署请求只携带 workspace / project / environment 上下文，后端在同一信任边界内解析连接资产、注入密钥引用并注册运行时连接。

## 可选方案

### 方案 A: 保留 `connections.json`，补 schema 与校验

- 优势：改动最小；现有前端连接库、Tauri IPC、部署参数可继续使用；适合已发布产品的兼容演进。
- 劣势：继续保留单文件配置岔路；`metadata` 仍是无结构 JSON；密钥、本机覆盖与工程拓扑容易混杂；与 Device / Capability YAML 资产体系不一致。

### 方案 B: 将连接配置整体迁入 Store

- 优势：便于事务写入、权限收紧和本机私密数据管理；可复用 Store migration 与 async handle。
- 劣势：SQLite 不适合作为工程可审查资产真值源；连接拓扑无法自然进入版本管理和 code review；会让设备/能力资产走 YAML、连接资产走数据库，增加模型割裂。

### 方案 C: 新增连接 YAML 资产，Store 只存私密和运行态

- 优势：与 Device / Capability DSL 的资产路线一致；工程拓扑可审查、可 diff、可由 AI 安全读取；密钥和本机差异留在本机 Store；运行态状态不污染工程文件；为强类型协议配置铺路。
- 劣势：需要一次性改造前端连接管理 UI、Tauri IPC、部署恢复和测试；短期改动面最大；需要新增资产加载、校验、secret 解析和环境覆盖流程。

### 方案 D: 连接配置由 Device DSL 反向生成

- 优势：设备资产成为唯一入口，业务用户不需要单独维护连接库；设备与连接关系天然一致。
- 劣势：多个设备共享同一物理连接时会产生重复或冲突；连接治理、密钥、本机覆盖不是设备语义的一部分；会让 Device DSL 承担协议资源池职责，违背“设备语义高于协议适配但不吞并连接资源”的边界。

## 后果

### 正面影响

- 连接、设备、能力统一进入 `dsl/` 工程资产体系，减少长期心智模型分裂。
- 工程文件只包含可审查连接契约，密钥和本机差异不会进入 Git、导出包、AI 上下文或部署会话快照。
- 强类型 `ConnectionProtocol` 取代任意 `metadata` 后，前端表单、后端校验和节点读取可以共享同一类型契约。
- `ConnectionManager` 边界更清晰，只负责运行时治理、RAII 借出、健康状态和共享会话，不再隐含文件格式或工程语义。
- 部署链路由后端解析连接资产，减少前端传入完整运行时连接数组导致的状态漂移。

### 负面影响

- 需要删除或重写现有 `connections.json` IPC、前端 `useConnectionLibrary`、连接管理 UI 的持久化路径。
- 需要新增连接资产文件读写、版本或覆盖策略、Store secret/local override 表，以及 ts-rs 类型导出。
- 既有 `WorkflowGraph.connections` 字段会成为历史兼容字段，需明确废弃时机并避免继续依赖。
- E2E 与部署恢复测试需要重写，短期会增加验证成本。

### 风险

- **密钥解析边界不清**：如果 secret 引用与明文字段并存太久，仍可能发生泄露。缓解：ConnectionSpec schema 禁止明文敏感字段，保存和导入时 fail fast。
- **环境覆盖过度自由**：如果 overlay 仍是任意 JSON，会重新引入 `metadata` 问题。缓解：环境覆盖也使用强类型 patch，至少按协议分支校验。
- **部署恢复缺少快照语义**：如果恢复时重新读取最新连接资产，可能与上次部署不同。缓解：deployment-session 保存 resolved config 的脱敏摘要、资产版本或 hash；真正密钥仍按 secret ref 重新读取。
- **连接热切换语义复杂**：运行中替换连接涉及共享会话、draining 和硬件释放。缓解：本 ADR 不引入热切换；连接资产改动后需要重新部署，热切换另开 ADR。

## 备注

- 本 ADR 取代 RFC-0003 Phase 4 中“连接配置从 `connections.json` 迁入 Store”的原始表述。新的解释是：连接工程资产进入 YAML，Store 只保存密钥、本机覆盖与运行态历史。
- Device / Capability 资产继续保持 YAML 真值源，不引入 Store 索引回退。
- `connections.json` 不作为兼容格式保留；如果开发期存在旧文件，允许提供一次性导入工具，但导入后应删除旧文件或标记为废弃。
- 实施完成前，文档中提及 `connections.json` 的地方应标注为 legacy 或逐步改写为连接资产模型。
