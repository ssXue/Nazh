//! Nazh I/O 节点与模板引擎（Ring 1）。

use std::sync::Arc;

use connections::SharedConnectionManager;
use nazh_core::{
    EngineError, NodeCapabilities, NodeRegistry, Plugin, PluginManifest, SharedResources,
    WorkflowNodeDefinition,
};

pub mod template;

mod bark_push;
mod debug_console;
mod http_client;
mod modbus_read;
mod mqtt_client;
mod native;
mod serial_trigger;
mod sql_writer;
mod timer;

pub use bark_push::{BarkPushNode, BarkPushNodeConfig};
pub use debug_console::{DebugConsoleNode, DebugConsoleNodeConfig};
pub use http_client::{HttpClientNode, HttpClientNodeConfig};
pub use modbus_read::{ModbusReadNode, ModbusReadNodeConfig};
pub use mqtt_client::{MqttClientNode, MqttClientNodeConfig};
pub use native::{NativeNode, NativeNodeConfig};
pub use serial_trigger::{SerialTriggerNode, SerialTriggerNodeConfig};
pub use sql_writer::{SqlWriterNode, SqlWriterNodeConfig};
pub use timer::{TimerNode, TimerNodeConfig};

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

    fn register(&self, registry: &mut NodeRegistry) {
        registry.register_with_capabilities("native", NodeCapabilities::empty(), |def, res| {
            let mut config: NativeNodeConfig = def.parse_config()?;
            inherit_connection_id(&mut config.connection_id, def);
            let cm = downcast_connection_manager(&res)?;
            Ok(Arc::new(NativeNode::new(def.id().to_owned(), config, cm)))
        });

        registry.register_with_capabilities("timer", NodeCapabilities::TRIGGER, |def, _res| {
            let config: TimerNodeConfig = def.parse_config()?;
            Ok(Arc::new(TimerNode::new(def.id().to_owned(), config)))
        });

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

        registry.register_with_capabilities("sqlWriter", NodeCapabilities::FILE_IO, |def, _res| {
            let config: SqlWriterNodeConfig = def.parse_config()?;
            Ok(Arc::new(SqlWriterNode::new(def.id().to_owned(), config)))
        });

        registry.register_with_capabilities(
            "debugConsole",
            NodeCapabilities::empty(),
            |def, _res| {
                let config: DebugConsoleNodeConfig = def.parse_config()?;
                Ok(Arc::new(DebugConsoleNode::new(def.id().to_owned(), config)))
            },
        );

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
