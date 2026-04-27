//! 脚本引擎基座。
//!
//! [`ScriptNodeBase`] 封装了脚本引擎初始化、脚本编译和求值的通用逻辑。
//! 所有基于脚本的节点（If、Switch、TryCatch、Loop、Code）均通过组合此基座
//! 来复用脚本执行能力。添加新的脚本节点时只需嵌入 `ScriptNodeBase` 字段。

use std::sync::Arc;

use nazh_core::WorkflowVariables;
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

/// Rhai 脚本中暴露给脚本的 `vars` 全局对象（ADR-0012）。
///
/// 内部持有 `Arc<WorkflowVariables>` + `node_id`（用于 `set` / `cas` 时记录 `updated_by`）。
/// 未注入变量时（旧行为兼容）调用任意方法均返回 `ErrorRuntime`。
///
/// 脚本用法：
/// ```rhai
/// let v = vars.get("counter");   // 读取
/// vars.set("counter", v + 1);    // 写入（类型检查）
/// let ok = vars.cas("c", 0, 1);  // CAS，返回 bool
/// ```
#[derive(Clone)]
pub(crate) struct ScriptVars {
    binding: Arc<VarsBinding>,
}

struct VarsBinding {
    node_id: String,
    variables: Option<Arc<WorkflowVariables>>,
}

impl ScriptVars {
    fn new(node_id: String, variables: Option<Arc<WorkflowVariables>>) -> Self {
        Self {
            binding: Arc::new(VarsBinding { node_id, variables }),
        }
    }

    fn require_vars(&self) -> Result<&Arc<WorkflowVariables>, Box<EvalAltResult>> {
        self.binding.variables.as_ref().ok_or_else(|| {
            to_script_error(format!(
                "脚本节点 `{}` 未注入 vars——工作流定义中 variables 字段为空",
                self.binding.node_id
            ))
        })
    }

    /// `vars.get(name)` Rhai 方法：读取工作流变量值。
    fn rhai_get(&mut self, name: &str) -> Result<Dynamic, Box<EvalAltResult>> {
        let vars = self.require_vars()?;
        let value = vars
            .get_value(name)
            .ok_or_else(|| to_script_error(EngineError::unknown_variable(name).to_string()))?;
        rhai::serde::to_dynamic(value)
            .map_err(|err| to_script_error(format!("变量 `{name}` 无法转 Dynamic：{err}")))
    }

    /// `vars.set(name, value)` Rhai 方法：写入工作流变量（类型校验）。
    // Rhai 1.x register_fn 要求 Dynamic 值参以 owned 形式接收；clippy 误报 needless_pass_by_value
    #[allow(clippy::needless_pass_by_value)]
    fn rhai_set(&mut self, name: &str, value: Dynamic) -> Result<(), Box<EvalAltResult>> {
        let vars = self.require_vars()?;
        let json_value: Value = rhai::serde::from_dynamic(&value)
            .map_err(|err| to_script_error(format!("变量 `{name}` 写入值无法序列化：{err}")))?;
        vars.set(name, json_value, Some(&self.binding.node_id))
            .map_err(|err| to_script_error(err.to_string()))
    }

    /// `vars.cas(name, expected, new)` Rhai 方法：比较交换，返回是否成功。
    // Rhai 1.x register_fn 要求 Dynamic 值参以 owned 形式接收；clippy 误报 needless_pass_by_value
    #[allow(clippy::needless_pass_by_value)]
    fn rhai_cas(
        &mut self,
        name: &str,
        expected: Dynamic,
        new: Dynamic,
    ) -> Result<bool, Box<EvalAltResult>> {
        let vars = self.require_vars()?;
        let expected_value: Value = rhai::serde::from_dynamic(&expected)
            .map_err(|err| to_script_error(format!("CAS expected 值反序列化失败：{err}")))?;
        let new_value: Value = rhai::serde::from_dynamic(&new)
            .map_err(|err| to_script_error(format!("CAS new 值反序列化失败：{err}")))?;
        vars.compare_and_swap(
            name,
            &expected_value,
            new_value,
            Some(&self.binding.node_id),
        )
        .map_err(|err| to_script_error(err.to_string()))
    }
}

/// 向引擎注册 `ScriptVars` 类型及其 `get` / `set` / `cas` 方法。
///
/// 注意：仅注册类型与方法绑定；`ScriptVars` 实例通过 `Scope` 在每次 `evaluate` 时推入。
fn register_vars_helpers(engine: &mut Engine) {
    engine
        .register_type_with_name::<ScriptVars>("ScriptVars")
        .register_fn("get", ScriptVars::rhai_get)
        .register_fn("set", ScriptVars::rhai_set)
        .register_fn("cas", ScriptVars::rhai_cas);
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
    /// ADR-0012：每次 evaluate 时 push 进 Scope，供脚本通过 `vars.*` 访问。
    script_vars: ScriptVars,
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
    ///
    /// `variables` 传 `Some(arc)` 时脚本可通过 `vars.get` / `vars.set` / `vars.cas`
    /// 读写工作流变量（ADR-0012）；传 `None` 时调用会在运行期返回脚本错误。
    pub fn new(
        id: impl Into<String>,
        script: &str,
        max_operations: u64,
        ai: Option<ScriptAiRuntime>,
        variables: Option<Arc<WorkflowVariables>>,
    ) -> Result<Self, EngineError> {
        let id = id.into();
        let mut engine = Engine::new();
        engine.set_max_operations(max_operations);
        NazhScriptPackage::new().register_into_engine(&mut engine);
        register_ai_complete(&mut engine, &id, ai);
        register_vars_helpers(&mut engine);
        let ast = engine
            .compile(script)
            .map_err(|error| EngineError::script_compile(id.clone(), error.to_string()))?;
        let script_vars = ScriptVars::new(id.clone(), variables);
        Ok(Self {
            id,
            engine,
            ast,
            script_vars,
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    /// 将 JSON payload 转换为 Rhai 作用域，并推入 `vars` 全局对象。
    fn prepare_scope(&self, payload: Value) -> Result<Scope<'static>, EngineError> {
        let dynamic = to_dynamic(payload)
            .map_err(|error| EngineError::payload_conversion(self.id.clone(), error.to_string()))?;
        let mut scope = Scope::new();
        scope.push_dynamic("payload", dynamic);
        scope.push("vars", self.script_vars.clone());
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod variables_tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use nazh_core::{PinType, VariableDeclaration, WorkflowVariables};

    use super::*;

    fn vars_arc(name: &str, ty: PinType, initial: serde_json::Value) -> Arc<WorkflowVariables> {
        let mut decls = HashMap::new();
        decls.insert(
            name.to_owned(),
            VariableDeclaration {
                variable_type: ty,
                initial,
            },
        );
        Arc::new(WorkflowVariables::from_declarations(&decls).unwrap())
    }

    #[test]
    fn rhai_脚本可读写变量() {
        let vars = vars_arc("counter", PinType::Integer, serde_json::Value::from(5_i64));
        let base = ScriptNodeBase::new(
            "test-script",
            r#"
                let v = vars.get("counter");
                vars.set("counter", v + 1);
                vars.get("counter")
            "#,
            10_000,
            None,
            Some(Arc::clone(&vars)),
        )
        .unwrap();

        let (_, result) = base.evaluate(serde_json::Value::Null).unwrap();
        let final_value = base.dynamic_to_value(&result).unwrap();
        assert_eq!(final_value, serde_json::Value::from(6_i64));
        assert_eq!(
            vars.get("counter").unwrap().value,
            serde_json::Value::from(6_i64)
        );
    }

    #[test]
    fn rhai_脚本写入未声明变量返回错误() {
        let vars = vars_arc("a", PinType::Integer, serde_json::Value::from(0_i64));
        let base = ScriptNodeBase::new(
            "test-script-2",
            r#"vars.set("undeclared", 42)"#,
            10_000,
            None,
            Some(Arc::clone(&vars)),
        )
        .unwrap();
        let err = base.evaluate(serde_json::Value::Null).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("undeclared") || msg.contains("UnknownVariable"),
            "错误消息应包含变量名，实际：{msg}"
        );
    }

    #[test]
    fn rhai_脚本_cas_成功返回_true() {
        let vars = vars_arc("c", PinType::Integer, serde_json::Value::from(0_i64));
        let base = ScriptNodeBase::new(
            "test-script-3",
            r#"vars.cas("c", 0, 1)"#,
            10_000,
            None,
            Some(Arc::clone(&vars)),
        )
        .unwrap();
        let (_, result) = base.evaluate(serde_json::Value::Null).unwrap();
        let final_value = base.dynamic_to_value(&result).unwrap();
        assert_eq!(final_value, serde_json::Value::from(true));
    }

    #[test]
    fn rhai_脚本_cas_期望值不匹配时返回_false() {
        let vars = vars_arc("c", PinType::Integer, serde_json::Value::from(0_i64));
        let base = ScriptNodeBase::new(
            "test-script-cas-mismatch",
            // 期望值给 99（实际是 0），CAS 应返回 false 且变量保持 0
            r#"
                let ok = vars.cas("c", 99, 1);
                #{ ok: ok, current: vars.get("c") }
            "#,
            10_000,
            None,
            Some(Arc::clone(&vars)),
        )
        .unwrap();
        let (_, result) = base.evaluate(serde_json::Value::Null).unwrap();
        let final_value = base.dynamic_to_value(&result).unwrap();
        assert_eq!(
            final_value["ok"],
            serde_json::Value::from(false),
            "CAS 期望值不匹配应返回 false"
        );
        assert_eq!(
            final_value["current"],
            serde_json::Value::from(0_i64),
            "CAS 失败后变量应保持原值"
        );
    }

    #[test]
    fn rhai_脚本无_variables_注入时_vars_未定义() {
        let base = ScriptNodeBase::new(
            "test-script-4",
            r#"vars.get("anything")"#,
            10_000,
            None,
            None,
        )
        .unwrap();
        let err = base.evaluate(serde_json::Value::Null).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("未注入"),
            "未注入 variables 时调用 vars.* 应返回 `未注入` 错误，实际：{msg}"
        );
    }
}
