//! 设备资产 IPC 响应类型与结构化提案类型。

use serde::{Deserialize, Serialize};

use crate::asset_files::AssetFieldSource;

// ---- IPC 响应类型 ----

/// 设备资产摘要（IPC 响应）。
///
/// `connection` 字段从设备 DSL 的 `connection` 块直接派生，
/// 便于前端在列表视图直接展示设备绑定的连接资源（设备语义高于协议适配）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct DeviceAssetSummary {
    pub id: String,
    pub name: String,
    pub device_type: String,
    pub version: i64,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection: Option<DeviceConnectionRef>,
}

/// 设备资产摘要中携带的连接引用（DSL `connection` 块的扁平视图）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct DeviceConnectionRef {
    #[serde(rename = "type")]
    pub connection_type: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<u8>,
}

/// 设备资产完整详情（IPC 响应）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct DeviceAssetDetail {
    pub id: String,
    pub name: String,
    pub device_type: String,
    pub version: i64,
    pub spec_json: serde_json::Value,
    pub spec_yaml: String,
    pub yaml_file_path: Option<String>,
    pub created_at: String,
    pub updated_at: String,
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

/// 设备资产版本摘要（IPC 响应）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct AssetVersionSummary {
    pub version: i64,
    pub created_at: String,
    pub source_summary: Option<String>,
}

/// 设备资产版本详情（IPC 响应）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct StoredAssetVersion {
    pub asset_id: String,
    pub version: i64,
    pub spec_json: serde_json::Value,
    pub source_summary: Option<String>,
    pub created_at: String,
}

/// AI 来源追溯记录。
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct FieldSource {
    pub field_path: String,
    pub source_text: String,
    pub confidence: f64,
}

/// 快照摘要（IPC 响应）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct DeviceSnapshotSummary {
    pub version: i64,
    pub label: String,
    pub description: String,
    pub reason: String,
    pub created_at: String,
}

// ---- 结构化提案类型（ESI 导入、PDF 抽取共用） ----

/// AI 抽取的不确定项。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UncertaintyItem {
    pub field_path: String,
    pub guessed_value: String,
    pub reason: String,
}

/// 设备 + 能力的结构化抽取提案（RFC-0004 Phase 4A）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceExtractionProposal {
    pub device_yamls: Vec<String>,
    pub capability_yamls: Vec<String>,
    pub uncertainties: Vec<UncertaintyItem>,
    pub warnings: Vec<String>,
}

// ---- From 转换 ----

impl From<FieldSource> for AssetFieldSource {
    fn from(value: FieldSource) -> Self {
        Self {
            field_path: value.field_path,
            source_text: value.source_text,
            confidence: value.confidence,
        }
    }
}

impl From<AssetFieldSource> for FieldSource {
    fn from(value: AssetFieldSource) -> Self {
        Self {
            field_path: value.field_path,
            source_text: value.source_text,
            confidence: value.confidence,
        }
    }
}
