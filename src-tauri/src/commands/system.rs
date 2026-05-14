use tauri::{AppHandle, Manager};
use tauri_bindings::NetworkInterfaceInfo;

/// 重启 nazh-desktop 应用。
///
/// 用于 `EtherCAT` TX/RX 任务死亡等进程级资源不可恢复场景（ADR-0023 方案 B）。
/// Tauri v2 的 `AppHandle::restart()` 永不返回，调用后进程立即终止并重新启动。
#[tauri::command]
pub(crate) async fn restart_app(app: AppHandle) {
    // 清理启动标记，避免下次启动时误报意外终止
    if let Ok(data_dir) = app.path().app_local_data_dir() {
        crate::session_marker::clean_shutdown(&data_dir);
    }
    app.restart();
}

/// 枚举本机所有网络接口，供 EtherCAT 等需要绑定物理网卡的连接面板使用。
#[tauri::command]
pub(crate) async fn list_network_interfaces(
) -> Result<Vec<NetworkInterfaceInfo>, String> {
    let interfaces = pnet_datalink::interfaces();
    let result = interfaces
        .into_iter()
        .map(|iface| {
            let is_loopback = iface.is_loopback();
            let is_up = iface.is_up();
            let mac = iface.mac.map(|m| m.to_string());
            let ipv4 = iface
                .ips
                .iter()
                .filter(|ip| ip.is_ipv4())
                .map(|ip| ip.to_string())
                .collect();
            NetworkInterfaceInfo {
                name: iface.name,
                description: iface.description,
                mac,
                ipv4,
                is_loopback,
                is_up,
            }
        })
        .collect();
    Ok(result)
}
