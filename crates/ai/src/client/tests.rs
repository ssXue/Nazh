use std::collections::HashMap;
use std::sync::Arc;

use nazh_core::ai::{
    AiCompletionRequest, AiGenerationParams, AiMessage, AiMessageRole, AiReasoningEffort,
    AiService, AiThinkingConfig, AiThinkingMode,
};
use tokio::sync::RwLock;

use super::OpenAiCompatibleService;
use super::protocol::{ChatMessagePayload, build_chat_payload};
use super::provider_policy::{TEST_MAX_TOKENS, build_connection_test_params};
use super::types::ResolvedProvider;
use crate::config::{AiAgentSettings, AiConfigFile, AiProviderSecretRecord};

fn test_provider(base_url: &str, default_model: &str) -> ResolvedProvider {
    ResolvedProvider {
        base_url: base_url.to_owned(),
        api_key: "sk-test".to_owned(),
        default_model: default_model.to_owned(),
        extra_headers: HashMap::new(),
    }
}

fn test_messages() -> Vec<ChatMessagePayload> {
    vec![ChatMessagePayload {
        role: "user".to_owned(),
        content: "Hi".to_owned(),
    }]
}

#[test]
fn deepseek_payload_sends_thinking_options_and_omits_sampling_when_enabled() {
    let provider = test_provider("https://api.deepseek.com", "deepseek-v4-pro");
    let params = AiGenerationParams {
        temperature: Some(0.8),
        max_tokens: Some(256),
        top_p: Some(0.9),
        thinking: Some(AiThinkingConfig {
            kind: AiThinkingMode::Enabled,
        }),
        reasoning_effort: Some(AiReasoningEffort::Max),
    };

    let payload = build_chat_payload(
        provider.default_model.clone(),
        test_messages(),
        &params,
        false,
        true,
    );
    let Ok(json) = serde_json::to_value(payload) else {
        panic!("payload serializes");
    };

    assert_eq!(json["model"], "deepseek-v4-pro");
    assert_eq!(json["thinking"]["type"], "enabled");
    assert_eq!(json["reasoning_effort"], "max");
    assert_eq!(json["max_tokens"], 256);
    assert!(json.get("temperature").is_none());
    assert!(json.get("top_p").is_none());
}

#[test]
fn non_deepseek_payload_omits_deepseek_specific_options() {
    let provider = test_provider("https://api.openai.com/v1", "gpt-4o-mini");
    let params = AiGenerationParams {
        temperature: Some(0.3),
        max_tokens: Some(128),
        top_p: Some(0.8),
        thinking: Some(AiThinkingConfig {
            kind: AiThinkingMode::Enabled,
        }),
        reasoning_effort: Some(AiReasoningEffort::High),
    };

    let payload = build_chat_payload(
        provider.default_model.clone(),
        test_messages(),
        &params,
        false,
        false,
    );
    let Ok(json) = serde_json::to_value(payload) else {
        panic!("payload serializes");
    };

    assert!(json.get("thinking").is_none());
    assert!(json.get("reasoning_effort").is_none());
    assert!((json["temperature"].as_f64().unwrap_or_default() - 0.3).abs() < 0.001);
    assert!((json["top_p"].as_f64().unwrap_or_default() - 0.8).abs() < 0.001);
}

#[tokio::test]
async fn stream_invalid_json_event_propagates_parse_error() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let listener = match TcpListener::bind("127.0.0.1:0").await {
        Ok(listener) => listener,
        Err(error) => panic!("绑定本地 SSE 测试服务失败: {error}"),
    };
    let local_addr = match listener.local_addr() {
        Ok(addr) => addr,
        Err(error) => panic!("读取本地 SSE 测试地址失败: {error}"),
    };
    let base_url = format!("http://{local_addr}");

    let server = tokio::spawn(async move {
        let (mut socket, _) = match listener.accept().await {
            Ok(accepted) => accepted,
            Err(error) => panic!("接收 SSE 测试请求失败: {error}"),
        };
        let mut buffer = [0_u8; 1024];
        if let Err(error) = socket.read(&mut buffer).await {
            panic!("读取 SSE 测试请求失败: {error}");
        }
        if let Err(error) = socket
            .write_all(
                concat!(
                    "HTTP/1.1 200 OK\r\n",
                    "content-type: text/event-stream\r\n",
                    "\r\n",
                    "data: {not-json}\n\n"
                )
                .as_bytes(),
            )
            .await
        {
            panic!("写入 SSE 测试响应失败: {error}");
        }
    });

    let config = AiConfigFile {
        version: 1,
        providers: vec![AiProviderSecretRecord {
            id: "local".to_owned(),
            name: "本地测试".to_owned(),
            base_url,
            api_key: "sk-test".to_owned(),
            default_model: "gpt-test".to_owned(),
            extra_headers: HashMap::new(),
            enabled: true,
        }],
        active_provider_id: Some("local".to_owned()),
        copilot_params: AiGenerationParams::default(),
        agent_settings: AiAgentSettings::default(),
    };
    let service = OpenAiCompatibleService::new(Arc::new(RwLock::new(config)));
    let stream_result = service
        .stream_complete(AiCompletionRequest {
            provider_id: "local".to_owned(),
            model: None,
            messages: vec![AiMessage {
                role: AiMessageRole::User,
                content: "Hi".to_owned(),
            }],
            params: AiGenerationParams::default(),
            timeout_ms: Some(1_000),
        })
        .await;
    let Ok(mut rx) = stream_result else {
        panic!("创建 stream receiver 失败: {stream_result:?}");
    };

    let Some(result) = rx.recv().await else {
        panic!("stream should yield parse error");
    };
    let Err(error) = result else {
        panic!("stream should return error, got {result:?}");
    };
    assert!(
        matches!(error, nazh_core::ai::AiError::ResponseParseError(_)),
        "invalid JSON should propagate parse error, got {error:?}"
    );
    assert!(
        error.to_string().contains("{not-json}"),
        "parse error should include event preview, got {error}"
    );

    if let Err(error) = server.await {
        panic!("SSE 测试服务异常结束: {error}");
    }
}

#[test]
fn deepseek_connection_test_disables_thinking_for_lightweight_probe() {
    let provider = test_provider("https://api.deepseek.com", "deepseek-v4-flash");
    let thinking_enabled = true;
    let params = build_connection_test_params(thinking_enabled);
    let payload = build_chat_payload(
        provider.default_model.clone(),
        test_messages(),
        &params,
        false,
        thinking_enabled,
    );
    let Ok(json) = serde_json::to_value(payload) else {
        panic!("payload serializes");
    };

    assert_eq!(json["thinking"]["type"], "disabled");
    assert_eq!(json["temperature"], 0.0);
    assert_eq!(json["max_tokens"], TEST_MAX_TOKENS);
}

#[tokio::test]
async fn resolve_provider_snapshot_keeps_provider_and_agent_settings_atomic() {
    let config = AiConfigFile {
        version: 1,
        providers: vec![AiProviderSecretRecord {
            id: "p1".to_owned(),
            name: "测试提供商".to_owned(),
            base_url: "https://api.deepseek.com".to_owned(),
            api_key: "sk-test".to_owned(),
            default_model: "deepseek-chat".to_owned(),
            extra_headers: HashMap::new(),
            enabled: true,
        }],
        active_provider_id: Some("p1".to_owned()),
        copilot_params: AiGenerationParams::default(),
        agent_settings: AiAgentSettings {
            system_prompt: None,
            timeout_ms: None,
            thinking_enabled: true,
        },
    };
    let service = OpenAiCompatibleService::new(Arc::new(RwLock::new(config)));

    let snapshot = match service.resolve_provider_snapshot("p1").await {
        Ok(snapshot) => snapshot,
        Err(error) => panic!("应能解析提供商快照: {error}"),
    };

    assert_eq!(snapshot.provider.default_model, "deepseek-chat");
    assert!(snapshot.agent_settings.thinking_enabled);
}
