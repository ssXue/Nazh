//! 沙箱化脚本节点。
//!
//! 用户编写的业务逻辑脚本在有界 Rhai 虚拟机中执行，
//! 脚本可修改 `payload` 变量或返回新值作为输出 payload。

use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use uuid::Uuid;

use nazh_ai_core::{AiGenerationParams, AiService};
use serde::{Deserialize, Serialize};

use nazh_core::EngineError;
use nazh_core::{NodeExecution, NodeTrait};
use scripting::{ScriptAiRuntime, ScriptNodeBase, default_max_operations};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeNodeAiConfig {
    pub provider_id: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub top_p: Option<f32>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeNodeConfig {
    pub script: String,
    #[serde(default = "default_max_operations")]
    pub max_operations: u64,
    #[serde(default)]
    pub ai: Option<CodeNodeAiConfig>,
}

/// 沙箱化脚本节点，基于 [`ScriptNodeBase`] 实现。
pub struct CodeNode {
    base: ScriptNodeBase,
}

impl CodeNode {
    /// # Errors
    ///
    /// 脚本编译失败时返回 [`EngineError::ScriptCompile`]。
    #[allow(clippy::needless_pass_by_value)]
    pub fn new(
        id: impl Into<String>,
        config: CodeNodeConfig,
        ai_service: Option<Arc<dyn AiService>>,
    ) -> Result<Self, EngineError> {
        let id = id.into();
        let CodeNodeConfig {
            script,
            max_operations,
            ai,
        } = config;

        let ai = match ai {
            Some(ai_config) => {
                let service = ai_service.ok_or_else(|| {
                    EngineError::invalid_graph(format!(
                        "Code 节点 `{id}` 配置了 AI，但部署资源中缺少 AiService"
                    ))
                })?;
                Some(ScriptAiRuntime::new(
                    id.clone(),
                    service,
                    ai_config.provider_id,
                    ai_config.system_prompt,
                    ai_config.model,
                    AiGenerationParams {
                        temperature: ai_config.temperature,
                        max_tokens: ai_config.max_tokens,
                        top_p: ai_config.top_p,
                    },
                    ai_config.timeout_ms,
                )?)
            }
            None => None,
        };

        Ok(Self {
            base: ScriptNodeBase::new(id, &script, max_operations, ai)?,
        })
    }
}

#[async_trait]
impl NodeTrait for CodeNode {
    scripting::delegate_node_base!("code");

    async fn transform(
        &self,
        _trace_id: Uuid,
        payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        let (scope, result) = self.base.evaluate(payload)?;
        let new_payload = if result.is_unit() {
            self.base.payload_from_scope(&scope)?
        } else {
            self.base.dynamic_to_value(&result)?
        };
        Ok(NodeExecution::broadcast(new_payload))
    }
}
