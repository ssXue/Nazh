//! 设备资产 IPC 命令域（RFC-0004 Phase 1 + Phase 4A）。
//!
//! 提供设备资产的 CRUD、AI 抽取、结构化提案和 Pin schema 生成命令。

pub(crate) mod assets;
pub(crate) mod fields;
pub(crate) mod snapshots;
pub(crate) mod types;
pub(crate) mod versions;

// ---- PDF 文本提取 ----

use base64::Engine;

use types::DeviceExtractionProposal;

/// 从 PDF 文件（base64 编码）中提取纯文本。
#[tauri::command]
pub(crate) async fn extract_text_from_pdf(pdf_base64: String) -> Result<String, String> {
    let pdf_bytes = base64::engine::general_purpose::STANDARD
        .decode(&pdf_base64)
        .map_err(|e| format!("PDF base64 解码失败: {e}"))?;

    tracing::info!("PDF 文本提取开始，文件大小 {} 字节", pdf_bytes.len());

    let text = pdf_extract::extract_text_from_mem(&pdf_bytes)
        .map_err(|e| format!("PDF 文本提取失败: {e}"))?;

    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("PDF 文本提取结果为空，文件可能是扫描件或图片型 PDF".to_owned());
    }

    tracing::info!("PDF 文本提取完成，提取字符数 {}", trimmed.len());

    Ok(trimmed.to_owned())
}

// ---- EtherCAT ESI 导入 ----

/// 从 `EtherCAT` ESI XML 文件导入设备 DSL 草稿。
#[tauri::command]
pub(crate) async fn import_ethercat_esi(
    esi_xml: String,
) -> Result<DeviceExtractionProposal, String> {
    use nazh_dsl_core::parse_device_yaml;

    let result = crate::ethercat_esi::import_esi_to_device_yaml(&esi_xml)?;
    for yaml in &result.device_yamls {
        parse_device_yaml(yaml)
            .map_err(|error| format!("ESI 导入结果不是合法 DeviceSpec: {error}"))?;
    }
    Ok(DeviceExtractionProposal {
        device_yamls: result.device_yamls,
        capability_yamls: Vec::new(),
        uncertainties: Vec::new(),
        warnings: result.warnings,
    })
}
