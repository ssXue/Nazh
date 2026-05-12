//! Nazh 的 Tauri 桌面壳层。
//!
//! 桌面壳层负责注册 IPC 命令、初始化窗口效果与持久化配置，业务命令按域拆到
//! `commands/*`，运行时状态与事件转发分别位于 `runtime` / `events` 模块。

mod asset_files;
mod commands;
mod ethercat_esi;
mod events;
mod observability;
mod registry;
mod runtime;
mod state;
mod util;
mod workspace;

use ai::AiConfigFile;
use state::DesktopState;
use store::{Store, StoreHandle};
use tauri::{Manager, State};
#[cfg(target_os = "windows")]
use window_vibrancy::apply_blur;
#[cfg(target_os = "macos")]
use window_vibrancy::{NSVisualEffectMaterial, apply_vibrancy};

/// 初始化全局 tracing subscriber，同时输出到 stderr 和滚动日志文件。
///
/// `log_dir` 为 `None` 时仅输出到 stderr；为 `Some(dir)` 时在 `dir` 下创建按天轮转的
/// 日志文件，最多保留 7 个文件。返回的 `WorkerGuard` 必须在调用者处持有以保证异步
/// 写入在进程退出前刷盘。
fn init_tracing(
    log_dir: Option<&std::path::Path>,
) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("nazh_engine=info,nazh_desktop_lib=info,ai=info"));

    let stderr_layer = fmt::layer()
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false);

    let Some(log_dir) = log_dir else {
        tracing_subscriber::registry()
            .with(filter)
            .with(stderr_layer)
            .init();
        return None;
    };

    match std::fs::create_dir_all(log_dir) {
        Ok(()) => {
            let file_appender = tracing_appender::rolling::RollingFileAppender::builder()
                .rotation(tracing_appender::rolling::Rotation::DAILY)
                .max_log_files(7)
                .filename_prefix("nazh")
                .filename_suffix("log")
                .build(log_dir);

            match file_appender {
                Ok(appender) => {
                    let (non_blocking, guard) = tracing_appender::non_blocking(appender);
                    let file_layer = fmt::layer()
                        .with_target(true)
                        .with_thread_ids(false)
                        .with_file(false)
                        .with_line_number(false)
                        .with_ansi(false)
                        .with_writer(non_blocking);

                    tracing_subscriber::registry()
                        .with(filter)
                        .with(stderr_layer)
                        .with(file_layer)
                        .init();

                    tracing::info!(path = ?log_dir, "日志文件持久化已启用（按天轮转，最多保留 7 个文件）");
                    Some(guard)
                }
                Err(error) => {
                    tracing_subscriber::registry()
                        .with(filter)
                        .with(stderr_layer)
                        .init();
                    tracing::warn!(?error, path = ?log_dir, "无法创建日志文件 appender，仅使用 stderr");
                    None
                }
            }
        }
        Err(error) => {
            tracing_subscriber::registry()
                .with(filter)
                .with(stderr_layer)
                .init();
            tracing::warn!(?error, path = ?log_dir, "无法创建日志目录，仅使用 stderr");
            None
        }
    }
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
                            *store = Some(StoreHandle::new(file_store));
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
#[allow(clippy::too_many_lines)]
pub fn run() {
    let builder = tauri::Builder::default()
        .manage(DesktopState::default())
        .setup(|app| {
            // 在 setup 内初始化 tracing，以便使用 AppHandle 解析日志目录
            let log_dir = workspace::resolve_project_workspace_dir(app.handle(), None)
                .ok()
                .map(|(dir, _)| dir.join("logs"));
            let guard = init_tracing(log_dir.as_deref());
            if let Some(guard) = guard {
                let state: State<'_, DesktopState> = app.state();
                match state.tracing_guard.lock() {
                    Ok(mut slot) => *slot = Some(guard),
                    Err(error) => {
                        tracing::warn!(?error, "tracing_guard 锁 poisoned，日志文件可能无法正常刷盘");
                    }
                }
            }

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
            commands::observability::clear_observability,
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
            commands::ai::load_ai_api_key,
            commands::ai::save_ai_config,
            commands::ai::load_ai_asset_context,
            commands::copilot::copilot_list_conversations,
            commands::copilot::copilot_create_conversation,
            commands::copilot::copilot_delete_conversation,
            commands::copilot::copilot_load_conversation,
            commands::copilot::copilot_dispatch_tool,
            commands::copilot::copilot_save_message,
            commands::copilot::copilot_get_tool_definitions,
            commands::copilot::copilot_clear_embeddings,
            commands::copilot::copilot_store_embeddings,
            commands::human_loop::respond_human_loop,
            commands::human_loop::list_pending_approvals,
            commands::devices::list_device_assets,
            commands::devices::load_device_asset,
            commands::devices::save_device_asset,
            commands::devices::delete_device_asset,
            commands::devices::list_asset_versions,
            commands::devices::load_asset_version,
            commands::devices::list_device_snapshots,
            commands::devices::create_device_snapshot,
            commands::devices::rollback_device_snapshot,
            commands::devices::delete_device_snapshot,
            commands::devices::patch_device_field,
            commands::devices::add_device_signal,
            commands::devices::remove_device_signal,
            commands::devices::add_device_alarm,
            commands::devices::remove_device_alarm,
            commands::devices::generate_pin_schema,
            commands::devices::save_device_asset_sources,
            commands::devices::load_device_asset_sources,
            commands::devices::extract_text_from_pdf,
            commands::devices::import_ethercat_esi,
            commands::capabilities::list_capabilities,
            commands::capabilities::load_capability,
            commands::capabilities::save_capability,
            commands::capabilities::delete_capability,
            commands::capabilities::list_capability_versions,
            commands::capabilities::load_capability_version,
            commands::capabilities::generate_capabilities_from_device_cmd,
            commands::capabilities::save_capability_sources,
            commands::capabilities::load_capability_sources
        ]);

    if let Err(error) = builder.run(tauri::generate_context!()) {
        tracing::error!("Nazh 桌面壳层运行失败: {error}");
    }
}
