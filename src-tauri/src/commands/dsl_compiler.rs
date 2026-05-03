//! Workflow DSL 编译器 IPC 命令域（RFC-0004 Phase 4B + 4C）。
//!
//! 提供 DSL 编译、资产快照加载和 AI 编排生成命令。

use std::sync::Arc;

use nazh_dsl_core::{parse_workflow_yaml, CapabilitySpec, DeviceSpec};
use dsl_compiler::{compile, CompilerContext};
use nazh_engine::{AiCompletionRequest, AiGenerationParams, AiMessage, AiMessageRole, AiService};
use serde::{Deserialize, Serialize};
use store::Store;
use tauri::{AppHandle, Emitter, State};

use crate::state::DesktopState;
use super::devices::UncertaintyItem;

// ---- IPC 类型 ----

/// DSL 编译请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompileWorkflowRequest {
    /// `WorkflowSpec` YAML 文本。
    pub workflow_yaml: String,
}

/// DSL 编译诊断条目。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticItem {
    /// 严重程度。
    pub severity: String,
    /// 消息内容。
    pub message: String,
}

/// DSL 编译结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompileWorkflowResponse {
    /// 编译成功时输出的 `WorkflowGraph` JSON。
    pub graph_json: Option<serde_json::Value>,
    /// 编译失败时的错误信息。
    pub error: Option<String>,
    /// 校验诊断信息（警告，非致命）。
    pub diagnostics: Vec<DiagnosticItem>,
}

/// 编译器资产快照（只读）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompilerAssetSnapshot {
    /// 所有设备资产的 `DeviceSpec` JSON 列表。
    pub devices: Vec<serde_json::Value>,
    /// 所有能力资产的 `CapabilitySpec` JSON 列表。
    pub capabilities: Vec<serde_json::Value>,
}

// ---- 命令 ----

/// 编译 Workflow DSL YAML 为 `WorkflowGraph` JSON。
#[tauri::command]
pub(crate) async fn compile_workflow_dsl(
    state: State<'_, DesktopState>,
    request: CompileWorkflowRequest,
) -> Result<CompileWorkflowResponse, String> {
    // 解析 WorkflowSpec YAML
    let spec = parse_workflow_yaml(&request.workflow_yaml)
        .map_err(|e| format!("Workflow DSL 解析失败: {e}"))?;

    // 从 Store 构建编译上下文
    let (devices, capabilities) = load_asset_snapshots(&state)?;

    let ctx = CompilerContext::new(devices, capabilities);

    // 引用校验
    let diagnostics = Vec::new();
    if let Err(e) = ctx.validate_references(&spec) {
        return Ok(CompileWorkflowResponse {
            graph_json: None,
            error: Some(e.to_string()),
            diagnostics,
        });
    }

    // 编译
    match compile(&ctx, &spec) {
        Ok(graph_json) => Ok(CompileWorkflowResponse {
            graph_json: Some(graph_json),
            error: None,
            diagnostics,
        }),
        Err(e) => Ok(CompileWorkflowResponse {
            graph_json: None,
            error: Some(e.to_string()),
            diagnostics,
        }),
    }
}

/// 加载编译器资产快照（所有设备 + 能力）。
#[tauri::command]
pub(crate) async fn load_compiler_asset_snapshot(
    state: State<'_, DesktopState>,
) -> Result<CompilerAssetSnapshot, String> {
    let (devices, capabilities) = load_asset_snapshots(&state)?;
    let device_jsons: Vec<serde_json::Value> = devices
        .iter()
        .filter_map(|d| serde_json::to_value(d).ok())
        .collect();
    let cap_jsons: Vec<serde_json::Value> = capabilities
        .iter()
        .filter_map(|c| serde_json::to_value(c).ok())
        .collect();
    Ok(CompilerAssetSnapshot {
        devices: device_jsons,
        capabilities: cap_jsons,
    })
}

// ---- AI 编排生成（RFC-0004 Phase 4C）----

/// AI 生成的 Workflow DSL 提案。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiWorkflowDslProposal {
    /// AI 生成的 `WorkflowSpec` YAML。
    pub workflow_yaml: String,
    /// AI 标记的不确定项。
    pub uncertainties: Vec<UncertaintyItem>,
    /// AI 生成的警告。
    pub warnings: Vec<String>,
    /// 自动编译结果。
    pub compile_result: Option<CompileWorkflowResponse>,
}

/// AI proposal 的 JSON 输出结构（内部解析用）。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawAiProposal {
    workflow_yaml: String,
    #[serde(default)]
    uncertainties: Vec<UncertaintyItem>,
    #[serde(default)]
    warnings: Vec<String>,
}

const ORCHESTRATION_SYSTEM_PROMPT: &str = "你是一个工业工作流编排专家。根据用户的自然语言目标和可用设备能力，\
    生成 Workflow DSL YAML。\n\n\
    输出严格 JSON 格式，结构如下：\n\
    {\"workflowYaml\": \"<WorkflowSpec YAML 文本>\", \
     \"uncertainties\": [{\"fieldPath\": \"...\", \"guessedValue\": \"...\", \"reason\": \"...\"}], \
     \"warnings\": [\"...\"]}\n\n\
    WorkflowSpec YAML 结构参考：\n\
    ```yaml\n\
    id: <工作流唯一标识>\n\
    description: <描述>\n\
    version: \"1.0.0\"\n\
    devices:\n\
      - <设备 ID>\n\
    variables:\n\
      <变量名>: <初始值>\n\
    states:\n\
      <状态名>:\n\
        entry:\n\
          - capability: <能力 ID>\n\
            args:\n\
              <参数名>: <值>\n\
    transitions:\n\
      - from: <源状态>\n\
        to: <目标状态>\n\
        when: <Rhai 条件表达式>\n\
    timeout:\n\
      <状态名>: <时长，如 60s>\n\
    on_timeout: <超时转入的状态>\n\
    ```\n\n\
    规则：\n\
    - 只引用上下文中列出的设备 ID 和能力 ID\n\
    - transition 的 when 表达式使用 Rhai 语法\n\
    - timeout 格式为数字+单位（如 30s、5m）\n\
    - uncertainties 标记不确定的设计决策\n\
    - warnings 标记潜在的安全或可靠性问题\n\
    - 只输出 JSON，不要解释";

/// AI 生成 Workflow DSL 提案。
#[tauri::command]
pub(crate) async fn ai_generate_workflow_dsl(
    state: State<'_, DesktopState>,
    goal: String,
    provider_id: Option<String>,
) -> Result<AiWorkflowDslProposal, String> {
    // 从 Store 加载资产快照，构建上下文描述
    let (devices, capabilities) = load_asset_snapshots(&state)?;
    let context_desc = build_asset_context_description(&devices, &capabilities);

    let request = build_orchestration_request(&goal, &context_desc, provider_id);

    let response = state
        .ai_service
        .complete(request)
        .await
        .map_err(|e| format!("AI 编排生成失败: {e}"))?;

    // 提取 JSON
    let json_text = extract_json_from_ai_response(&response.content);
    let raw: RawAiProposal =
        serde_json::from_str(&json_text).map_err(|e| format!("AI 输出 JSON 结构无效: {e}"))?;

    // 自动编译验证
    let compile_result = match parse_workflow_yaml(&raw.workflow_yaml) {
        Ok(spec) => {
            let ctx = CompilerContext::new(devices, capabilities);
            match ctx.validate_references(&spec) {
                Ok(()) => match compile(&ctx, &spec) {
                    Ok(graph_json) => Some(CompileWorkflowResponse {
                        graph_json: Some(graph_json),
                        error: None,
                        diagnostics: vec![],
                    }),
                    Err(e) => Some(CompileWorkflowResponse {
                        graph_json: None,
                        error: Some(e.to_string()),
                        diagnostics: vec![],
                    }),
                },
                Err(e) => Some(CompileWorkflowResponse {
                    graph_json: None,
                    error: Some(e.to_string()),
                    diagnostics: vec![],
                }),
            }
        }
        Err(e) => Some(CompileWorkflowResponse {
            graph_json: None,
            error: Some(format!("YAML 解析失败: {e}")),
            diagnostics: vec![],
        }),
    };

    Ok(AiWorkflowDslProposal {
        workflow_yaml: raw.workflow_yaml,
        uncertainties: raw.uncertainties,
        warnings: raw.warnings,
        compile_result,
    })
}

/// 流式 AI 生成 Workflow DSL 提案。
#[tauri::command]
pub(crate) async fn ai_generate_workflow_dsl_stream(
    app: AppHandle,
    state: State<'_, DesktopState>,
    goal: String,
    provider_id: Option<String>,
    stream_id: String,
) -> Result<(), String> {
    let (devices, capabilities) = load_asset_snapshots(&state)?;
    let context_desc = build_asset_context_description(&devices, &capabilities);
    let request = build_orchestration_request(&goal, &context_desc, provider_id);
    let service = Arc::clone(&state.ai_service);
    let mut rx = service
        .stream_complete(request)
        .await
        .map_err(|e| format!("启动 AI 流式编排失败: {e}"))?;

    let event_name = format!("copilot://stream/{stream_id}");
    tokio::spawn(async move {
        while let Some(chunk_result) = rx.recv().await {
            match chunk_result {
                Ok(chunk) => {
                    let is_done = chunk.done;
                    let payload: serde_json::Value =
                        serde_json::to_value(&chunk).unwrap_or_default();
                    let _ = app.emit(&event_name, payload);
                    if is_done {
                        break;
                    }
                }
                Err(error) => {
                    let payload: serde_json::Value = serde_json::json!({
                        "error": error.to_string(),
                        "done": true
                    });
                    let _ = app.emit(&event_name, payload);
                    break;
                }
            }
        }
    });

    Ok(())
}

// ---- AI 辅助函数 ----

/// 从设备和能力列表构建上下文描述文本（嵌入 prompt）。
fn build_asset_context_description(devices: &[DeviceSpec], capabilities: &[CapabilitySpec]) -> String {
    let mut lines: Vec<String> = vec![String::from("可用设备资产：")];
    for d in devices {
        lines.push(format!(
            "- {} (type={}, connection={})",
            d.id, d.device_type, d.connection.connection_type,
        ));
        for s in &d.signals {
            let type_str = serde_json::to_value(s.signal_type)
                .map(|v| v.to_string())
                .unwrap_or_default();
            lines.push(format!(
                "  信号: {} ({}, unit={})",
                s.id,
                type_str,
                s.unit.as_deref().unwrap_or("-"),
            ));
        }
    }
    lines.push(String::from("\n可用能力资产："));
    for c in capabilities {
        lines.push(format!(
            "- {} (device={}, desc={})",
            c.id, c.device_id, c.description,
        ));
        for inp in &c.inputs {
            lines.push(format!("  输入: {} (required={})", inp.id, inp.required));
        }
        if !c.preconditions.is_empty() {
            lines.push(format!("  前置条件: {}", c.preconditions.join("; ")));
        }
        let level_str = serde_json::to_value(c.safety.level)
            .map(|v| v.to_string())
            .unwrap_or_default();
        lines.push(format!("  安全等级: {level_str}"));
    }
    lines.join("\n")
}

/// 构建 AI 编排请求。
fn build_orchestration_request(
    goal: &str,
    context_desc: &str,
    provider_id: Option<String>,
) -> AiCompletionRequest {
    AiCompletionRequest {
        provider_id: provider_id.unwrap_or_default(),
        model: None,
        messages: vec![
            AiMessage {
                role: AiMessageRole::System,
                content: ORCHESTRATION_SYSTEM_PROMPT.to_owned(),
            },
            AiMessage {
                role: AiMessageRole::User,
                content: format!("{context_desc}\n\n用户目标：\n{goal}"),
            },
        ],
        params: AiGenerationParams {
            temperature: Some(0.2),
            max_tokens: Some(8192),
            ..Default::default()
        },
        timeout_ms: None,
    }
}

/// 从 AI 响应中提取 JSON 内容。
fn extract_json_from_ai_response(content: &str) -> String {
    let trimmed = content.trim();
    if let Some(stripped) = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        && let Some(inner) = stripped.strip_suffix("```")
    {
        return inner.trim().to_owned();
    }
    trimmed.to_owned()
}

// ---- Store 辅助函数 ----

/// 获取 Store 的 Arc 引用。
fn get_store(state: &State<'_, DesktopState>) -> Result<Arc<Store>, String> {
    state
        .store
        .read()
        .map(|guard| Arc::clone(&guard))
        .map_err(|e| format!("Store 锁异常: {e}"))
}

/// 从 Store 加载所有设备和能力资产。
fn load_asset_snapshots(
    state: &State<'_, DesktopState>,
) -> Result<(Vec<DeviceSpec>, Vec<CapabilitySpec>), String> {
    let store = get_store(state)?;

    // 加载所有设备资产
    let device_summaries = store
        .list_device_assets()
        .map_err(|e| format!("列出设备资产失败: {e}"))?;

    let mut devices = Vec::new();
    for summary in &device_summaries {
        if let Some(asset) = store
            .load_device_asset(&summary.id)
            .map_err(|e| format!("加载设备 `{}` 失败: {e}", summary.id))?
        {
            let spec: DeviceSpec = serde_json::from_value(asset.spec_json)
                .map_err(|e| format!("设备 `{}` 反序列化失败: {e}", summary.id))?;
            devices.push(spec);
        }
    }

    // 加载所有能力资产
    let cap_summaries = store
        .list_capabilities(None)
        .map_err(|e| format!("列出能力资产失败: {e}"))?;

    let mut capabilities = Vec::new();
    for summary in &cap_summaries {
        if let Some(cap) = store
            .load_capability(&summary.id)
            .map_err(|e| format!("加载能力 `{}` 失败: {e}", summary.id))?
        {
            let spec: CapabilitySpec = serde_json::from_value(cap.spec_json)
                .map_err(|e| format!("能力 `{}` 反序列化失败: {e}", summary.id))?;
            capabilities.push(spec);
        }
    }

    Ok((devices, capabilities))
}
