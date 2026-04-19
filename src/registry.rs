//! 插件集成测试：验证标准库插件注册的节点类型完整性。

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::standard_registry;
    use nazh_core::{NodeRegistry, SharedResources, WorkflowNodeDefinition};

    fn stub_factory(
        _def: &WorkflowNodeDefinition,
        _res: SharedResources,
    ) -> Result<std::sync::Arc<dyn nazh_core::NodeTrait>, nazh_core::EngineError> {
        Err(nazh_core::EngineError::unsupported_node_type("test-stub"))
    }

    #[test]
    fn registered_types_list_returns_sorted_entries() {
        let mut registry = NodeRegistry::new();
        registry.register("code", stub_factory);
        registry.register("native", stub_factory);
        registry.register("timer", stub_factory);

        let entries = registry.registered_types_list();

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].name, "code");
        assert_eq!(entries[1].name, "native");
        assert_eq!(entries[2].name, "timer");
    }

    #[test]
    fn registered_types_list_empty_registry() {
        let registry = NodeRegistry::new();
        let entries = registry.registered_types_list();
        assert!(entries.is_empty());
    }

    #[test]
    fn flow_plugin_注册全部流程控制节点() {
        let registry = standard_registry();
        let types = registry.registered_types();

        for expected in ["if", "switch", "tryCatch", "loop", "code"] {
            assert!(
                types.contains(&expected),
                "FlowPlugin 缺少节点类型: {expected}"
            );
        }
    }

    #[test]
    fn io_plugin_注册全部_io_节点() {
        let registry = standard_registry();
        let types = registry.registered_types();

        for expected in [
            "native",
            "timer",
            "serialTrigger",
            "modbusRead",
            "httpClient",
            "sqlWriter",
            "debugConsole",
        ] {
            assert!(
                types.contains(&expected),
                "IoPlugin 缺少节点类型: {expected}"
            );
        }
    }

    #[test]
    fn 两个插件合并后覆盖全部_12_种节点类型() {
        let entries = standard_registry().registered_types_list();
        assert_eq!(entries.len(), 12, "应注册 12 种节点类型");
    }
}
