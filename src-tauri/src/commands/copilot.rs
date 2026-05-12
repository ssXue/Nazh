//! Copilot 对话式副驾驶 IPC 命令。

use chrono::Utc;
use serde::Deserialize;
use serde_json::json;
use store::AssetEmbedding;
use tauri::State;
use tauri_bindings::{CopilotConversationResponse, CopilotMessageResponse};
use uuid::Uuid;

use crate::commands::copilot_tools;
use crate::state::DesktopState;

fn map_conversation(
    c: &store::CopilotConversation,
) -> CopilotConversationResponse {
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

/// 单条前端预计算的 embedding 输入。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EmbeddingInput {
    asset_type: String,
    asset_id: String,
    chunk_index: i32,
    chunk_text: String,
    embedding: Vec<f32>,
    model: String,
}

/// 清除全部 asset embedding 索引。
#[tauri::command]
pub(crate) async fn copilot_clear_embeddings(
    state: State<'_, DesktopState>,
) -> Result<(), String> {
    let handle = state.store_handle()?;
    handle
        .delete_all_asset_embeddings()
        .await
        .map_err(|e| e.to_string())
}

/// 批量写入前端预计算的 embedding 向量。
///
/// 前端使用 AI SDK `embed()` / `embedMany()` 生成向量后调用此接口持久化。
#[tauri::command]
#[allow(clippy::cast_possible_wrap)]
pub(crate) async fn copilot_store_embeddings(
    embeddings: Vec<EmbeddingInput>,
    state: State<'_, DesktopState>,
) -> Result<serde_json::Value, String> {
    let handle = state.store_handle()?;
    let now = Utc::now().to_rfc3339();
    let count = embeddings.len();

    for emb in embeddings {
        let record = AssetEmbedding {
            id: Uuid::new_v4().to_string(),
            asset_type: emb.asset_type,
            asset_id: emb.asset_id,
            chunk_index: emb.chunk_index,
            chunk_text: emb.chunk_text,
            embedding: emb.embedding,
            model: emb.model,
            updated_at: now.clone(),
        };
        handle
            .upsert_asset_embedding(record)
            .await
            .map_err(|e| format!("写入 embedding 失败: {e}"))?;
    }

    Ok(json!({ "stored": count }))
}

/// 调度单个 Copilot 查询工具。
///
/// 仅处理只读查询工具（如 `query_node_catalog`、`search_devices` 等），
/// 画布操作工具（`create_workflow`、`add_workflow_node` 等）由前端直接执行。
/// 返回工具执行结果的 JSON 字符串。
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

/// 返回 Copilot 工具定义列表（前端用于构造 AI 请求的 tools 参数）。
#[tauri::command]
pub(crate) fn copilot_get_tool_definitions() -> Vec<serde_json::Value> {
    copilot_tools::all_copilot_tools()
        .into_iter()
        .map(|def| {
            json!({
                "name": def.name,
                "description": def.description,
                "parameters": def.parameters,
            })
        })
        .collect()
}
