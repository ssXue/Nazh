//! Copilot 工具定义与分发：定义可被 AI 模型调用的只读/读写工具，
//! 并将工具调用分发到引擎内部 `API`。

#![allow(dead_code)]

use std::sync::Arc;

use nazh_engine::{
    AiToolCall, AiToolDefinition, AiToolResult, SharedConnectionManager, WorkflowGraph,
    WorkflowNodeDefinition,
};
use serde_json::json;
use tauri::{AppHandle, Emitter};
use tauri_bindings::list_node_types_response;
use uuid::Uuid;

use crate::registry::shared_node_registry;

/// 工具分发所需的引擎上下文快照（从 `DesktopState` 克隆）。
pub(crate) struct CopilotToolCtx {
    pub(crate) connection_manager: SharedConnectionManager,
    /// 运行时工作流摘要列表（在组装 ctx 时克隆）。
    pub(crate) workflow_summaries: Vec<serde_json::Value>,
    /// 当前活跃工作流 ID。
    pub(crate) active_workflow_id: Option<String>,
    /// 活跃项目的工程工作区路径（设备/能力资产读取依赖此路径）。
    pub(crate) workspace_path: Option<String>,
    pub(crate) app: AppHandle,
    /// 当前 copilot 流事件的 channel 名称（`copilot://stream/{streamId}`）。
    pub(crate) stream_event_name: String,
    /// 画布操作引用映射：AI 用的 ref → 实际节点 ID。
    pub(crate) ref_map: std::sync::Mutex<std::collections::HashMap<String, String>>,
}

/// 构建系统提示用的节点目录文本。
///
/// 自动扫描所有已注册节点类型，提取 pin schema 与 capabilities。
/// 无需手动维护——添加新节点后自动出现在目录中。
pub(crate) fn build_node_catalog_text() -> String {
    let registry = shared_node_registry();
    let types = registry.registered_types();
    let resources: nazh_engine::SharedResources = Arc::new(
        nazh_engine::RuntimeResources::new()
            .with_resource(nazh_engine::shared_connection_manager()),
    );

    let mut lines = Vec::new();

    for node_type in &types {
        let definition = WorkflowNodeDefinition::probe(
            (*node_type).to_owned(),
            serde_json::Value::Object(serde_json::Map::new()),
        );

        let Ok(node) = registry.create(&definition, resources.clone()) else {
            continue;
        };

        let caps = registry
            .capabilities_of(node_type)
            .map(format_capabilities)
            .unwrap_or_default();

        let inputs = format_pins(&node.input_pins());
        let outputs = format_pins(&node.output_pins());

        lines.push(format!("- {node_type}"));

        if !inputs.is_empty() {
            lines.push(format!("  输入: {inputs}"));
        }
        if !outputs.is_empty() {
            lines.push(format!("  输出: {outputs}"));
        }
        if !caps.is_empty() {
            lines.push(format!("  特性: {caps}"));
        }
        if let Some(desc) = collect_pin_descriptions(&node.input_pins(), &node.output_pins()) {
            lines.push(format!("  说明: {desc}"));
        }
    }

    lines.join("\n")
}

/// 将 capabilities 位标志格式化为中文标签列表。
fn format_capabilities(caps: nazh_engine::NodeCapabilities) -> String {
    let mut tags = Vec::new();
    if caps.contains(nazh_engine::NodeCapabilities::TRIGGER) {
        tags.push("触发器");
    }
    if caps.contains(nazh_engine::NodeCapabilities::BRANCHING) {
        tags.push("条件分支");
    }
    if caps.contains(nazh_engine::NodeCapabilities::DEVICE_IO) {
        tags.push("设备I/O(需connectionId)");
    }
    if caps.contains(nazh_engine::NodeCapabilities::NETWORK_IO) {
        tags.push("网络I/O");
    }
    if caps.contains(nazh_engine::NodeCapabilities::FILE_IO) {
        tags.push("文件I/O");
    }
    if caps.contains(nazh_engine::NodeCapabilities::PURE) {
        tags.push("纯计算");
    }
    if caps.contains(nazh_engine::NodeCapabilities::MULTI_OUTPUT) {
        tags.push("多输出");
    }
    tags.join("、")
}

/// 将 pin 列表格式化为简洁文本，如 `in(json) → out(json)` 或多行格式。
fn format_pins(pins: &[nazh_engine::PinDefinition]) -> String {
    if pins.is_empty() {
        return String::new();
    }
    pins.iter()
        .map(|p| {
            let type_str = pin_type_label(&p.pin_type);
            let kind_suffix = match p.kind {
                nazh_engine::PinKind::Data => "/data",
                nazh_engine::PinKind::Reactive => "/reactive",
                nazh_engine::PinKind::Exec => "",
            };
            format!("{}({}{})", p.id, type_str, kind_suffix)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// 从引脚描述中收集有意义的说明文本。
fn collect_pin_descriptions(
    inputs: &[nazh_engine::PinDefinition],
    outputs: &[nazh_engine::PinDefinition],
) -> Option<String> {
    let descs: Vec<String> = inputs
        .iter()
        .chain(outputs.iter())
        .filter_map(|p| p.description.as_ref())
        .filter(|d| !d.is_empty())
        .cloned()
        .collect();
    if descs.is_empty() {
        None
    } else {
        Some(descs.join("; "))
    }
}

/// 将 `PinType` 格式化为简短标签。
fn pin_type_label(ty: &nazh_engine::PinType) -> String {
    match ty {
        nazh_engine::PinType::Any => "any".to_owned(),
        nazh_engine::PinType::Bool => "bool".to_owned(),
        nazh_engine::PinType::Integer => "int".to_owned(),
        nazh_engine::PinType::Float => "float".to_owned(),
        nazh_engine::PinType::String => "string".to_owned(),
        nazh_engine::PinType::Json => "json".to_owned(),
        nazh_engine::PinType::Binary => "binary".to_owned(),
        nazh_engine::PinType::Array { .. } => "array".to_owned(),
        nazh_engine::PinType::Custom { name } => format!("custom:{name}"),
    }
}

/// 返回所有 copilot 工具定义。
#[allow(clippy::too_many_lines)]
pub fn all_copilot_tools() -> Vec<AiToolDefinition> {
    vec![
        AiToolDefinition {
            name: "query_node_catalog".to_owned(),
            description: "列出工作流引擎中所有可用的节点类型及其描述。".to_owned(),
            parameters: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        AiToolDefinition {
            name: "describe_node".to_owned(),
            description: "获取指定节点类型的输入/输出 pin schema。".to_owned(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "node_type": {
                        "type": "string",
                        "description": "节点类型标识符，如 timer、http、modbusRead 等"
                    }
                },
                "required": ["node_type"]
            }),
        },
        AiToolDefinition {
            name: "list_connections".to_owned(),
            description: "列出当前配置的所有连接（串口、Modbus、MQTT、HTTP 等）。".to_owned(),
            parameters: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        AiToolDefinition {
            name: "search_devices".to_owned(),
            description: "搜索已定义的设备 DSL 资产，返回设备 ID、名称、类型等摘要信息。".to_owned(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "keyword": {
                        "type": "string",
                        "description": "搜索关键词（匹配 ID、名称或类型）"
                    }
                },
                "required": []
            }),
        },
        AiToolDefinition {
            name: "search_capabilities".to_owned(),
            description: "搜索已定义的能力 DSL 资产，返回能力 ID、名称、关联设备等摘要信息。".to_owned(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "device_id": {
                        "type": "string",
                        "description": "按设备 ID 过滤"
                    },
                    "keyword": {
                        "type": "string",
                        "description": "搜索关键词（匹配 ID 或名称）"
                    }
                },
                "required": []
            }),
        },
        AiToolDefinition {
            name: "get_active_workflow".to_owned(),
            description: "获取当前活跃工作流的结构信息（节点列表、连接关系等）。".to_owned(),
            parameters: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        AiToolDefinition {
            name: "query_workflow_status".to_owned(),
            description: "获取所有已部署工作流的运行时状态摘要。".to_owned(),
            parameters: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        AiToolDefinition {
            name: "read_asset_yaml".to_owned(),
            description: "读取指定设备或能力资产的完整 YAML 内容。".to_owned(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "asset_type": {
                        "type": "string",
                        "enum": ["device", "capability"],
                        "description": "资产类型"
                    },
                    "asset_id": {
                        "type": "string",
                        "description": "资产 ID"
                    }
                },
                "required": ["asset_type", "asset_id"]
            }),
        },
        AiToolDefinition {
            name: "validate_workflow".to_owned(),
            description: "验证工作流 JSON 结构是否合法（DAG 拓扑校验、节点类型存在性）。不执行部署。".to_owned(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "workflow_json": {
                        "type": "string",
                        "description": "工作流 JSON 字符串，格式: { name?, nodes: { id: { type, config?, connection_id?, timeout_ms?, buffer? } }, edges: [{ from, to, source_port_id?, target_port_id? }], connections?: [...], variables?: {...} }"
                    }
                },
                "required": ["workflow_json"]
            }),
        },
        // --- 画布操作工具（增量式构建工作流）---
        AiToolDefinition {
            name: "create_workflow".to_owned(),
            description: "在画布上创建新工作流工程。当用户要求创建工作流时，先调用此工具初始化画布，然后依次调用 add_workflow_node 添加节点，再用 add_workflow_edge 连接节点。".to_owned(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "工程名称"
                    },
                    "description": {
                        "type": "string",
                        "description": "工程描述"
                    }
                },
                "required": []
            }),
        },
        AiToolDefinition {
            name: "add_workflow_node".to_owned(),
            description: "在画布上添加一个工作流节点。每个节点用 ref 标识（在后续 add_workflow_edge 中引用）。node_type 必须是 query_node_catalog 返回的合法类型。".to_owned(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "ref": {
                        "type": "string",
                        "description": "节点引用 ID（同一工作流内稳定的英文别名，如 timer、debug、modbus_read）"
                    },
                    "node_type": {
                        "type": "string",
                        "description": "节点类型标识符，必须是 query_node_catalog 返回的合法类型"
                    },
                    "label": {
                        "type": "string",
                        "description": "节点显示名称"
                    },
                    "config": {
                        "type": "object",
                        "description": "节点配置（如 {\"interval_ms\": 5000}）"
                    },
                    "connection_id": {
                        "type": "string",
                        "description": "关联的连接 ID（仅设备I/O节点需要）"
                    }
                },
                "required": ["ref", "node_type"]
            }),
        },
        AiToolDefinition {
            name: "add_workflow_edge".to_owned(),
            description: "在画布上连接两个节点。from_ref 和 to_ref 必须是之前 add_workflow_node 中定义的 ref 值。".to_owned(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "from_ref": {
                        "type": "string",
                        "description": "起始节点的 ref"
                    },
                    "to_ref": {
                        "type": "string",
                        "description": "目标节点的 ref"
                    },
                    "source_port_id": {
                        "type": "string",
                        "description": "起始节点的输出端口 ID（可选，默认使用第一个输出）"
                    },
                    "target_port_id": {
                        "type": "string",
                        "description": "目标节点的输入端口 ID（可选，默认使用第一个输入）"
                    }
                },
                "required": ["from_ref", "to_ref"]
            }),
        },
        // --- 画布编辑/删除工具 ---
        AiToolDefinition {
            name: "edit_workflow_node".to_owned(),
            description: "修改画布上已有节点的配置。node_id 必须是当前画布上存在的节点 ID（从画布状态中可见）。只传需要修改的字段，未传的字段保持不变。".to_owned(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "node_id": {
                        "type": "string",
                        "description": "目标节点的实际 ID"
                    },
                    "label": {
                        "type": "string",
                        "description": "新显示名称"
                    },
                    "config": {
                        "type": "object",
                        "description": "要更新的配置字段（与现有配置浅合并）"
                    },
                    "connection_id": {
                        "type": "string",
                        "description": "新的关联连接 ID"
                    }
                },
                "required": ["node_id"]
            }),
        },
        AiToolDefinition {
            name: "delete_workflow_node".to_owned(),
            description: "删除画布上的一个节点及其所有连线。node_id 必须是当前画布上存在的节点 ID。".to_owned(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "node_id": {
                        "type": "string",
                        "description": "要删除的节点实际 ID"
                    }
                },
                "required": ["node_id"]
            }),
        },
        AiToolDefinition {
            name: "delete_workflow_edge".to_owned(),
            description: "删除两个节点之间的连线。".to_owned(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "from": {
                        "type": "string",
                        "description": "起始节点 ID"
                    },
                    "to": {
                        "type": "string",
                        "description": "目标节点 ID"
                    }
                },
                "required": ["from", "to"]
            }),
        },
    ]
}

/// 将工具调用分发到对应的处理函数。
pub async fn dispatch_tool(call: &AiToolCall, ctx: &CopilotToolCtx) -> AiToolResult {
    let result = match call.name.as_str() {
        "query_node_catalog" => tool_query_node_catalog(call),
        "describe_node" => tool_describe_node(call),
        "list_connections" => tool_list_connections(call, ctx).await,
        "search_devices" => tool_search_devices(call, ctx).await,
        "search_capabilities" => tool_search_capabilities(call, ctx).await,
        "get_active_workflow" => tool_get_active_workflow(call, ctx),
        "query_workflow_status" => tool_query_workflow_status(call, ctx),
        "read_asset_yaml" => tool_read_asset_yaml(call, ctx).await,
        "validate_workflow" => tool_validate_workflow(call),
        "create_workflow" => tool_create_workflow(call, ctx),
        "add_workflow_node" => tool_add_workflow_node(call, ctx),
        "add_workflow_edge" => tool_add_workflow_edge(call, ctx),
        "edit_workflow_node" => tool_edit_workflow_node(call, ctx),
        "delete_workflow_node" => tool_delete_workflow_node(call, ctx),
        "delete_workflow_edge" => tool_delete_workflow_edge(call, ctx),
        _ => Err(format!("未知工具: {}", call.name)),
    };
    to_tool_result(call, result)
}

fn to_tool_result(call: &AiToolCall, result: Result<String, String>) -> AiToolResult {
    match result {
        Ok(content) => AiToolResult {
            tool_call_id: call.id.clone(),
            content,
            is_error: false,
        },
        Err(message) => AiToolResult {
            tool_call_id: call.id.clone(),
            content: message,
            is_error: true,
        },
    }
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
    use crate::commands::devices::list_device_assets;

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
    use crate::commands::devices::load_device_asset;

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

fn tool_create_workflow(call: &AiToolCall, ctx: &CopilotToolCtx) -> Result<String, String> {
    let args = parse_args(call)?;
    let name = args["name"].as_str().unwrap_or("新工作流");

    let _ = ctx.app.emit(
        &ctx.stream_event_name,
        json!({
            "canvasOp": {
                "type": "create_workflow",
                "name": name,
            }
        }),
    );

    Ok(format!("工作流「{name}」已创建，可以开始添加节点"))
}

fn tool_add_workflow_node(call: &AiToolCall, ctx: &CopilotToolCtx) -> Result<String, String> {
    let args = parse_args(call)?;
    let ref_id = args["ref"].as_str().ok_or("缺少 ref 参数")?.to_owned();
    let node_type = args["node_type"]
        .as_str()
        .ok_or("缺少 node_type 参数")?
        .to_owned();
    let label = args["label"].as_str().map(str::to_owned);
    let config = if args.get("config").is_some() {
        Some(args["config"].clone())
    } else {
        None
    };
    let connection_id = args["connection_id"].as_str().map(str::to_owned);

    // 分配确定性节点 ID
    let node_id = format!("ai_{}", Uuid::new_v4().as_simple());
    ctx.ref_map
        .lock()
        .map_err(|e| format!("ref_map 锁失败: {e}"))?
        .insert(ref_id.clone(), node_id.clone());

    tracing::info!(
        ref = %ref_id,
        node_id = %node_id,
        node_type = %node_type,
        "copilot add_workflow_node"
    );

    let _ = ctx.app.emit(
        &ctx.stream_event_name,
        json!({
            "canvasOp": {
                "type": "add_node",
                "nodeId": node_id,
                "ref": ref_id,
                "nodeType": node_type,
                "label": label,
                "config": config,
                "connectionId": connection_id,
            }
        }),
    );

    Ok(format!("节点 {ref_id}（{node_type}）已添加，ID: {node_id}"))
}

fn tool_add_workflow_edge(call: &AiToolCall, ctx: &CopilotToolCtx) -> Result<String, String> {
    let args = parse_args(call)?;
    let from_ref = args["from_ref"].as_str().ok_or("缺少 from_ref 参数")?;
    let to_ref = args["to_ref"].as_str().ok_or("缺少 to_ref 参数")?;
    let source_port_id = args["source_port_id"].as_str().map(str::to_owned);
    let target_port_id = args["target_port_id"].as_str().map(str::to_owned);

    let ref_map = ctx
        .ref_map
        .lock()
        .map_err(|e| format!("ref_map 锁失败: {e}"))?;
    let from_id = ref_map
        .get(from_ref)
        .ok_or_else(|| format!("未知 from_ref: {from_ref}，请先通过 add_workflow_node 创建该节点"))?
        .clone();
    let to_id = ref_map
        .get(to_ref)
        .ok_or_else(|| format!("未知 to_ref: {to_ref}，请先通过 add_workflow_node 创建该节点"))?
        .clone();
    drop(ref_map);

    tracing::info!(
        from_ref = %from_ref,
        to_ref = %to_ref,
        from_id = %from_id,
        to_id = %to_id,
        "copilot add_workflow_edge"
    );

    let _ = ctx.app.emit(
        &ctx.stream_event_name,
        json!({
            "canvasOp": {
                "type": "add_edge",
                "fromRef": from_ref,
                "toRef": to_ref,
                "fromId": from_id,
                "toId": to_id,
                "sourcePortId": source_port_id,
                "targetPortId": target_port_id,
            }
        }),
    );

    Ok(format!("连线 {from_ref} → {to_ref} 已添加"))
}

fn tool_edit_workflow_node(call: &AiToolCall, ctx: &CopilotToolCtx) -> Result<String, String> {
    let args = parse_args(call)?;
    let node_id = args["node_id"]
        .as_str()
        .ok_or("缺少 node_id 参数")?
        .to_owned();
    let label = args["label"].as_str().map(str::to_owned);
    let config = args.get("config").cloned();
    let connection_id = args["connection_id"].as_str().map(str::to_owned);

    tracing::info!(
        node_id = %node_id,
        "copilot edit_workflow_node"
    );

    let _ = ctx.app.emit(
        &ctx.stream_event_name,
        json!({
            "canvasOp": {
                "type": "update_node",
                "nodeId": node_id,
                "label": label,
                "config": config,
                "connectionId": connection_id,
            }
        }),
    );

    Ok(format!("节点 {node_id} 已更新"))
}

fn tool_delete_workflow_node(call: &AiToolCall, ctx: &CopilotToolCtx) -> Result<String, String> {
    let args = parse_args(call)?;
    let node_id = args["node_id"]
        .as_str()
        .ok_or("缺少 node_id 参数")?
        .to_owned();

    tracing::info!(
        node_id = %node_id,
        "copilot delete_workflow_node"
    );

    let _ = ctx.app.emit(
        &ctx.stream_event_name,
        json!({
            "canvasOp": {
                "type": "delete_node",
                "nodeId": node_id,
            }
        }),
    );

    Ok(format!("节点 {node_id} 已删除"))
}

fn tool_delete_workflow_edge(call: &AiToolCall, ctx: &CopilotToolCtx) -> Result<String, String> {
    let args = parse_args(call)?;
    let from = args["from"].as_str().ok_or("缺少 from 参数")?.to_owned();
    let to = args["to"].as_str().ok_or("缺少 to 参数")?.to_owned();

    tracing::info!(
        from = %from,
        to = %to,
        "copilot delete_workflow_edge"
    );

    let _ = ctx.app.emit(
        &ctx.stream_event_name,
        json!({
            "canvasOp": {
                "type": "delete_edge",
                "from": from,
                "to": to,
            }
        }),
    );

    Ok(format!("连线 {from} → {to} 已删除"))
}

/// 前端直调 Copilot 时使用的无状态工具调度。
///
/// 仅处理查询类工具（不包含画布操作，画布操作由前端直接执行）。
/// 返回工具执行结果的 JSON 字符串。
pub async fn dispatch_query_tool(
    tool_name: &str,
    arguments_json: &str,
    connection_manager: &nazh_engine::SharedConnectionManager,
    active_workflow_id: Option<&String>,
    workflow_summaries: &[serde_json::Value],
    workspace_path: Option<&String>,
    app: &tauri::AppHandle,
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
        stream_event_name: String::new(),
        ref_map: std::sync::Mutex::new(std::collections::HashMap::new()),
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
        _ => Err(format!("未知工具: {tool_name}")),
    }
}
