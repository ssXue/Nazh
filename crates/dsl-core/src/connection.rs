//! Connection DSL 类型定义（ADR-0025）。
//!
//! 连接资产描述工程可审查的协议拓扑与治理策略；明文密钥、本机覆盖和运行态状态
//! 不属于本模型。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::DslError;

/// 连接资产结构化模型。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionSpec {
    pub id: String,
    pub protocol: ConnectionProtocol,
    pub governance: ConnectionGovernanceSpec,
    #[serde(default)]
    #[serde(skip_serializing_if = "ConnectionSecretRefs::is_empty")]
    pub secrets: ConnectionSecretRefs,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// 协议拓扑配置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ConnectionProtocol {
    ModbusTcp {
        host: String,
        port: u16,
        #[serde(default)]
        #[serde(skip_serializing_if = "Option::is_none")]
        unit_id: Option<u8>,
    },
    Serial {
        port_path: String,
        baud_rate: u32,
        data_bits: u8,
        parity: SerialParity,
        stop_bits: u8,
        flow_control: SerialFlowControl,
        #[serde(default)]
        #[serde(skip_serializing_if = "Option::is_none")]
        encoding: Option<String>,
        #[serde(default)]
        #[serde(skip_serializing_if = "Option::is_none")]
        delimiter: Option<String>,
        #[serde(default)]
        #[serde(skip_serializing_if = "Option::is_none")]
        read_timeout_ms: Option<u64>,
        #[serde(default)]
        #[serde(skip_serializing_if = "Option::is_none")]
        idle_gap_ms: Option<u64>,
        #[serde(default)]
        #[serde(skip_serializing_if = "Option::is_none")]
        max_frame_bytes: Option<u64>,
        #[serde(default)]
        #[serde(skip_serializing_if = "Option::is_none")]
        trim: Option<bool>,
    },
    Mqtt {
        host: String,
        port: u16,
        topic: String,
        #[serde(default)]
        #[serde(skip_serializing_if = "Option::is_none")]
        client_id: Option<String>,
    },
    Http {
        url: String,
        method: HttpMethod,
        #[serde(default)]
        #[serde(skip_serializing_if = "Vec::is_empty")]
        headers: Vec<HeaderSpec>,
    },
    Bark {
        server_url: String,
        #[serde(default)]
        #[serde(skip_serializing_if = "Option::is_none")]
        request_timeout_ms: Option<u64>,
    },
    CanSlcan {
        channel: String,
        baud_rate: u32,
        bitrate: u32,
    },
    Ethercat {
        backend: EthercatBackend,
        interface: String,
        cycle_time_ms: u64,
        op_timeout_ms: u64,
    },
}

/// 串口校验位。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SerialParity {
    None,
    Odd,
    Even,
}

/// 串口流控。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SerialFlowControl {
    None,
    Software,
    Hardware,
}

/// HTTP 方法。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

/// HTTP Header 声明。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeaderSpec {
    pub name: String,
    pub value: HeaderValueSpec,
}

/// Header 值来源。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum HeaderValueSpec {
    Literal { value: String },
    SecretRef { id: String },
}

/// `EtherCAT` 后端。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EthercatBackend {
    Ethercrab,
    Mock,
}

/// 连接治理策略。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionGovernanceSpec {
    pub connect_timeout_ms: u64,
    pub operation_timeout_ms: u64,
    pub heartbeat_interval_ms: u64,
    pub heartbeat_timeout_ms: u64,
    pub rate_limit_max_attempts: u32,
    pub rate_limit_window_ms: u64,
    pub rate_limit_cooldown_ms: u64,
    pub circuit_failure_threshold: u32,
    pub circuit_open_ms: u64,
    pub reconnect_base_ms: u64,
    pub reconnect_max_ms: u64,
}

/// 密钥引用集合，key 是协议语义名，value 必须是 `secret://...`。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ConnectionSecretRefs(pub BTreeMap<String, String>);

impl ConnectionSecretRefs {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl ConnectionSpec {
    /// 校验连接资产的语义完整性。
    ///
    /// 检查项：ID 非空、协议字段非空、治理阈值显式有效、密钥引用格式、
    /// HTTP 敏感 Header 不允许明文 literal。
    pub fn validate(&self) -> Result<(), DslError> {
        validate_non_empty("connection", "id", &self.id)?;
        self.protocol.validate(&self.id)?;
        self.governance.validate(&self.id)?;
        validate_secret_refs(&self.id, &self.secrets)?;
        Ok(())
    }
}

impl ConnectionProtocol {
    fn validate(&self, connection_id: &str) -> Result<(), DslError> {
        match self {
            Self::ModbusTcp {
                host,
                port,
                unit_id: _,
            } => {
                validate_non_empty(connection_id, "protocol.host", host)?;
                validate_port(connection_id, "protocol.port", *port)?;
            }
            Self::Serial {
                port_path,
                baud_rate,
                data_bits,
                parity: _,
                stop_bits,
                flow_control: _,
                encoding: _,
                delimiter: _,
                read_timeout_ms,
                idle_gap_ms,
                max_frame_bytes,
                trim: _,
            } => {
                validate_non_empty(connection_id, "protocol.port_path", port_path)?;
                validate_positive_u32(connection_id, "protocol.baud_rate", *baud_rate)?;
                if !matches!(*data_bits, 5..=8) {
                    return validation_error(connection_id, "protocol.data_bits 必须在 5-8 之间");
                }
                if !matches!(*stop_bits, 1 | 2) {
                    return validation_error(connection_id, "protocol.stop_bits 只能是 1 或 2");
                }
                if let Some(value) = read_timeout_ms {
                    validate_positive_u64(connection_id, "protocol.read_timeout_ms", *value)?;
                }
                if let Some(value) = idle_gap_ms {
                    validate_positive_u64(connection_id, "protocol.idle_gap_ms", *value)?;
                }
                if let Some(value) = max_frame_bytes {
                    validate_positive_u64(connection_id, "protocol.max_frame_bytes", *value)?;
                }
            }
            Self::Mqtt {
                host,
                port,
                topic,
                client_id: _,
            } => {
                validate_non_empty(connection_id, "protocol.host", host)?;
                validate_port(connection_id, "protocol.port", *port)?;
                validate_non_empty(connection_id, "protocol.topic", topic)?;
            }
            Self::Http {
                url,
                method: _,
                headers,
            } => {
                validate_non_empty(connection_id, "protocol.url", url)?;
                validate_headers(connection_id, headers)?;
            }
            Self::Bark {
                server_url,
                request_timeout_ms,
            } => {
                validate_non_empty(connection_id, "protocol.server_url", server_url)?;
                if let Some(value) = request_timeout_ms {
                    validate_positive_u64(connection_id, "protocol.request_timeout_ms", *value)?;
                }
            }
            Self::CanSlcan {
                channel,
                baud_rate,
                bitrate,
            } => {
                validate_non_empty(connection_id, "protocol.channel", channel)?;
                validate_positive_u32(connection_id, "protocol.baud_rate", *baud_rate)?;
                validate_can_bitrate(connection_id, *bitrate)?;
            }
            Self::Ethercat {
                backend: _,
                interface,
                cycle_time_ms,
                op_timeout_ms,
            } => {
                validate_non_empty(connection_id, "protocol.interface", interface)?;
                validate_positive_u64(connection_id, "protocol.cycle_time_ms", *cycle_time_ms)?;
                validate_positive_u64(connection_id, "protocol.op_timeout_ms", *op_timeout_ms)?;
            }
        }

        Ok(())
    }
}

impl ConnectionGovernanceSpec {
    fn validate(&self, connection_id: &str) -> Result<(), DslError> {
        validate_positive_u64(
            connection_id,
            "governance.connect_timeout_ms",
            self.connect_timeout_ms,
        )?;
        validate_positive_u64(
            connection_id,
            "governance.operation_timeout_ms",
            self.operation_timeout_ms,
        )?;
        validate_positive_u64(
            connection_id,
            "governance.heartbeat_interval_ms",
            self.heartbeat_interval_ms,
        )?;
        validate_positive_u64(
            connection_id,
            "governance.heartbeat_timeout_ms",
            self.heartbeat_timeout_ms,
        )?;
        validate_positive_u32(
            connection_id,
            "governance.rate_limit_max_attempts",
            self.rate_limit_max_attempts,
        )?;
        validate_positive_u64(
            connection_id,
            "governance.rate_limit_window_ms",
            self.rate_limit_window_ms,
        )?;
        validate_positive_u64(
            connection_id,
            "governance.rate_limit_cooldown_ms",
            self.rate_limit_cooldown_ms,
        )?;
        validate_positive_u32(
            connection_id,
            "governance.circuit_failure_threshold",
            self.circuit_failure_threshold,
        )?;
        validate_positive_u64(
            connection_id,
            "governance.circuit_open_ms",
            self.circuit_open_ms,
        )?;
        validate_positive_u64(
            connection_id,
            "governance.reconnect_base_ms",
            self.reconnect_base_ms,
        )?;
        validate_positive_u64(
            connection_id,
            "governance.reconnect_max_ms",
            self.reconnect_max_ms,
        )?;

        if self.heartbeat_timeout_ms <= self.heartbeat_interval_ms {
            return validation_error(
                connection_id,
                "governance.heartbeat_timeout_ms 必须大于 heartbeat_interval_ms",
            );
        }

        if self.reconnect_max_ms < self.reconnect_base_ms {
            return validation_error(
                connection_id,
                "governance.reconnect_max_ms 必须大于等于 reconnect_base_ms",
            );
        }

        Ok(())
    }
}

fn validate_headers(connection_id: &str, headers: &[HeaderSpec]) -> Result<(), DslError> {
    for header in headers {
        validate_non_empty(connection_id, "protocol.headers.name", &header.name)?;
        match &header.value {
            HeaderValueSpec::Literal { value } => {
                validate_non_empty(connection_id, "protocol.headers.value", value)?;
                if is_sensitive_header(&header.name) {
                    return validation_error(
                        connection_id,
                        "敏感 HTTP Header 必须使用 secret-ref，不能写明文 literal",
                    );
                }
            }
            HeaderValueSpec::SecretRef { id } => validate_secret_ref(connection_id, id)?,
        }
    }

    Ok(())
}

fn validate_secret_refs(
    connection_id: &str,
    secrets: &ConnectionSecretRefs,
) -> Result<(), DslError> {
    for (key, value) in &secrets.0 {
        validate_non_empty(connection_id, "secrets.key", key)?;
        validate_secret_ref(connection_id, value)?;
    }

    Ok(())
}

fn validate_secret_ref(connection_id: &str, value: &str) -> Result<(), DslError> {
    validate_non_empty(connection_id, "secret-ref", value)?;
    if !value.starts_with("secret://") {
        return validation_error(connection_id, "密钥引用必须使用 secret:// 前缀");
    }
    Ok(())
}

fn validate_non_empty(context: &str, field: &str, value: &str) -> Result<(), DslError> {
    if value.trim().is_empty() {
        return validation_error(context, &format!("{field} 不能为空"));
    }
    Ok(())
}

fn validate_port(connection_id: &str, field: &str, value: u16) -> Result<(), DslError> {
    if value == 0 {
        return validation_error(connection_id, &format!("{field} 必须在 1-65535 之间"));
    }
    Ok(())
}

fn validate_positive_u32(connection_id: &str, field: &str, value: u32) -> Result<(), DslError> {
    if value == 0 {
        return validation_error(connection_id, &format!("{field} 必须大于 0"));
    }
    Ok(())
}

fn validate_positive_u64(connection_id: &str, field: &str, value: u64) -> Result<(), DslError> {
    if value == 0 {
        return validation_error(connection_id, &format!("{field} 必须大于 0"));
    }
    Ok(())
}

fn validate_can_bitrate(connection_id: &str, bitrate: u32) -> Result<(), DslError> {
    if !matches!(
        bitrate,
        10_000 | 20_000 | 50_000 | 100_000 | 125_000 | 250_000 | 500_000 | 800_000 | 1_000_000
    ) {
        return validation_error(connection_id, "protocol.bitrate 不在支持的 CAN 速率列表中");
    }
    Ok(())
}

fn is_sensitive_header(name: &str) -> bool {
    matches!(
        name.trim().to_ascii_lowercase().as_str(),
        "authorization" | "proxy-authorization" | "cookie" | "x-api-key" | "api-key"
    )
}

fn validation_error<T>(context: &str, detail: &str) -> Result<T, DslError> {
    Err(DslError::Validation {
        context: format!("connection `{context}`"),
        detail: detail.to_owned(),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::needless_raw_string_hashes)]
#[path = "connection_tests.rs"]
mod tests;
