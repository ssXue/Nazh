use super::*;

fn sample_provider_record(id: &str, api_key: &str) -> AiProviderSecretRecord {
    AiProviderSecretRecord {
        id: id.to_owned(),
        name: format!("测试提供商-{id}"),
        base_url: "https://api.example.com/v1".to_owned(),
        api_key: api_key.to_owned(),
        default_model: "test-model".to_owned(),
        extra_headers: HashMap::new(),
        enabled: true,
    }
}

#[test]
fn to_view_excludes_api_key_and_marks_saved() {
    let record = sample_provider_record("p1", "sk-secret-123");
    let file = AiConfigFile {
        version: 1,
        providers: vec![record],
        active_provider_id: Some("p1".to_owned()),
        copilot_params: AiGenerationParams::default(),
        agent_settings: AiAgentSettings::default(),
    };
    let view = file.to_view();
    assert_eq!(view.providers.len(), 1);
    assert!(view.providers[0].has_api_key);
    assert_eq!(view.providers[0].id, "p1");
}

#[test]
fn to_view_empty_key_marks_unsaved() {
    let record = sample_provider_record("p2", "");
    let file = AiConfigFile {
        version: 1,
        providers: vec![record],
        active_provider_id: None,
        copilot_params: AiGenerationParams::default(),
        agent_settings: AiAgentSettings::default(),
    };
    let view = file.to_view();
    assert!(!view.providers[0].has_api_key);
}

#[test]
fn merge_update_keep_preserves_existing_key() {
    let record = sample_provider_record("p1", "sk-old-key");
    let mut file = AiConfigFile {
        version: 1,
        providers: vec![record],
        active_provider_id: None,
        copilot_params: AiGenerationParams::default(),
        agent_settings: AiAgentSettings::default(),
    };
    file.merge_update(AiConfigUpdate {
        version: 1,
        providers: vec![AiProviderUpsert {
            id: "p1".to_owned(),
            name: "新名称".to_owned(),
            base_url: "https://new.example.com/v1".to_owned(),
            default_model: "new-model".to_owned(),
            extra_headers: HashMap::new(),
            enabled: true,
            api_key: AiSecretInput::Keep,
        }],
        active_provider_id: Some("p1".to_owned()),
        copilot_params: AiGenerationParams::default(),
        agent_settings: AiAgentSettings::default(),
    });
    assert_eq!(file.providers[0].api_key, "sk-old-key");
    assert_eq!(file.providers[0].name, "新名称");
    assert_eq!(file.active_provider_id.as_deref(), Some("p1"));
}

#[test]
fn merge_update_clear_removes_key() {
    let record = sample_provider_record("p1", "sk-old-key");
    let mut file = AiConfigFile {
        version: 1,
        providers: vec![record],
        active_provider_id: None,
        copilot_params: AiGenerationParams::default(),
        agent_settings: AiAgentSettings::default(),
    };
    file.merge_update(AiConfigUpdate {
        version: 1,
        providers: vec![AiProviderUpsert {
            id: "p1".to_owned(),
            name: "p1".to_owned(),
            base_url: "https://api.example.com/v1".to_owned(),
            default_model: "model".to_owned(),
            extra_headers: HashMap::new(),
            enabled: true,
            api_key: AiSecretInput::Clear,
        }],
        active_provider_id: None,
        copilot_params: AiGenerationParams::default(),
        agent_settings: AiAgentSettings::default(),
    });
    assert!(file.providers[0].api_key.is_empty());
}

#[test]
fn merge_update_set_replaces_key() {
    let mut file = AiConfigFile::default();
    file.merge_update(AiConfigUpdate {
        version: 1,
        providers: vec![AiProviderUpsert {
            id: "p-new".to_owned(),
            name: "新提供商".to_owned(),
            base_url: "https://api.example.com/v1".to_owned(),
            default_model: "model".to_owned(),
            extra_headers: HashMap::new(),
            enabled: true,
            api_key: AiSecretInput::Set("sk-brand-new".to_owned()),
        }],
        active_provider_id: None,
        copilot_params: AiGenerationParams::default(),
        agent_settings: AiAgentSettings::default(),
    });
    assert_eq!(file.providers[0].api_key, "sk-brand-new");
}

#[test]
fn merge_update_only_keeps_one_enabled_provider() {
    let mut file = AiConfigFile::default();
    file.merge_update(AiConfigUpdate {
        version: 1,
        providers: vec![
            AiProviderUpsert {
                id: "p1".to_owned(),
                name: "主提供商".to_owned(),
                base_url: "https://api.example.com/v1".to_owned(),
                default_model: "model-a".to_owned(),
                extra_headers: HashMap::new(),
                enabled: true,
                api_key: AiSecretInput::Set("sk-a".to_owned()),
            },
            AiProviderUpsert {
                id: "p2".to_owned(),
                name: "备选提供商".to_owned(),
                base_url: "https://api.example.org/v1".to_owned(),
                default_model: "model-b".to_owned(),
                extra_headers: HashMap::new(),
                enabled: true,
                api_key: AiSecretInput::Set("sk-b".to_owned()),
            },
        ],
        active_provider_id: Some("p2".to_owned()),
        copilot_params: AiGenerationParams::default(),
        agent_settings: AiAgentSettings::default(),
    });

    assert_eq!(file.active_provider_id.as_deref(), Some("p2"));
    assert!(!file.providers[0].enabled);
    assert!(file.providers[1].enabled);
}

#[test]
fn merge_update_without_active_provider_falls_back_to_first_provider() {
    let mut file = AiConfigFile::default();
    file.merge_update(AiConfigUpdate {
        version: 1,
        providers: vec![
            AiProviderUpsert {
                id: "p1".to_owned(),
                name: "主提供商".to_owned(),
                base_url: "https://api.example.com/v1".to_owned(),
                default_model: "model-a".to_owned(),
                extra_headers: HashMap::new(),
                enabled: false,
                api_key: AiSecretInput::Set("sk-a".to_owned()),
            },
            AiProviderUpsert {
                id: "p2".to_owned(),
                name: "备选提供商".to_owned(),
                base_url: "https://api.example.org/v1".to_owned(),
                default_model: "model-b".to_owned(),
                extra_headers: HashMap::new(),
                enabled: true,
                api_key: AiSecretInput::Set("sk-b".to_owned()),
            },
        ],
        active_provider_id: None,
        copilot_params: AiGenerationParams::default(),
        agent_settings: AiAgentSettings::default(),
    });

    assert_eq!(file.active_provider_id.as_deref(), Some("p1"));
    assert!(file.providers[0].enabled);
    assert!(!file.providers[1].enabled);
}

#[test]
fn to_view_includes_agent_settings() {
    let file = AiConfigFile {
        version: 1,
        providers: vec![sample_provider_record("p1", "sk-secret-123")],
        active_provider_id: Some("p1".to_owned()),
        copilot_params: AiGenerationParams::default(),
        agent_settings: AiAgentSettings {
            system_prompt: Some("你是全局代理".to_owned()),
            timeout_ms: Some(12_000),
            thinking_enabled: false,
        },
    };

    let view = file.to_view();

    assert_eq!(
        view.agent_settings.system_prompt.as_deref(),
        Some("你是全局代理")
    );
    assert_eq!(view.agent_settings.timeout_ms, Some(12_000));
}

#[test]
fn merge_update_filters_sensitive_extra_headers() {
    let mut headers = HashMap::new();
    headers.insert("Authorization".to_owned(), "Bearer leaked".to_owned());
    headers.insert("X-Api-Key".to_owned(), "secret".to_owned());
    headers.insert("X-Trace-Id".to_owned(), "trace-1".to_owned());

    let mut file = AiConfigFile::default();
    file.merge_update(AiConfigUpdate {
        version: 1,
        providers: vec![AiProviderUpsert {
            id: "p1".to_owned(),
            name: "主提供商".to_owned(),
            base_url: "https://api.example.com/v1".to_owned(),
            default_model: "model-a".to_owned(),
            extra_headers: headers,
            enabled: true,
            api_key: AiSecretInput::Set("sk-a".to_owned()),
        }],
        active_provider_id: Some("p1".to_owned()),
        copilot_params: AiGenerationParams::default(),
        agent_settings: AiAgentSettings::default(),
    });

    assert!(
        !file.providers[0]
            .extra_headers
            .contains_key("Authorization")
    );
    assert!(!file.providers[0].extra_headers.contains_key("X-Api-Key"));
    assert_eq!(
        file.providers[0].extra_headers.get("X-Trace-Id"),
        Some(&"trace-1".to_owned())
    );
}

#[test]
fn to_view_does_not_return_sensitive_extra_headers_from_legacy_config() {
    let mut record = sample_provider_record("p1", "sk-secret-123");
    record
        .extra_headers
        .insert("authorization".to_owned(), "Bearer leaked".to_owned());
    record
        .extra_headers
        .insert("X-Request-Id".to_owned(), "req-1".to_owned());
    let file = AiConfigFile {
        version: 1,
        providers: vec![record],
        active_provider_id: Some("p1".to_owned()),
        copilot_params: AiGenerationParams::default(),
        agent_settings: AiAgentSettings::default(),
    };

    let view = file.to_view();

    assert!(
        !view.providers[0]
            .extra_headers
            .contains_key("authorization")
    );
    assert_eq!(
        view.providers[0].extra_headers.get("X-Request-Id"),
        Some(&"req-1".to_owned())
    );
}
