//! Copilot 查询工具分发。
//!
//! AI 调用已前移到前端（RFC-0005），本模块仅保留前端通过 IPC 调度的
//! 只读查询工具（节点目录、连接状态、设备/能力资产、Rhai API 参考等）。
//! 画布操作工具由前端直接执行，不经过 Rust。

use std::sync::Arc;

use nazh_engine::{AiToolCall, SharedConnectionManager, WorkflowGraph, WorkflowNodeDefinition};
use serde_json::json;
use tauri::AppHandle;
use tauri_bindings::list_node_types_response;

use crate::registry::shared_node_registry;

/// 查询工具分发所需的引擎上下文快照。
pub(crate) struct CopilotToolCtx {
    pub(crate) connection_manager: SharedConnectionManager,
    /// 运行时工作流摘要列表（在组装 ctx 时克隆）。
    pub(crate) workflow_summaries: Vec<serde_json::Value>,
    /// 当前活跃工作流 ID。
    pub(crate) active_workflow_id: Option<String>,
    /// 活跃项目的工程工作区路径（设备/能力资产读取依赖此路径）。
    pub(crate) workspace_path: Option<String>,
    pub(crate) app: AppHandle,
}

/// 解析工具调用的 JSON 参数。
fn parse_args(call: &AiToolCall) -> Result<serde_json::Value, String> {
    if call.arguments.trim().is_empty() {
        Ok(json!({}))
    } else {
        serde_json::from_str(&call.arguments).map_err(|e| format!("参数解析失败: {e}"))
    }
}

fn tool_query_node_catalog(_call: &AiToolCall) -> Result<String, String> {
    let registry = shared_node_registry();
    let response = list_node_types_response(registry);
    serde_json::to_string_pretty(&response).map_err(|e| format!("序列化节点目录失败: {e}"))
}

fn tool_describe_node(call: &AiToolCall) -> Result<String, String> {
    let args = parse_args(call)?;
    let node_type = args["node_type"].as_str().ok_or("缺少 node_type 参数")?;

    let registry = shared_node_registry();
    let definition = WorkflowNodeDefinition::probe(
        node_type.to_owned(),
        serde_json::Value::Object(serde_json::Map::new()),
    );

    let resources: nazh_engine::SharedResources = Arc::new(
        nazh_engine::RuntimeResources::new()
            .with_resource(nazh_engine::shared_connection_manager()),
    );

    let node = registry
        .create(&definition, resources)
        .map_err(|e| format!("无法实例化节点 `{node_type}`: {e}"))?;

    let result = json!({
        "node_type": node_type,
        "input_pins": node.input_pins(),
        "output_pins": node.output_pins(),
    });
    serde_json::to_string_pretty(&result).map_err(|e| format!("序列化节点描述失败: {e}"))
}

async fn tool_list_connections(_call: &AiToolCall, ctx: &CopilotToolCtx) -> Result<String, String> {
    let connections = ctx.connection_manager.list().await;
    let summaries: Vec<serde_json::Value> = connections
        .iter()
        .map(|c| {
            json!({
                "id": c.id,
                "kind": c.kind,
                "in_use": c.in_use,
            })
        })
        .collect();
    serde_json::to_string_pretty(&summaries).map_err(|e| format!("序列化连接列表失败: {e}"))
}

async fn tool_search_devices(call: &AiToolCall, ctx: &CopilotToolCtx) -> Result<String, String> {
    use crate::commands::devices::assets::list_device_assets;

    let args = parse_args(call)?;
    let keyword = args["keyword"].as_str().unwrap_or("").to_lowercase();

    let devices = list_device_assets(ctx.app.clone(), ctx.workspace_path.clone()).await?;

    let filtered: Vec<_> = devices
        .into_iter()
        .filter(|d| {
            if keyword.is_empty() {
                return true;
            }
            d.id.to_lowercase().contains(&keyword)
                || d.name.to_lowercase().contains(&keyword)
                || d.device_type.to_lowercase().contains(&keyword)
        })
        .map(|d| {
            json!({
                "id": d.id,
                "name": d.name,
                "device_type": d.device_type,
                "version": d.version,
            })
        })
        .collect();

    serde_json::to_string_pretty(&filtered).map_err(|e| format!("序列化设备列表失败: {e}"))
}

async fn tool_search_capabilities(
    call: &AiToolCall,
    ctx: &CopilotToolCtx,
) -> Result<String, String> {
    use crate::commands::capabilities::list_capabilities;

    let args = parse_args(call)?;
    let keyword = args["keyword"].as_str().unwrap_or("").to_lowercase();
    let device_filter = args["device_id"].as_str();

    let capabilities = list_capabilities(
        ctx.app.clone(),
        device_filter.map(str::to_owned),
        ctx.workspace_path.clone(),
    )
    .await?;

    let filtered: Vec<_> = capabilities
        .into_iter()
        .filter(|c| {
            if keyword.is_empty() {
                return true;
            }
            c.id.to_lowercase().contains(&keyword) || c.name.to_lowercase().contains(&keyword)
        })
        .map(|c| {
            json!({
                "id": c.id,
                "device_id": c.device_id,
                "name": c.name,
                "description": c.description,
                "version": c.version,
            })
        })
        .collect();

    serde_json::to_string_pretty(&filtered).map_err(|e| format!("序列化能力列表失败: {e}"))
}

fn tool_get_active_workflow(_call: &AiToolCall, ctx: &CopilotToolCtx) -> Result<String, String> {
    let Some(active_id) = &ctx.active_workflow_id else {
        return Ok("当前没有活跃的工作流".to_owned());
    };

    let summary = ctx
        .workflow_summaries
        .iter()
        .find(|s| s["workflow_id"].as_str() == Some(active_id.as_str()));

    let Some(summary) = summary else {
        return Ok(format!("活跃工作流 `{active_id}` 未找到"));
    };

    serde_json::to_string_pretty(&summary).map_err(|e| format!("序列化工作流信息失败: {e}"))
}

fn tool_query_workflow_status(_call: &AiToolCall, ctx: &CopilotToolCtx) -> Result<String, String> {
    if ctx.workflow_summaries.is_empty() {
        return Ok("当前没有已部署的工作流".to_owned());
    }
    serde_json::to_string_pretty(&ctx.workflow_summaries)
        .map_err(|e| format!("序列化工作流状态失败: {e}"))
}

async fn tool_read_asset_yaml(call: &AiToolCall, ctx: &CopilotToolCtx) -> Result<String, String> {
    use crate::commands::capabilities::load_capability;
    use crate::commands::devices::assets::load_device_asset;

    let args = parse_args(call)?;
    let asset_type = args["asset_type"].as_str().ok_or("缺少 asset_type 参数")?;
    let asset_id = args["asset_id"].as_str().ok_or("缺少 asset_id 参数")?;

    match asset_type {
        "device" => {
            let asset = load_device_asset(
                ctx.app.clone(),
                asset_id.to_owned(),
                ctx.workspace_path.clone(),
            )
            .await?
            .ok_or_else(|| format!("设备 `{asset_id}` 不存在"))?;
            Ok(asset.spec_yaml)
        }
        "capability" => {
            let asset = load_capability(
                ctx.app.clone(),
                asset_id.to_owned(),
                ctx.workspace_path.clone(),
            )
            .await?
            .ok_or_else(|| format!("能力 `{asset_id}` 不存在"))?;
            Ok(asset.spec_yaml)
        }
        _ => Err(format!("不支持的资产类型: {asset_type}")),
    }
}

fn tool_validate_workflow(call: &AiToolCall) -> Result<String, String> {
    let args = parse_args(call)?;
    let workflow_json = args["workflow_json"]
        .as_str()
        .ok_or("缺少 workflow_json 参数")?;

    let graph = WorkflowGraph::from_json(workflow_json)
        .map_err(|e| format!("工作流 JSON 解析失败: {e}"))?;

    let registry = shared_node_registry();
    let mut unknown_types: Vec<String> = Vec::new();
    for (node_id, node_def) in &graph.nodes {
        let definition = WorkflowNodeDefinition::probe(
            node_def.node_type().to_owned(),
            node_def.config().clone(),
        );
        let resources: nazh_engine::SharedResources = Arc::new(
            nazh_engine::RuntimeResources::new()
                .with_resource(nazh_engine::shared_connection_manager()),
        );
        if registry.create(&definition, resources).is_err() {
            unknown_types.push(format!("{}: {}", node_id, node_def.node_type()));
        }
    }

    if !unknown_types.is_empty() {
        let result = json!({
            "valid": false,
            "errors": unknown_types.iter()
                .map(|t| format!("未知节点类型: {t}"))
                .collect::<Vec<_>>()
        });
        return serde_json::to_string_pretty(&result).map_err(|e| format!("序列化失败: {e}"));
    }

    let result = json!({
        "valid": true,
        "node_count": graph.nodes.len(),
        "edge_count": graph.edges.len(),
    });
    serde_json::to_string_pretty(&result).map_err(|e| format!("序列化失败: {e}"))
}

fn tool_get_scripting_reference(_call: &AiToolCall) -> String {
    scripting::generate_api_reference()
}

/// 前端 copilot 查询工具调度入口。
///
/// 仅处理只读查询工具，画布操作由前端直接执行。
/// 返回工具执行结果的 JSON 字符串。
pub async fn dispatch_query_tool(
    tool_name: &str,
    arguments_json: &str,
    connection_manager: &SharedConnectionManager,
    active_workflow_id: Option<&String>,
    workflow_summaries: &[serde_json::Value],
    workspace_path: Option<&String>,
    app: &AppHandle,
) -> Result<String, String> {
    let call = AiToolCall {
        id: String::new(),
        name: tool_name.to_owned(),
        arguments: arguments_json.to_owned(),
    };

    let ctx = CopilotToolCtx {
        connection_manager: Arc::clone(connection_manager),
        workflow_summaries: workflow_summaries.to_vec(),
        active_workflow_id: active_workflow_id.cloned(),
        workspace_path: workspace_path.cloned(),
        app: app.clone(),
    };

    match tool_name {
        "query_node_catalog" => tool_query_node_catalog(&call),
        "describe_node" => tool_describe_node(&call),
        "list_connections" => tool_list_connections(&call, &ctx).await,
        "search_devices" => tool_search_devices(&call, &ctx).await,
        "search_capabilities" => tool_search_capabilities(&call, &ctx).await,
        "get_active_workflow" => tool_get_active_workflow(&call, &ctx),
        "query_workflow_status" => tool_query_workflow_status(&call, &ctx),
        "read_asset_yaml" => tool_read_asset_yaml(&call, &ctx).await,
        "validate_workflow" => tool_validate_workflow(&call),
        "get_scripting_reference" => Ok(tool_get_scripting_reference(&call)),
        _ => Err(format!("未知工具: {tool_name}")),
    }
}
