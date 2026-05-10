//! Copilot 对话式副驾驶 IPC 命令。

use std::sync::Arc;

use chrono::Utc;
use nazh_engine::{AiCompletionRequest, AiMessage, AiMessageRole, AiService};
use tauri::{Emitter, State};
use tauri_bindings::{CopilotConversationResponse, CopilotMessageResponse};
use uuid::Uuid;

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

/// 发送用户消息并流式获取 AI 回复。返回 streamId，前端通过
/// `copilot://stream/{streamId}` 监听流式事件。
#[tauri::command]
pub(crate) async fn copilot_chat(
    conversation_id: String,
    user_message: String,
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
) -> Result<String, String> {
    let handle = state.store_handle()?;
    let now = Utc::now().to_rfc3339();

    // 持久化用户消息
    let user_msg_id = Uuid::new_v4().to_string();
    handle
        .append_copilot_message(&conversation_id, &user_msg_id, "user", &user_message, &now)
        .await
        .map_err(|e| e.to_string())?;

    // 加载历史消息构建上下文
    let history = handle
        .list_copilot_messages(&conversation_id)
        .await
        .map_err(|e| e.to_string())?;

    // 取最近 20 条（含刚插入的用户消息）组装 AI 请求
    let recent: Vec<_> = history.iter().rev().take(20).collect::<Vec<_>>().into_iter().rev().collect();
    let messages: Vec<AiMessage> = recent
        .iter()
        .map(|m| AiMessage {
            role: match m.role.as_str() {
                "user" => AiMessageRole::User,
                "assistant" => AiMessageRole::Assistant,
                _ => AiMessageRole::System,
            },
            content: m.content.clone(),
        })
        .collect();

    // 从 AI 配置中解析活跃提供商
    let provider_id = state
        .ai_config
        .read()
        .await
        .active_provider_id
        .clone()
        .ok_or_else(|| "未配置 AI 提供商，请先在设置中配置并激活一个提供商".to_owned())?;

    let stream_id = Uuid::new_v4().to_string();
    let event_name = format!("copilot://stream/{stream_id}");

    let service = Arc::clone(&state.ai_service);
    let handle_clone = handle;
    let conv_id = conversation_id.clone();

    tokio::spawn(async move {
        let request = AiCompletionRequest {
            provider_id,
            model: None,
            messages,
            params: nazh_engine::AiGenerationParams::default(),
            timeout_ms: None,
        };

        let rx_result = service.stream_complete(request).await;
        let mut rx = match rx_result {
            Ok(rx) => rx,
            Err(error) => {
                let _ = app.emit(
                    &event_name,
                    serde_json::json!({ "error": error.to_string(), "done": true }),
                );
                return;
            }
        };

        let mut accumulated = String::new();
        while let Some(chunk_result) = rx.recv().await {
            match chunk_result {
                Ok(chunk) => {
                    if !chunk.delta.is_empty() {
                        accumulated.push_str(&chunk.delta);
                    }
                    let is_done = chunk.done;
                    let payload: serde_json::Value =
                        serde_json::to_value(&chunk).unwrap_or_default();
                    let _ = app.emit(&event_name, payload);
                    if is_done {
                        break;
                    }
                }
                Err(error) => {
                    let _ = app.emit(
                        &event_name,
                        serde_json::json!({ "error": error.to_string(), "done": true }),
                    );
                    return;
                }
            }
        }

        // 流式完成后，持久化 AI 回复
        if !accumulated.is_empty() {
            let msg_id = Uuid::new_v4().to_string();
            let now = Utc::now().to_rfc3339();
            if let Err(error) = handle_clone
                .append_copilot_message(&conv_id, &msg_id, "assistant", &accumulated, &now)
                .await
            {
                tracing::error!(?error, "持久化 copilot AI 回复失败");
            }
        }
    });

    Ok(stream_id)
}
