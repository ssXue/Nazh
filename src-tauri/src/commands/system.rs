use tauri::AppHandle;

/// 重启 nazh-desktop 应用。
///
/// 用于 `EtherCAT` TX/RX 任务死亡等进程级资源不可恢复场景（ADR-0023 方案 B）。
/// Tauri v2 的 `AppHandle::restart()` 永不返回，调用后进程立即终止并重新启动。
#[tauri::command]
pub(crate) async fn restart_app(app: AppHandle) {
    app.restart();
}
