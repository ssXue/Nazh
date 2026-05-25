//! 连接资产诊断响应类型（ADR-0026 Phase 3）。

use serde::Serialize;
#[cfg(feature = "ts-export")]
use ts_rs::TS;

/// 连接资产诊断结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
pub struct ConnectionDiagnosticResult {
    pub ok: bool,
    pub connection_id: String,
    pub protocol: String,
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub latency_ms: Option<u64>,
    pub message: String,
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub detail: Option<String>,
}
