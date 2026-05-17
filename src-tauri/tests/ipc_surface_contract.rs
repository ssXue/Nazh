//! IPC 命令表面契约测试。
//!
//! 从 `generate_handler!` 块提取实际注册命令，与预期列表比对。
//! 增删 IPC 命令时**必须**同步更新本文件的 `EXPECTED_COMMANDS` 列表。

#![allow(clippy::unwrap_used, clippy::expect_used)]
//!
//! 参考模式：`src/registry.rs` 节点注册表面契约测试。

use std::fs;
use std::path::PathBuf;

/// 从 `tauri::generate_handler![...]` 块提取所有 `commands::module::function` 条目。
fn extract_commands_from_handler(src: &str) -> Vec<String> {
    let mut in_handler = false;
    let mut commands = Vec::new();

    for line in src.lines() {
        if line.contains("generate_handler![") {
            in_handler = true;
            continue;
        }
        if !in_handler {
            continue;
        }
        if line.contains("]);") {
            break;
        }
        let trimmed = line.trim().trim_end_matches(',');
        if let Some(cmd) = trimmed.strip_prefix("commands::") {
            commands.push(cmd.to_owned());
        }
    }

    commands.sort();
    commands
}

/// 从 `src-tauri/src/commands/*.rs` 提取所有 `#[tauri::command]` 函数名。
///
/// `#[tauri::command]` 和 `fn` 之间可能有其他属性（如 `#[allow(...)]`），
/// 且函数可能是 async 或同步的。
fn extract_command_functions(commands_dir: &PathBuf) -> Vec<String> {
    let mut functions = Vec::new();
    collect_command_functions_recursive(commands_dir, &mut functions);
    functions.sort();
    functions
}

fn collect_command_functions_recursive(dir: &PathBuf, functions: &mut Vec<String>) {
    let entries = fs::read_dir(dir).unwrap_or_else(|e| {
        panic!("无法读取 commands 目录 {}: {e}", dir.display());
    });

    for entry in entries {
        let entry = entry.expect("目录条目读取失败");
        let path = entry.path();
        if path.is_dir() {
            collect_command_functions_recursive(&path, functions);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let content = fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("无法读取 {}: {e}", path.display()));
            let mut saw_command_attr = false;
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed == "#[tauri::command]" {
                    saw_command_attr = true;
                    continue;
                }
                if !saw_command_attr {
                    continue;
                }
                // 跳过 `#[tauri::command]` 和 `fn` 之间的其他属性
                if trimmed.starts_with("#[") {
                    continue;
                }
                if let Some(rest) = trimmed
                    .strip_prefix("pub(crate) async fn ")
                    .or_else(|| trimmed.strip_prefix("pub async fn "))
                    .or_else(|| trimmed.strip_prefix("pub(crate) fn "))
                    .or_else(|| trimmed.strip_prefix("pub fn "))
                {
                    let name = rest.split('(').next().unwrap_or(rest).trim();
                    functions.push(name.to_owned());
                }
                saw_command_attr = false;
            }
        }
    }
}

fn src_tauri_lib_rs() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs")
}

fn commands_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/commands")
}

/// 预期 IPC 命令列表（按域分组）。
///
/// 增删命令时必须同步更新此列表。总计 82 个。
const EXPECTED_COMMANDS: &[&str] = &[
    // workflow lifecycle (3)
    "workflow_deploy::deploy_workflow",
    "workflow_dispatch::dispatch_payload",
    "workflow_undeploy::undeploy_workflow",
    // catalog (2)
    "catalog::list_node_types",
    "catalog::describe_node_pins",
    // variables (9)
    "variables::snapshot_workflow_variables",
    "variables::set_workflow_variable",
    "variables::delete_workflow_variable",
    "variables::reset_workflow_variable",
    "variables::query_variable_history",
    "variables::set_global_variable",
    "variables::get_global_variable",
    "variables::list_global_variables",
    "variables::delete_global_variable",
    // runtime (4)
    "runtime::list_runtime_workflows",
    "runtime::set_active_runtime_workflow",
    "runtime::list_dead_letters",
    "runtime::subscribe_reactive_pin",
    // observability (3)
    "observability::query_observability",
    "observability::clear_observability",
    "observability::query_deployment_audit",
    // connections (4)
    "connections::list_connections",
    "connections::load_connection_definitions",
    "connections::save_connection_definitions",
    "connections::reset_connection_circuit_breaker",
    // deployment session (7)
    "deployment_session::load_deployment_session_file",
    "deployment_session::load_deployment_session_state_file",
    "deployment_session::list_deployment_sessions_file",
    "deployment_session::save_deployment_session_file",
    "deployment_session::set_deployment_session_active_project_file",
    "deployment_session::remove_deployment_session_file",
    "deployment_session::clear_deployment_session_file",
    // serial (2)
    "serial::list_serial_ports",
    "serial::test_serial_connection",
    // project library (3)
    "project_library::load_project_board_files",
    "project_library::save_project_board_files",
    "project_library::save_flowgram_export_file",
    // ai (4)
    "ai::load_ai_config",
    "ai::load_ai_api_key",
    "ai::save_ai_config",
    "ai::load_ai_asset_context",
    // copilot (7)
    "copilot::copilot_list_conversations",
    "copilot::copilot_create_conversation",
    "copilot::copilot_delete_conversation",
    "copilot::copilot_rename_conversation",
    "copilot::copilot_load_conversation",
    "copilot::copilot_dispatch_tool",
    "copilot::copilot_save_message",
    // human loop (2)
    "human_loop::respond_human_loop",
    "human_loop::list_pending_approvals",
    // devices (21)
    "devices::assets::list_device_assets",
    "devices::assets::load_device_asset",
    "devices::assets::save_device_asset",
    "devices::assets::delete_device_asset",
    "devices::versions::list_asset_versions",
    "devices::versions::load_asset_version",
    "devices::snapshots::list_device_snapshots",
    "devices::snapshots::create_device_snapshot",
    "devices::snapshots::rollback_device_snapshot",
    "devices::snapshots::delete_device_snapshot",
    "devices::fields::patch_device_field",
    "devices::fields::bind_device_connection",
    "devices::fields::add_device_signal",
    "devices::fields::remove_device_signal",
    "devices::fields::add_device_alarm",
    "devices::fields::remove_device_alarm",
    "devices::fields::generate_pin_schema",
    "devices::fields::save_device_asset_sources",
    "devices::fields::load_device_asset_sources",
    "devices::extract_text_from_pdf",
    "devices::import_ethercat_esi",
    // capabilities (9)
    "capabilities::list_capabilities",
    "capabilities::load_capability",
    "capabilities::save_capability",
    "capabilities::delete_capability",
    "capabilities::list_capability_versions",
    "capabilities::load_capability_version",
    "capabilities::generate_capabilities_from_device_cmd",
    "capabilities::save_capability_sources",
    "capabilities::load_capability_sources",
    // system (2)
    "system::restart_app",
    "system::list_network_interfaces",
];

#[test]
#[allow(clippy::unwrap_used)]
fn ipc_command_surface_matches_expected() {
    let lib_rs = src_tauri_lib_rs();
    let content = fs::read_to_string(&lib_rs)
        .unwrap_or_else(|e| panic!("无法读取 {}: {e}", lib_rs.display()));
    let actual = extract_commands_from_handler(&content);
    let mut expected: Vec<String> = EXPECTED_COMMANDS.iter().map(|s| (*s).to_owned()).collect();
    expected.sort();

    assert_eq!(
        actual.len(),
        expected.len(),
        "IPC 命令数量不匹配: 实际 {}, 预期 {}",
        actual.len(),
        expected.len(),
    );

    let added: Vec<&str> = actual
        .iter()
        .filter(|a| !expected.iter().any(|e| e == *a))
        .map(String::as_str)
        .collect();
    let removed: Vec<&str> = expected
        .iter()
        .filter(|e| !actual.iter().any(|a| a == *e))
        .map(String::as_str)
        .collect();

    assert!(
        added.is_empty() && removed.is_empty(),
        "IPC 命令列表与预期不一致\n\
         新增（代码中有、预期列表缺失）: {added:?}\n\
         移除（预期列表中有、代码缺失）: {removed:?}\n\
         \n\
         请更新本测试的 EXPECTED_COMMANDS 列表，并同步更新根 AGENTS.md 的 IPC surface 章节。",
    );
}

#[test]
#[allow(clippy::unwrap_used)]
fn every_command_in_handler_has_tauri_attribute() {
    let lib_rs = src_tauri_lib_rs();
    let content = fs::read_to_string(&lib_rs)
        .unwrap_or_else(|e| panic!("无法读取 {}: {e}", lib_rs.display()));
    let handler_commands = extract_commands_from_handler(&content);

    // 提取各命令文件中的函数名（不含模块前缀）
    let command_fns = extract_command_functions(&commands_dir());

    // handler 条目格式: "module::function" → 取 function 部分
    let handler_fn_names: Vec<&str> = handler_commands
        .iter()
        .map(|c| c.split("::").last().unwrap_or(c))
        .collect();

    let missing_attr: Vec<&str> = handler_fn_names
        .iter()
        .filter(|name| !command_fns.iter().any(|f| f == *name))
        .copied()
        .collect();

    let missing_handler: Vec<&String> = command_fns
        .iter()
        .filter(|name| !handler_fn_names.contains(&name.as_str()))
        .collect();

    assert!(
        missing_attr.is_empty() && missing_handler.is_empty(),
        "命令注册与 #[tauri::command] 不一致\n\
         handler 中有但缺少 #[tauri::command]: {missing_attr:?}\n\
         有 #[tauri::command] 但未注册到 handler: {missing_handler:?}",
    );
}
