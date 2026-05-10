use std::collections::HashMap;
use std::sync::Arc;

use nazh_core::ai::{
    AiGenerationParams, AiMessageRole, AiReasoningEffort, AiThinkingConfig,
    AiThinkingMode,
};
use serde_json::json;
use tokio::sync::RwLock;

use super::OpenAiCompatibleService;
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

fn test_messages() -> Vec<serde_json::Value> {
    vec![json!({ "role": "user", "content": "Hi" })]
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

    let body = super::build_request_json(
        &provider.default_model,
        &test_messages(),
        &params,
        false,
        true,
        true,
        &[],
    );

    assert_eq!(body["model"], "deepseek-v4-pro");
    assert_eq!(body["thinking"]["type"], "enabled");
    assert_eq!(body["reasoning_effort"], "max");
    assert_eq!(body["max_tokens"], 256);
    assert!(body.get("temperature").is_none());
    assert!(body.get("top_p").is_none());
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

    let body = super::build_request_json(
        &provider.default_model,
        &test_messages(),
        &params,
        false,
        false,
        true,
        &[],
    );

    assert!(body.get("thinking").is_none());
    assert!(body.get("reasoning_effort").is_none());
    assert!((body["temperature"].as_f64().unwrap_or_default() - 0.3).abs() < 0.001);
    assert!((body["top_p"].as_f64().unwrap_or_default() - 0.8).abs() < 0.001);
}

#[test]
fn deepseek_connection_test_disables_thinking_for_lightweight_probe() {
    let provider = test_provider("https://api.deepseek.com", "deepseek-v4-flash");
    let thinking_enabled = true;
    let params = build_connection_test_params(thinking_enabled);
    let body = super::build_request_json(
        &provider.default_model,
        &test_messages(),
        &params,
        false,
        true,
        thinking_enabled,
        &[],
    );

    assert_eq!(body["thinking"]["type"], "disabled");
    // DeepSeek + thinking_enabled 组合下省略采样参数
    assert!(body.get("temperature").is_none());
    assert_eq!(body["max_tokens"], TEST_MAX_TOKENS);
}

#[test]
fn convert_messages_maps_all_roles() {
    use nazh_core::ai::AiMessage;

    let messages = vec![
        AiMessage::simple(AiMessageRole::System, "系统提示".to_owned()),
        AiMessage::simple(AiMessageRole::User, "用户输入".to_owned()),
        AiMessage::simple(AiMessageRole::Assistant, "助手回复".to_owned()),
    ];

    let converted = super::convert_messages(&messages);

    assert_eq!(converted.len(), 3);
    assert_eq!(converted[0]["role"], "system");
    assert_eq!(converted[0]["content"], "系统提示");
    assert_eq!(converted[1]["role"], "user");
    assert_eq!(converted[2]["role"], "assistant");
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
            ..Default::default()
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
