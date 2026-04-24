//! 插件集成测试：验证标准库插件注册的节点类型完整性。

#[cfg(test)]
mod tests {
    use crate::standard_registry;
    use nazh_core::NodeCapabilities;

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

    #[test]
    fn 标准注册表节点能力标签与_adr_0011_契约一致() {
        let registry = standard_registry();

        let expect = |name: &str, caps: NodeCapabilities| {
            assert_eq!(
                registry.capabilities_of(name),
                Some(caps),
                "节点 {name} 能力标签与 ADR-0011 约定不符"
            );
        };

        // 流程控制
        expect("code", NodeCapabilities::empty());
        expect("if", NodeCapabilities::PURE | NodeCapabilities::BRANCHING);
        expect(
            "switch",
            NodeCapabilities::PURE | NodeCapabilities::BRANCHING,
        );
        expect("tryCatch", NodeCapabilities::BRANCHING);
        expect(
            "loop",
            NodeCapabilities::BRANCHING | NodeCapabilities::MULTI_OUTPUT,
        );

        // I/O
        expect("native", NodeCapabilities::empty());
        expect("debugConsole", NodeCapabilities::empty());
        expect("timer", NodeCapabilities::TRIGGER);
        expect(
            "serialTrigger",
            NodeCapabilities::TRIGGER | NodeCapabilities::DEVICE_IO,
        );
        expect("modbusRead", NodeCapabilities::DEVICE_IO);
        expect("httpClient", NodeCapabilities::NETWORK_IO);
        expect("mqttClient", NodeCapabilities::NETWORK_IO);
        expect("barkPush", NodeCapabilities::NETWORK_IO);
        expect("sqlWriter", NodeCapabilities::FILE_IO);
    }
}
