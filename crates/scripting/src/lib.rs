//! 脚本引擎基座。
//!
//! [`ScriptNodeBase`] 封装了脚本引擎初始化、脚本编译和求值的通用逻辑。
//! 所有基于脚本的节点（If、Switch、TryCatch、Loop、Code）均通过组合此基座
//! 来复用脚本执行能力。添加新的脚本节点时只需嵌入 `ScriptNodeBase` 字段。

use std::sync::Arc;

use nazh_core::WorkflowVariables;
use rhai::{
    AST, Dynamic, Engine, EvalAltResult, Position, Scope,
    packages::Package,
    serde::{from_dynamic, to_dynamic},
};
use serde_json::Value;

use nazh_core::EngineError;

mod package;

pub use package::{NazhScriptPackage, generate_api_reference};

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
    #[allow(clippy::needless_pass_by_value)]
    fn rhai_set(&mut self, name: &str, value: Dynamic) -> Result<(), Box<EvalAltResult>> {
        let vars = self.require_vars()?;
        let json_value: Value = rhai::serde::from_dynamic(&value)
            .map_err(|err| to_script_error(format!("变量 `{name}` 写入值无法序列化：{err}")))?;
        vars.set(name, json_value, Some(&self.binding.node_id))
            .map_err(|err| to_script_error(err.to_string()))
    }

    /// `vars.cas(name, expected, new)` Rhai 方法：比较交换，返回是否成功。
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
fn register_vars_helpers(engine: &mut Engine) {
    engine
        .register_type_with_name::<ScriptVars>("ScriptVars")
        .register_fn("get", ScriptVars::rhai_get)
        .register_fn("set", ScriptVars::rhai_set)
        .register_fn("cas", ScriptVars::rhai_cas);
}

// Rhai register_fn 要求 Box<EvalAltResult> 返回类型
#[allow(clippy::unnecessary_box_returns)]
fn to_script_error(message: impl Into<String>) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorRuntime(
        message.into().into(),
        Position::NONE,
    ))
}

/// 脚本节点的通用基座。
///
/// 封装了引擎初始化、脚本编译和求值逻辑，供所有基于脚本的节点复用。
pub struct ScriptNodeBase {
    id: String,
    engine: Engine,
    ast: AST,
    /// ADR-0012：每次 evaluate 时 push 进 Scope，供脚本通过 `vars.*` 访问。
    script_vars: ScriptVars,
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
        variables: Option<Arc<WorkflowVariables>>,
    ) -> Result<Self, EngineError> {
        let id = id.into();
        let mut engine = Engine::new();
        engine.set_max_operations(max_operations);
        NazhScriptPackage::new().register_into_engine(&mut engine);
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
    pub fn evaluate(&self, payload: Value) -> Result<(Scope<'static>, Dynamic), EngineError> {
        let mut scope = self.prepare_scope(payload)?;
        let result = self
            .engine
            .eval_ast_with_scope::<Dynamic>(&mut scope, &self.ast)
            .map_err(|error| EngineError::script_runtime(self.id.clone(), error.to_string()))?;
        Ok((scope, result))
    }

    /// 执行 Rhai 脚本，脚本错误不转换为 `EngineError` 而是作为 `Err(String)` 返回。
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
#[path = "scripting_tests.rs"]
mod variables_tests;
