//! Nazh I/O 节点与模板引擎（Ring 1）。

use std::sync::Arc;

use connections::SharedConnectionManager;
use nazh_core::{EngineError, NodeRegistry, Plugin, PluginManifest, SharedResources};

pub mod template;

mod debug_console;
mod bark_push;
mod http_client;
mod modbus_read;
mod mqtt_client;
mod native;
mod serial_trigger;
mod sql_writer;
mod timer;

pub use debug_console::{DebugConsoleNode, DebugConsoleNodeConfig};
pub use bark_push::{BarkPushNode, BarkPushNodeConfig};
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

pub struct IoPlugin;

impl Plugin for IoPlugin {
    fn manifest(&self) -> PluginManifest {
        PluginManifest {
            name: "nodes-io",
            version: env!("CARGO_PKG_VERSION"),
        }
    }

    fn register(&self, registry: &mut NodeRegistry) {
        registry.register("native", |def, res| {
            let mut config: NativeNodeConfig = def.parse_config()?;
            if config.connection_id.is_none() {
                config.connection_id.clone_from(&def.connection_id);
            }
            let cm = downcast_connection_manager(&res)?;
            Ok(Arc::new(NativeNode::new(def.id.clone(), config, cm)))
        });

        registry.register("timer", |def, _res| {
            let config: TimerNodeConfig = def.parse_config()?;
            Ok(Arc::new(TimerNode::new(def.id.clone(), config)))
        });

        registry.register("serialTrigger", |def, _res| {
            let config: SerialTriggerNodeConfig = def.parse_config()?;
            Ok(Arc::new(SerialTriggerNode::new(def.id.clone(), config)))
        });

        registry.register("modbusRead", |def, res| {
            let mut config: ModbusReadNodeConfig = def.parse_config()?;
            if config.connection_id.is_none() {
                config.connection_id.clone_from(&def.connection_id);
            }
            let cm = downcast_connection_manager(&res)?;
            Ok(Arc::new(ModbusReadNode::new(def.id.clone(), config, cm)))
        });

        registry.register("httpClient", |def, _res| {
            let config: HttpClientNodeConfig = def.parse_config()?;
            Ok(Arc::new(HttpClientNode::new(def.id.clone(), config)?))
        });

        registry.register("barkPush", |def, _res| {
            let config: BarkPushNodeConfig = def.parse_config()?;
            Ok(Arc::new(BarkPushNode::new(def.id.clone(), config)?))
        });

        registry.register("sqlWriter", |def, _res| {
            let config: SqlWriterNodeConfig = def.parse_config()?;
            Ok(Arc::new(SqlWriterNode::new(def.id.clone(), config)))
        });

        registry.register("debugConsole", |def, _res| {
            let config: DebugConsoleNodeConfig = def.parse_config()?;
            Ok(Arc::new(DebugConsoleNode::new(def.id.clone(), config)))
        });

        registry.register("mqttClient", |def, res| {
            let mut config: MqttClientNodeConfig = def.parse_config()?;
            if config.connection_id.is_none() {
                config.connection_id.clone_from(&def.connection_id);
            }
            let cm = downcast_connection_manager(&res)?;
            Ok(Arc::new(MqttClientNode::new(def.id.clone(), config, cm)))
        });
    }
}
