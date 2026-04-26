//! 脚本引擎基座。
//!
//! [`ScriptNodeBase`] 封装了脚本引擎初始化、脚本编译和求值的通用逻辑。
//! 所有基于脚本的节点（If、Switch、TryCatch、Loop、Code）均通过组合此基座
//! 来复用脚本执行能力。添加新的脚本节点时只需嵌入 `ScriptNodeBase` 字段。

use std::sync::Arc;

use nazh_core::ai::{AiCompletionRequest, AiGenerationParams, AiMessage, AiMessageRole, AiService};
use rhai::{
    AST, Dynamic, Engine, EvalAltResult, Position, Scope,
    packages::Package,
    serde::{from_dynamic, to_dynamic},
};
use serde_json::Value;

use nazh_core::EngineError;

mod package;

pub use package::NazhScriptPackage;

/// Rhai 脚本步数上限的默认值（50,000 步）。
pub fn default_max_operations() -> u64 {
    50_000
}

/// 为嵌入 `ScriptNodeBase` 的脚本节点委托 [`NodeTrait`](nazh_core::NodeTrait) 元数据方法。
///
/// 需要节点结构体含有 `base: ScriptNodeBase` 字段。
#[macro_export]
macro_rules! delegate_node_base {
    ($kind:expr) => {
        fn id(&self) -> &str {
            self.base.id()
        }
        fn kind(&self) -> &'static str {
            $kind
        }
    };
}

/// 脚本节点的通用基座。
///
/// 封装了引擎初始化、脚本编译和求值逻辑，供所有基于脚本的节点复用。
/// 新增脚本节点时，在节点结构体中嵌入 `ScriptNodeBase` 字段，
/// 然后在 `execute()` 中调用 [`evaluate`](ScriptNodeBase::evaluate) 或
/// [`evaluate_catching`](ScriptNodeBase::evaluate_catching) 即可。
pub struct ScriptNodeBase {
    id: String,
    engine: Engine,
    ast: AST,
}

/// 脚本节点的 AI 运行时配置。
#[derive(Clone)]
pub struct ScriptAiRuntime {
    node_id: String,
    service: Arc<dyn AiService>,
    provider_id: String,
    system_prompt: Option<String>,
    model: Option<String>,
    params: AiGenerationParams,
    timeout_ms: Option<u64>,
}

impl ScriptAiRuntime {
    /// 构造脚本节点的 AI 调用器。
    pub fn new(
        node_id: impl Into<String>,
        service: Arc<dyn AiService>,
        provider_id: impl Into<String>,
        system_prompt: Option<String>,
        model: Option<String>,
        params: AiGenerationParams,
        timeout_ms: Option<u64>,
    ) -> Result<Self, EngineError> {
        let node_id = node_id.into();
        let provider_id = provider_id.into();
        if provider_id.trim().is_empty() {
            return Err(EngineError::node_config(node_id, "AI provider_id 不能为空"));
        }

        Ok(Self {
            node_id,
            service,
            provider_id,
            system_prompt,
            model,
            params,
            timeout_ms,
        })
    }

    fn build_request(&self, prompt: String) -> AiCompletionRequest {
        let mut messages = Vec::with_capacity(2);
        if let Some(system_prompt) = self
            .system_prompt
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            messages.push(AiMessage {
                role: AiMessageRole::System,
                content: format!(
                    "[格式要求]\n如果用户要求结构化数据输出，请直接输出合法 JSON（无 Markdown 代码块包裹），不要附加解释性文字。\n如果用户未明确要求格式，则自由回复。\n\n{system_prompt}"
                ),
            });
        }
        messages.push(AiMessage {
            role: AiMessageRole::User,
            content: prompt,
        });

        AiCompletionRequest {
            provider_id: self.provider_id.clone(),
            model: self.model.clone(),
            messages,
            params: self.params.clone(),
            timeout_ms: self.timeout_ms,
        }
    }

    fn complete(&self, prompt: String) -> Result<Dynamic, Box<EvalAltResult>> {
        if prompt.trim().is_empty() {
            return Err(to_script_error("AI prompt 不能为空"));
        }

        let request = self.build_request(prompt);
        let service = Arc::clone(&self.service);
        let node_id = self.node_id.clone();
        let join_result = std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| format!("节点 `{node_id}` 无法创建 AI 调用运行时: {error}"))?
                .block_on(async move {
                    service
                        .complete(request)
                        .await
                        .map(|response| response.content)
                        .map_err(|error| error.to_string())
                })
        })
        .join();

        match join_result {
            Ok(Ok(content)) => Ok(parse_ai_response(content)),
            Ok(Err(message)) => Err(to_script_error(message)),
            Err(_) => Err(to_script_error(format!(
                "节点 `{}` 的 AI 调用线程发生 panic",
                self.node_id
            ))),
        }
    }
}

#[derive(Clone)]
enum ScriptAiBinding {
    Enabled(Arc<ScriptAiRuntime>),
    Disabled(String),
}

impl ScriptAiBinding {
    fn complete(&self, prompt: String) -> Result<Dynamic, Box<EvalAltResult>> {
        match self {
            Self::Enabled(runtime) => runtime.complete(prompt),
            Self::Disabled(message) => Err(to_script_error(message.clone())),
        }
    }
}

// Rhai register_fn 要求 Box<EvalAltResult> 返回类型
#[allow(clippy::unnecessary_box_returns)]
fn to_script_error(message: impl Into<String>) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorRuntime(
        message.into().into(),
        Position::NONE,
    ))
}

fn strip_markdown_fences(s: &str) -> &str {
    let trimmed = s.trim();
    if let Some(rest) = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```JSON"))
        .or_else(|| trimmed.strip_prefix("```"))
        && let Some(body) = rest.strip_suffix("```")
    {
        return body.trim();
    }
    trimmed
}

fn parse_ai_response(content: String) -> Dynamic {
    let cleaned = strip_markdown_fences(&content);
    if cleaned.is_empty() {
        return Dynamic::UNIT;
    }
    match serde_json::from_str::<Value>(cleaned) {
        Ok(value) => to_dynamic(value).unwrap_or_else(|_| Dynamic::from(content)),
        Err(_) => Dynamic::from(content),
    }
}

fn register_ai_complete(engine: &mut Engine, node_id: &str, ai: Option<ScriptAiRuntime>) {
    let binding = Arc::new(ai.map_or_else(
        || ScriptAiBinding::Disabled(format!("脚本节点 `{node_id}` 未启用 AI 能力")),
        |runtime| ScriptAiBinding::Enabled(Arc::new(runtime)),
    ));

    engine.register_fn(
        "ai_complete",
        move |prompt: String| -> Result<Dynamic, Box<EvalAltResult>> { binding.complete(prompt) },
    );
}

impl ScriptNodeBase {
    /// 创建基座：编译脚本并设置步数上限。
    pub fn new(
        id: impl Into<String>,
        script: &str,
        max_operations: u64,
        ai: Option<ScriptAiRuntime>,
    ) -> Result<Self, EngineError> {
        let id = id.into();
        let mut engine = Engine::new();
        engine.set_max_operations(max_operations);
        NazhScriptPackage::new().register_into_engine(&mut engine);
        register_ai_complete(&mut engine, &id, ai);
        let ast = engine
            .compile(script)
            .map_err(|error| EngineError::script_compile(id.clone(), error.to_string()))?;
        Ok(Self { id, engine, ast })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    /// 将 JSON payload 转换为 Rhai 作用域。
    fn prepare_scope(&self, payload: Value) -> Result<Scope<'static>, EngineError> {
        let dynamic = to_dynamic(payload)
            .map_err(|error| EngineError::payload_conversion(self.id.clone(), error.to_string()))?;
        let mut scope = Scope::new();
        scope.push_dynamic("payload", dynamic);
        Ok(scope)
    }

    /// 执行 Rhai 脚本，返回作用域和结果值。
    ///
    /// 脚本运行时错误会转换为 [`EngineError::ScriptRuntime`]。
    pub fn evaluate(&self, payload: Value) -> Result<(Scope<'static>, Dynamic), EngineError> {
        let mut scope = self.prepare_scope(payload)?;
        let result = self
            .engine
            .eval_ast_with_scope::<Dynamic>(&mut scope, &self.ast)
            .map_err(|error| EngineError::script_runtime(self.id.clone(), error.to_string()))?;
        Ok((scope, result))
    }

    /// 执行 Rhai 脚本，脚本错误不转换为 `EngineError` 而是作为 `Err(String)` 返回。
    ///
    /// 用于需要自行处理脚本错误的节点（如 `TryCatchNode`）。
    pub fn evaluate_catching(
        &self,
        payload: Value,
    ) -> Result<(Scope<'static>, Result<Dynamic, String>), EngineError> {
        let mut scope = self.prepare_scope(payload)?;
        let script_result = self
            .engine
            .eval_ast_with_scope::<Dynamic>(&mut scope, &self.ast)
            .map_or_else(|error| Err(error.to_string()), Ok);
        Ok((scope, script_result))
    }

    /// 从 Rhai 作用域中提取 `payload` 变量并转换为 JSON Value。
    pub fn payload_from_scope(&self, scope: &Scope<'_>) -> Result<Value, EngineError> {
        let payload = scope.get_value::<Dynamic>("payload").ok_or_else(|| {
            EngineError::payload_conversion(
                self.id.clone(),
                "脚本执行后作用域中未保留 `payload` 变量",
            )
        })?;
        from_dynamic::<Value>(&payload)
            .map_err(|error| EngineError::payload_conversion(self.id.clone(), error.to_string()))
    }

    /// 将 Dynamic 结果转换为 JSON Value。
    pub fn dynamic_to_value(&self, result: &Dynamic) -> Result<Value, EngineError> {
        from_dynamic::<Value>(result)
            .map_err(|error| EngineError::payload_conversion(self.id.clone(), error.to_string()))
    }
}
