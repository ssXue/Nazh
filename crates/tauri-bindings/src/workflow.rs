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
/// 服务于前端连接期校验——FlowGram `canAddLine` 钩子通过缓存的 pin
/// schema 即时判断"上游产出 → 下游期望"是否兼容。
///
/// 注意：`config` 必须是合法的节点 config（能让 [`NodeRegistry::create`]
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
