//! Copilot 对话式副驾驶 IPC 命令。

use chrono::Utc;
use serde_json::json;
use tauri::State;
use tauri_bindings::{CopilotConversationResponse, CopilotMessageResponse};
use uuid::Uuid;

use crate::commands::copilot_tools;
use crate::state::DesktopState;

fn map_conversation(c: &store::CopilotConversation) -> CopilotConversationResponse {
    CopilotConversationResponse {
        id: c.id.clone(),
        title: c.title.clone(),
        created_at: c.created_at.clone(),
        updated_at: c.updated_at.clone(),
    }
}

fn map_message(m: &store::CopilotMessage) -> CopilotMessageResponse {
    CopilotMessageResponse {
        id: m.id.clone(),
        conversation_id: m.conversation_id.clone(),
        role: m.role.clone(),
        content: m.content.clone(),
        thinking: m.thinking.clone(),
        created_at: m.created_at.clone(),
    }
}

#[tauri::command]
pub(crate) async fn copilot_list_conversations(
    state: State<'_, DesktopState>,
) -> Result<Vec<CopilotConversationResponse>, String> {
    let handle = state.store_handle()?;
    handle
        .list_copilot_conversations()
        .await
        .map(|list| list.into_iter().map(|c| map_conversation(&c)).collect())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn copilot_create_conversation(
    state: State<'_, DesktopState>,
) -> Result<CopilotConversationResponse, String> {
    let handle = state.store_handle()?;
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    handle
        .create_copilot_conversation(&id, "新对话", &now)
        .await
        .map(|c| map_conversation(&c))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn copilot_delete_conversation(
    id: String,
    state: State<'_, DesktopState>,
) -> Result<(), String> {
    let handle = state.store_handle()?;
    handle
        .delete_copilot_conversation(&id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn copilot_load_conversation(
    id: String,
    state: State<'_, DesktopState>,
) -> Result<Vec<CopilotMessageResponse>, String> {
    let handle = state.store_handle()?;
    handle
        .list_copilot_messages(&id)
        .await
        .map(|msgs| msgs.iter().map(map_message).collect())
        .map_err(|e| e.to_string())
}
/// ADR-0029：需要两阶段确认的写入工具。
const WRITE_TOOLS: &[&str] = &[
    "save_device_asset",
    "delete_device_asset",
    "save_capability_asset",
    "add_device_signal",
    "remove_device_signal",
    "bind_device_connection",
    "patch_device_field",
];

/// 调度单个 Copilot 查询工具。
///
/// 只读工具直接执行。写入工具（`WRITE_TOOLS`）需要两阶段确认（ADR-0029）：
/// 1. 首次调用：校验参数、生成操作摘要、存储 pending action、返回 `pending_confirmation`
/// 2. 用户在前端点击确认后，前端调用 `copilot_confirm_action(token)` 完成写入
#[tauri::command]
pub(crate) async fn copilot_dispatch_tool(
    tool_name: String,
    arguments_json: String,
    workspace_path: Option<String>,
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
) -> Result<String, String> {
    // 组装运行时状态快照
    let (active_workflow_id, workflow_summaries) = {
        let active_id = state.active_workflow_id.lock().await.clone();
        let workflows = state.workflows.lock().await;
        let summaries: Vec<serde_json::Value> = workflows
            .values()
            .map(|w| {
                let is_active = active_id.as_ref().is_some_and(|id| w.workflow_id == *id);
                let s = w.summary(is_active);
                json!({
                    "workflow_id": s.workflow_id,
                    "node_count": s.node_count,
                    "edge_count": s.edge_count,
                    "active": s.active,
                    "deployed_at": s.deployed_at,
                })
            })
            .collect();
        (active_id, summaries)
    };

    // 写入工具的两阶段确认门控（ADR-0029）
    if WRITE_TOOLS.contains(&tool_name.as_str()) {
        let args: serde_json::Value =
            serde_json::from_str(&arguments_json).map_err(|e| format!("参数解析失败: {e}"))?;
        let confirmed = args
            .get("confirmed")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);

        if !confirmed {
            // 阶段 1：校验参数 → 生成摘要 → 存储 pending → 返回 pending_confirmation
            let summary = copilot_tools::generate_write_summary(&tool_name, &args)?;
            let token = Uuid::new_v4().to_string();

            state.pending_copilot_actions.insert(
                token.clone(),
                crate::state::PendingCopilotAction {
                    tool_name: tool_name.clone(),
                    arguments_json: arguments_json.clone(),
                    summary: summary.clone(),
                    created_at: Utc::now(),
                },
            );

            return Ok(serde_json::json!({
                "status": "pending_confirmation",
                "summary": summary,
                "token": token,
            })
            .to_string());
        }

        // confirmed == true：执行写入
        return copilot_tools::dispatch_query_tool(
            &tool_name,
            &arguments_json,
            &state.connection_manager,
            active_workflow_id.as_ref(),
            &workflow_summaries,
            workspace_path.as_ref(),
            &app,
        )
        .await;
    }

    // 只读工具直接执行
    copilot_tools::dispatch_query_tool(
        &tool_name,
        &arguments_json,
        &state.connection_manager,
        active_workflow_id.as_ref(),
        &workflow_summaries,
        workspace_path.as_ref(),
        &app,
    )
    .await
}

/// 确认并执行一个 pending 的 copilot 写入操作（ADR-0029）。
#[tauri::command]
pub(crate) async fn copilot_confirm_action(
    token: String,
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
) -> Result<String, String> {
    let (_key, action) = state
        .pending_copilot_actions
        .remove(&token)
        .ok_or_else(|| "确认令牌无效或已过期".to_owned())?;

    // TTL 检查：5 分钟
    let elapsed = Utc::now()
        .signed_duration_since(action.created_at)
        .num_seconds();
    if elapsed > 300 {
        return Err("确认令牌已过期（超过 5 分钟），请重新发起操作".to_owned());
    }

    // 执行写入
    let (active_workflow_id, workflow_summaries) = {
        let active_id = state.active_workflow_id.lock().await.clone();
        let workflows = state.workflows.lock().await;
        let summaries: Vec<serde_json::Value> = workflows
            .values()
            .map(|w| {
                let is_active = active_id.as_ref().is_some_and(|id| w.workflow_id == *id);
                let s = w.summary(is_active);
                json!({
                    "workflow_id": s.workflow_id,
                    "node_count": s.node_count,
                    "edge_count": s.edge_count,
                    "active": s.active,
                    "deployed_at": s.deployed_at,
                })
            })
            .collect();
        (active_id, summaries)
    };

    // 在参数中注入 confirmed: true 以跳过门控
    let mut args: serde_json::Value = serde_json::from_str(&action.arguments_json)
        .map_err(|e| format!("pending action 参数解析失败: {e}"))?;
    args.as_object_mut()
        .map(|obj| obj.insert("confirmed".to_owned(), serde_json::json!(true)));

    let result = copilot_tools::dispatch_query_tool(
        &action.tool_name,
        &serde_json::to_string(&args).map_err(|e| format!("参数序列化失败: {e}"))?,
        &state.connection_manager,
        active_workflow_id.as_ref(),
        &workflow_summaries,
        None,
        &app,
    )
    .await?;

    Ok(serde_json::json!({
        "status": "confirmed",
        "summary": action.summary,
        "result": result,
    })
    .to_string())
}

/// 取消一个 pending 的 copilot 写入操作（ADR-0029）。
#[tauri::command]
pub(crate) async fn copilot_cancel_action(
    token: String,
    state: State<'_, DesktopState>,
) -> Result<String, String> {
    let (_key, action) = state
        .pending_copilot_actions
        .remove(&token)
        .ok_or_else(|| "令牌无效或已过期".to_owned())?;
    Ok(format!("操作已取消：{}", action.summary))
}

/// 重命名 copilot 对话标题。
#[tauri::command]
pub(crate) async fn copilot_rename_conversation(
    id: String,
    title: String,
    state: State<'_, DesktopState>,
) -> Result<(), String> {
    let handle = state.store_handle()?;
    let now = Utc::now().to_rfc3339();
    handle
        .rename_copilot_conversation(&id, &title, &now)
        .await
        .map_err(|e| e.to_string())
}

/// 保存一条消息到 copilot 会话。
///
/// 前端直调 AI 时用于持久化用户消息和 AI 回复。
/// `thinking` 为助手消息携带的推理过程，多轮对话时必须回传给 API。
#[tauri::command]
pub(crate) async fn copilot_save_message(
    conversation_id: String,
    role: String,
    content: String,
    thinking: Option<String>,
    state: State<'_, DesktopState>,
) -> Result<(), String> {
    let handle = state.store_handle()?;
    let msg_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    handle
        .append_copilot_message(
            &conversation_id,
            &msg_id,
            &role,
            &content,
            thinking.as_deref(),
            &now,
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}
