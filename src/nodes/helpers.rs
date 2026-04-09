//! 节点实现的共享基础设施：Rhai 脚本基座与 payload 操作辅助。
//!
//! ## `RhaiNodeBase`
//!
//! [`RhaiNodeBase`] 封装了 Rhai 引擎初始化、脚本编译和求值的通用逻辑。
//! 所有基于脚本的节点（If、Switch、TryCatch、Loop、Rhai）均通过组合此基座
//! 来复用脚本执行能力。添加新的脚本节点时只需嵌入 `RhaiNodeBase` 字段。
//!
//! ## `with_connection`
//!
//! [`with_connection`] 封装了连接的"借出 → 操作 → 释放"异步生命周期，
//! 保证连接在操作完成后（无论成功与否）都会被正确释放。

use ::rhai::{
    serde::{from_dynamic, to_dynamic},
    Dynamic, Engine, Scope, AST,
};
use serde_json::{Map, Value};

use crate::{ConnectionLease, EngineError, SharedConnectionManager, WorkflowContext};

/// Rhai 脚本步数上限的默认值（50,000 步）。
pub(crate) fn default_max_operations() -> u64 {
    50_000
}

/// 将 JSON payload 转换为 Map，非对象值会被包装为 `{"value": ...}`。
pub(crate) fn into_payload_map(payload: Value) -> Map<String, Value> {
    match payload {
        Value::Object(map) => map,
        other => {
            let mut map = Map::new();
            map.insert("value".to_owned(), other);
            map
        }
    }
}

/// 将连接租约信息序列化后写入 payload map 的 `_connection` 字段。
pub(crate) fn insert_connection_lease(
    node_id: &str,
    payload_map: &mut Map<String, Value>,
    lease: &ConnectionLease,
) -> Result<(), EngineError> {
    let lease_value = serde_json::to_value(lease)
        .map_err(|error| EngineError::payload_conversion(node_id.to_owned(), error.to_string()))?;
    payload_map.insert("_connection".to_owned(), lease_value);
    Ok(())
}

/// 在连接借出-释放的生命周期内执行操作。
///
/// 自动从 [`SharedConnectionManager`] 借出连接（若 `connection_id` 非空），
/// 执行闭包，最后无论成功与否都释放连接。新增需要连接的节点时使用此辅助。
pub(crate) async fn with_connection<F>(
    connection_manager: &SharedConnectionManager,
    connection_id: Option<&str>,
    operation: F,
) -> Result<WorkflowContext, EngineError>
where
    F: FnOnce(Option<&ConnectionLease>) -> Result<WorkflowContext, EngineError>,
{
    let lease = if let Some(conn_id) = connection_id {
        let mut manager = connection_manager.write().await;
        Some(manager.borrow(conn_id)?)
    } else {
        None
    };

    let result = operation(lease.as_ref());

    if let Some(conn_id) = connection_id {
        let mut manager = connection_manager.write().await;
        let release_result = manager.release(conn_id);
        if result.is_ok() {
            release_result?;
        }
    }

    result
}

/// Rhai 脚本节点的通用基座。
///
/// 封装了引擎初始化、脚本编译和求值逻辑，供所有基于脚本的节点复用。
/// 新增脚本节点时，在节点结构体中嵌入 `RhaiNodeBase` 字段，
/// 然后在 `execute()` 中调用 [`evaluate`](RhaiNodeBase::evaluate) 或
/// [`evaluate_catching`](RhaiNodeBase::evaluate_catching) 即可。
pub(crate) struct RhaiNodeBase {
    id: String,
    ai_description: String,
    engine: Engine,
    ast: AST,
}

impl RhaiNodeBase {
    /// 创建基座：编译脚本并设置步数上限。
    pub(crate) fn new(
        id: impl Into<String>,
        ai_description: impl Into<String>,
        script: &str,
        max_operations: u64,
    ) -> Result<Self, EngineError> {
        let id = id.into();
        let mut engine = Engine::new();
        engine.set_max_operations(max_operations);
        let ast = engine
            .compile(script)
            .map_err(|error| EngineError::rhai_compile(id.clone(), error.to_string()))?;
        Ok(Self {
            id,
            ai_description: ai_description.into(),
            engine,
            ast,
        })
    }

    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    pub(crate) fn ai_description(&self) -> &str {
        &self.ai_description
    }

    /// 执行 Rhai 脚本，返回作用域和结果值。
    ///
    /// 脚本运行时错误会转换为 [`EngineError::RhaiRuntime`]。
    pub(crate) fn evaluate(
        &self,
        ctx: &WorkflowContext,
    ) -> Result<(Scope<'static>, Dynamic), EngineError> {
        let payload = to_dynamic(ctx.payload.clone())
            .map_err(|error| EngineError::payload_conversion(self.id.clone(), error.to_string()))?;
        let mut scope = Scope::new();
        scope.push_dynamic("payload", payload);
        let result = self
            .engine
            .eval_ast_with_scope::<Dynamic>(&mut scope, &self.ast)
            .map_err(|error| EngineError::rhai_runtime(self.id.clone(), error.to_string()))?;
        Ok((scope, result))
    }

    /// 执行 Rhai 脚本，脚本错误不转换为 `EngineError` 而是作为 `Err(String)` 返回。
    ///
    /// 用于需要自行处理脚本错误的节点（如 [`TryCatchNode`](super::TryCatchNode)）。
    pub(crate) fn evaluate_catching(
        &self,
        ctx: &WorkflowContext,
    ) -> Result<(Scope<'static>, Result<Dynamic, String>), EngineError> {
        let payload = to_dynamic(ctx.payload.clone())
            .map_err(|error| EngineError::payload_conversion(self.id.clone(), error.to_string()))?;
        let mut scope = Scope::new();
        scope.push_dynamic("payload", payload);
        let script_result =
            match self
                .engine
                .eval_ast_with_scope::<Dynamic>(&mut scope, &self.ast)
            {
                Ok(value) => Ok(value),
                Err(error) => Err(error.to_string()),
            };
        Ok((scope, script_result))
    }

    /// 从 Rhai 作用域中提取 `payload` 变量并转换为 JSON Value。
    pub(crate) fn payload_from_scope(&self, scope: &Scope<'_>) -> Result<Value, EngineError> {
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
    pub(crate) fn dynamic_to_value(&self, result: &Dynamic) -> Result<Value, EngineError> {
        from_dynamic::<Value>(result)
            .map_err(|error| EngineError::payload_conversion(self.id.clone(), error.to_string()))
    }
}
