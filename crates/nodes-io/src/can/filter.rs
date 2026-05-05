//! CAN 接收过滤器。
//!
//! 借鉴 python-can 的 `set_filters()` 设计：
//! 过滤器在硬件/内核层生效（如 `SocketCAN` 的 `setsockopt(CAN_RAW_FILTER)`），
//! 不匹配帧被底层丢弃，不进入用户态。

use serde::{Deserialize, Serialize};

/// 单个 CAN 过滤器。
///
/// 匹配规则：`(received_id & can_mask) == (can_id & can_mask)`
/// 若 `extended` 为 `true`，则额外要求 `received_is_extended == true`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanFilter {
    /// 过滤 ID。
    pub can_id: u32,
    /// 过滤掩码。
    pub can_mask: u32,
    /// 是否仅匹配扩展帧。
    #[serde(default)]
    pub extended: bool,
}

impl CanFilter {
    /// 创建标准帧过滤器（11-bit ID）。
    pub fn standard(can_id: u32, can_mask: u32) -> Self {
        Self {
            can_id: can_id & 0x7FF,
            can_mask: can_mask & 0x7FF,
            extended: false,
        }
    }

    /// 创建扩展帧过滤器（29-bit ID）。
    #[allow(dead_code)]
    pub fn extended(can_id: u32, can_mask: u32) -> Self {
        Self {
            can_id: can_id & 0x1FFF_FFFF,
            can_mask: can_mask & 0x1FFF_FFFF,
            extended: true,
        }
    }

    /// 测试给定帧是否匹配此过滤器。
    pub fn matches(&self, frame_id: u32, frame_is_extended: bool) -> bool {
        if self.extended && !frame_is_extended {
            return false;
        }
        (frame_id & self.can_mask) == (self.can_id & self.can_mask)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn 标准帧过滤器匹配() {
        let f = CanFilter::standard(0x123, 0x7FF);
        assert!(f.matches(0x123, false));
        assert!(!f.matches(0x124, false));
    }

    #[test]
    fn 扩展帧过滤器不匹配标准帧() {
        let f = CanFilter::extended(0x18FF_0000, 0x1FFF_0000);
        assert!(f.matches(0x18FF_1234, true));
        assert!(!f.matches(0x123, false)); // 标准帧不匹配扩展过滤器
    }

    #[test]
    fn 掩码部分匹配() {
        let f = CanFilter::standard(0x100, 0x700);
        // 0x100 & 0x700 == 0x100, 0x123 & 0x700 == 0x100
        assert!(f.matches(0x123, false));
        assert!(f.matches(0x100, false));
        // 0x200 & 0x700 == 0x200 != 0x100
        assert!(!f.matches(0x200, false));
    }
}
