use std::time::Duration;

use nazh_core::ai::{AiError, StreamChunk};
use tokio::sync::mpsc;

use super::protocol::{ChatMessagePayload, StreamApiResponse, build_chat_payload};
use super::response::{parse_api_error, response_preview};
use super::types::StreamRequestContext;

pub(super) enum StreamEventAction {
    Ignore,
    Done,
    Chunk(StreamChunk),
}

pub(super) fn parse_stream_event_data(data: &str) -> Result<StreamEventAction, AiError> {
    if data == "[DONE]" {
        return Ok(StreamEventAction::Done);
    }

    let parsed: StreamApiResponse = serde_json::from_str(data).map_err(|error| {
        AiError::ResponseParseError(format!(
            "流式事件 JSON 解析失败: {error}；事件预览: {}",
            response_preview(data)
        ))
    })?;

    let Some(choice) = parsed.choices.first() else {
        return Ok(StreamEventAction::Ignore);
    };

    let content_delta = choice.delta.content.clone().unwrap_or_default();
    let thinking_delta = choice.delta.thinking.clone();
    let has_content = !content_delta.is_empty();
    let has_thinking = thinking_delta
        .as_ref()
        .is_some_and(|value| !value.is_empty());
    let is_done = choice
        .finish_reason
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());
    let finish_reason = choice
        .finish_reason
        .clone()
        .filter(|value| !value.trim().is_empty());

    if !has_content && !has_thinking && !is_done {
        return Ok(StreamEventAction::Ignore);
    }

    Ok(StreamEventAction::Chunk(StreamChunk {
        delta: content_delta,
        thinking: has_thinking.then_some(thinking_delta).flatten(),
        finish_reason,
        done: is_done,
    }))
}

/// 发送流式 HTTP 请求并返回 SSE Response。
async fn send_stream_request(
    ctx: &StreamRequestContext,
    messages: Vec<ChatMessagePayload>,
) -> Result<reqwest::Response, AiError> {
    let body = build_chat_payload(
        ctx.model.clone(),
        messages,
        &ctx.params,
        true,
        ctx.include_deepseek_options,
    );

    let mut builder = ctx
        .http
        .post(&ctx.url)
        .bearer_auth(&ctx.api_key)
        .json(&body);

    for (key, value) in &ctx.extra_headers {
        builder = builder.header(key.as_str(), value.as_str());
    }

    let response = tokio::time::timeout(Duration::from_millis(ctx.timeout_ms), builder.send())
        .await
        .map_err(|_| AiError::RequestTimeout(ctx.timeout_ms))?
        .map_err(|error| AiError::NetworkError(error.to_string()))?;

    let status = response.status().as_u16();
    if !response.status().is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(parse_api_error(status, &body));
    }

    Ok(response)
}

/// 流式 chat completion，逐 chunk 通过 channel 返回。
pub(super) async fn open_stream(
    ctx: StreamRequestContext,
    messages: Vec<ChatMessagePayload>,
) -> Result<mpsc::Receiver<Result<StreamChunk, AiError>>, AiError> {
    let response = send_stream_request(&ctx, messages).await?;

    let (tx, rx) = mpsc::channel(32);

    tokio::spawn(async move {
        use futures_util::StreamExt;

        let mut stream = sseer::response_to_stream(response);

        while let Some(event_result) = stream.next().await {
            let event = match event_result {
                Ok(event) => event,
                Err(error) => {
                    let _ = tx
                        .send(Err(AiError::NetworkError(format!(
                            "AI 流式响应读取失败: {error}"
                        ))))
                        .await;
                    return;
                }
            };

            match parse_stream_event_data(&event.data) {
                Ok(StreamEventAction::Ignore) => {}
                Ok(StreamEventAction::Done) => {
                    let _ = tx
                        .send(Ok(StreamChunk {
                            delta: String::new(),
                            thinking: None,
                            finish_reason: None,
                            done: true,
                        }))
                        .await;
                    return;
                }
                Ok(StreamEventAction::Chunk(chunk)) => {
                    let is_done = chunk.done;
                    if tx.send(Ok(chunk)).await.is_err() || is_done {
                        return;
                    }
                }
                Err(error) => {
                    let _ = tx.send(Err(error)).await;
                    return;
                }
            }
        }

        let _ = tx
            .send(Err(AiError::NetworkError(
                "AI 流式输出意外中断，未收到结束信号".to_owned(),
            )))
            .await;
    });

    Ok(rx)
}
