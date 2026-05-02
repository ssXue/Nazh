//! HITL 节点表单 schema 定义。

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 表单字段选项。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
}

/// 表单字段定义——简化 JSON Schema 子集。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum FormSchemaField {
    #[serde(rename = "boolean")]
    Boolean {
        name: String,
        label: String,
        #[serde(default)]
        required: bool,
        #[serde(default)]
        default: Option<bool>,
    },
    #[serde(rename = "number")]
    Number {
        name: String,
        label: String,
        #[serde(default)]
        required: bool,
        #[serde(default)]
        default: Option<f64>,
        #[serde(default)]
        min: Option<f64>,
        #[serde(default)]
        max: Option<f64>,
        #[serde(default)]
        unit: Option<String>,
    },
    #[serde(rename = "string")]
    StringField {
        name: String,
        label: String,
        #[serde(default)]
        required: bool,
        #[serde(default)]
        default: Option<String>,
        #[serde(default)]
        multiline: bool,
        #[serde(default)]
        max_length: Option<usize>,
    },
    #[serde(rename = "select")]
    Select {
        name: String,
        label: String,
        #[serde(default)]
        required: bool,
        options: Vec<SelectOption>,
        #[serde(default)]
        default: Option<String>,
    },
}

impl FormSchemaField {
    /// 返回字段名和默认值（如果有）。
    pub fn default_value(&self) -> Option<(String, Value)> {
        match self {
            Self::Boolean { name, default, .. } => default.map(|v| (name.clone(), Value::Bool(v))),
            Self::Number { name, default, .. } => {
                default.map(|v| (name.clone(), serde_json::json!(v)))
            }
            Self::StringField { name, default, .. } => default
                .as_ref()
                .map(|v| (name.clone(), Value::String(v.clone()))),
            Self::Select { name, default, .. } => default
                .as_ref()
                .map(|v| (name.clone(), Value::String(v.clone()))),
        }
    }

    /// 字段名。
    pub fn name(&self) -> &str {
        match self {
            Self::Boolean { name, .. }
            | Self::Number { name, .. }
            | Self::StringField { name, .. }
            | Self::Select { name, .. } => name,
        }
    }
}
