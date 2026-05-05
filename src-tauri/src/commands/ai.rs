use std::sync::Arc;

use ai::{AiConfigUpdate, AiConfigView, AiProviderDraft, AiTestResult};
use nazh_engine::{AiCompletionRequest, AiCompletionResponse, AiService};
use tauri::{AppHandle, Emitter, State};
use tokio::fs;

use crate::commands::{
    capabilities::{list_capabilities, load_capability},
    devices::{list_device_assets, load_device_asset},
};
use crate::state::DesktopState;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiDeviceAssetContext {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) device_type: String,
    pub(crate) version: i64,
    pub(crate) yaml: String,
    pub(crate) yaml_file_path: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiCapabilityAssetContext {
    pub(crate) id: String,
    pub(crate) device_id: String,
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    pub(crate) version: i64,
    pub(crate) yaml: String,
    pub(crate) yaml_file_path: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiAssetContext {
    pub(crate) devices: Vec<AiDeviceAssetContext>,
    pub(crate) capabilities: Vec<AiCapabilityAssetContext>,
}

#[tauri::command]
pub(crate) async fn load_ai_config(state: State<'_, DesktopState>) -> Result<AiConfigView, String> {
    let config = state.ai_config.read().await;
    Ok(config.to_view())
}

#[tauri::command]
pub(crate) async fn save_ai_config(
    app: AppHandle,
    state: State<'_, DesktopState>,
    update: AiConfigUpdate,
) -> Result<AiConfigView, String> {
    let path = DesktopState::ai_config_file_path(&app)?;
    let dir = path.parent().ok_or("无法确定 AI 配置文件目录")?;
    fs::create_dir_all(dir)
        .await
        .map_err(|error| format!("创建 AI 配置目录失败: {error}"))?;

    let mut config = state.ai_config.write().await;
    config.merge_update(update);

    let tmp_path = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(&*config)
        .map_err(|error| format!("序列化 AI 配置失败: {error}"))?;
    fs::write(&tmp_path, &text)
        .await
        .map_err(|error| format!("写入 AI 配置临时文件失败: {error}"))?;
    fs::rename(&tmp_path, &path)
        .await
        .map_err(|error| format!("原子重命名 AI 配置文件失败: {error}"))?;

    Ok(config.to_view())
}

#[tauri::command]
pub(crate) async fn test_ai_provider(
    state: State<'_, DesktopState>,
    draft: AiProviderDraft,
) -> Result<AiTestResult, String> {
    state
        .ai_service
        .test_connection(draft)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) async fn copilot_complete(
    state: State<'_, DesktopState>,
    request: AiCompletionRequest,
) -> Result<AiCompletionResponse, String> {
    state
        .ai_service
        .complete(request)
        .await
        .map_err(|error| error.to_string())
}

/// 流式 copilot completion，通过 Tauri 事件逐 token 发送到前端。
#[tauri::command]
pub(crate) async fn copilot_complete_stream(
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
    request: AiCompletionRequest,
    stream_id: String,
) -> Result<(), String> {
    let service = Arc::clone(&state.ai_service);

    let mut rx = service
        .stream_complete(request)
        .await
        .map_err(|error| error.to_string())?;

    let event_name = format!("copilot://stream/{stream_id}");

    let app_clone = app.clone();
    tokio::spawn(async move {
        while let Some(chunk_result) = rx.recv().await {
            match chunk_result {
                Ok(chunk) => {
                    let is_done = chunk.done;
                    let payload: serde_json::Value =
                        serde_json::to_value(&chunk).unwrap_or_default();
                    let _ = app_clone.emit(&event_name, payload);
                    if is_done {
                        break;
                    }
                }
                Err(error) => {
                    let payload: serde_json::Value = serde_json::json!({
                        "error": error.to_string(),
                        "done": true
                    });
                    let _ = app_clone.emit(&event_name, payload);
                    break;
                }
            }
        }
    });

    Ok(())
}

/// 读取 AI 编辑链路可见的已审查设备/能力资产上下文。
#[tauri::command]
pub(crate) async fn load_ai_asset_context(
    app: AppHandle,
    workspace_path: Option<String>,
) -> Result<AiAssetContext, String> {
    let device_summaries = list_device_assets(app.clone(), workspace_path.clone()).await?;
    let capability_summaries = list_capabilities(app.clone(), None, workspace_path.clone()).await?;

    let mut devices = Vec::with_capacity(device_summaries.len());
    for summary in device_summaries {
        if let Some(asset) =
            load_device_asset(app.clone(), summary.id.clone(), workspace_path.clone()).await?
        {
            devices.push(AiDeviceAssetContext {
                id: asset.id,
                name: asset.name,
                device_type: asset.device_type,
                version: asset.version,
                yaml: asset.spec_yaml,
                yaml_file_path: asset.yaml_file_path,
            });
        }
    }

    let mut capabilities = Vec::with_capacity(capability_summaries.len());
    for summary in capability_summaries {
        if let Some(capability) =
            load_capability(app.clone(), summary.id.clone(), workspace_path.clone()).await?
        {
            capabilities.push(AiCapabilityAssetContext {
                id: capability.id,
                device_id: capability.device_id,
                name: capability.name,
                description: capability.description,
                version: capability.version,
                yaml: capability.spec_yaml,
                yaml_file_path: capability.yaml_file_path,
            });
        }
    }

    Ok(AiAssetContext {
        devices,
        capabilities,
    })
}
