//! CAN 后端注册表与工厂函数。

mod mock;
mod slcan;

use crate::can::{CanBus, CanBusConfig, CanError};

pub use self::mock::MockBackend;
pub use self::slcan::SlCanBackend;

/// 工厂函数 —— 根据 `CanBusConfig::interface` 路由到对应后端。
///
/// | interface | 后端 | 平台 |
/// |-----------|------|------|
/// | `"slcan"` | `SlCanBackend` | 全平台（USB 串口） |
/// | `"mock"` | `MockBackend` | 全平台（测试） |
pub async fn create_can_bus(config: &CanBusConfig) -> Result<Box<dyn CanBus>, CanError> {
    match config.interface.as_str() {
        "slcan" => {
            let bus = SlCanBackend::open(config).await?;
            Ok(Box::new(bus))
        }
        "mock" | "virtual" => {
            let bus = MockBackend::open(config)?;
            Ok(Box::new(bus))
        }
        other => Err(CanError::UnsupportedInterface(other.to_owned())),
    }
}
