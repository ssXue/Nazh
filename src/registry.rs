//! 插件集成测试：验证标准库插件注册的节点类型完整性。

#[cfg(test)]
mod tests {
    use crate::standard_registry;

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
            "mqttClient",
            "httpClient",
            "barkPush",
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
    fn 两个插件合并后覆盖全部_14_种节点类型() {
        let registry = standard_registry();
        assert_eq!(
            registry.registered_types().len(),
            14,
            "应注册 14 种节点类型"
        );
    }
}
