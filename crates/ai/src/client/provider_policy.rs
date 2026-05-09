use nazh_core::ai::{AiGenerationParams, AiThinkingConfig, AiThinkingMode};

pub(super) const TEST_MAX_TOKENS: u32 = 1;

pub(super) fn is_thinking_enabled(params: &AiGenerationParams) -> bool {
    params
        .thinking
        .as_ref()
        .is_some_and(|thinking| thinking.kind == AiThinkingMode::Enabled)
}

pub(super) fn provider_accepts_deepseek_options(base_url: &str, model: &str) -> bool {
    let normalized_base_url = base_url.to_ascii_lowercase();
    let normalized_model = model.to_ascii_lowercase();
    normalized_base_url.contains("deepseek") || normalized_model.contains("deepseek")
}

pub(super) fn build_connection_test_params(disable_deepseek_thinking: bool) -> AiGenerationParams {
    AiGenerationParams {
        temperature: Some(0.0),
        max_tokens: Some(TEST_MAX_TOKENS),
        top_p: None,
        thinking: disable_deepseek_thinking.then_some(AiThinkingConfig {
            kind: AiThinkingMode::Disabled,
        }),
        reasoning_effort: None,
    }
}
