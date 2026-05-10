//! Copilot 对话式副驾驶 IPC 命令。

use std::sync::Arc;

use chrono::Utc;
use nazh_engine::{
    AiCompletionRequest, AiEmbeddingRequest, AiGenerationParams, AiMessage, AiMessageRole,
    AiService,
};
use serde_json::json;
use store::AssetEmbedding;
use tauri::{Emitter, State};
use tauri_bindings::{CopilotConversationResponse, CopilotMessageResponse};
use uuid::Uuid;

use ai::OpenAiCompatibleService;

use crate::commands::copilot_tools::{self, CopilotToolCtx};
use crate::state::DesktopState;

const MAX_TOOL_ROUNDS: u32 = 10;
const COPILOT_HISTORY_LIMIT: usize = 20;

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

/// 发送用户消息并流式获取 AI 回复（支持多轮工具调用）。
///
/// 返回 streamId，前端通过 `copilot://stream/{streamId}` 监听流式事件。
/// 当 `agent_settings.tool_calling_enabled` 为 true 时，AI 可调用引擎工具。
#[tauri::command]
#[allow(clippy::too_many_lines)]
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

    let recent: Vec<_> = history
        .iter()
        .rev()
        .take(COPILOT_HISTORY_LIMIT)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    let messages: Vec<AiMessage> = recent
        .iter()
        .map(|m| {
            AiMessage::simple(
                match m.role.as_str() {
                    "user" => AiMessageRole::User,
                    "assistant" => AiMessageRole::Assistant,
                    _ => AiMessageRole::System,
                },
                m.content.clone(),
            )
        })
        .collect();

    // 从 AI 配置中解析活跃提供商和工具调用开关
    let (provider_id, tool_calling_enabled, rag_enabled, user_system_prompt) = {
        let config = state.ai_config.read().await;
        let pid = config
            .active_provider_id
            .clone()
            .ok_or_else(|| "未配置 AI 提供商，请先在设置中配置并激活一个提供商".to_owned())?;
        (
            pid,
            config.agent_settings.tool_calling_enabled,
            config.agent_settings.rag_enabled,
            config.agent_settings.system_prompt.clone(),
        )
    };

    let stream_id = Uuid::new_v4().to_string();
    let event_name = format!("copilot://stream/{stream_id}");

    // 注册流取消标志
    let cancel_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    state
        .copilot_streams
        .insert(stream_id.clone(), Arc::clone(&cancel_flag));

    let service = Arc::clone(&state.ai_service);
    let handle_clone = handle;
    let conv_id = conversation_id.clone();
    let streams_registry = state.copilot_streams.clone();
    let stream_id_for_spawn = stream_id.clone();

    // 组装工具上下文（提前读取运行时状态快照）
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

    let rag_provider_id = provider_id.clone();

    let tool_ctx = Arc::new(CopilotToolCtx {
        connection_manager: state.connection_manager.clone(),
        workflow_summaries,
        active_workflow_id,
        stream_event_name: event_name.clone(),
        app: app.clone(),
    });

    tokio::spawn(async move {
        let mut messages = messages;

        // 注入系统提示（内置角色说明 + 用户自定义提示）
        inject_system_prompt(&mut messages, tool_calling_enabled, user_system_prompt.as_deref());

        tracing::info!(
            stream_id = %stream_id_for_spawn,
            msg_count = messages.len(),
            tool_calling_enabled,
            "copilot 流开始"
        );

        // RAG 上下文注入
        if rag_enabled {
            inject_rag_context(&service, &handle_clone, &mut messages, &rag_provider_id).await;
        }

        // 不向模型传递 tools 参数——让模型通过 JSON Lines 协议输出画布操作。
        // 旧 AI 编排模式证明：不传 tools 时模型才会严格遵循 JSON Lines 协议；
        // 传 tools 后模型倾向于调用工具或输出文本，导致协议失效。
        let tools: Vec<nazh_engine::AiToolDefinition> = vec![];

        let mut accumulated = String::new();

        for round in 1..=MAX_TOOL_ROUNDS {
            // 每轮开头检查取消
            if cancel_flag.load(std::sync::atomic::Ordering::Relaxed) {
                tracing::info!(round, stream_id = %stream_id_for_spawn, "copilot 流被用户取消");
                emit_error(&app, &event_name, "用户已取消生成");
                return;
            }

            tracing::info!(round, stream_id = %stream_id_for_spawn, "copilot 开始第 {round} 轮推理");

            let request = AiCompletionRequest {
                provider_id: provider_id.clone(),
                model: None,
                messages: messages.clone(),
                params: AiGenerationParams::default(),
                timeout_ms: None,
                tools: tools.clone(),
            };

            let rx_result = service.stream_complete(request).await;
            let mut rx = match rx_result {
                Ok(rx) => rx,
                Err(error) => {
                    tracing::error!(?error, stream_id = %stream_id_for_spawn, "copilot stream_complete 失败");
                    emit_error(&app, &event_name, &error.to_string());
                    return;
                }
            };

            let mut text_buf = String::new();
            let mut tool_calls_buf: Vec<nazh_engine::AiToolCall> = Vec::new();
            let mut finish_reason: Option<String> = None;
            let mut chunk_count: u32 = 0;

            while let Some(chunk_result) = rx.recv().await {
                // 检查取消
                if cancel_flag.load(std::sync::atomic::Ordering::Relaxed) {
                    tracing::info!(round, chunk_count, stream_id = %stream_id_for_spawn, "copilot 流在 chunk 接收中被取消");
                    emit_error(&app, &event_name, "用户已取消生成");
                    return;
                }

                match chunk_result {
                    Ok(chunk) => {
                        chunk_count += 1;
                        let delta_len = chunk.delta.len();
                        if !chunk.delta.is_empty() {
                            text_buf.push_str(&chunk.delta);
                        }
                        if let Some(tc) = &chunk.tool_calls {
                            tool_calls_buf.extend(tc.clone());
                        }
                        if let Some(reason) = &chunk.finish_reason {
                            finish_reason = Some(reason.clone());
                        }

                        // 推送给前端（文本 delta + thinking）
                        let payload: serde_json::Value =
                            serde_json::to_value(&chunk).unwrap_or_default();
                        let _ = app.emit(&event_name, payload);

                        // 每 50 个 chunk 打一次进度日志
                        if chunk_count % 50 == 0 {
                            tracing::debug!(
                                round,
                                chunk_count,
                                delta_len,
                                text_buf_len = text_buf.len(),
                                stream_id = %stream_id_for_spawn,
                                "copilot 流进度"
                            );
                        }
                    }
                    Err(error) => {
                        tracing::error!(
                            ?error,
                            round,
                            chunk_count,
                            text_buf_len = text_buf.len(),
                            stream_id = %stream_id_for_spawn,
                            "copilot chunk 错误"
                        );
                        emit_error(&app, &event_name, &error.to_string());
                        return;
                    }
                }
            }

            tracing::info!(
                round,
                chunk_count,
                text_buf_len = text_buf.len(),
                ?finish_reason,
                tool_calls = tool_calls_buf.len(),
                stream_id = %stream_id_for_spawn,
                "copilot 第 {round} 轮流结束"
            );

            // 非工具调用结束 → 正常完成
            if finish_reason.as_deref() != Some("tool_calls") || tool_calls_buf.is_empty() {
                accumulated = text_buf;
                tracing::info!(
                    accumulated_len = accumulated.len(),
                    stream_id = %stream_id_for_spawn,
                    "copilot 流正常完成，accumulated 全文：\n{}",
                    &accumulated
                );
                break;
            }

            // 工具调用 → 推送通知，执行工具，追加消息，继续循环
            tracing::info!(
                round,
                tool_count = tool_calls_buf.len(),
                "copilot 工具调用循环"
            );

            let _ = app.emit(
                &event_name,
                json!({ "toolCalls": tool_calls_buf, "toolCallRound": round }),
            );

            // 追加助手消息（含 tool_calls）
            messages.push(AiMessage {
                role: AiMessageRole::Assistant,
                content: text_buf.clone(),
                tool_calls: Some(tool_calls_buf.clone()),
                tool_call_id: None,
            });

            // 逐个执行工具
            for call in &tool_calls_buf {
                tracing::info!(
                    tool = %call.name,
                    call_id = %call.id,
                    "copilot 执行工具"
                );
                let result = copilot_tools::dispatch_tool(call, &tool_ctx).await;

                tracing::info!(
                    tool = %call.name,
                    is_error = result.is_error,
                    content_len = result.content.len(),
                    "copilot 工具执行完成"
                );

                let _ = app.emit(
                    &event_name,
                    json!({
                        "toolResult": {
                            "toolCallId": result.tool_call_id,
                            "name": call.name,
                            "isError": result.is_error,
                            "contentPreview": result.content.chars().take(200).collect::<String>(),
                        }
                    }),
                );

                messages.push(AiMessage {
                    role: AiMessageRole::Tool,
                    content: result.content,
                    tool_calls: None,
                    tool_call_id: Some(result.tool_call_id),
                });
            }

            if round == MAX_TOOL_ROUNDS {
                emit_error(&app, &event_name, "工具调用超过最大循环次数");
                return;
            }
        }

        // 持久化最终 AI 回复
        if !accumulated.is_empty() {
            let msg_id = Uuid::new_v4().to_string();
            let now = Utc::now().to_rfc3339();
            tracing::info!(
                msg_id = %msg_id,
                accumulated_len = accumulated.len(),
                stream_id = %stream_id_for_spawn,
                "copilot 持久化 AI 回复"
            );
            if let Err(error) = handle_clone
                .append_copilot_message(&conv_id, &msg_id, "assistant", &accumulated, &now)
                .await
            {
                tracing::error!(?error, "持久化 copilot AI 回复失败");
            }
        }

        // 流结束，从注册表移除
        streams_registry.remove(&stream_id_for_spawn);
        tracing::info!(stream_id = %stream_id_for_spawn, "copilot 流结束，注册表移除");
    });

    Ok(stream_id)
}

/// 内置系统提示模板。
const BUILTIN_SYSTEM_PROMPT: &str = "\
你是 Nazh 工业边缘平台的对话式副驾驶。Nazh 是一个本地运行的工业边缘工作流编排引擎，\
集成了设备数据采集、协议适配（Modbus、MQTT、串口、CAN/EtherCAT）、数据变换、脚本逻辑（Rhai）、\
AI 辅助和桌面运维 UI。

你的职责是帮助用户完成以下任务：
- 查询和解释工作流节点类型、设备资产、能力资产
- 解答 Nazh 平台的使用问题和工作流设计建议
- 根据用户描述创建工作流

回答时请遵循：
1. 用中文回答
2. 结合 Nazh 平台上下文作答，不要泛泛而谈
3. 使用 Markdown 格式回复，代码块用对应的语言标记";

/// 在消息列表头部注入系统提示。
fn inject_system_prompt(
    messages: &mut Vec<AiMessage>,
    tool_calling_enabled: bool,
    user_prompt: Option<&str>,
) {
    let mut parts = vec![BUILTIN_SYSTEM_PROMPT.to_owned()];

    if tool_calling_enabled {
        let node_catalog = copilot_tools::build_node_catalog_text();
        parts.push(format!(
            "\n\n## 可用节点类型目录\n\n\
             {node_catalog}\n\n\
             ## 工作流操作协议\n\n\
             当用户要求创建工作流、添加节点、创建连线时，你必须使用 JSON Lines 操作协议：\n\
             - 只输出 JSON Lines，每行一个 JSON 对象，不要输出任何其他文字\n\
             - 不要输出 Markdown、代码块、解释文字、序号或注释\n\
             - 先输出 project，再输出 create_node，再输出 create_edge，最后输出 done\n\
             - done 的 summary 字段用来说明你做了什么\n\n\
             操作格式：\n\
             {{\"type\":\"project\",\"name\":\"工程名\"}}\n\
             {{\"type\":\"create_node\",\"ref\":\"timer\",\"nodeType\":\"timer\",\"label\":\"定时触发\",\"config\":{{\"interval_ms\":5000}}}}\n\
             {{\"type\":\"create_node\",\"ref\":\"debug\",\"nodeType\":\"debugConsole\",\"label\":\"调试输出\"}}\n\
             {{\"type\":\"create_edge\",\"fromRef\":\"timer\",\"toRef\":\"debug\"}}\n\
             {{\"type\":\"done\",\"summary\":\"已创建 timer→debugConsole 工作流\"}}\n\n\
             规则：\n\
             - 节点类型（nodeType）只能从上面的目录中选择，不要编造不存在的类型\n\
             - ref 是本次输出流内稳定的简短英文别名（如 timer、debug、modbus），不是系统 node id\n\
             - connectionId 仅在用户明确给出可复用连接 ID 时才填写，否则省略\n\
             - 对于工业场景，优先从最小可运行链路开始\n\n\
             ## 回答模式\n\n\
             如果用户只是在提问（不涉及创建/修改工作流），用 Markdown 正常回答。\n\
             一旦涉及创建工作流或添加节点，立即切换到 JSON Lines 模式，只输出 JSON Lines。"
        ));
    }

    if let Some(extra) = user_prompt.filter(|s| !s.trim().is_empty()) {
        parts.push(format!("\n\n用户补充指令：{extra}"));
    }

    let system_message = AiMessage::simple(AiMessageRole::System, parts.concat());
    messages.insert(0, system_message);
}

/// 注入 RAG 上下文到消息列表头部。
async fn inject_rag_context(
    service: &Arc<OpenAiCompatibleService>,
    handle: &store::StoreHandle,
    messages: &mut Vec<AiMessage>,
    provider_id: &str,
) {
    // 取最近一条用户消息作为查询
    let query = messages
        .iter()
        .rev()
        .find(|m| m.role == AiMessageRole::User)
        .map(|m| m.content.clone())
        .unwrap_or_default();

    if query.is_empty() {
        return;
    }

    // 尝试嵌入查询
    let embedding_result = service
        .embed(nazh_engine::AiEmbeddingRequest {
            provider_id: provider_id.to_owned(),
            model: None,
            input: vec![query],
            timeout_ms: None,
        })
        .await;

    let Ok(embedding_response) = embedding_result else {
        tracing::debug!("RAG embedding 失败，跳过上下文注入");
        return;
    };

    let Some(query_embedding) = embedding_response.embeddings.first() else {
        return;
    };

    // 检索相似资产片段
    let search_result = handle
        .search_similar(query_embedding.clone(), None, 5)
        .await;

    let Ok(results) = search_result else {
        tracing::debug!("RAG 检索失败，跳过上下文注入");
        return;
    };

    if results.is_empty() {
        return;
    }

    let context_text: String = results
        .iter()
        .map(|r| format!("[{} / {}] {}", r.asset_type, r.asset_id, r.chunk_text))
        .collect::<Vec<_>>()
        .join("\n\n");

    let rag_message = AiMessage::simple(
        AiMessageRole::System,
        format!("相关项目上下文（供参考，非用户消息）：\n\n{context_text}"),
    );

    messages.insert(0, rag_message);
}

/// 发送错误事件到前端。
fn emit_error(app: &tauri::AppHandle, event_name: &str, message: &str) {
    let _ = app.emit(event_name, json!({ "error": message, "done": true }));
}

/// 取消正在进行的 copilot 流式生成。
#[tauri::command]
pub(crate) async fn copilot_cancel_stream(
    stream_id: String,
    state: State<'_, DesktopState>,
) -> Result<bool, String> {
    tracing::info!(stream_id = %stream_id, "copilot_cancel_stream 收到取消请求");
    if let Some(flag) = state.copilot_streams.get(&stream_id) {
        flag.store(true, std::sync::atomic::Ordering::Relaxed);
        tracing::info!(stream_id = %stream_id, "copilot 取消标志已设置");
        Ok(true)
    } else {
        tracing::info!(stream_id = %stream_id, "copilot 流已结束，无需取消");
        Ok(false)
    }
}

/// 分块参数。
const CHUNK_SIZE: usize = 500;
const CHUNK_OVERLAP: usize = 100;
const EMBED_BATCH_SIZE: usize = 20;

/// 将滑动窗口应用于文本，返回 `(chunk_text, chunk_index)` 对。
fn chunk_text(text: &str) -> Vec<(String, usize)> {
    if text.len() <= CHUNK_SIZE {
        return vec![(text.to_owned(), 0)];
    }

    let step = CHUNK_SIZE - CHUNK_OVERLAP;
    let mut chunks = Vec::new();
    let mut start = 0;
    let mut idx: usize = 0;

    while start < text.len() {
        let end = (start + CHUNK_SIZE).min(text.len());
        chunks.push((text[start..end].to_owned(), idx));
        start += step;
        idx += 1;
        if end == text.len() {
            break;
        }
    }

    chunks
}

/// 待嵌入的分块。
struct PendingChunk {
    asset_type: String,
    asset_id: String,
    chunk_text: String,
    chunk_index: i32,
}

/// 索引所有设备与能力资产到 embedding 向量库。
///
/// 返回索引的资产数（去重）和总分块数。
#[tauri::command]
#[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
pub(crate) async fn copilot_index_assets(
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
) -> Result<serde_json::Value, String> {
    use crate::commands::ai::load_ai_asset_context;

    let ctx = load_ai_asset_context(app, None).await?;

    let provider_id = {
        let config = state.ai_config.read().await;
        config
            .active_provider_id
            .clone()
            .ok_or_else(|| "未配置 AI 提供商，请先在设置中配置并激活一个提供商".to_owned())?
    };

    let handle = state.store_handle()?;

    let mut pending: Vec<PendingChunk> = Vec::new();
    let mut asset_count: usize = 0;

    for d in &ctx.devices {
        asset_count += 1;
        for (text, idx) in chunk_text(&d.yaml) {
            pending.push(PendingChunk {
                asset_type: "device".to_owned(),
                asset_id: d.id.clone(),
                chunk_text: text,
                chunk_index: idx as i32,
            });
        }
    }

    for c in &ctx.capabilities {
        asset_count += 1;
        for (text, idx) in chunk_text(&c.yaml) {
            pending.push(PendingChunk {
                asset_type: "capability".to_owned(),
                asset_id: c.id.clone(),
                chunk_text: text,
                chunk_index: idx as i32,
            });
        }
    }

    let total_chunks = pending.len();

    // 先清除旧索引
    handle.delete_all_asset_embeddings().await.map_err(|e| e.to_string())?;

    // 批量嵌入并写入
    let service = Arc::clone(&state.ai_service);
    let now = Utc::now().to_rfc3339();

    for batch in pending.chunks(EMBED_BATCH_SIZE) {
        let texts: Vec<String> = batch.iter().map(|c| c.chunk_text.clone()).collect();

        let response = service
            .embed(AiEmbeddingRequest {
                provider_id: provider_id.clone(),
                model: None,
                input: texts,
                timeout_ms: None,
            })
            .await
            .map_err(|e| format!("嵌入失败: {e}"))?;

        for (i, embedding) in response.embeddings.iter().enumerate() {
            let chunk = &batch[i];
            let record = AssetEmbedding {
                id: Uuid::new_v4().to_string(),
                asset_type: chunk.asset_type.clone(),
                asset_id: chunk.asset_id.clone(),
                chunk_index: chunk.chunk_index,
                chunk_text: chunk.chunk_text.clone(),
                embedding: embedding.clone(),
                model: response.model.clone(),
                updated_at: now.clone(),
            };
            handle
                .upsert_asset_embedding(record)
                .await
                .map_err(|e| format!("写入 embedding 失败: {e}"))?;
        }
    }

    Ok(json!({
        "assetCount": asset_count,
        "totalChunks": total_chunks,
    }))
}
