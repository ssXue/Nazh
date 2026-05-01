//! 工作流变量的部署期初始化器（ADR-0012）。
//!
//! 把 [`WorkflowGraph::variables`](super::types::WorkflowGraph) 声明转成可注入
//! 引擎的 `Arc<WorkflowVariables>`。在 `deploy_workflow_with_ai` 阶段 0（早于
//! pin 校验、早于 `on_deploy`）调用——初值类型不匹配立即整图失败。
//!
//! ## 接受 `Option<&HashMap>` 的设计
//!
//! `WorkflowGraph.variables` 是 `Option<HashMap<...>>`（ts-rs `ts(optional)` 要求
//! `Option<T>` 才能生成 TS 端的 `?:` 字段）。本初始化器吞下这层 Option：
//!
//! - `None`（旧图 / 未声明变量）→ 返回 `Arc::new(WorkflowVariables::empty())`
//! - `Some(&map)` → 走 `WorkflowVariables::from_declarations(map)`，类型校验失败整图拒部署
//!
//! 这样 `deploy_workflow_with_ai` 的调用是单行：
//! `let vars = build_workflow_variables(graph.variables.as_ref())?;`
//! ——不需要在 `deploy.rs` 里散落 `unwrap_or` / `match` 分支。

use std::collections::HashMap;
use std::hash::BuildHasher;
use std::sync::Arc;

use nazh_core::{EngineError, VariableDeclaration, WorkflowVariables};

/// 把 schema 声明（可选）转成共享变量实例。
///
/// # Errors
///
/// 任一变量的初值类型与声明类型不匹配时返回
/// [`EngineError::VariableInitialMismatch`]。`None` 永远成功（返回空 `Arc`）。
pub fn build_workflow_variables<S: BuildHasher>(
    declarations: Option<&HashMap<String, VariableDeclaration, S>>,
) -> Result<Arc<WorkflowVariables>, EngineError> {
    match declarations {
        None => Ok(Arc::new(WorkflowVariables::empty())),
        Some(map) => Ok(Arc::new(WorkflowVariables::from_declarations(map)?)),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use nazh_core::PinType;

    #[test]
    fn none_声明_返回空变量集() {
        let vars = build_workflow_variables(None::<&HashMap<String, VariableDeclaration>>).unwrap();
        assert!(vars.snapshot().is_empty());
    }

    #[test]
    fn 空声明_构造_空变量集() {
        let map = HashMap::new();
        let vars = build_workflow_variables(Some(&map)).unwrap();
        assert!(vars.snapshot().is_empty());
    }

    #[test]
    fn 含声明_构造成功() {
        let mut decls = HashMap::new();
        decls.insert(
            "x".to_owned(),
            VariableDeclaration {
                variable_type: PinType::Integer,
                initial: serde_json::Value::from(42_i64),
            },
        );
        let vars = build_workflow_variables(Some(&decls)).unwrap();
        assert_eq!(
            vars.get("x").unwrap().value,
            serde_json::Value::from(42_i64)
        );
    }

    #[test]
    fn 初值类型不匹配_整体失败() {
        let mut decls = HashMap::new();
        decls.insert(
            "bad".to_owned(),
            VariableDeclaration {
                variable_type: PinType::Float,
                initial: serde_json::Value::from("not-a-number"),
            },
        );
        let err = build_workflow_variables(Some(&decls)).unwrap_err();
        assert!(matches!(err, EngineError::VariableInitialMismatch { .. }));
    }
}
