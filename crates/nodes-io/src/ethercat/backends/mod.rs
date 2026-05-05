//! EtherCAT 后端工厂。

mod ethercrab_backend;
mod mock;

use super::{EthercatBus, EthercatConfig, EthercatError};

/// 根据配置创建 EtherCAT 总线后端。
pub async fn create_ethercat_bus(
    config: &EthercatConfig,
) -> Result<Box<dyn EthercatBus>, EthercatError> {
    match config.backend.as_str() {
        "ethercrab" => ethercrab_backend::EthercrabBackend::create(config)
            .await
            .map(|b| Box::new(b) as Box<dyn EthercatBus>),
        "mock" | "" => Ok(Box::new(mock::MockBackend::new(config))),
        other => Err(EthercatError::UnsupportedBackend(other.to_owned())),
    }
}
