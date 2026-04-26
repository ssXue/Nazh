//! 壳层 IPC 私有类型。运行时类型已上移到 Ring 0（`nazh_core::ai`）。

use serde::{Deserialize, Serialize};
#[cfg(feature = "ts-export")]
use ts_rs::TS;

/// 连通性测试结果。仅用于壳层 IPC，不参与运行时 trait。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AiTestResult {
    pub success: bool,
    pub message: String,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub latency_ms: Option<u64>,
}
