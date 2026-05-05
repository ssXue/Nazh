//! CAN 帧统一格式。
//!
//! 借鉴 python-can 的 `Message` 设计，为所有后端（SLCAN / `SocketCAN` / Mock）
//! 提供跨平台一致的帧表示。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 字节序。
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ByteOrder {
    #[default]
    BigEndian,
    LittleEndian,
}

/// 跨平台统一 CAN 帧格式。
#[derive(Debug, Clone, PartialEq)]
pub struct CanFrame {
    /// 仲裁 ID（11-bit 标准帧 或 29-bit 扩展帧）。
    pub id: u32,
    /// 数据段（CAN 2.0 最大 8 byte，CAN-FD 最大 64 byte）。
    pub data: Vec<u8>,
    /// 数据长度码（0-8 for CAN 2.0，0-64 for CAN-FD）。
    pub dlc: u8,
    /// 是否为扩展帧（29-bit ID）。
    pub is_extended: bool,
    /// 是否为 CAN-FD 帧。
    pub is_fd: bool,
    /// 是否为远程请求帧（RTR）。
    pub is_remote: bool,
    /// 接收/发送时间戳。
    pub timestamp: Option<DateTime<Utc>>,
}

impl CanFrame {
    /// 创建标准数据帧。
    #[allow(clippy::cast_possible_truncation)]
    pub fn new_standard(id: u32, data: &[u8]) -> Self {
        let dlc = data.len().clamp(0, 8) as u8;
        Self {
            id,
            data: data[..dlc as usize].to_vec(),
            dlc,
            is_extended: false,
            is_fd: false,
            is_remote: false,
            timestamp: Some(Utc::now()),
        }
    }

    /// 创建扩展数据帧。
    #[allow(clippy::cast_possible_truncation)]
    pub fn new_extended(id: u32, data: &[u8]) -> Self {
        let dlc = data.len().clamp(0, 8) as u8;
        Self {
            id,
            data: data[..dlc as usize].to_vec(),
            dlc,
            is_extended: true,
            is_fd: false,
            is_remote: false,
            timestamp: Some(Utc::now()),
        }
    }

    /// 从帧数据中提取信号值。
    ///
    /// # Arguments
    /// * `offset` — 数据段起始字节偏移（0-7）
    /// * `length` — 字节长度（1-8）
    /// * `byte_order` — 字节序
    #[allow(dead_code)]
    pub fn extract_bytes(&self, offset: u8, length: u8) -> Option<Vec<u8>> {
        let start = offset as usize;
        let end = start + length as usize;
        if end > self.data.len() {
            return None;
        }
        Some(self.data[start..end].to_vec())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn 标准帧创建() {
        let frame = CanFrame::new_standard(0x123, &[0x01, 0x02, 0x03]);
        assert_eq!(frame.id, 0x123);
        assert_eq!(frame.dlc, 3);
        assert!(!frame.is_extended);
        assert_eq!(frame.data, vec![0x01, 0x02, 0x03]);
    }

    #[test]
    fn 扩展帧创建() {
        let frame = CanFrame::new_extended(0x18FF_1234, &[0xAB, 0xCD]);
        assert_eq!(frame.id, 0x18FF_1234);
        assert!(frame.is_extended);
        assert_eq!(frame.dlc, 2);
    }

    #[test]
    fn 数据超长自动截断() {
        let frame = CanFrame::new_standard(0x001, &[0; 16]);
        assert_eq!(frame.dlc, 8);
        assert_eq!(frame.data.len(), 8);
    }

    #[test]
    fn 提取字节切片() {
        let frame = CanFrame::new_standard(0x001, &[0x01, 0x02, 0x03, 0x04]);
        assert_eq!(frame.extract_bytes(1, 2), Some(vec![0x02, 0x03]));
        assert!(frame.extract_bytes(5, 4).is_none());
    }
}
