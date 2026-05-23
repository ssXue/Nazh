//! EtherCAT 后端工厂。

mod mock;
mod soem_backend;

use super::{EthercatBus, EthercatConfig, EthercatError};

/// 根据配置创建 EtherCAT 总线后端。
pub async fn create_ethercat_bus(
    config: &EthercatConfig,
) -> Result<Box<dyn EthercatBus>, EthercatError> {
    match config.backend.as_str() {
        "soem" => {
            soem_backend::SoemBackend::create(config).map(|b| Box::new(b) as Box<dyn EthercatBus>)
        }
        "mock" | "" => mock::MockBackend::new(config).map(|b| Box::new(b) as Box<dyn EthercatBus>),
        other => Err(EthercatError::UnsupportedBackend(other.to_owned())),
    }
}
