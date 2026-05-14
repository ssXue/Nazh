use serde::Serialize;

#[cfg(feature = "ts-export")]
use ts_rs::TS;

/// 网络接口信息（`list_network_interfaces`）。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct NetworkInterfaceInfo {
    /// 接口名称（如 eth0、enp3s0）。
    pub name: String,
    /// 接口描述 / 显示名称。
    pub description: String,
    /// MAC 地址（冒号分隔十六进制）。
    pub mac: Option<String>,
    /// IPv4 地址列表。
    pub ipv4: Vec<String>,
    /// 是否为回环接口。
    pub is_loopback: bool,
    /// 是否已启用（UP）。
    pub is_up: bool,
}
