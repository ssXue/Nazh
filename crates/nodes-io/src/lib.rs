//! Nazh I/O 节点与模板引擎（Ring 1）。

use std::sync::Arc;

use connections::{ConnectionManager, SharedConnectionManager};
use nazh_core::{EngineError, NodeRegistry, Plugin, PluginManifest, SharedResources};

pub mod template;

mod debug_console;
mod http_client;
mod modbus_read;
mod native;
mod serial_trigger;
mod sql_writer;
mod timer;

pub use debug_console::{DebugConsoleNode, DebugConsoleNodeConfig};
pub use http_client::{HttpClientNode, HttpClientNodeConfig};
pub use modbus_read::{ModbusReadNode, ModbusReadNodeConfig};
pub use native::{NativeNode, NativeNodeConfig};
pub use serial_trigger::{SerialTriggerNode, SerialTriggerNodeConfig};
pub use sql_writer::{SqlWriterNode, SqlWriterNodeConfig};
pub use timer::{TimerNode, TimerNodeConfig};

fn downcast_connection_manager(
    resources: &SharedResources,
) -> Result<SharedConnectionManager, EngineError> {
    resources
        .clone()
        .downcast::<ConnectionManager>()
        .map_err(|_| {
            EngineError::invalid_graph("部署资源中缺少 ConnectionManager")
        })
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
            let description = def.resolve_description("打印 payload 元数据，可选附加连接上下文");
            let cm = downcast_connection_manager(&res)?;
            Ok(Arc::new(NativeNode::new(def.id.clone(), config, description, cm)))
        });
        let _ = registry.alias("native/log", "native");
        let _ = registry.alias("log", "native");

        registry.register("timer", |def, _res| {
            let config: TimerNodeConfig = def.parse_config()?;
            let description = def.resolve_description("按固定间隔触发工作流并注入计时元数据");
            Ok(Arc::new(TimerNode::new(def.id.clone(), config, description)))
        });

        registry.register("serialTrigger", |def, _res| {
            let config: SerialTriggerNodeConfig = def.parse_config()?;
            let description =
                def.resolve_description("接收串口外设主动上报的 ASCII/HEX 数据流并触发工作流");
            Ok(Arc::new(SerialTriggerNode::new(def.id.clone(), config, description)))
        });
        let _ = registry.alias("serial/trigger", "serialTrigger");
        let _ = registry.alias("serial", "serialTrigger");

        registry.register("modbusRead", |def, res| {
            let mut config: ModbusReadNodeConfig = def.parse_config()?;
            if config.connection_id.is_none() {
                config.connection_id.clone_from(&def.connection_id);
            }
            let description =
                def.resolve_description("读取模拟 Modbus 寄存器并将遥测数据写入 payload");
            let cm = downcast_connection_manager(&res)?;
            Ok(Arc::new(ModbusReadNode::new(def.id.clone(), config, description, cm)))
        });
        let _ = registry.alias("modbus/read", "modbusRead");

        registry.register("httpClient", |def, _res| {
            let config: HttpClientNodeConfig = def.parse_config()?;
            let description =
                def.resolve_description("将 payload 发送到 HTTP 端点（如钉钉机器人告警）");
            Ok(Arc::new(HttpClientNode::new(def.id.clone(), config, description)?))
        });
        let _ = registry.alias("http/client", "httpClient");

        registry.register("sqlWriter", |def, _res| {
            let config: SqlWriterNodeConfig = def.parse_config()?;
            let description = def.resolve_description("将当前 payload 持久化到本地 SQLite 表");
            Ok(Arc::new(SqlWriterNode::new(def.id.clone(), config, description)))
        });
        let _ = registry.alias("sql/writer", "sqlWriter");

        registry.register("debugConsole", |def, _res| {
            let config: DebugConsoleNodeConfig = def.parse_config()?;
            let description = def.resolve_description("将 payload 打印到调试控制台以供检查");
            Ok(Arc::new(DebugConsoleNode::new(def.id.clone(), config, description)))
        });
        let _ = registry.alias("debug/console", "debugConsole");
    }
}
