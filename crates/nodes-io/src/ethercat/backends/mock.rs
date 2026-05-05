//! Mock EtherCAT 后端 —— 模拟从站，用于无硬件环境的开发测试。

use std::collections::HashMap;
use std::sync::RwLock;

use async_trait::async_trait;

use crate::ethercat::{EthercatBus, EthercatConfig, EthercatError, SlaveState};

/// 模拟从站。
struct MockSlave {
    name: String,
    input_bytes: Vec<u8>,
    output_bytes: Vec<u8>,
    al_status: u16,
    online: bool,
}

/// Mock EtherCAT 主站后端。
pub struct MockBackend {
    slaves: RwLock<HashMap<u16, MockSlave>>,
    cycle_time_ms: u64,
}

impl MockBackend {
    pub fn new(config: &EthercatConfig) -> Self {
        // 创建默认模拟从站：3 个从站，各 4 字节输入 / 4 字节输出
        let mut slaves = HashMap::new();
        for addr in 1..=3u16 {
            let mut input_bytes = vec![0u8; 4];
            // 填充递增模式便于识别
            for (i, byte) in input_bytes.iter_mut().enumerate() {
                *byte = u8::try_from(addr).unwrap_or(0) << 4 | u8::try_from(i).unwrap_or(0);
            }
            slaves.insert(
                addr,
                MockSlave {
                    name: format!("MockSlave-{addr}"),
                    input_bytes,
                    output_bytes: vec![0u8; 4],
                    al_status: 0x08, // OP 状态
                    online: true,
                },
            );
        }
        Self {
            slaves: RwLock::new(slaves),
            cycle_time_ms: config.cycle_time_ms,
        }
    }
}

#[async_trait]
impl EthercatBus for MockBackend {
    async fn read_inputs(&self, slave_address: u16) -> Result<Vec<u8>, EthercatError> {
        let slaves = self
            .slaves
            .read()
            .map_err(|_| EthercatError::LockContended)?;
        let slave = slaves
            .get(&slave_address)
            .ok_or(EthercatError::SlaveNotFound {
                address: slave_address,
            })?;

        if !slave.online {
            return Err(EthercatError::PdoReadFailed(format!(
                "从站 {slave_address} 离线"
            )));
        }

        // 模拟数据递增（模拟真实传感器数据变化）
        let mut data = slave.input_bytes.clone();
        for byte in &mut data {
            *byte = byte.wrapping_add(1);
        }

        Ok(data)
    }

    async fn write_outputs(&self, slave_address: u16, data: &[u8]) -> Result<(), EthercatError> {
        let mut slaves = self
            .slaves
            .write()
            .map_err(|_| EthercatError::LockContended)?;
        let slave = slaves
            .get_mut(&slave_address)
            .ok_or(EthercatError::SlaveNotFound {
                address: slave_address,
            })?;

        if data.len() != slave.output_bytes.len() {
            return Err(EthercatError::DataLengthMismatch {
                expected: slave.output_bytes.len(),
                actual: data.len(),
            });
        }

        slave.output_bytes.copy_from_slice(data);
        Ok(())
    }

    fn get_slave_states(&self) -> Vec<SlaveState> {
        let Ok(slaves) = self.slaves.read() else {
            return Vec::new();
        };

        slaves
            .iter()
            .map(|(addr, slave)| SlaveState {
                address: *addr,
                name: slave.name.clone(),
                al_status: slave.al_status,
                al_status_text: al_status_to_text(slave.al_status),
                online: slave.online,
                input_bytes: slave.input_bytes.len(),
                output_bytes: slave.output_bytes.len(),
            })
            .collect()
    }

    fn shutdown(&self) -> Result<(), EthercatError> {
        tracing::info!("Mock EtherCAT 主站已关闭");
        Ok(())
    }

    fn channel_info(&self) -> String {
        format!(
            "mock-ethercat ({} 从站, {}ms 周期)",
            self.slaves.read().map_or(0, |s| s.len()),
            self.cycle_time_ms,
        )
    }
}

/// 将 AL 状态码转为可读文本。
fn al_status_to_text(code: u16) -> String {
    match code {
        0x01 => "初始化".to_owned(),
        0x02 => "预运行".to_owned(),
        0x03 => "引导".to_owned(),
        0x04 => "安全运行".to_owned(),
        0x08 => "运行".to_owned(),
        other => format!("未知(0x{other:04X})"),
    }
}
