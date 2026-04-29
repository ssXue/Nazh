use std::sync::Arc;

use nazh_engine::{
    RuntimeResources, SharedResources, WorkflowNodeDefinition, shared_connection_manager,
};
use tauri_bindings::{
    DescribeNodePinsRequest, DescribeNodePinsResponse, ListNodeTypesResponse,
    list_node_types_response,
};

use crate::registry::shared_node_registry;

#[tauri::command]
pub(crate) async fn list_node_types() -> Result<ListNodeTypesResponse, String> {
    Ok(list_node_types_response(shared_node_registry()))
}

/// 给定节点类型 + config，返回该节点实例的 input/output pin schema。
///
/// 用于前端连接期校验：FlowGram `canAddLine` 钩子通过缓存的 pin schema
/// 即时判断"上游产出 → 下游期望"是否兼容，错连立刻拒绝并给视觉反馈。
///
/// 实例化是无副作用的（只读 config + 资源句柄克隆，不进入 `on_deploy`）。
/// 返回错误时前端会写 fallback `Any/Any` 缓存——部署期校验作为 backstop。
#[tauri::command]
pub(crate) async fn describe_node_pins(
    request: DescribeNodePinsRequest,
) -> Result<DescribeNodePinsResponse, String> {
    let definition = WorkflowNodeDefinition::probe(request.node_type, request.config);

    // 仅注入 connection_manager——describe_pins 不读连接，只让需要 conn 句柄的
    // 节点构造器（modbus / mqtt / http）能克隆出引用。无 AI service / observability，
    // 这些与 pin schema 无关。
    let resources: SharedResources =
        Arc::new(RuntimeResources::new().with_resource(shared_connection_manager()));

    let node = shared_node_registry()
        .create(&definition, resources)
        .map_err(|error| format!("无法实例化节点：{error}"))?;

    Ok(DescribeNodePinsResponse {
        input_pins: node.input_pins(),
        output_pins: node.output_pins(),
    })
}
