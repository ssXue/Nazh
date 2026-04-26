//! Nazh Tauri 命令的请求/响应类型集中地。
//!
//! 这些类型只服务于 Tauri 桌面壳层与前端的 IPC 契约，不属于引擎运行时；
//! 因此从 Ring 0（`nazh-core`）迁出，独立成一个 crate。详见 ADR-0017。
//!
//! `ts-rs` 通过 `ts-export` feature 启用，CI 用
//! `cargo test -p tauri-bindings --features ts-export export_bindings`
//! 触发本 crate 与所有依赖 crate 的 TypeScript 类型导出。

use nazh_core::{NodeRegistry, PinDefinition};
use serde::{Deserialize, Serialize};

#[cfg(feature = "ts-export")]
use ts_rs::TS;

/// 工作流部署成功后的响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct DeployResponse {
    pub node_count: usize,
    pub edge_count: usize,
    pub root_nodes: Vec<String>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub project_id: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub workflow_id: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub replaced_existing: Option<bool>,
}

/// 载荷分发成功后的响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct DispatchResponse {
    pub trace_id: String,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub workflow_id: Option<String>,
}

/// 工作流卸载后的响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct UndeployResponse {
    pub had_workflow: bool,
    pub aborted_timer_count: usize,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub workflow_id: Option<String>,
}

/// 已注册节点类型的信息条目。
///
/// `capabilities` 是 [`nazh_core::NodeCapabilities`] 的原始位图（`u32::bits()`），
/// 前端需按 ADR-0011 定义的位分配解读。位分配与常量表同步在
/// `web/src/lib/nodeCapabilities.ts`。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct NodeTypeEntry {
    /// 节点类型主名称（如 "code"）。
    pub name: String,
    /// 类型级能力标签位图（详见 ADR-0011）。
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(type = "number"))]
    pub capabilities: u32,
}

/// `list_node_types` IPC 命令的响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ListNodeTypesResponse {
    pub types: Vec<NodeTypeEntry>,
}

/// `describe_node_pins` IPC 命令的请求。
///
/// 给定节点类型 + config，返回该实例化节点的输入/输出引脚 schema。
/// 服务于 ADR-0010 Phase 2 前端连接期校验：FlowGram `canAddLine` 钩子
/// 通过缓存的 pin schema 即时判断"上游产出 → 下游期望"是否兼容。
///
/// 注意：`config` 必须是合法的节点 config（能让 `NodeRegistry::create`
/// 成功）。无效 config 会返回错误，前端缓存写 fallback `Any/Any`，
/// 部署期校验作为 backstop 兜底。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct DescribeNodePinsRequest {
    /// 节点类型主名称（如 `"modbusRead"` / `"switch"` / `"mqttClient"`）。
    pub node_type: String,
    /// 节点 config（与 `WorkflowNodeDefinition::config` 同 schema）。
    pub config: serde_json::Value,
}

/// `describe_node_pins` IPC 命令的响应。
///
/// 直接返回 [`PinDefinition`] 列表，前端 ts-rs 已导出该类型——
/// 与节点 trait 的 `input_pins(&self)` / `output_pins(&self)` 同形态。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct DescribeNodePinsResponse {
    pub input_pins: Vec<PinDefinition>,
    pub output_pins: Vec<PinDefinition>,
}

/// 把 [`NodeRegistry`] 中的节点类型按字母排序后包装成 [`ListNodeTypesResponse`]。
///
/// 排序属于 IPC 展示层关注点，不污染 Ring 0 的注册表 API。
pub fn list_node_types_response(registry: &NodeRegistry) -> ListNodeTypesResponse {
    let mut names: Vec<String> = registry
        .registered_types()
        .into_iter()
        .map(str::to_owned)
        .collect();
    names.sort_unstable();
    ListNodeTypesResponse {
        types: names
            .into_iter()
            .map(|name| {
                let capabilities = registry.capabilities_of(&name).unwrap_or_default().bits();
                NodeTypeEntry { name, capabilities }
            })
            .collect(),
    }
}

/// 触发本 crate 与所有依赖 crate 的 ts-rs 导出。
///
/// 集中入口避免新增类型时漏导出；CI 通过 `git diff --exit-code -- web/src/generated/`
/// 兜底，开发者改了 Rust 类型却忘了 regenerate 会立刻失败。
#[cfg(feature = "ts-export")]
pub fn export_all() -> Result<(), ts_rs::ExportError> {
    nazh_core::export_bindings::export_all()?;
    connections::export_bindings::export_all()?;
    ai::export_bindings::export_all()?;
    nazh_engine::export_bindings::export_all()?;

    DeployResponse::export()?;
    DispatchResponse::export()?;
    UndeployResponse::export()?;
    NodeTypeEntry::export()?;
    ListNodeTypesResponse::export()?;
    DescribeNodePinsRequest::export()?;
    DescribeNodePinsResponse::export()?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use nazh_core::{
        EngineError, NodeCapabilities, NodeTrait, SharedResources, WorkflowNodeDefinition,
    };
    use std::sync::Arc;

    fn stub_factory(
        _def: &WorkflowNodeDefinition,
        _res: SharedResources,
    ) -> Result<Arc<dyn NodeTrait>, EngineError> {
        Err(EngineError::unsupported_node_type("test-stub"))
    }

    #[test]
    fn list_node_types_response_排序后输出全部类型() {
        let mut registry = NodeRegistry::new();
        registry.register_with_capabilities("timer", NodeCapabilities::empty(), stub_factory);
        registry.register_with_capabilities("code", NodeCapabilities::empty(), stub_factory);
        registry.register_with_capabilities("native", NodeCapabilities::empty(), stub_factory);

        let response = list_node_types_response(&registry);
        assert_eq!(response.types.len(), 3);
        assert_eq!(response.types[0].name, "code");
        assert_eq!(response.types[1].name, "native");
        assert_eq!(response.types[2].name, "timer");
    }

    #[test]
    fn list_node_types_response_空注册表返回空列表() {
        let registry = NodeRegistry::new();
        let response = list_node_types_response(&registry);
        assert!(response.types.is_empty());
    }

    #[test]
    fn list_node_types_response_透传能力标签位图() {
        let mut registry = NodeRegistry::new();
        registry.register_with_capabilities("timer", NodeCapabilities::TRIGGER, stub_factory);
        registry.register_with_capabilities(
            "modbusRead",
            NodeCapabilities::DEVICE_IO,
            stub_factory,
        );
        registry.register_with_capabilities("plain", NodeCapabilities::empty(), stub_factory);

        let response = list_node_types_response(&registry);
        let by_name: std::collections::HashMap<&str, u32> = response
            .types
            .iter()
            .map(|entry| (entry.name.as_str(), entry.capabilities))
            .collect();

        assert_eq!(by_name["timer"], NodeCapabilities::TRIGGER.bits());
        assert_eq!(by_name["modbusRead"], NodeCapabilities::DEVICE_IO.bits());
        assert_eq!(by_name["plain"], 0);
    }

    #[cfg(feature = "ts-export")]
    #[test]
    fn export_bindings() {
        super::export_all().unwrap();
    }
}
