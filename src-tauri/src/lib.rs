//! Nazh 的 Tauri 桌面壳层。
//!
//! 桌面壳层负责注册 IPC 命令、初始化窗口效果与持久化配置，业务命令按域拆到
//! `commands/*`，运行时状态与事件转发分别位于 `runtime` / `events` 模块。

mod commands;
mod events;
mod observability;
mod registry;
mod runtime;
mod state;
mod util;
mod workspace;

use ai::AiConfigFile;
use state::DesktopState;
use std::sync::Arc;
use store::Store;
use tauri::{Manager, State};
#[cfg(target_os = "windows")]
use window_vibrancy::apply_blur;
#[cfg(target_os = "macos")]
use window_vibrancy::{NSVisualEffectMaterial, apply_vibrancy};

/// 初始化全局 tracing subscriber，输出到 stderr。
///
/// 通过 `RUST_LOG` 环境变量控制日志级别，默认为 `nazh_engine=info,nazh_desktop_lib=info`。
fn init_tracing() {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("nazh_engine=info,nazh_desktop_lib=info"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .init();
}

#[cfg(target_os = "macos")]
fn apply_window_glass(window: &tauri::WebviewWindow) {
    if let Err(error) = apply_vibrancy(window, NSVisualEffectMaterial::HudWindow, None, Some(16.0))
    {
        tracing::warn!("应用 macOS 窗口玻璃效果失败: {error}");
    }
}

#[cfg(target_os = "windows")]
fn apply_window_glass(window: &tauri::WebviewWindow) {
    if let Err(error) = apply_blur(window, Some((18, 18, 18, 125))) {
        tracing::warn!("应用 Windows 窗口模糊效果失败: {error}");
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn apply_window_glass(_window: &tauri::WebviewWindow) {}

fn init_persistent_store(app: &tauri::App) {
    // 初始化文件持久化 Store，替换 Default 提供的内存 Store
    let state: State<'_, DesktopState> = app.state();
    if let Ok(data_dir) = app.path().app_local_data_dir() {
        match std::fs::create_dir_all(&data_dir) {
            Ok(()) => {
                let store_path = data_dir.join("store.sqlite3");
                match Store::open(&store_path) {
                    Ok(file_store) => match state.store.write() {
                        Ok(mut store) => {
                            *store = Arc::new(file_store);
                            tracing::info!(path = ?store_path, "持久化 Store 已打开");
                        }
                        Err(error) => {
                            tracing::warn!(?error, "Store 写锁 poisoned，继续使用内存 Store");
                        }
                    },
                    Err(error) => {
                        tracing::warn!(?error, "无法打开持久化 Store，继续使用内存 Store");
                    }
                }
            }
            Err(error) => {
                tracing::warn!(?error, path = ?data_dir, "无法创建持久化 Store 目录，继续使用内存 Store");
            }
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();

    let builder = tauri::Builder::default()
        .manage(DesktopState::default())
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                apply_window_glass(&window);
            } else {
                tracing::warn!("未找到主窗口，跳过玻璃效果初始化");
            }

            init_persistent_store(app);

            let app_handle = app.handle().clone();
            let state: State<'_, DesktopState> = app.state();
            let manager = state.connection_manager.clone();
            let ai_config_arc = state.ai_config.clone();
            tauri::async_runtime::spawn({
                let app_handle = app_handle.clone();
                async move {
                    DesktopState::load_connections_from_disk(&app_handle, manager, None).await;
                }
            });
            tauri::async_runtime::spawn(async move {
                if let Ok(path) = DesktopState::ai_config_file_path(&app_handle)
                    && path.exists()
                    && let Ok(text) = tokio::fs::read_to_string(&path).await
                    && let Ok(mut file_config) = serde_json::from_str::<AiConfigFile>(&text)
                {
                    file_config.normalize();
                    let mut config = ai_config_arc.write().await;
                    *config = file_config;
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::workflow_deploy::deploy_workflow,
            commands::workflow_dispatch::dispatch_payload,
            commands::workflow_undeploy::undeploy_workflow,
            commands::connections::list_connections,
            commands::catalog::list_node_types,
            commands::catalog::describe_node_pins,
            commands::variables::snapshot_workflow_variables,
            commands::variables::set_workflow_variable,
            commands::variables::delete_workflow_variable,
            commands::variables::reset_workflow_variable,
            commands::variables::query_variable_history,
            commands::variables::set_global_variable,
            commands::variables::get_global_variable,
            commands::variables::list_global_variables,
            commands::variables::delete_global_variable,
            commands::runtime::list_runtime_workflows,
            commands::runtime::set_active_runtime_workflow,
            commands::runtime::list_dead_letters,
            commands::runtime::subscribe_reactive_pin,
            commands::observability::query_observability,
            commands::connections::load_connection_definitions,
            commands::connections::save_connection_definitions,
            commands::deployment_session::load_deployment_session_file,
            commands::deployment_session::load_deployment_session_state_file,
            commands::deployment_session::list_deployment_sessions_file,
            commands::deployment_session::save_deployment_session_file,
            commands::deployment_session::set_deployment_session_active_project_file,
            commands::deployment_session::remove_deployment_session_file,
            commands::deployment_session::clear_deployment_session_file,
            commands::serial::list_serial_ports,
            commands::serial::test_serial_connection,
            commands::project_library::load_project_board_files,
            commands::project_library::save_project_board_files,
            commands::project_library::save_flowgram_export_file,
            commands::ai::load_ai_config,
            commands::ai::save_ai_config,
            commands::ai::test_ai_provider,
            commands::ai::copilot_complete,
            commands::ai::copilot_complete_stream,
            commands::human_loop::respond_human_loop,
            commands::human_loop::list_pending_approvals
        ]);

    if let Err(error) = builder.run(tauri::generate_context!()) {
        tracing::error!("Nazh 桌面壳层运行失败: {error}");
    }
}
