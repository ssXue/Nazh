//! Nazh I/O 节点与模板引擎（Ring 1）。

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
