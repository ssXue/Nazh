//! 插件系统：统一的插件注册接口与节点工厂注册表。
//!
//! [`Plugin`] trait 定义了插件向引擎贡献能力的统一接口。
//! [`NodeRegistry`] 管理节点类型名称到工厂函数的映射。
//! 每个 Ring 1 crate 实现 `Plugin` 并在 `register()` 中注册自己的节点工厂。

use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::{EngineError, NodeTrait};

/// 部署时传递给节点工厂的共享资源（通过 downcast 访问具体类型）。
pub type SharedResources = Arc<dyn Any + Send + Sync>;

fn default_node_buffer() -> usize {
    32
}

/// 工作流图中的单节点配置。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct WorkflowNodeDefinition {
    #[serde(default)]
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    #[serde(default)]
    #[ts(optional)]
    pub connection_id: Option<String>,
    #[serde(default)]
    pub config: Value,
    #[serde(default)]
    #[ts(optional)]
    pub ai_description: Option<String>,
    #[serde(default)]
    #[ts(optional, type = "number")]
    pub timeout_ms: Option<u64>,
    #[serde(default = "default_node_buffer")]
    pub buffer: usize,
}

impl<'de> Deserialize<'de> for WorkflowNodeDefinition {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Input {
            #[serde(default)]
            id: String,
            #[serde(rename = "type", alias = "kind")]
            node_type: String,
            #[serde(default)]
            connection_id: Option<String>,
            #[serde(default)]
            config: Value,
            #[serde(default)]
            ai_description: Option<String>,
            #[serde(default)]
            timeout_ms: Option<u64>,
            #[serde(default = "default_node_buffer")]
            buffer: usize,
        }

        let input = Input::deserialize(deserializer)?;
        Ok(Self {
            id: input.id,
            node_type: input.node_type,
            connection_id: input.connection_id,
            config: input.config,
            ai_description: input.ai_description,
            timeout_ms: input.timeout_ms,
            buffer: input.buffer,
        })
    }
}

impl WorkflowNodeDefinition {
    /// 从 `config` 字段反序列化出指定类型的配置结构体。
    pub fn parse_config<T: serde::de::DeserializeOwned>(&self) -> Result<T, EngineError> {
        serde_json::from_value(self.config.clone())
            .map_err(|error| EngineError::node_config(self.id.clone(), error.to_string()))
    }

    /// 获取节点的 AI 描述，若未配置则使用 `fallback`。
    pub fn resolve_description(&self, fallback: &str) -> String {
        self.ai_description
            .clone()
            .unwrap_or_else(|| fallback.to_owned())
    }
}

/// 节点工厂函数签名。
type FactoryFn = dyn Fn(&WorkflowNodeDefinition, SharedResources) -> Result<Arc<dyn NodeTrait>, EngineError>
    + Send
    + Sync;

/// 节点注册表，管理节点类型名称到工厂函数的映射。
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

    /// 注册一个节点工厂。若该名称已存在，新工厂会覆盖旧工厂。
    pub fn register<F>(&mut self, node_type: impl Into<String>, factory: F)
    where
        F: Fn(&WorkflowNodeDefinition, SharedResources) -> Result<Arc<dyn NodeTrait>, EngineError>
            + Send
            + Sync
            + 'static,
    {
        self.factories.insert(node_type.into(), Arc::new(factory));
    }

    /// 为已注册的节点类型添加别名（共享同一个工厂函数实例）。
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
    pub fn create(
        &self,
        definition: &WorkflowNodeDefinition,
        resources: SharedResources,
    ) -> Result<Arc<dyn NodeTrait>, EngineError> {
        let factory = self
            .factories
            .get(&definition.node_type)
            .ok_or_else(|| EngineError::unsupported_node_type(&definition.node_type))?;
        factory(definition, resources)
    }

    /// 返回所有已注册的节点类型名称（含别名）。
    pub fn registered_types(&self) -> Vec<&str> {
        self.factories.keys().map(String::as_str).collect()
    }

    /// 返回已注册节点类型的列表，按工厂函数指针去重合并别名。
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

/// 插件元信息。
pub struct PluginManifest {
    pub name: &'static str,
    pub version: &'static str,
}

/// 插件向引擎贡献能力的统一接口。
///
/// 每个 Ring 1 crate 实现此 trait，在 `register()` 中向 `NodeRegistry`
/// 注册自己的节点工厂和别名。引擎启动时通过 `PluginHost` 加载所有插件。
pub trait Plugin: Send + Sync {
    fn manifest(&self) -> PluginManifest;
    fn register(&self, registry: &mut NodeRegistry);
}

/// 插件宿主，按顺序加载插件并构建 `NodeRegistry`。
pub struct PluginHost {
    registry: NodeRegistry,
}

impl PluginHost {
    pub fn new() -> Self {
        Self {
            registry: NodeRegistry::new(),
        }
    }

    /// 加载一个插件，将其贡献注册到内部 `NodeRegistry`。
    pub fn load(&mut self, plugin: &dyn Plugin) {
        let manifest = plugin.manifest();
        tracing::info!(
            plugin = manifest.name,
            version = manifest.version,
            "加载插件"
        );
        plugin.register(&mut self.registry);
    }

    /// 消费宿主，返回构建好的 `NodeRegistry`。
    pub fn into_registry(self) -> NodeRegistry {
        self.registry
    }
}

impl Default for PluginHost {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::{ContextRef, DataStore, NodeExecution};
    use async_trait::async_trait;

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
        fn ai_description(&self) -> &str {
            ""
        }
        async fn execute(
            &self,
            _ctx: &ContextRef,
            _store: &dyn DataStore,
        ) -> Result<NodeExecution, EngineError> {
            Ok(NodeExecution::broadcast(serde_json::Value::Null))
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
            registry.register("alpha", |def, _res| {
                Ok(Arc::new(StubNode {
                    id: def.id.clone(),
                    kind: "alpha",
                }))
            });
            let _ = registry.alias("a", "alpha");
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
            registry.register("beta", |def, _res| {
                Ok(Arc::new(StubNode {
                    id: def.id.clone(),
                    kind: "beta",
                }))
            });
        }
    }

    fn stub_resources() -> SharedResources {
        Arc::new(())
    }

    fn node_def(id: &str, node_type: &str) -> WorkflowNodeDefinition {
        WorkflowNodeDefinition {
            id: id.to_owned(),
            node_type: node_type.to_owned(),
            connection_id: None,
            config: serde_json::Value::Object(Default::default()),
            ai_description: None,
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
    fn 插件注册的别名可解析到同一工厂() {
        let mut host = PluginHost::new();
        host.load(&TestPlugin);
        let registry = host.into_registry();

        let node = registry
            .create(&node_def("n2", "a"), stub_resources())
            .unwrap();
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
    fn 未注册的节点类型返回错误() {
        let host = PluginHost::new();
        let registry = host.into_registry();

        let result = registry.create(&node_def("n1", "nonexistent"), stub_resources());
        assert!(result.is_err());
    }

    #[test]
    fn 资源通过_downcast_传递到工厂() {
        struct Marker(u64);

        let mut registry = NodeRegistry::new();
        registry.register("check", |def, res| {
            let marker = res
                .downcast_ref::<Marker>()
                .ok_or_else(|| EngineError::invalid_graph("downcast 失败"))?;
            Ok(Arc::new(StubNode {
                id: format!("{}:{}", def.id, marker.0),
                kind: "check",
            }))
        });

        let resources: SharedResources = Arc::new(Marker(42));
        let node = registry
            .create(&node_def("n1", "check"), resources)
            .unwrap();
        assert_eq!(node.id(), "n1:42");
    }
}
