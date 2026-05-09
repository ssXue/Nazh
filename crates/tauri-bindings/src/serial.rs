use serde::Serialize;

#[cfg(feature = "ts-export")]
use ts_rs::TS;

/// 串口设备信息（`list_serial_ports`）。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SerialPortInfo {
    pub path: String,
    pub port_type: String,
    pub description: String,
}

/// 串口连接测试结果（`test_serial_connection`）。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct TestSerialResult {
    pub ok: bool,
    pub message: String,
}
