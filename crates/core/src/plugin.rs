//! 插件系统：统一的插件注册接口与节点工厂注册表。
//!
//! [`Plugin`] trait 定义了插件向引擎贡献能力的统一接口。
//! [`NodeRegistry`] 管理节点类型名称到工厂函数的映射。
//! 每个 Ring 1 crate 实现 `Plugin` 并在 `register()` 中注册自己的节点工厂。

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
#[cfg(feature = "ts-export")]
use ts_rs::TS;

use crate::{EngineError, NodeCapabilities, NodeTrait};

/// 部署时传递给节点工厂的共享资源包。
///
/// 资源按具体类型存取，避免不同节点插件之间耦合到同一个聚合结构。
#[derive(Default)]
pub struct RuntimeResources {
    entries: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl RuntimeResources {
    /// 创建一个空的运行时资源包。
    pub fn new() -> Self {
        Self::default()
    }

    /// 插入一个可克隆的资源，并返回自身以便链式构建。
    #[must_use]
    pub fn with_resource<T>(mut self, resource: T) -> Self
    where
        T: Any + Clone + Send + Sync,
    {
        self.insert(resource);
        self
    }

    /// 插入一个可克隆的资源。
    pub fn insert<T>(&mut self, resource: T)
    where
        T: Any + Clone + Send + Sync,
    {
        self.entries.insert(TypeId::of::<T>(), Box::new(resource));
    }

    /// 按类型读取资源副本；若不存在则返回 `None`。
    pub fn get<T>(&self) -> Option<T>
    where
        T: Any + Clone + Send + Sync,
    {
        self.entries
            .get(&TypeId::of::<T>())
            .and_then(|entry| entry.downcast_ref::<T>())
            .cloned()
    }
}

/// 部署时传递给节点工厂的共享资源句柄。
pub type SharedResources = Arc<RuntimeResources>;

fn default_node_buffer() -> usize {
    32
}

/// 工作流图中的单节点配置。
///
/// 字段私有以防止未来引入校验/不变量时被外部直接绕过。外部读取通过
/// 同名访问器（`id()` / `node_type()` / ...），规范化缺省值通过 [`normalize`] 方法。
///
/// [`normalize`]: WorkflowNodeDefinition::normalize
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
pub struct WorkflowNodeDefinition {
    #[serde(default)]
    id: String,
    #[serde(rename = "type")]
    node_type: String,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    connection_id: Option<String>,
    #[serde(default)]
    config: Value,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional, type = "number"))]
    timeout_ms: Option<u64>,
    #[serde(default = "default_node_buffer")]
    buffer: usize,
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
            timeout_ms: input.timeout_ms,
            buffer: input.buffer,
        })
    }
}

impl WorkflowNodeDefinition {
    pub fn id(&self) -> &str {
        &self.id
    }

    /// 节点类型名称（如 `"code"`、`"timer"`、`"modbusRead"`）。
    pub fn node_type(&self) -> &str {
        &self.node_type
    }

    /// 节点绑定的连接资源标识（若有）。
    pub fn connection_id(&self) -> Option<&str> {
        self.connection_id.as_deref()
    }

    pub fn config(&self) -> &Value {
        &self.config
    }

    /// 仅用于部署前小幅改写 config JSON（如 sqlite 相对路径 → 绝对路径）。
    pub fn config_mut(&mut self) -> &mut Value {
        &mut self.config
    }

    pub fn timeout_ms(&self) -> Option<u64> {
        self.timeout_ms
    }

    pub fn buffer(&self) -> usize {
        self.buffer
    }

    /// 规范化节点定义的缺省值：
    /// - `id` 为空时填入 `fallback_id`（通常是图中的节点键）；
    /// - `connection_id` 为 `None` 时填入 `fallback_connection_id`。
    ///
    /// 该方法仅填充缺失字段，不覆盖已有值。
    pub fn normalize(&mut self, fallback_id: &str, fallback_connection_id: Option<&str>) {
        if self.id.is_empty() {
            fallback_id.clone_into(&mut self.id);
        }
        if self.connection_id.is_none()
            && let Some(value) = fallback_connection_id
        {
            self.connection_id = Some(value.to_owned());
        }
    }

    pub fn parse_config<T: serde::de::DeserializeOwned>(&self) -> Result<T, EngineError> {
        serde_json::from_value(self.config.clone())
            .map_err(|error| EngineError::node_config(self.id.clone(), error.to_string()))
    }
}

/// 节点工厂函数签名。
type FactoryFn = dyn Fn(&WorkflowNodeDefinition, SharedResources) -> Result<Arc<dyn NodeTrait>, EngineError>
    + Send
    + Sync;

/// 注册表中单个节点类型的全部信息——工厂 + 类型级能力标签。
struct NodeEntry {
    factory: Arc<FactoryFn>,
    capabilities: NodeCapabilities,
}

/// 节点注册表，管理节点类型名称到工厂函数与能力标签的映射。
///
/// 能力标签 [`NodeCapabilities`] 在注册时以「类型级别」登记，供前端渲染、
/// 可观测性分桶与未来调度策略使用，无需实例化节点即可查询。
pub struct NodeRegistry {
    entries: HashMap<String, NodeEntry>,
}

impl NodeRegistry {
    /// 创建一个空的注册表。
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// 注册节点工厂并声明类型级能力标签（ADR-0011）。
    ///
    /// 若该名称已存在，新工厂与新能力标签均会覆盖旧值。
    pub fn register_with_capabilities<F>(
        &mut self,
        node_type: impl Into<String>,
        capabilities: NodeCapabilities,
        factory: F,
    ) where
        F: Fn(&WorkflowNodeDefinition, SharedResources) -> Result<Arc<dyn NodeTrait>, EngineError>
            + Send
            + Sync
            + 'static,
    {
        self.entries.insert(
            node_type.into(),
            NodeEntry {
                factory: Arc::new(factory),
                capabilities,
            },
        );
    }

    /// 根据节点定义中的 `node_type` 查找工厂并创建节点实例。
    pub fn create(
        &self,
        definition: &WorkflowNodeDefinition,
        resources: SharedResources,
    ) -> Result<Arc<dyn NodeTrait>, EngineError> {
        let entry = self
            .entries
            .get(definition.node_type())
            .ok_or_else(|| EngineError::unsupported_node_type(definition.node_type()))?;
        (entry.factory)(definition, resources)
    }

    /// 返回所有已注册的节点类型名称。
    ///
    /// 顺序未定义；调用方需要排序请自行处理。Tauri IPC 层在
    /// `tauri_bindings::list_node_types_response` 中负责字母排序与封装。
    pub fn registered_types(&self) -> Vec<&str> {
        self.entries.keys().map(String::as_str).collect()
    }

    /// 查询指定节点类型声明的能力标签。
    ///
    /// 未注册的类型返回 `None`；通过 [`register`](Self::register) 注册但未声明
    /// 能力的类型返回 `Some(NodeCapabilities::empty())`。
    pub fn capabilities_of(&self, node_type: &str) -> Option<NodeCapabilities> {
        self.entries.get(node_type).map(|entry| entry.capabilities)
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

        let resources: SharedResources =
            Arc::new(RuntimeResources::new().with_resource(Marker(42)));
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
}
