use super::*;
use crate::{EngineError, NodeExecution};
use async_trait::async_trait;
use serde_json::Value;
use uuid::Uuid;

struct StubNode {
    id: String,
    kind: &'static str,
}

#[async_trait]
impl NodeTrait for StubNode {
    fn id(&self) -> &str {
        &self.id
    }
    fn kind(&self) -> &'static str {
        self.kind
    }
    async fn transform(
        &self,
        _trace_id: Uuid,
        _payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        Ok(NodeExecution::broadcast(Value::Null))
    }
}

struct TestPlugin;

impl Plugin for TestPlugin {
    fn manifest(&self) -> PluginManifest {
        PluginManifest {
            name: "test-plugin",
            version: "0.1.0",
        }
    }

    fn register(&self, registry: &mut NodeRegistry) {
        registry.register_with_capabilities("alpha", NodeCapabilities::empty(), |def, _res| {
            Ok(Arc::new(StubNode {
                id: def.id.clone(),
                kind: "alpha",
            }))
        });
    }
}

struct AnotherPlugin;

impl Plugin for AnotherPlugin {
    fn manifest(&self) -> PluginManifest {
        PluginManifest {
            name: "another",
            version: "0.2.0",
        }
    }

    fn register(&self, registry: &mut NodeRegistry) {
        registry.register_with_capabilities("beta", NodeCapabilities::empty(), |def, _res| {
            Ok(Arc::new(StubNode {
                id: def.id.clone(),
                kind: "beta",
            }))
        });
    }
}

fn stub_resources() -> SharedResources {
    Arc::new(RuntimeResources::new())
}

fn node_def(id: &str, node_type: &str) -> WorkflowNodeDefinition {
    WorkflowNodeDefinition {
        id: id.to_owned(),
        node_type: node_type.to_owned(),
        connection_id: None,
        config: serde_json::Value::Object(serde_json::Map::default()),
        timeout_ms: None,
        buffer: 32,
    }
}

#[test]
fn 插件注册的节点可通过注册表创建() {
    let mut host = PluginHost::new();
    host.load(&TestPlugin);
    let registry = host.into_registry();

    let node = registry
        .create(&node_def("n1", "alpha"), stub_resources())
        .unwrap();
    assert_eq!(node.id(), "n1");
    assert_eq!(node.kind(), "alpha");
}

#[test]
fn 多插件按顺序加载合并注册表() {
    let mut host = PluginHost::new();
    host.load(&TestPlugin);
    host.load(&AnotherPlugin);
    let registry = host.into_registry();

    assert!(
        registry
            .create(&node_def("n1", "alpha"), stub_resources())
            .is_ok()
    );
    assert!(
        registry
            .create(&node_def("n2", "beta"), stub_resources())
            .is_ok()
    );
}

#[test]
fn register_with_capabilities_登记类型级能力() {
    let mut registry = NodeRegistry::new();
    let caps = NodeCapabilities::NETWORK_IO | NodeCapabilities::TRIGGER;
    registry.register_with_capabilities("mqtt", caps, |def, _res| {
        Ok(Arc::new(StubNode {
            id: def.id.clone(),
            kind: "mqtt",
        }))
    });

    assert_eq!(registry.capabilities_of("mqtt"), Some(caps));
}

#[test]
fn register_with_capabilities_可显式声明空能力() {
    let mut registry = NodeRegistry::new();
    registry.register_with_capabilities("plain", NodeCapabilities::empty(), |def, _res| {
        Ok(Arc::new(StubNode {
            id: def.id.clone(),
            kind: "plain",
        }))
    });

    assert_eq!(
        registry.capabilities_of("plain"),
        Some(NodeCapabilities::empty())
    );
}

#[test]
fn capabilities_of_未注册类型返回_none() {
    let registry = NodeRegistry::new();
    assert!(registry.capabilities_of("nonexistent").is_none());
}

#[test]
fn 未注册的节点类型返回错误() {
    let host = PluginHost::new();
    let registry = host.into_registry();

    let result = registry.create(&node_def("n1", "nonexistent"), stub_resources());
    assert!(result.is_err());
}

#[test]
fn 资源通过类型读取传递到工厂() {
    #[derive(Clone)]
    struct Marker(u64);

    let mut registry = NodeRegistry::new();
    registry.register_with_capabilities("check", NodeCapabilities::empty(), |def, res| {
        let marker = res
            .get::<Marker>()
            .ok_or_else(|| EngineError::invalid_graph("downcast 失败"))?;
        Ok(Arc::new(StubNode {
            id: format!("{}:{}", def.id, marker.0),
            kind: "check",
        }))
    });

    let resources: SharedResources = Arc::new(RuntimeResources::new().with_resource(Marker(42)));
    let node = registry
        .create(&node_def("n1", "check"), resources)
        .unwrap();
    assert_eq!(node.id(), "n1:42");
}

#[test]
fn 资源包可同时携带多种资源() {
    #[derive(Clone)]
    struct MarkerA(&'static str);
    #[derive(Clone)]
    struct MarkerB(u64);

    let resources = RuntimeResources::new()
        .with_resource(MarkerA("alpha"))
        .with_resource(MarkerB(7));

    assert_eq!(resources.get::<MarkerA>().unwrap().0, "alpha");
    assert_eq!(resources.get::<MarkerB>().unwrap().0, 7);
}
