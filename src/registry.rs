//! 节点注册表：将节点类型名称映射到工厂函数。
//!
//! [`NodeRegistry`] 是引擎核心与具体节点实现之间的解耦层。
//! 引擎核心（DAG runner、拓扑排序、通道布线）完全不知道任何具体节点类型，
//! 只通过注册表按名称查找工厂函数来实例化节点。
//!
//! ## 设计原则
//!
//! - **引擎核心零内置节点**：所有节点类型（包括 if / switch 等流程原语）
//!   都通过注册表接入，没有任何硬编码。
//! - **统一注册接口**：无论是编译期预注册的"标准库"节点，还是运行时
//!   动态发现的 Sidecar 插件，都通过同一个 [`NodeRegistry::register`] 入口。
//! - **别名支持**：同一节点可注册多个名称（如 `"rhai"` / `"code"` / `"code/rhai"`），
//!   前端 AST 使用任意别名均可正确解析。

use std::collections::HashMap;
use std::sync::Arc;

use crate::graph::types::WorkflowNodeDefinition;
use crate::{EngineError, NodeTrait, SharedConnectionManager};

/// 节点工厂函数签名。
///
/// 接收节点定义和共享连接管理器，返回一个 trait object 节点实例。
/// 工厂函数必须满足 `Send + Sync`，因为注册表可能跨线程共享。
type FactoryFn = dyn Fn(&WorkflowNodeDefinition, SharedConnectionManager) -> Result<Arc<dyn NodeTrait>, EngineError>
    + Send
    + Sync;

/// 节点注册表，管理节点类型名称到工厂函数的映射。
///
/// # 使用方式
///
/// ```rust,ignore
/// use nazh_engine::{NodeRegistry, SharedConnectionManager};
///
/// // 创建包含所有标准库节点的注册表
/// let registry = NodeRegistry::with_standard_nodes();
///
/// // 也可以在标准库基础上追加自定义节点
/// let mut registry = NodeRegistry::with_standard_nodes();
/// registry.register("my_custom", |def, cm| {
///     // ... 创建自定义节点
///     # todo!()
/// });
/// ```
pub struct NodeRegistry {
    factories: HashMap<String, Arc<FactoryFn>>,
}

impl NodeRegistry {
    /// 创建一个空的注册表。
    pub fn new() -> Self {
        Self {
            factories: HashMap::new(),
        }
    }

    /// 创建一个预注册了所有标准库节点的注册表。
    ///
    /// 标准库包含引擎内置的全部节点类型（流程原语 + 常用 I/O + 调试工具），
    /// 等价于此前硬编码在 `instantiate.rs` match 中的所有分支。
    #[must_use]
    pub fn with_standard_nodes() -> Self {
        let mut registry = Self::new();
        super::graph::instantiate::register_standard_nodes(&mut registry);
        registry
    }

    /// 注册一个节点工厂。
    ///
    /// 若该名称已存在，新工厂会覆盖旧工厂。
    pub fn register<F>(&mut self, node_type: impl Into<String>, factory: F)
    where
        F: Fn(
                &WorkflowNodeDefinition,
                SharedConnectionManager,
            ) -> Result<Arc<dyn NodeTrait>, EngineError>
            + Send
            + Sync
            + 'static,
    {
        self.factories.insert(node_type.into(), Arc::new(factory));
    }

    /// 为已注册的节点类型添加别名。
    ///
    /// 别名与原名共享同一个工厂函数实例（`Arc` 克隆）。
    ///
    /// # Errors
    ///
    /// 若 `canonical` 名称未注册，返回 [`EngineError::UnsupportedNodeType`]。
    pub fn alias(&mut self, alias: impl Into<String>, canonical: &str) -> Result<(), EngineError> {
        let factory = self
            .factories
            .get(canonical)
            .ok_or_else(|| EngineError::unsupported_node_type(canonical))?
            .clone();
        self.factories.insert(alias.into(), factory);
        Ok(())
    }

    /// 根据节点定义中的 `node_type` 查找工厂并创建节点实例。
    ///
    /// # Errors
    ///
    /// 节点类型未注册或工厂函数执行失败时返回错误。
    pub fn create(
        &self,
        definition: &WorkflowNodeDefinition,
        connection_manager: SharedConnectionManager,
    ) -> Result<Arc<dyn NodeTrait>, EngineError> {
        let factory = self
            .factories
            .get(&definition.node_type)
            .ok_or_else(|| EngineError::unsupported_node_type(&definition.node_type))?;
        factory(definition, connection_manager)
    }

    /// 返回所有已注册的节点类型名称（含别名）。
    pub fn registered_types(&self) -> Vec<&str> {
        self.factories.keys().map(String::as_str).collect()
    }

    /// 返回已注册节点类型的列表，按工厂函数指针去重合并别名。
    ///
    /// 同一个工厂函数被注册为多个名称时，选择最短名称作为主名称，
    /// 其余名称作为别名。
    pub fn registered_types_with_aliases(&self) -> Vec<crate::NodeTypeEntry> {
        let mut seen: Vec<Arc<FactoryFn>> = Vec::new();
        let mut entries: Vec<crate::NodeTypeEntry> = Vec::new();

        for (name, factory) in &self.factories {
            if let Some(pos) = seen.iter().position(|f| Arc::ptr_eq(f, factory)) {
                entries[pos].aliases.push(name.clone());
            } else {
                seen.push(factory.clone());
                entries.push(crate::NodeTypeEntry {
                    name: name.clone(),
                    aliases: vec![],
                });
            }
        }

        for entry in &mut entries {
            let mut all = vec![entry.name.clone()];
            all.append(&mut entry.aliases);
            all.sort_by(|a, b| a.len().cmp(&b.len()).then(a.cmp(b)));
            entry.name = all.remove(0);
            entry.aliases = all;
        }

        entries.sort_by(|a, b| a.name.cmp(&b.name));
        entries
    }
}

impl Default for NodeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::NodeRegistry;

    fn stub_factory(
        _def: &crate::graph::types::WorkflowNodeDefinition,
        _cm: crate::SharedConnectionManager,
    ) -> Result<std::sync::Arc<dyn crate::NodeTrait>, crate::EngineError> {
        Err(crate::EngineError::unsupported_node_type("test-stub"))
    }

    #[test]
    fn registered_types_with_aliases_groups_aliases() {
        let mut registry = NodeRegistry::new();
        registry.register("rhai", stub_factory);
        let _ = registry.alias("code", "rhai");
        let _ = registry.alias("code/rhai", "rhai");

        registry.register("native", stub_factory);
        let _ = registry.alias("log", "native");

        registry.register("timer", stub_factory);

        let entries = registry.registered_types_with_aliases();

        assert_eq!(entries.len(), 3);

        let code_entry = entries.iter().find(|e| e.name == "code").unwrap();
        assert_eq!(code_entry.aliases, vec!["rhai", "code/rhai"]);

        let log_entry = entries.iter().find(|e| e.name == "log").unwrap();
        assert_eq!(log_entry.aliases, vec!["native"]);

        let timer_entry = entries.iter().find(|e| e.name == "timer").unwrap();
        assert!(timer_entry.aliases.is_empty());
    }

    #[test]
    fn registered_types_with_aliases_empty_registry() {
        let registry = NodeRegistry::new();
        let entries = registry.registered_types_with_aliases();
        assert!(entries.is_empty());
    }
}
