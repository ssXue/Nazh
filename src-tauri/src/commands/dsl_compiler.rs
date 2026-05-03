//! Workflow DSL 编译器 IPC 命令域（RFC-0004 Phase 4B）。
//!
//! 提供 DSL 编译和资产快照加载命令，供前端 DSL 编辑器和 AI 编排控制台消费。

use std::sync::Arc;

use nazh_dsl_core::{parse_workflow_yaml, CapabilitySpec, DeviceSpec};
use dsl_compiler::{compile, CompilerContext};
use serde::{Deserialize, Serialize};
use store::Store;
use tauri::State;

use crate::state::DesktopState;

// ---- IPC 类型 ----

/// DSL 编译请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompileWorkflowRequest {
    /// WorkflowSpec YAML 文本。
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

// ---- 辅助函数 ----

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
