//! 节点执行系统：统一的异步节点 Trait 与分发策略。
//!
//! 本模块定义了工作流节点的核心抽象 [`NodeTrait`]，以及节点输出的
//! 分发机制 [`NodeDispatch`]。具体节点实现分布在各 Ring 1 crate 中。
//!
//! ## 元数据与业务数据分离
//!
//! 节点通过 [`NodeOutput::metadata`] 返回执行元数据（协议参数、连接信息等），
//! 与业务 payload 在结构上完全分离。元数据通过 [`ExecutionEvent::Completed`]
//! 事件通道传递给前端，不进入 payload。
//!
//! ## 节点能力标签
//!
//! 节点类型的能力（[`NodeCapabilities`]）在注册时通过
//! [`NodeRegistry::register_with_capabilities`](crate::NodeRegistry::register_with_capabilities)
//! 声明；消费者（IPC / Runner / 可观测性）通过
//! [`NodeRegistry::capabilities_of`](crate::NodeRegistry::capabilities_of) 查询，
//! 无需实例化节点。详见 ADR-0011。

use async_trait::async_trait;
use bitflags::bitflags;
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::{EngineError, LifecycleGuard, NodeLifecycleContext, PinDefinition};

bitflags! {
    /// 节点类型的能力标签位图（ADR-0011）。
    ///
    /// # 这是什么
    ///
    /// 标签属于「类型级别」的契约——同类型的**所有实例、所有 config 组合**都必须满足该
    /// 标签对应的不变量。若某能力只在某些 config 下成立（如 `mqttClient` 仅在 subscribe
    /// 模式下才是 `TRIGGER`），**不要**在类型级别声明，保守空着。
    ///
    /// # 语义治理（为什么要严格）
    ///
    /// bitflags 的值很便宜、位的含义很贵。一旦 `PURE` 被错误地打在有副作用的节点上，
    /// 未来的 Runner 缓存层就会静默吐脏数据。因此本枚举有三道防线：
    ///
    /// 1. **位分配**：由 `node_capabilities_位分配与_adr_0011_一致` 单测锁死；任何
    ///    位值改动会立刻 CI 报错，反向保护 IPC 契约（位图会直接发给前端）。
    /// 2. **语义契约**：每个常量的 doc 注释写明「契约 / 反例 / 消费者」三段。PR review
    ///    的职责是对着这段检查作者有没有撒谎——特别是 `PURE` 和 `BLOCKING`。
    /// 3. **内置节点映射**：`src/registry.rs::标准注册表节点能力标签与_adr_0011_契约一致`
    ///    把 ADR 表格的 14 个条目固化为测试，任何节点的能力变化都要同步改 ADR + 测试。
    ///
    /// # 新增或修改能力位的 checklist
    ///
    /// 走 ADR 流程（新 ADR 或修订 ADR-0011），**同一 PR 必须**同步：
    /// 1. 本枚举位值
    /// 2. `crates/core/src/node.rs` 的位分配单测
    /// 3. `web/src/lib/node-capabilities.ts` 前端常量表（位值、名字、label、颜色）
    /// 4. `src/registry.rs` 的契约测试（若影响已有节点）
    /// 5. ADR-0011 的实施记录表格
    ///
    /// # 消费者
    ///
    /// - **Runner**（ADR-0011 第二、三阶段待做）：按 [`BLOCKING`](Self::BLOCKING)
    ///   自动 `spawn_blocking`；按 [`PURE`](Self::PURE) 启用 input-hash 缓存。
    ///   通过 `registry.capabilities_of(node.kind())` 查询，不在 `NodeTrait` 上读。
    /// - **Tauri IPC**：[`bits()`](Self::bits) 以 `u32` 发前端，前端常量表解位。
    /// - **可观测性**：按 IO 类标签分桶统计 p99 / 失败率。
    ///
    /// # 为什么 `NodeTrait` 没有 `capabilities()` 方法
    ///
    /// 能力只在**注册表**登记，不在 `NodeTrait` 上。首次实施时两处都加过，review
    /// 发现 trait 方法零消费者、11 个 override 全是类型级值的复读，遂移除。
    /// 未来若需要**实例级精化**（如 mqttClient 按 mode 返回不同 bits），请新增
    /// `fn instance_capabilities(&self, type_caps: NodeCapabilities) -> NodeCapabilities`
    /// 而非恢复旧方法。详见 `crates/core/AGENTS.md`。
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct NodeCapabilities: u32 {
        /// **纯计算**：同输入必得同输出、无任何外部副作用。
        ///
        /// **契约**（作者声明 `PURE` 时等于承诺以下全部为真）：
        /// - `transform` 只读 `payload` 参数，不读文件/网络/环境变量/时钟；
        /// - `transform` 不写磁盘、不发网络、不改全局可变状态、不取连接；
        /// - `tracing::info!` / `trace_id` 相关日志**不算副作用**（可容忍）；
        /// - 返回的 `metadata` 字段也必须是 payload 的确定性函数。
        ///
        /// **反例**（下列节点**不应**打 `PURE`）：
        /// - 用 `chrono::Utc::now()` 打时间戳 —— 时钟不是输入；
        /// - 读取 `connection_id` 获取连接 —— 状态机依赖；
        /// - 含 Rhai / WASM 等用户脚本（`code` 节点）—— 无法静态保证。
        ///
        /// **消费者**：Runner 第三阶段将以 `hash(payload) → cached_output` 缓存
        /// 纯节点结果。若此处撒谎，缓存命中会返回过期/错误的 payload，非常难调试。
        const PURE         = 0b0000_0001;

        /// **网络 I/O**：通过网络栈与远端通信（TCP/UDP/HTTP/MQTT/gRPC/Kafka 等）。
        ///
        /// **契约**：`transform` 路径上至少有一次网络往返。
        /// **反例**：纯本地计算节点（`if` / `switch`）、只访问本地文件的节点（`sqlWriter`）。
        /// **消费者**：前端画布渲染蓝色 badge；监控按此类聚合 p99 延迟 / 失败率。
        const NETWORK_IO   = 0b0000_0010;

        /// **文件/本地数据库 I/O**：读写本地磁盘文件、sqlite、日志等。
        ///
        /// **契约**：涉及文件系统调用或本地持久化层。
        /// **反例**：不包含内存缓存、共享内存等"不落盘"的存储。
        /// **消费者**：紫色 badge；便于运维识别"这个节点会吃磁盘 IOPS"。
        const FILE_IO      = 0b0000_0100;

        /// **设备 I/O**：通过工业总线/外设接口通信（Modbus / 串口 / OPC-UA / GPIO / CAN 等）。
        ///
        /// **契约**：访问受限的物理资源，通常需要独占借出连接。
        /// **反例**：走 TCP 上层协议（MQTT、HTTP）即使目的是设备也归为 [`NETWORK_IO`](Self::NETWORK_IO)。
        /// **消费者**：橙色 badge；未来可用于"设备节点并发上限 = N"的 ConcurrencyPolicy。
        const DEVICE_IO    = 0b0000_1000;

        /// **触发器**：不依赖上游 payload，由外部时钟/事件驱动生成执行。
        ///
        /// **契约**：该节点在 DAG 中位于根部（或由外部任务推入），不是被动响应上游。
        /// **反例**：`mqttClient` 在 `publish` 模式下是普通变换节点——`TRIGGER` 只属于
        /// "订阅/定时"类型的**恒成立**节点；混合语义节点在类型级别不声明 `TRIGGER`。
        /// **消费者**：前端画布自动把触发器布局在顶端；未来调度层识别入口。
        const TRIGGER      = 0b0001_0000;

        /// **分支节点**：按条件路由到下游的一部分端口（[`NodeDispatch::Route`]）。
        ///
        /// **契约**：`transform` 的输出 `dispatch` 使用 `Route` 而非 `Broadcast`。
        /// **反例**：始终广播的节点（`native` / `timer`）。
        /// **消费者**：前端画布识别分支，给出多端口视觉；DAG 校验器对分支分析更精确。
        const BRANCHING    = 0b0010_0000;

        /// **多输出**：单次 `transform` 产出多条 [`NodeOutput`]（循环展开、批处理拆分等）。
        ///
        /// **契约**：`NodeExecution::outputs.len() > 1` 在某些 payload 下成立。
        /// **反例**：总是单条输出的节点（即使 `Broadcast` 给多下游）。
        /// **消费者**：事件系统知道一次 tick 会产生多个 Completed；监控分桶时避免误算。
        const MULTI_OUTPUT = 0b0100_0000;

        /// **同步阻塞**：`transform` 内部使用阻塞 API，需要 Runner 包 `spawn_blocking`。
        ///
        /// **契约**：transform 路径上使用 `std::fs` / `std::net` / `rusqlite` 同步 API
        /// 等**未在异步运行时上**的阻塞调用，且**节点内部没有自行** `spawn_blocking`。
        /// **反例**：`sqlWriter` 虽用 `rusqlite`（同步），但在 `transform` 内部已
        /// `tokio::task::spawn_blocking` 包装，对外是 async-friendly，**不应**标 `BLOCKING`。
        /// **消费者**：Runner 第二阶段将把带此标签的节点统一包 `spawn_blocking`，
        /// 避免它们占用 Tokio worker 导致饿死其他节点。
        const BLOCKING     = 0b1000_0000;
    }
}

/// 节点输出的分发策略。
#[derive(Debug, Clone)]
pub enum NodeDispatch {
    /// 向所有下游节点广播。
    Broadcast,
    /// 按端口名称路由到特定下游。
    Route(Vec<String>),
}

/// 节点执行后产出的单条输出。
///
/// 包含变换后的 payload、执行元数据和分发策略。Runner 负责将 payload
/// 写入 [`DataStore`] 并生成 [`ContextRef`] 发往下游，元数据通过事件通道独立传递。
#[derive(Debug, Clone)]
pub struct NodeOutput {
    pub payload: Value,
    /// 节点执行元数据（如 `"timer"` → `{...}`），通过事件通道传递，不进入 payload。
    pub metadata: Map<String, Value>,
    pub dispatch: NodeDispatch,
}

/// 节点执行结果，可包含多条输出（如循环节点为每个元素生成一条）。
#[derive(Debug, Clone)]
pub struct NodeExecution {
    pub outputs: Vec<NodeOutput>,
}

impl NodeExecution {
    /// 创建一条广播到所有下游的执行结果。
    pub fn broadcast(payload: Value) -> Self {
        Self {
            outputs: vec![NodeOutput {
                payload,
                metadata: Map::new(),
                dispatch: NodeDispatch::Broadcast,
            }],
        }
    }

    /// 创建一条按端口路由的执行结果。
    pub fn route<I, S>(payload: Value, ports: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            outputs: vec![NodeOutput {
                payload,
                metadata: Map::new(),
                dispatch: NodeDispatch::Route(ports.into_iter().map(Into::into).collect()),
            }],
        }
    }

    /// 从多条输出构造执行结果。
    pub fn from_outputs(outputs: Vec<NodeOutput>) -> Self {
        Self { outputs }
    }

    /// 为所有输出附加执行元数据（Builder 模式）。
    ///
    /// 元数据键使用不带下划线的名称（如 `"timer"`）。
    #[must_use]
    #[allow(clippy::needless_pass_by_value)]
    pub fn with_metadata(mut self, metadata: Map<String, Value>) -> Self {
        let last = self.outputs.len().saturating_sub(1);
        for (i, output) in self.outputs.iter_mut().enumerate() {
            if i == last {
                output.metadata = metadata;
                return self;
            }
            output.metadata.clone_from(&metadata);
        }
        self
    }

    /// 获取第一条输出（如果存在）。
    pub fn first(&self) -> Option<&NodeOutput> {
        self.outputs.first()
    }
}

/// 所有工作流节点的统一异步 Trait。
///
/// 实现必须满足 `Send + Sync`，因为每个节点在独立的 Tokio 任务中运行。
/// 新节点类型只需实现此 Trait 即可接入工作流 DAG。
///
/// ## transform 签名
///
/// 节点接收 `trace_id`（追踪标识）和 `payload`（业务数据），
/// 返回包含变换后 payload、执行元数据和分发策略的 [`NodeExecution`]。
/// Runner 负责从 [`DataStore`](crate::DataStore) 读取输入数据、调用 `transform`，
/// 将 payload 写入 `DataStore` 并分发到下游，元数据通过
/// [`ExecutionEvent::Completed`](crate::ExecutionEvent::Completed) 事件独立传递。
///
/// 节点不接触 `DataStore` —— 它是 `(trace_id, payload) → (payload, metadata)` 的纯变换。
#[async_trait]
pub trait NodeTrait: Send + Sync {
    /// 节点在工作流图中的唯一标识。
    fn id(&self) -> &str;
    /// 返回节点类型标识（如 `"native"`、`"code"`、`"timer"` 等）。
    fn kind(&self) -> &'static str;

    /// 静态声明节点的输入引脚（ADR-0010）。
    ///
    /// **契约**：返回的 [`PinDefinition::id`] 在该节点上稳定（部署后不可改）；
    /// 同方向不可重复 id；标记 `required: true` 的输入引脚必须有上游入边
    /// 指向，否则部署期校验失败。返回的引脚类型决定边类型兼容矩阵
    /// （详见 [`PinType::is_compatible_with`](crate::PinType::is_compatible_with)）。
    ///
    /// **默认实现**返回单 `Any` 输入（id = `"in"`，required = `true`），
    /// 兼容存量"一进一出"节点，零改动通过部署期校验。多输入或具名输入
    /// 节点需 override。
    ///
    /// **是 `&self` 实例方法而非 `'static` 表**：与 `NodeCapabilities` 类型级
    /// 标签互补——pin 允许实例级精化（典型场景见 `output_pins`）。
    ///
    /// **消费者**：
    /// - `src/graph/deploy.rs` 阶段 0.5 校验器
    /// - 未来 IPC `describe_node_pins` 命令（Phase 2）
    /// - Phase 2 前端画布渲染多端口
    fn input_pins(&self) -> Vec<PinDefinition> {
        vec![PinDefinition::default_input()]
    }

    /// 静态声明节点的输出引脚（ADR-0010）。
    ///
    /// **契约**：返回的 [`PinDefinition::id`] 集合**必须包含** `transform` 路径
    /// 上 [`NodeDispatch::Route`] 可能产出的所有 port id，否则路由会丢消息且
    /// 部署期校验也无法保护错连（运行时悄无声息）。改 `output_pins` 与改
    /// `transform` 的 `Route([...])` 必须同步——这是 [`Route`](NodeDispatch::Route)
    /// 节点共用的不变量，不在每个实现里重复说明。
    ///
    /// **默认实现**返回单 `Any` 输出（id = `"out"`，required = `false`）。
    /// 分支节点（`if` / `switch` / `loop` / `tryCatch`）必须 override；多输出
    /// 节点（[`MULTI_OUTPUT`](NodeCapabilities::MULTI_OUTPUT)）通常也需要。
    ///
    /// **`required` 语义**：输出端口的 `required` 表示"每次执行必触发"；
    /// 通常分支节点的所有出口都标 `required: false`（同一次 transform 只会
    /// 经过其中一条），多输出节点（如 `loop` 的 `body` 与 `done` 同时触发）
    /// 才考虑标 `required: true`。Phase 1 不强制校验输出 required，仅作文档。
    ///
    /// **实例级精化**：`switch` 节点根据 `branches` config 动态生成 pin 列表，
    /// 这正是把 `output_pins` 设计为 `&self` 方法的原因。
    fn output_pins(&self) -> Vec<PinDefinition> {
        vec![PinDefinition::default_output()]
    }

    /// 执行节点逻辑：接收业务数据，返回变换后的 payload 与执行元数据。
    ///
    /// `payload` 由 Runner 从 `DataStore` 读出（`read_mut`，已是 owned 副本），
    /// 节点只需做变换。执行元数据（如连接信息、协议详情）通过
    /// [`NodeExecution::with_metadata`] 返回，与业务数据分离。
    async fn transform(&self, trace_id: Uuid, payload: Value)
    -> Result<NodeExecution, EngineError>;

    /// 节点部署时调用，早于任何 [`transform`](Self::transform)（ADR-0009）。
    ///
    /// 触发器 / 长连接节点（MQTT 订阅、Timer、Serial 监听等）在此建立连接、
    /// 订阅主题、拉起后台任务，通过 [`NodeLifecycleContext::handle`] 把外部
    /// 消息推进 DAG。返回的 [`LifecycleGuard`] 由 Runner 持有，撤销时按逆
    /// 拓扑序 `shutdown` 或依赖 `Drop` 兜底清理。
    ///
    /// **默认实现**返回 [`LifecycleGuard::noop`]——纯变换节点（`code` / `if`
    /// / `httpClient` 单次请求等）无需重写。
    ///
    /// # Errors
    ///
    /// 部署失败（建连超时、订阅被拒等）返回 [`EngineError`]；Runner 会按
    /// 逆拓扑序 drop 已注册的 guard，整图进入 `DeployFailed` 状态。
    async fn on_deploy(&self, _ctx: NodeLifecycleContext) -> Result<LifecycleGuard, EngineError> {
        Ok(LifecycleGuard::noop())
    }
}

/// 将 JSON payload 转换为 Map，非对象值会被包装为 `{"value": ...}`。
pub fn into_payload_map(payload: Value) -> Map<String, Value> {
    match payload {
        Value::Object(map) => map,
        other => {
            let mut map = Map::new();
            map.insert("value".to_owned(), other);
            map
        }
    }
}

/// 判定节点是否为 **pure-form**（UE5 Blueprint 风格的"表达式节点"）。
///
/// 定义：节点的 `input_pins` 与 `output_pins` 中**没有任何** [`PinKind::Exec`]
/// 引脚——意味着它**不参与触发链**：既不会被上游 Exec 边推、也不会向下游 Exec 推。
/// 此种节点在 `deploy_workflow` 的 spawn 阶段被跳过 Tokio task 创建，
/// 仅在被下游 Data 输入拉取时按需 `transform`（递归求值）。
///
/// **与 [`NodeCapabilities::PURE`] 的关系**：正交。
/// - `is_pure_form` 看引脚形态，由 `input_pins` / `output_pins` 自动推导
/// - `PURE` capability 是节点作者声明的"同输入同输出 + 无副作用"承诺，启用
///   未来 Phase 4 的输入哈希缓存。
///
/// 一个节点可以是 pure-form 而不打 PURE（少见，谨慎），也可以是 PURE 而非
/// pure-form（如 `if` / `switch`——参与触发链的纯函数）。`c2f` / `minutesSince`
/// 这种"理想 pure 计算节点"两者都满足。
pub fn is_pure_form(node: &dyn NodeTrait) -> bool {
    let no_exec_input = node
        .input_pins()
        .iter()
        .all(|p| p.kind != crate::PinKind::Exec);
    let no_exec_output = node
        .output_pins()
        .iter()
        .all(|p| p.kind != crate::PinKind::Exec);
    no_exec_input && no_exec_output
}

/// 为持有 `id` 字段的非脚本节点实现 [`NodeTrait`] 元数据方法。
#[macro_export]
macro_rules! impl_node_meta {
    ($kind:expr) => {
        fn id(&self) -> &str {
            &self.id
        }
        fn kind(&self) -> &'static str {
            $kind
        }
    };
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// 位分配由 ADR-0011 锁死；任何改动都会破坏 IPC 契约与前端常量表，必须同步。
    #[test]
    fn node_capabilities_位分配与_adr_0011_一致() {
        assert_eq!(NodeCapabilities::PURE.bits(), 0b0000_0001);
        assert_eq!(NodeCapabilities::NETWORK_IO.bits(), 0b0000_0010);
        assert_eq!(NodeCapabilities::FILE_IO.bits(), 0b0000_0100);
        assert_eq!(NodeCapabilities::DEVICE_IO.bits(), 0b0000_1000);
        assert_eq!(NodeCapabilities::TRIGGER.bits(), 0b0001_0000);
        assert_eq!(NodeCapabilities::BRANCHING.bits(), 0b0010_0000);
        assert_eq!(NodeCapabilities::MULTI_OUTPUT.bits(), 0b0100_0000);
        assert_eq!(NodeCapabilities::BLOCKING.bits(), 0b1000_0000);
    }

    #[test]
    fn node_capabilities_可按位组合() {
        let caps = NodeCapabilities::PURE | NodeCapabilities::BRANCHING;
        assert!(caps.contains(NodeCapabilities::PURE));
        assert!(caps.contains(NodeCapabilities::BRANCHING));
        assert!(!caps.contains(NodeCapabilities::NETWORK_IO));
    }

    #[test]
    fn node_capabilities_default_是空集合() {
        let caps = NodeCapabilities::default();
        assert!(caps.is_empty());
        assert_eq!(caps.bits(), 0);
    }

    mod is_pure_form_tests {
        use super::*;
        use crate::{PinDefinition, PinDirection, PinKind, PinType};
        use async_trait::async_trait;
        use serde_json::Value;

        struct StubNode {
            inputs: Vec<PinDefinition>,
            outputs: Vec<PinDefinition>,
        }

        #[async_trait]
        impl NodeTrait for StubNode {
            fn id(&self) -> &str {
                "stub"
            }
            fn kind(&self) -> &'static str {
                "stub"
            }
            fn input_pins(&self) -> Vec<PinDefinition> {
                self.inputs.clone()
            }
            fn output_pins(&self) -> Vec<PinDefinition> {
                self.outputs.clone()
            }
            async fn transform(
                &self,
                _: Uuid,
                payload: Value,
            ) -> Result<NodeExecution, EngineError> {
                Ok(NodeExecution::broadcast(payload))
            }
        }

        fn data_pin(id: &str, dir: PinDirection) -> PinDefinition {
            PinDefinition {
                id: id.to_owned(),
                label: id.to_owned(),
                pin_type: PinType::Float,
                direction: dir,
                required: false,
                kind: PinKind::Data,
                description: None,
            }
        }

        fn exec_pin(id: &str, dir: PinDirection) -> PinDefinition {
            PinDefinition {
                id: id.to_owned(),
                label: id.to_owned(),
                pin_type: PinType::Any,
                direction: dir,
                required: matches!(dir, PinDirection::Input),
                kind: PinKind::Exec,
                description: None,
            }
        }

        #[test]
        fn 全_data_引脚是_pure_form() {
            let n = StubNode {
                inputs: vec![data_pin("in", PinDirection::Input)],
                outputs: vec![data_pin("out", PinDirection::Output)],
            };
            assert!(is_pure_form(&n));
        }

        #[test]
        fn 输入混_exec_不是_pure_form() {
            let n = StubNode {
                inputs: vec![exec_pin("in", PinDirection::Input)],
                outputs: vec![data_pin("out", PinDirection::Output)],
            };
            assert!(!is_pure_form(&n));
        }

        #[test]
        fn 输出混_exec_不是_pure_form() {
            let n = StubNode {
                inputs: vec![data_pin("in", PinDirection::Input)],
                outputs: vec![exec_pin("out", PinDirection::Output)],
            };
            assert!(!is_pure_form(&n));
        }

        #[test]
        fn 仅有输出且全_data_仍是_pure_form() {
            let n = StubNode {
                inputs: vec![],
                outputs: vec![data_pin("out", PinDirection::Output)],
            };
            assert!(is_pure_form(&n));
        }
    }
}
