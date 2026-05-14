use tauri::{AppHandle, Manager};
use tauri_bindings::NetworkInterfaceInfo;

/// 重启或关闭 nazh-desktop 应用。
///
/// 用于 `EtherCAT` TX/RX 任务死亡等进程级资源不可恢复场景（ADR-0023 方案 B）。
///
/// 生产模式（`.app` / 打包二进制）：调用 `AppHandle::restart()` 全进程重启。
/// 开发模式（`tauri dev`）：`app.restart()` 会导致新进程连不上 Vite dev server
/// （`tauri dev` 随旧进程退出一起杀掉 Vite），表现为透明空窗。
/// 因此开发模式下直接 `exit(0)`，由用户手动重启。
#[tauri::command]
pub(crate) async fn restart_app(app: AppHandle) {
    if let Ok(data_dir) = app.path().app_local_data_dir() {
        crate::session_marker::clean_shutdown(&data_dir);
    }

    let is_dev = app
        .get_webview_window("main")
        .and_then(|w| w.url().ok())
        .is_some_and(|url| url.scheme() == "http" || url.scheme() == "https");

    if is_dev {
        tracing::info!("开发模式检测到，跳过自动重启（Vite dev server 不随应用重启），直接退出");
        app.exit(0);
    } else {
        app.restart();
    }
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
