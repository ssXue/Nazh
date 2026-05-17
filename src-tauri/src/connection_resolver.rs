//! 连接资产到运行时连接定义的解析器。
//!
//! 工程资产只声明可审查的协议拓扑与治理策略；本模块在部署前合并本机 Store 中的
//! 密钥和环境覆盖，生成 `ConnectionManager` 可消费的运行时连接定义。

use serde_json::{Map, Value, json};
use store::StoreHandle;
use tauri::AppHandle;

use nazh_dsl_core::{
    ConnectionGovernanceSpec, ConnectionProtocol, HeaderValueSpec, parse_connection_yaml_validated,
};
use nazh_engine::ConnectionDefinition;

use crate::asset_files::list_connection_asset_yaml_files;

/// 从工作区连接资产解析运行时连接定义。
pub(crate) async fn resolve_connection_definitions(
    app: &AppHandle,
    workspace_path: Option<&str>,
    environment_id: Option<&str>,
    store: Option<&StoreHandle>,
) -> Result<Vec<ConnectionDefinition>, String> {
    let files = list_connection_asset_yaml_files(app, workspace_path).await?;
    let mut definitions = Vec::with_capacity(files.len());

    for path in files {
        let yaml = tokio::fs::read_to_string(&path)
            .await
            .map_err(|error| format!("读取连接资产失败 `{}`: {error}", path.display()))?;
        let spec = parse_connection_yaml_validated(&yaml)
            .map_err(|error| format!("连接资产无效: {error}"))?;
        let (kind, mut metadata) =
            protocol_to_runtime_metadata(&spec.id, &spec.protocol, store).await?;

        insert_governance_metadata(&mut metadata, &spec.governance)?;
        insert_secret_metadata(&spec.id, &mut metadata, &spec.secrets.0, store).await?;
        apply_local_overrides(&spec.id, environment_id, &mut metadata, store).await?;

        definitions.push(ConnectionDefinition {
            id: spec.id,
            kind,
            metadata: Value::Object(metadata),
        });
    }

    Ok(definitions)
}

#[allow(clippy::too_many_lines)]
async fn protocol_to_runtime_metadata(
    connection_id: &str,
    protocol: &ConnectionProtocol,
    store: Option<&StoreHandle>,
) -> Result<(String, Map<String, Value>), String> {
    let mut metadata = Map::new();
    let kind = match protocol {
        ConnectionProtocol::ModbusTcp {
            host,
            port,
            unit_id,
        } => {
            metadata.insert("host".to_owned(), json!(host));
            metadata.insert("port".to_owned(), json!(port));
            if let Some(unit_id) = unit_id {
                metadata.insert("unit_id".to_owned(), json!(unit_id));
            }
            "modbus"
        }
        ConnectionProtocol::Serial {
            port_path,
            baud_rate,
            data_bits,
            parity,
            stop_bits,
            flow_control,
            encoding,
            delimiter,
            read_timeout_ms,
            idle_gap_ms,
            max_frame_bytes,
            trim,
        } => {
            metadata.insert("port_path".to_owned(), json!(port_path));
            metadata.insert("baud_rate".to_owned(), json!(baud_rate));
            metadata.insert("data_bits".to_owned(), json!(data_bits));
            metadata.insert("parity".to_owned(), enum_to_json(parity)?);
            metadata.insert("stop_bits".to_owned(), json!(stop_bits));
            metadata.insert("flow_control".to_owned(), enum_to_json(flow_control)?);
            insert_optional(&mut metadata, "encoding", encoding.as_ref())?;
            insert_optional(&mut metadata, "delimiter", delimiter.as_ref())?;
            insert_optional(&mut metadata, "read_timeout_ms", read_timeout_ms.as_ref())?;
            insert_optional(&mut metadata, "idle_gap_ms", idle_gap_ms.as_ref())?;
            insert_optional(&mut metadata, "max_frame_bytes", max_frame_bytes.as_ref())?;
            insert_optional(&mut metadata, "trim", trim.as_ref())?;
            "serial"
        }
        ConnectionProtocol::Mqtt {
            host,
            port,
            topic,
            client_id,
        } => {
            metadata.insert("host".to_owned(), json!(host));
            metadata.insert("port".to_owned(), json!(port));
            metadata.insert("topic".to_owned(), json!(topic));
            if let Some(client_id) = client_id {
                metadata.insert("client_id".to_owned(), json!(client_id));
            }
            "mqtt"
        }
        ConnectionProtocol::Http {
            url,
            method,
            headers,
        } => {
            metadata.insert("url".to_owned(), json!(url));
            metadata.insert("method".to_owned(), enum_to_json(method)?);
            let mut header_map = Map::new();
            for header in headers {
                let value = resolve_header_value(connection_id, &header.value, store).await?;
                header_map.insert(header.name.clone(), Value::String(value));
            }
            if !header_map.is_empty() {
                metadata.insert("headers".to_owned(), Value::Object(header_map));
            }
            "http"
        }
        ConnectionProtocol::Bark {
            server_url,
            request_timeout_ms,
        } => {
            metadata.insert("server_url".to_owned(), json!(server_url));
            insert_optional(
                &mut metadata,
                "request_timeout_ms",
                request_timeout_ms.as_ref(),
            )?;
            "bark"
        }
        ConnectionProtocol::CanSlcan {
            channel,
            baud_rate,
            bitrate,
        } => {
            metadata.insert("interface".to_owned(), json!("slcan"));
            metadata.insert("channel".to_owned(), json!(channel));
            metadata.insert("baud_rate".to_owned(), json!(baud_rate));
            metadata.insert("bitrate".to_owned(), json!(bitrate));
            "can-slcan"
        }
        ConnectionProtocol::Ethercat {
            backend,
            interface,
            cycle_time_ms,
            op_timeout_ms,
        } => {
            metadata.insert("backend".to_owned(), enum_to_json(backend)?);
            metadata.insert("interface".to_owned(), json!(interface));
            metadata.insert("cycle_time_ms".to_owned(), json!(cycle_time_ms));
            metadata.insert("op_timeout_ms".to_owned(), json!(op_timeout_ms));
            "ethercat"
        }
    };

    Ok((kind.to_owned(), metadata))
}

fn insert_governance_metadata(
    metadata: &mut Map<String, Value>,
    governance: &ConnectionGovernanceSpec,
) -> Result<(), String> {
    let value = serde_json::to_value(governance)
        .map_err(|error| format!("序列化连接治理策略失败: {error}"))?;
    metadata.insert("governance".to_owned(), value);
    Ok(())
}

async fn insert_secret_metadata(
    connection_id: &str,
    metadata: &mut Map<String, Value>,
    secrets: &std::collections::BTreeMap<String, String>,
    store: Option<&StoreHandle>,
) -> Result<(), String> {
    for (key, secret_ref) in secrets {
        let value = resolve_secret_ref(connection_id, secret_ref, store).await?;
        metadata.insert(key.clone(), Value::String(value));
    }
    Ok(())
}

async fn apply_local_overrides(
    connection_id: &str,
    environment_id: Option<&str>,
    metadata: &mut Map<String, Value>,
    store: Option<&StoreHandle>,
) -> Result<(), String> {
    let Some(environment_id) = environment_id else {
        return Ok(());
    };
    let Some(store) = store else {
        return Ok(());
    };

    let overrides = store
        .list_connection_local_overrides(connection_id, Some(environment_id))
        .await
        .map_err(|error| format!("读取连接本机覆盖失败 `{connection_id}`: {error}"))?;
    for item in overrides {
        metadata.insert(item.key, item.value);
    }
    Ok(())
}

async fn resolve_header_value(
    connection_id: &str,
    value: &HeaderValueSpec,
    store: Option<&StoreHandle>,
) -> Result<String, String> {
    match value {
        HeaderValueSpec::Literal { value } => Ok(value.clone()),
        HeaderValueSpec::SecretRef { id } => resolve_secret_ref(connection_id, id, store).await,
    }
}

async fn resolve_secret_ref(
    connection_id: &str,
    secret_ref: &str,
    store: Option<&StoreHandle>,
) -> Result<String, String> {
    let secret_key = secret_ref
        .strip_prefix("secret://")
        .ok_or_else(|| format!("连接 `{connection_id}` 的密钥引用必须使用 secret:// 前缀"))?;
    let Some(store) = store else {
        return Err(format!(
            "连接 `{connection_id}` 需要密钥 `{secret_key}`，但 Store 未就绪"
        ));
    };
    let secret = store
        .load_connection_secret(connection_id, secret_key)
        .await
        .map_err(|error| format!("读取连接密钥失败 `{connection_id}/{secret_key}`: {error}"))?;
    secret
        .map(|record| record.value)
        .ok_or_else(|| format!("连接 `{connection_id}` 缺少本机密钥 `{secret_key}`"))
}

fn enum_to_json<T: serde::Serialize>(value: &T) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|error| format!("序列化连接协议枚举失败: {error}"))
}

fn insert_optional<T: serde::Serialize>(
    metadata: &mut Map<String, Value>,
    key: &str,
    value: Option<&T>,
) -> Result<(), String> {
    if let Some(value) = value {
        metadata.insert(
            key.to_owned(),
            serde_json::to_value(value)
                .map_err(|error| format!("序列化连接可选字段 `{key}` 失败: {error}"))?,
        );
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use nazh_dsl_core::{SerialFlowControl, SerialParity};
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn secret_ref_缺少_store_返回可定位错误() {
        let error = resolve_secret_ref("bark-main", "secret://device_key", None)
            .await
            .unwrap_err();

        assert!(error.contains("bark-main"));
        assert!(error.contains("device_key"));
    }

    #[tokio::test]
    async fn serial_protocol_保留帧参数到运行时元数据() {
        let protocol = ConnectionProtocol::Serial {
            port_path: "/dev/ttyUSB0".to_owned(),
            baud_rate: 115_200,
            data_bits: 8,
            parity: SerialParity::None,
            stop_bits: 1,
            flow_control: SerialFlowControl::None,
            encoding: Some("ascii".to_owned()),
            delimiter: Some("\\n".to_owned()),
            read_timeout_ms: Some(100),
            idle_gap_ms: Some(80),
            max_frame_bytes: Some(512),
            trim: Some(true),
        };

        let (_, metadata) = protocol_to_runtime_metadata("serial-main", &protocol, None)
            .await
            .unwrap();

        assert_eq!(metadata.get("delimiter"), Some(&json!("\\n")));
        assert_eq!(metadata.get("max_frame_bytes"), Some(&json!(512)));
        assert_eq!(metadata.get("trim"), Some(&json!(true)));
    }
}
