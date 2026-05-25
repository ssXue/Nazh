//! 连接资产一次性诊断（ADR-0026 Phase 3）。
//!
//! 加载单个连接资产，合并 Store 密钥和环境覆盖，做一次性协议级探测，立即释放。
//! 仅使用 `src-tauri` 可用的 crate：serialport + tokio TCP。

use std::time::Instant;

use nazh_dsl_core::parse_connection_yaml_validated;
use tauri::{AppHandle, State};
use tauri_bindings::ConnectionDiagnosticResult;

use crate::state::DesktopState;

#[tauri::command]
pub(crate) async fn test_connection_asset(
    app: AppHandle,
    state: State<'_, DesktopState>,
    connection_id: String,
    workspace_path: Option<String>,
    environment_id: Option<String>,
) -> Result<ConnectionDiagnosticResult, String> {
    let workspace = workspace_path.as_deref();
    let store_handle = state.store_handle().ok();

    let path = crate::asset_files::connection_asset_latest_path(&app, workspace, &connection_id)
        .map_err(|e| format!("找不到连接资产 `{connection_id}`: {e}"))?;
    if !path.exists() {
        return Err(format!("连接资产文件不存在: `{connection_id}`"));
    }
    let yaml = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| format!("读取连接资产失败: {e}"))?;
    let spec = parse_connection_yaml_validated(&yaml).map_err(|e| format!("连接资产无效: {e}"))?;

    let kind = crate::connection_resolver::resolve_connection_kind(&spec.protocol);
    let protocol_label = kind.to_owned();

    let metadata = crate::connection_resolver::build_probe_metadata(
        &spec,
        environment_id.as_deref(),
        store_handle.as_ref(),
    )
    .await?;

    let start = Instant::now();
    let result = probe_connection(kind, &metadata).await;
    let latency_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);

    match result {
        Ok(detail) => Ok(ConnectionDiagnosticResult {
            ok: true,
            connection_id: connection_id.clone(),
            protocol: protocol_label,
            latency_ms: Some(latency_ms),
            message: "连接诊断通过".to_owned(),
            detail,
        }),
        Err(message) => Ok(ConnectionDiagnosticResult {
            ok: false,
            connection_id: connection_id.clone(),
            protocol: protocol_label,
            latency_ms: Some(latency_ms),
            message,
            detail: None,
        }),
    }
}

async fn probe_connection(
    kind: &str,
    metadata: &serde_json::Map<String, serde_json::Value>,
) -> Result<Option<String>, String> {
    match kind {
        "serial" => probe_serial(metadata).await,
        "modbus" => probe_tcp(metadata, "host", "port", 502, "Modbus TCP").await,
        "mqtt" => probe_tcp(metadata, "host", "port", 1883, "MQTT broker").await,
        "http" | "bark" => probe_http(metadata).await,
        "can-slcan" => probe_can(metadata).await,
        "ethercat" => Err("EtherCAT 诊断需要独占主站，请通过部署后运行态健康状态观察".to_owned()),
        _ => Err(format!("不支持的协议类型: {kind}")),
    }
}

async fn probe_serial(
    metadata: &serde_json::Map<String, serde_json::Value>,
) -> Result<Option<String>, String> {
    let port_path = metadata
        .get("port_path")
        .and_then(serde_json::Value::as_str)
        .ok_or("缺少 port_path 参数")?
        .to_owned();
    if port_path.is_empty() {
        return Err("串口路径为空".to_owned());
    }
    let baud_rate = metadata
        .get("baud_rate")
        .and_then(serde_json::Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .unwrap_or(9600);

    let label = port_path.clone();
    tokio::task::spawn_blocking(move || {
        let builder =
            serialport::new(&port_path, baud_rate).timeout(std::time::Duration::from_secs(3));
        builder
            .open()
            .map_err(|e| format!("无法打开串口 `{port_path}`: {e}"))?;
        Ok::<_, String>(())
    })
    .await
    .map_err(|e| format!("串口探测任务失败: {e}"))??;

    Ok(Some(format!("串口 `{label}` @ {baud_rate} 已打开并关闭")))
}

/// TCP 层连通性检测（适用于 Modbus / MQTT 等基于 TCP 的协议）。
async fn probe_tcp(
    metadata: &serde_json::Map<String, serde_json::Value>,
    host_key: &str,
    port_key: &str,
    default_port: u16,
    label: &str,
) -> Result<Option<String>, String> {
    let host = metadata
        .get(host_key)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("缺少 {host_key} 参数"))?;
    let port = metadata
        .get(port_key)
        .and_then(serde_json::Value::as_u64)
        .and_then(|v| u16::try_from(v).ok())
        .unwrap_or(default_port);

    let addr = format!("{host}:{port}");
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        tokio::net::TcpStream::connect(&addr),
    )
    .await
    .map_err(|_| format!("{label} 连接超时（5s）: `{addr}`"))?
    .map_err(|e| format!("{label} 连接失败 `{addr}`: {e}"))?;

    Ok(Some(format!("{label} `{addr}` TCP 连通")))
}

async fn probe_http(
    metadata: &serde_json::Map<String, serde_json::Value>,
) -> Result<Option<String>, String> {
    let url = metadata
        .get("url")
        .and_then(serde_json::Value::as_str)
        .ok_or("缺少 url 参数")?;

    let host_port = extract_host_port(url)?;
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        tokio::net::TcpStream::connect(&host_port),
    )
    .await
    .map_err(|_| format!("HTTP 连接超时（5s）: `{host_port}`"))?
    .map_err(|e| format!("HTTP 连接失败 `{host_port}`: {e}"))?;

    Ok(Some(format!("HTTP `{url}` TCP 连通（{host_port}）")))
}

fn extract_host_port(url: &str) -> Result<String, String> {
    let stripped = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .ok_or_else(|| format!("URL 格式无效（需 http:// 或 https://）: `{url}`"))?;
    let host_port = stripped.split('/').next().unwrap_or(stripped);
    if host_port.contains(':') {
        Ok(host_port.to_owned())
    } else if url.starts_with("https://") {
        Ok(format!("{host_port}:443"))
    } else {
        Ok(format!("{host_port}:80"))
    }
}

async fn probe_can(
    metadata: &serde_json::Map<String, serde_json::Value>,
) -> Result<Option<String>, String> {
    let channel = metadata
        .get("channel")
        .and_then(serde_json::Value::as_str)
        .ok_or("缺少 channel 参数")?
        .to_owned();
    let bitrate = metadata
        .get("bitrate")
        .and_then(serde_json::Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .unwrap_or(500_000);

    let label = channel.clone();
    tokio::task::spawn_blocking(move || {
        let builder = serialport::new(&channel, 115_200).timeout(std::time::Duration::from_secs(3));
        builder
            .open()
            .map_err(|e| format!("无法打开 CAN 通道 `{channel}`: {e}"))?;
        Ok::<_, String>(())
    })
    .await
    .map_err(|e| format!("CAN 通道探测任务失败: {e}"))??;

    Ok(Some(format!(
        "CAN 通道 `{label}` 串口已打开（bitrate={bitrate}），完整 SLCAN 初始化需部署后验证"
    )))
}
