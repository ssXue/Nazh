//! 插件集成测试：验证标准库插件注册的节点类型完整性。

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use crate::standard_registry;
    use nazh_core::NodeCapabilities;

    #[test]
    fn flow_plugin_注册全部流程控制节点() {
        let registry = standard_registry();
        let types = registry.registered_types();

        for expected in [
            "if",
            "switch",
            "tryCatch",
            "loop",
            "code",
            "subgraphInput",
            "subgraphOutput",
        ] {
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
            "humanLoop",
            "canRead",
            "canWrite",
        ] {
            assert!(
                types.contains(&expected),
                "IoPlugin 缺少节点类型: {expected}"
            );
        }
    }

    #[test]
    fn 两个插件合并后覆盖全部_24_种节点类型() {
        let registry = standard_registry();
        assert_eq!(
            registry.registered_types().len(),
            24,
            "应注册 24 种节点类型"
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

        // 子图桥接（ADR-0013）— 透传无副作用
        expect("subgraphInput", NodeCapabilities::PURE);
        expect("subgraphOutput", NodeCapabilities::PURE);

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
        expect("humanLoop", NodeCapabilities::BRANCHING);
        expect("canRead", NodeCapabilities::DEVICE_IO);
        expect("canWrite", NodeCapabilities::DEVICE_IO);

        // RFC-0004 Phase 3：DSL 编译器生成的节点
        expect("stateMachine", NodeCapabilities::BRANCHING);
        expect("capabilityCall", NodeCapabilities::DEVICE_IO);
    }

    #[test]
    fn pure_plugin_注册全部纯计算节点() {
        let registry = standard_registry();
        let types = registry.registered_types();

        for expected in ["c2f", "minutesSince", "lookup"] {
            assert!(
                types.contains(&expected),
                "PurePlugin 缺少节点类型: {expected}"
            );
        }
    }

    #[test]
    fn pure_plugin_节点能力标签符合_adr_0011_契约() {
        let registry = standard_registry();
        assert_eq!(
            registry.capabilities_of("c2f"),
            Some(NodeCapabilities::PURE),
            "c2f 同输入必得同输出，应声明 PURE"
        );
        assert_eq!(
            registry.capabilities_of("minutesSince"),
            Some(NodeCapabilities::empty()),
            "minutesSince 读取系统时钟，不能声明 PURE"
        );
        assert_eq!(
            registry.capabilities_of("lookup"),
            Some(NodeCapabilities::PURE),
            "lookup 同输入必得同输出，应声明 PURE"
        );
    }
}
