//! 设备资产 IPC 命令域（RFC-0004 Phase 1 + Phase 4A）。
//!
//! 提供设备资产的 CRUD、AI 抽取、结构化提案和 Pin schema 生成命令。

use std::sync::Arc;

use nazh_dsl_core::{parse_capability_yaml, parse_device_yaml, signals_to_pin_definitions};
use nazh_engine::{AiCompletionRequest, AiGenerationParams, AiMessage, AiMessageRole, AiService};
use serde::{Deserialize, Serialize};
use store::{
    AssetVersionSummary, DeviceAssetSummary, FieldSource, Store, StoredAssetVersion,
    StoredDeviceAsset,
};
use tauri::{AppHandle, Emitter, State};

use crate::state::DesktopState;
use base64::Engine;

// ---- IPC 响应类型 ----

/// 设备资产完整详情（IPC 响应）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct DeviceAssetDetail {
    pub id: String,
    pub name: String,
    pub device_type: String,
    pub version: i64,
    pub spec_json: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

impl From<StoredDeviceAsset> for DeviceAssetDetail {
    fn from(value: StoredDeviceAsset) -> Self {
        Self {
            id: value.id,
            name: value.name,
            device_type: value.device_type,
            version: value.version,
            spec_json: value.spec_json,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

/// Pin schema 条目（IPC 响应）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct PinSchemaEntry {
    pub id: String,
    pub label: String,
    pub pin_type: String,
    pub direction: String,
    pub description: Option<String>,
}

// ---- CRUD 命令 ----

/// 列出所有设备资产摘要。
#[tauri::command]
pub(crate) async fn list_device_assets(
    state: State<'_, DesktopState>,
) -> Result<Vec<DeviceAssetSummary>, String> {
    let store = get_store(&state)?;
    store
        .list_device_assets()
        .map_err(|e| format!("列出设备资产失败: {e}"))
}

/// 加载指定设备资产详情。
#[tauri::command]
pub(crate) async fn load_device_asset(
    state: State<'_, DesktopState>,
    id: String,
) -> Result<Option<DeviceAssetDetail>, String> {
    let store = get_store(&state)?;
    store
        .load_device_asset(&id)
        .map_err(|e| format!("加载设备资产失败: {e}"))
        .map(|opt| opt.map(DeviceAssetDetail::from))
}

/// 保存（或更新）设备资产。
///
/// 接收 YAML 格式的设备规格，解析校验后存储为 JSON。
#[tauri::command]
pub(crate) async fn save_device_asset(
    state: State<'_, DesktopState>,
    id: String,
    name: String,
    device_type: String,
    spec_yaml: String,
) -> Result<(), String> {
    // 解析 YAML 校验合法性
    let spec = parse_device_yaml(&spec_yaml).map_err(|e| format!("设备 DSL 解析失败: {e}"))?;
    let spec_json = serde_json::to_value(&spec).map_err(|e| format!("设备规格序列化失败: {e}"))?;

    let store = get_store(&state)?;
    store
        .save_device_asset(&id, &name, &device_type, &spec_json)
        .map_err(|e| format!("保存设备资产失败: {e}"))
}

/// 删除设备资产及其版本历史。
#[tauri::command]
pub(crate) async fn delete_device_asset(
    state: State<'_, DesktopState>,
    id: String,
) -> Result<(), String> {
    let store = get_store(&state)?;
    store
        .delete_device_asset(&id)
        .map_err(|e| format!("删除设备资产失败: {e}"))
}

// ---- 版本管理 ----

/// 列出设备资产的所有版本摘要。
#[tauri::command]
pub(crate) async fn list_asset_versions(
    state: State<'_, DesktopState>,
    asset_id: String,
) -> Result<Vec<AssetVersionSummary>, String> {
    let store = get_store(&state)?;
    store
        .list_asset_versions(&asset_id)
        .map_err(|e| format!("列出设备版本失败: {e}"))
}

/// 加载特定版本的设备资产。
#[tauri::command]
pub(crate) async fn load_asset_version(
    state: State<'_, DesktopState>,
    asset_id: String,
    version: i64,
) -> Result<Option<StoredAssetVersion>, String> {
    let store = get_store(&state)?;
    store
        .load_asset_version(&asset_id, version)
        .map_err(|e| format!("加载设备版本失败: {e}"))
}

// ---- AI 抽取 ----

const EXTRACTION_SYSTEM_PROMPT: &str = "你是一个工业设备建模专家。从用户提供的说明书文本中抽取设备信息，\
     输出 YAML 格式的 DeviceSpec。只输出 YAML，不要解释。";

/// 从文本中 AI 抽取 `DeviceSpec` YAML。
#[tauri::command]
pub(crate) async fn extract_device_from_text(
    state: State<'_, DesktopState>,
    text: String,
    provider_id: Option<String>,
) -> Result<String, String> {
    let resolved = resolve_provider_id(&state, provider_id).await?;
    let request = build_extraction_request(&text, resolved);

    let response = state
        .ai_service
        .complete(request)
        .await
        .map_err(|e| format!("AI 抽取失败: {e}"))?;

    // 校验输出是否为合法 DeviceSpec YAML
    let yaml_text = extract_yaml_from_response(&response.content);
    parse_device_yaml(&yaml_text).map_err(|e| format!("AI 输出不是合法的 DeviceSpec YAML: {e}"))?;

    Ok(yaml_text)
}

/// 流式 AI 抽取 `DeviceSpec` YAML。
#[tauri::command]
pub(crate) async fn extract_device_from_text_stream(
    app: AppHandle,
    state: State<'_, DesktopState>,
    text: String,
    provider_id: Option<String>,
    stream_id: String,
) -> Result<(), String> {
    let resolved = resolve_provider_id(&state, provider_id).await?;
    let request = build_extraction_request(&text, resolved);
    let service = Arc::clone(&state.ai_service);
    let mut rx = service
        .stream_complete(request)
        .await
        .map_err(|e| format!("启动 AI 流式抽取失败: {e}"))?;

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

// ---- AI 结构化提案（RFC-0004 Phase 4A）----

/// AI 抽取的不确定项。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UncertaintyItem {
    /// 不确定字段路径（如 "signals[0].range"）。
    pub field_path: String,
    /// AI 的猜测值。
    pub guessed_value: String,
    /// 不确定原因。
    pub reason: String,
}

/// 设备 + 能力的结构化抽取提案（RFC-0004 Phase 4A）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceExtractionProposal {
    /// 抽取出的 `DeviceSpec` YAML。
    pub device_yaml: String,
    /// AI 同时推断的能力 YAML 列表（从写信号 + 说明书推断）。
    pub capability_yamls: Vec<String>,
    /// AI 标记的不确定项。
    pub uncertainties: Vec<UncertaintyItem>,
    /// AI 生成的警告。
    pub warnings: Vec<String>,
}

/// AI proposal 的 JSON 输出结构（内部解析用）。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawExtractionProposal {
    device_yaml: String,
    #[serde(default)]
    capability_yamls: Vec<String>,
    #[serde(default)]
    uncertainties: Vec<UncertaintyItem>,
    #[serde(default)]
    warnings: Vec<String>,
}

const PROPOSAL_SYSTEM_PROMPT: &str = "你是一个工业设备建模专家。从用户提供的说明书文本中抽取设备信息并推断设备能力。\
    输出严格 JSON 格式，结构如下：\n\
    {\"deviceYaml\": \"<DeviceSpec YAML 文本>\", \
     \"capabilityYamls\": [\"<CapabilitySpec YAML 文本>\", ...], \
     \"uncertainties\": [{\"fieldPath\": \"...\", \"guessedValue\": \"...\", \"reason\": \"...\"}], \
     \"warnings\": [\"...\"]}\n\n\
    规则：\n\
    - deviceYaml 必须是合法的 DeviceSpec YAML\n\
    - capabilityYamls 从写信号（analog_output / digital_output）推断底层操作能力，每个能力封装一个写信号\n\
    - uncertainties 用于标记信息不完整或需要人工确认的字段\n\
    - warnings 用于标记潜在安全问题或不一致\n\
    - 只输出 JSON，不要解释";

/// 从文本中 AI 抽取设备 + 能力的结构化提案。
#[tauri::command]
pub(crate) async fn extract_device_proposal(
    state: State<'_, DesktopState>,
    text: String,
    provider_id: Option<String>,
) -> Result<DeviceExtractionProposal, String> {
    let resolved = resolve_provider_id(&state, provider_id).await.map_err(|e| {
        tracing::error!(error = %e, "解析 AI 提供商失败");
        e
    })?;
    tracing::info!(
        provider_id = %resolved,
        text_len = text.len(),
        "AI 结构化抽取开始（文本模式）"
    );
    let request = build_proposal_request(&text, resolved);

    let response = state
        .ai_service
        .complete(request)
        .await
        .map_err(|e| {
            let msg = format!("AI 结构化抽取失败: {e}");
            tracing::error!(error = %e, "AI completion 请求失败");
            msg
        })?;

    tracing::info!(
        response_len = response.content.len(),
        "AI 响应已收到，开始解析"
    );

    let json_text = extract_json_from_response(&response.content);

    let raw: RawExtractionProposal =
        serde_json::from_str(&json_text).map_err(|e| {
            let msg = format!("AI 输出 JSON 结构无效: {e}");
            tracing::error!(
                error = %e,
                json_preview = %json_text.chars().take(500).collect::<String>(),
                "AI 响应 JSON 反序列化失败"
            );
            msg
        })?;

    parse_device_yaml(&raw.device_yaml)
        .map_err(|e| {
            let msg = format!("AI 生成的 deviceYaml 不是合法 DeviceSpec: {e}");
            tracing::error!(error = %e, yaml_preview = %raw.device_yaml.chars().take(300).collect::<String>(), "{}", msg);
            msg
        })?;

    for (idx, cap_yaml) in raw.capability_yamls.iter().enumerate() {
        parse_capability_yaml(cap_yaml).map_err(|e| {
            let msg = format!("AI 生成的 capabilityYamls[{idx}] 不是合法 CapabilitySpec: {e}");
            tracing::error!(error = %e, idx, "{}", msg);
            msg
        })?;
    }

    Ok(DeviceExtractionProposal {
        device_yaml: raw.device_yaml,
        capability_yamls: raw.capability_yamls,
        uncertainties: raw.uncertainties,
        warnings: raw.warnings,
    })
}

/// 流式 AI 结构化抽取设备 + 能力提案。
#[tauri::command]
pub(crate) async fn extract_device_proposal_stream(
    app: AppHandle,
    state: State<'_, DesktopState>,
    text: String,
    provider_id: Option<String>,
    stream_id: String,
) -> Result<(), String> {
    let resolved = resolve_provider_id(&state, provider_id).await?;
    let request = build_proposal_request(&text, resolved);
    let service = Arc::clone(&state.ai_service);
    let mut rx = service
        .stream_complete(request)
        .await
        .map_err(|e| format!("启动 AI 流式结构化抽取失败: {e}"))?;

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

// ---- Pin Schema 生成 ----

/// 从设备资产生成 Pin 声明列表。
#[tauri::command]
pub(crate) async fn generate_pin_schema(
    state: State<'_, DesktopState>,
    device_id: String,
) -> Result<Vec<PinSchemaEntry>, String> {
    let store = get_store(&state)?;
    let asset = store
        .load_device_asset(&device_id)
        .map_err(|e| format!("加载设备资产失败: {e}"))?
        .ok_or_else(|| format!("设备资产 `{device_id}` 不存在"))?;

    // 从 JSON 反序列化为 DeviceSpec
    let spec: nazh_dsl_core::DeviceSpec = serde_json::from_value(asset.spec_json)
        .map_err(|e| format!("设备规格反序列化失败: {e}"))?;

    let pin_defs = signals_to_pin_definitions(&spec.signals);
    Ok(pin_defs
        .into_iter()
        .map(|pin| PinSchemaEntry {
            id: pin.id,
            label: pin.label,
            pin_type: format!("{:?}", pin.pin_type).to_lowercase(),
            direction: format!("{:?}", pin.direction).to_lowercase(),
            description: pin.description,
        })
        .collect())
}

// ---- 来源追溯 ----

/// 批量保存 AI 抽取来源追溯记录。
#[tauri::command]
pub(crate) async fn save_device_asset_sources(
    state: State<'_, DesktopState>,
    asset_id: String,
    sources: Vec<FieldSource>,
) -> Result<(), String> {
    let store = get_store(&state)?;
    store
        .save_asset_sources(&asset_id, &sources)
        .map_err(|e| format!("保存来源记录失败: {e}"))
}

/// 加载设备资产的来源追溯记录。
#[tauri::command]
pub(crate) async fn load_device_asset_sources(
    state: State<'_, DesktopState>,
    asset_id: String,
) -> Result<Vec<FieldSource>, String> {
    let store = get_store(&state)?;
    store
        .load_asset_sources(&asset_id)
        .map_err(|e| format!("加载来源记录失败: {e}"))
}

// ---- PDF 文本提取 ----

/// 从 PDF 文件（base64 编码）中提取纯文本。
#[tauri::command]
pub(crate) async fn extract_text_from_pdf(
    pdf_base64: String,
) -> Result<String, String> {
    let pdf_bytes = base64::engine::general_purpose::STANDARD
        .decode(&pdf_base64)
        .map_err(|e| format!("PDF base64 解码失败: {e}"))?;

    tracing::info!(
        "PDF 文本提取开始，文件大小 {} 字节",
        pdf_bytes.len()
    );

    let text = pdf_extract::extract_text_from_mem(&pdf_bytes)
        .map_err(|e| format!("PDF 文本提取失败: {e}"))?;

    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("PDF 文本提取结果为空，文件可能是扫描件或图片型 PDF".to_owned());
    }

    tracing::info!(
        "PDF 文本提取完成，提取字符数 {}",
        trimmed.len()
    );

    Ok(trimmed.to_owned())
}

/// 从 PDF 文件中提取文本并执行 AI 结构化抽取（一步到位）。
#[tauri::command]
pub(crate) async fn extract_device_from_pdf(
    state: State<'_, DesktopState>,
    pdf_base64: String,
    provider_id: Option<String>,
) -> Result<DeviceExtractionProposal, String> {
    tracing::info!("PDF 设备抽取开始：先提取文本");

    let text = extract_text_from_pdf(pdf_base64).await.map_err(|e| {
        tracing::error!(error = %e, "PDF 文本提取失败");
        e
    })?;

    let resolved = resolve_provider_id(&state, provider_id).await.map_err(|e| {
        tracing::error!(error = %e, "解析 AI 提供商失败");
        e
    })?;
    tracing::info!(
        provider_id = %resolved,
        text_len = text.len(),
        "PDF 文本提取完成，开始 AI 结构化抽取"
    );
    let request = build_proposal_request(&text, resolved);

    let response = state
        .ai_service
        .complete(request)
        .await
        .map_err(|e| {
            let msg = format!("AI 结构化抽取失败: {e}");
            tracing::error!(error = %e, "AI completion 请求失败");
            msg
        })?;

    tracing::info!(
        response_len = response.content.len(),
        "AI 响应已收到，开始解析"
    );

    let json_text = extract_json_from_response(&response.content);
    tracing::debug!(json_len = json_text.len(), "JSON 文本已提取，开始反序列化");

    let raw: RawExtractionProposal =
        serde_json::from_str(&json_text).map_err(|e| {
            let msg = format!("AI 输出 JSON 结构无效: {e}");
            tracing::error!(
                error = %e,
                json_preview = %json_text.chars().take(500).collect::<String>(),
                "AI 响应 JSON 反序列化失败"
            );
            msg
        })?;

    parse_device_yaml(&raw.device_yaml)
        .map_err(|e| {
            let msg = format!("AI 生成的 deviceYaml 不是合法 DeviceSpec: {e}");
            tracing::error!(error = %e, yaml_preview = %raw.device_yaml.chars().take(300).collect::<String>(), "{}", msg);
            msg
        })?;

    for (idx, cap_yaml) in raw.capability_yamls.iter().enumerate() {
        parse_capability_yaml(cap_yaml).map_err(|e| {
            let msg = format!("AI 生成的 capabilityYamls[{idx}] 不是合法 CapabilitySpec: {e}");
            tracing::error!(error = %e, idx, "{}", msg);
            msg
        })?;
    }

    Ok(DeviceExtractionProposal {
        device_yaml: raw.device_yaml,
        capability_yamls: raw.capability_yamls,
        uncertainties: raw.uncertainties,
        warnings: raw.warnings,
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

/// 解析有效的 `provider_id`：若传入为空则回退到配置中的活跃提供商。
async fn resolve_provider_id(
    state: &State<'_, DesktopState>,
    provider_id: Option<String>,
) -> Result<String, String> {
    if let Some(id) = provider_id.filter(|s| !s.is_empty()) {
        return Ok(id);
    }
    let config = state
        .ai_config
        .read()
        .await;
    config
        .active_provider_id
        .clone()
        .ok_or_else(|| "未配置 AI 提供商，请先在设置中添加并启用一个提供商".to_owned())
}

/// 构建 AI 抽取请求。
fn build_extraction_request(text: &str, provider_id: String) -> AiCompletionRequest {
    AiCompletionRequest {
        provider_id,
        model: None,
        messages: vec![
            AiMessage {
                role: AiMessageRole::System,
                content: EXTRACTION_SYSTEM_PROMPT.to_owned(),
            },
            AiMessage {
                role: AiMessageRole::User,
                content: build_extraction_prompt(text),
            },
        ],
        params: AiGenerationParams {
            temperature: Some(0.1),
            max_tokens: None,
            ..Default::default()
        },
        timeout_ms: None,
    }
}

/// 构建 AI 抽取 prompt。
fn build_extraction_prompt(text: &str) -> String {
    format!(
        "请从以下设备说明书中抽取设备信息，输出 YAML 格式的 DeviceSpec。\n\n\
         DeviceSpec 结构参考：\n\
         ```yaml\n\
         id: <设备唯一标识>\n\
         type: <设备类型>\n\
         manufacturer: <厂商>  # 可选\n\
         model: <型号>  # 可选\n\
         connection:\n\
           type: <协议类型，如 modbus-tcp / mqtt / serial>\n\
           id: <连接引用 ID>\n\
           unit: <站号>  # 可选\n\
         signals:\n\
           - id: <信号 ID>\n\
             signal_type: <analog_input / analog_output / digital_input / digital_output>\n\
             unit: <单位>  # 可选\n\
             range: [min, max]  # 可选\n\
             source:  # 三种类型，必须提供对应字段\n\
               # register 类型（Modbus）：\n\
               type: register\n\
               register: <地址，整数>\n\
               data_type: <bool / u16 / i16 / u32 / i32 / f32 / f64>\n\
               access: <read / write / read_write>  # 默认 read\n\
               bit: <位号>  # 可选\n\
               # topic 类型（MQTT）：\n\
               # type: topic\n\
               # topic: <MQTT 主题路径>\n\
               # serial_command 类型（串口）：\n\
               # type: serial_command\n\
               # command: <串口命令字符串>\n\
         alarms:\n\
           - id: <告警 ID>\n\
             condition: <Rhai 条件表达式>\n\
             severity: <info / warning / critical>\n\
             action: <动作>  # 可选\n\
         ```\n\n\
         重要规则：\n\
         - source.type 为 register 时必须提供 register 和 data_type 字段\n\
         - source.type 为 topic 时必须提供 topic 字段\n\
         - source.type 为 serial_command 时必须提供 command 字段\n\
         - 如果说明书中未明确指定协议，优先使用 register 类型\n\n\
         说明书文本：\n---\n{text}\n---"
    )
}

/// 从 AI 响应中提取 YAML 内容（去除 markdown 代码块包裹）。
fn extract_yaml_from_response(content: &str) -> String {
    let trimmed = content.trim();
    // 去除 ```yaml ... ``` 包裹
    if let Some(stripped) = trimmed
        .strip_prefix("```yaml")
        .or_else(|| trimmed.strip_prefix("```yml"))
        .or_else(|| trimmed.strip_prefix("```"))
        && let Some(inner) = stripped.strip_suffix("```")
    {
        return inner.trim().to_owned();
    }
    trimmed.to_owned()
}

/// 从 AI 响应中提取 JSON 内容（去除 markdown 代码块包裹）。
fn extract_json_from_response(content: &str) -> String {
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

/// 构建 AI 结构化提案请求。
fn build_proposal_request(text: &str, provider_id: String) -> AiCompletionRequest {
    AiCompletionRequest {
        provider_id,
        model: None,
        messages: vec![
            AiMessage {
                role: AiMessageRole::System,
                content: PROPOSAL_SYSTEM_PROMPT.to_owned(),
            },
            AiMessage {
                role: AiMessageRole::User,
                content: build_proposal_prompt(text),
            },
        ],
        params: AiGenerationParams {
            temperature: Some(0.1),
            max_tokens: None,
            ..Default::default()
        },
        timeout_ms: None,
    }
}

/// 构建 AI 结构化提案 prompt。
fn build_proposal_prompt(text: &str) -> String {
    format!(
        "请从以下设备说明书中抽取设备信息和推断设备能力。\n\n\
         DeviceSpec 结构参考：\n\
         ```yaml\n\
         id: <设备唯一标识>\n\
         type: <设备类型>\n\
         manufacturer: <厂商>\n\
         model: <型号>\n\
         connection:\n\
           type: <modbus-tcp / mqtt / serial>\n\
           id: <连接引用 ID>\n\
           unit: <站号>\n\
         signals:\n\
           - id: <信号 ID>\n\
             signal_type: <analog_input / analog_output / digital_input / digital_output>\n\
             unit: <单位>\n\
             range: [min, max]\n\
             source:  # 三种类型，必须提供对应字段\n\
               # register 类型（Modbus）：\n\
               type: register\n\
               register: <地址，整数>\n\
               data_type: <bool / u16 / i16 / u32 / i32 / f32 / f64>\n\
               access: <read / write / read_write>\n\
               # topic 类型（MQTT）：\n\
               # type: topic\n\
               # topic: <MQTT 主题路径>\n\
               # serial_command 类型（串口）：\n\
               # type: serial_command\n\
               # command: <串口命令字符串>\n\
         alarms:\n\
           - id: <告警 ID>\n\
             condition: <Rhai 表达式>\n\
             severity: <info / warning / critical>\n\
         ```\n\n\
         重要规则：\n\
         - source.type 为 register 时必须提供 register 和 data_type 字段\n\
         - source.type 为 topic 时必须提供 topic 字段\n\
         - source.type 为 serial_command 时必须提供 command 字段\n\
         - 如果说明书中未明确指定协议，优先使用 register 类型\n\n\
         CapabilitySpec 结构参考：\n\
         ```yaml\n\
         id: <能力 ID，格式 device.action>\n\
         device_id: <关联设备 ID>\n\
         description: <能力描述>\n\
         inputs:\n\
           - id: <参数 ID>\n\
             unit: <单位>\n\
             range: [min, max]\n\
             required: true\n\
         outputs:\n\
           - id: <输出 ID>\n\
             type: <bool / f64 / string>\n\
         preconditions:\n\
           - <Rhai 前置条件表达式>\n\
         implementation:\n\
           type: <modbus-write / mqtt-publish / serial-command>\n\
           register: <目标寄存器>\n\
           value: <值表达式，如 $param_id>\n\
         safety:\n\
           level: <high / medium / low>\n\
           requires_approval: false\n\
         ```\n\n\
         说明书文本：\n---\n{text}\n---"
    )
}
