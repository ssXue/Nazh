//! Nazh I/O 节点与模板引擎（Ring 1）。
//!
//! ## 按协议 feature 门控
//!
//! 协议相关的节点模块（http / mqtt / modbus / serial / sql / notify）通过
//! `io-*` feature 启用：
//!
//! - `io-all`（默认）— 全部节点 = 桌面 / 工作站默认体验
//! - `io-http` — `httpClient`
//! - `io-mqtt` — `mqttClient`
//! - `io-modbus` — `modbusRead`
//! - `io-serial` — `serialTrigger`
//! - `io-sql` — `sqlWriter`
//! - `io-notify` — `barkPush`（与 `io-http` 共享 reqwest）
//!
//! 永远启用：`debugConsole` / `native` / `timer` 与 `template` 公共工具
//! ——它们不依赖额外协议栈，是任何部署都可能用到的"管道节点"。
//!
//! 边缘部署示例：
//! ```bash
//! cargo build --no-default-features --features "io-mqtt,io-modbus"
//! ```
//! 这会跳过 `reqwest` / `rusqlite` / `serialport` 等二进制体积重的依赖。

use std::sync::Arc;

use connections::SharedConnectionManager;
use nazh_core::{
    EngineError, NodeCapabilities, NodeRegistry, Plugin, PluginManifest, SharedResources,
    WorkflowNodeDefinition, WorkflowVariables,
};

pub mod template;

// 永远启用的轻量节点
mod capability_call;
mod debug_console;
mod human_loop;
mod native;
mod timer;

// 协议相关节点：按 cfg 启用
#[cfg(feature = "io-notify")]
mod bark_push;
#[cfg(feature = "io-http")]
mod http_client;
#[cfg(feature = "io-modbus")]
mod modbus_read;
#[cfg(feature = "io-mqtt")]
mod mqtt_client;
#[cfg(feature = "io-serial")]
mod serial_trigger;
#[cfg(feature = "io-sql")]
mod sql_writer;

pub use capability_call::{CapabilityCallConfig, CapabilityCallNode, CapabilityImplSnapshot};
pub use debug_console::{DebugConsoleNode, DebugConsoleNodeConfig};
pub use human_loop::ApprovalRegistry;
pub use human_loop::HumanLoopNode;
pub use human_loop::HumanLoopNodeConfig;
pub use human_loop::WorkflowId;
pub use human_loop::registry::{HumanLoopResponse, PendingApprovalSummary, ResponseAction};
pub use native::{NativeNode, NativeNodeConfig};
pub use timer::{TimerNode, TimerNodeConfig};

#[cfg(feature = "io-notify")]
pub use bark_push::{BarkPushNode, BarkPushNodeConfig};
#[cfg(feature = "io-http")]
pub use http_client::{HttpClientNode, HttpClientNodeConfig};
#[cfg(feature = "io-modbus")]
pub use modbus_read::{ModbusReadNode, ModbusReadNodeConfig};
#[cfg(feature = "io-mqtt")]
pub use mqtt_client::{MqttClientNode, MqttClientNodeConfig, MqttMode};
#[cfg(feature = "io-serial")]
pub use serial_trigger::{SerialTriggerNode, SerialTriggerNodeConfig};
#[cfg(feature = "io-sql")]
pub use sql_writer::{SqlWriterNode, SqlWriterNodeConfig};

fn downcast_connection_manager(
    resources: &SharedResources,
) -> Result<SharedConnectionManager, EngineError> {
    resources
        .get::<SharedConnectionManager>()
        .ok_or_else(|| EngineError::invalid_graph("部署资源中缺少 ConnectionManager"))
}

/// 若节点 config 未指定 `connection_id`，则从 `WorkflowNodeDefinition` 顶层字段继承。
fn inherit_connection_id(config_conn: &mut Option<String>, def: &WorkflowNodeDefinition) {
    if config_conn.is_none() {
        *config_conn = def.connection_id().map(str::to_owned);
    }
}

pub struct IoPlugin;

impl Plugin for IoPlugin {
    fn manifest(&self) -> PluginManifest {
        PluginManifest {
            name: "nodes-io",
            version: env!("CARGO_PKG_VERSION"),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn register(&self, registry: &mut NodeRegistry) {
        // 永远启用的节点：debug / native / timer / humanLoop
        registry.register_with_capabilities("timer", NodeCapabilities::TRIGGER, |def, _res| {
            let config: TimerNodeConfig = def.parse_config()?;
            Ok(Arc::new(TimerNode::new(def.id().to_owned(), config)))
        });

        registry.register_with_capabilities(
            "humanLoop",
            NodeCapabilities::BRANCHING,
            |def, res| {
                let config: HumanLoopNodeConfig = def.parse_config()?;
                let approval_registry = res.get::<Arc<ApprovalRegistry>>().ok_or_else(|| {
                    EngineError::invalid_graph("ApprovalRegistry 未注入 RuntimeResources")
                })?;
                let workflow_id = res
                    .get::<WorkflowId>()
                    .map(|w| w.as_str().to_owned())
                    .unwrap_or_default();
                Ok(Arc::new(HumanLoopNode::new(
                    def.id().to_owned(),
                    config,
                    approval_registry,
                    workflow_id,
                )))
            },
        );

        registry.register_with_capabilities(
            "debugConsole",
            NodeCapabilities::empty(),
            |def, _res| {
                let config: DebugConsoleNodeConfig = def.parse_config()?;
                Ok(Arc::new(DebugConsoleNode::new(def.id().to_owned(), config)))
            },
        );

        registry.register_with_capabilities("native", NodeCapabilities::empty(), |def, res| {
            let mut config: NativeNodeConfig = def.parse_config()?;
            inherit_connection_id(&mut config.connection_id, def);
            let cm = downcast_connection_manager(&res)?;
            Ok(Arc::new(NativeNode::new(def.id().to_owned(), config, cm)))
        });

        // RFC-0004 Phase 3：DSL 编译器生成的通用能力调用节点。
        registry.register_with_capabilities(
            "capabilityCall",
            NodeCapabilities::DEVICE_IO,
            |def, res| {
                let config: CapabilityCallConfig = def.parse_config()?;
                let variables = res.get::<Arc<WorkflowVariables>>();
                let cm = downcast_connection_manager(&res)?;
                Ok(Arc::new(CapabilityCallNode::new(
                    def.id().to_owned(),
                    config,
                    variables,
                    cm,
                )))
            },
        );

        // 协议相关节点：按 feature 注册。前端 `list_node_types` 会反映当前
        // 构建启用的节点集合，未启用的协议在前端 FlowGram 节点库中自动隐藏。
        #[cfg(feature = "io-serial")]
        registry.register_with_capabilities(
            "serialTrigger",
            NodeCapabilities::TRIGGER | NodeCapabilities::DEVICE_IO,
            |def, res| {
                let config: SerialTriggerNodeConfig = def.parse_config()?;
                let cm = downcast_connection_manager(&res)?;
                Ok(Arc::new(SerialTriggerNode::new(
                    def.id().to_owned(),
                    config,
                    def.connection_id().map(str::to_owned),
                    cm,
                )))
            },
        );

        #[cfg(feature = "io-modbus")]
        registry.register_with_capabilities(
            "modbusRead",
            NodeCapabilities::DEVICE_IO,
            |def, res| {
                let mut config: ModbusReadNodeConfig = def.parse_config()?;
                inherit_connection_id(&mut config.connection_id, def);
                let cm = downcast_connection_manager(&res)?;
                Ok(Arc::new(ModbusReadNode::new(
                    def.id().to_owned(),
                    config,
                    cm,
                )))
            },
        );

        #[cfg(feature = "io-http")]
        registry.register_with_capabilities(
            "httpClient",
            NodeCapabilities::NETWORK_IO,
            |def, res| {
                let mut config: HttpClientNodeConfig = def.parse_config()?;
                inherit_connection_id(&mut config.connection_id, def);
                let cm = downcast_connection_manager(&res)?;
                Ok(Arc::new(HttpClientNode::new(
                    def.id().to_owned(),
                    config,
                    cm,
                )?))
            },
        );

        #[cfg(feature = "io-notify")]
        registry.register_with_capabilities(
            "barkPush",
            NodeCapabilities::NETWORK_IO,
            |def, res| {
                let mut config: BarkPushNodeConfig = def.parse_config()?;
                inherit_connection_id(&mut config.connection_id, def);
                let cm = downcast_connection_manager(&res)?;
                Ok(Arc::new(BarkPushNode::new(
                    def.id().to_owned(),
                    config,
                    cm,
                )?))
            },
        );

        #[cfg(feature = "io-sql")]
        registry.register_with_capabilities("sqlWriter", NodeCapabilities::FILE_IO, |def, _res| {
            let config: SqlWriterNodeConfig = def.parse_config()?;
            Ok(Arc::new(SqlWriterNode::new(def.id().to_owned(), config)))
        });

        #[cfg(feature = "io-mqtt")]
        registry.register_with_capabilities(
            "mqttClient",
            NodeCapabilities::NETWORK_IO,
            |def, res| {
                let mut config: MqttClientNodeConfig = def.parse_config()?;
                inherit_connection_id(&mut config.connection_id, def);
                let cm = downcast_connection_manager(&res)?;
                Ok(Arc::new(MqttClientNode::new(
                    def.id().to_owned(),
                    config,
                    cm,
                )))
            },
        );
    }
}
