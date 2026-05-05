//! Mock CAN 后端 —— 全平台测试回退。
//!
//! 无需任何物理硬件即可测试 `canRead` / `canWrite` 节点逻辑。
//! 模拟帧按固定周期生成，ID 和数据呈递增模式。

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::can::{BusState, CanBus, CanBusConfig, CanError, CanFilter, CanFrame};
use async_trait::async_trait;
use chrono::Utc;

/// Mock CAN 后端配置。
#[derive(Debug, Clone)]
pub struct MockConfig {
    pub channel: String,
    pub bitrate: u32,
    pub period_ms: u64,
}

impl From<&CanBusConfig> for MockConfig {
    fn from(config: &CanBusConfig) -> Self {
        Self {
            channel: config.channel.clone(),
            bitrate: config.bitrate,
            period_ms: 1000,
        }
    }
}

/// Mock CAN 后端。
pub struct MockBackend {
    config: MockConfig,
    counter: Arc<Mutex<u32>>,
    state: Arc<Mutex<BusState>>,
    filters: Arc<Mutex<Vec<CanFilter>>>,
}

impl MockBackend {
    #[allow(clippy::unnecessary_wraps)]
    pub fn open(config: &CanBusConfig) -> Result<Self, CanError> {
        Ok(Self {
            config: MockConfig::from(config),
            counter: Arc::new(Mutex::new(0)),
            state: Arc::new(Mutex::new(BusState::Active)),
            filters: Arc::new(Mutex::new(Vec::new())),
        })
    }

    /// 生成下一帧模拟数据。
    fn next_frame(&self) -> CanFrame {
        let mut counter = self
            .counter
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *counter = counter.wrapping_add(1);
        let c = *counter;

        // 模拟发动机 RPM 帧：ID 0x0C6，数据递增
        let data = vec![
            (c & 0xFF) as u8,
            ((c >> 8) & 0xFF) as u8,
            ((c >> 16) & 0xFF) as u8,
            ((c >> 24) & 0xFF) as u8,
            0x00,
            0x00,
            0x00,
            0x00,
        ];

        CanFrame {
            id: 0x0C6,
            data,
            dlc: 8,
            is_extended: false,
            is_fd: false,
            is_remote: false,
            timestamp: Some(Utc::now()),
        }
    }
}

#[async_trait]
impl CanBus for MockBackend {
    async fn send(&self, frame: &CanFrame) -> Result<(), CanError> {
        tracing::debug!(
            channel = %self.config.channel,
            id = format!("0x{:03X}", frame.id),
            dlc = frame.dlc,
            "[mock] CAN 帧已发送"
        );
        Ok(())
    }

    async fn recv(&self, timeout: Duration) -> Result<Option<CanFrame>, CanError> {
        tokio::time::sleep(std::cmp::min(
            timeout,
            Duration::from_millis(self.config.period_ms),
        ))
        .await;

        let frame = self.next_frame();

        // 应用过滤器
        let filters = self.filters.lock().map_err(|_| CanError::LockPoisoned)?;
        if !filters.is_empty()
            && !filters
                .iter()
                .any(|f| f.matches(frame.id, frame.is_extended))
        {
            return Ok(None);
        }

        Ok(Some(frame))
    }

    fn set_filters(&self, filters: &[CanFilter]) -> Result<(), CanError> {
        let mut current = self.filters.lock().map_err(|_| CanError::LockPoisoned)?;
        *current = filters.to_vec();
        Ok(())
    }

    fn shutdown(&self) -> Result<(), CanError> {
        if let Ok(mut state) = self.state.lock() {
            *state = BusState::Error;
        }
        Ok(())
    }

    fn channel_info(&self) -> String {
        format!(
            "Mock CAN {} @ {} kbps",
            self.config.channel,
            self.config.bitrate / 1000
        )
    }

    fn state(&self) -> BusState {
        *self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}
