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

    #[test]
    fn flow_plugin_注册全部流程控制节点() {
        let registry = standard_registry();
        let types = registry.registered_types();

        for expected in ["if", "switch", "tryCatch", "loop", "rhai", "code", "code/rhai"] {
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
            "native/log",
            "log",
            "timer",
            "serialTrigger",
            "serial/trigger",
            "serial",
            "modbusRead",
            "modbus/read",
            "httpClient",
            "http/client",
            "sqlWriter",
            "sql/writer",
            "debugConsole",
            "debug/console",
        ] {
            assert!(
                types.contains(&expected),
                "IoPlugin 缺少节点类型: {expected}"
            );
        }
    }

    #[test]
    fn 两个插件合并后覆盖全部_12_种主节点类型() {
        let entries = standard_registry().registered_types_with_aliases();
        assert_eq!(entries.len(), 12, "应注册 12 种主节点类型（去重别名后）");
    }
}
