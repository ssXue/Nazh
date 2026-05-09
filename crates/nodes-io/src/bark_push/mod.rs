//! Bark 推送节点：向 Bark 服务发送 iOS 推送通知。
//!
//! 默认使用 `POST https://api.day.app/{device_key}` 的 JSON 请求格式。
//! `device_key` 字段也支持直接粘贴形如 `https://api.day.app/{key}` 的 URL，
//! 节点会自动提取其中的 key 作为目标端点。

use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

use crate::template::TemplateVars;
use connections::{SharedConnectionManager, connection_metadata};
use nazh_core::{EngineError, NodeExecution, NodeTrait, into_payload_map};

mod config;
mod metadata;
mod request;

pub use config::BarkPushNodeConfig;

use config::{normalize_bark_archive_mode, normalize_bark_content_mode, normalize_bark_level};
use metadata::{BarkMetadataParams, build_bark_metadata};
use request::{
    build_bark_request_body, resolve_bark_endpoint, send_bark_push, validate_bark_response,
};

pub struct BarkPushNode {
    id: String,
    config: BarkPushNodeConfig,
    client: reqwest::Client,
    connection_manager: SharedConnectionManager,
}

impl BarkPushNode {
    pub fn new(
        id: impl Into<String>,
        config: BarkPushNodeConfig,
        connection_manager: SharedConnectionManager,
    ) -> Result<Self, EngineError> {
        let id = id.into();
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .map_err(|error| {
                EngineError::node_config(id.clone(), format!("Bark 客户端初始化失败: {error}"))
            })?;
        Ok(Self {
            id,
            config,
            client,
            connection_manager,
        })
    }

    fn resolve_config(
        &self,
        connection_metadata: Option<&Value>,
    ) -> Result<BarkPushNodeConfig, EngineError> {
        let mut config_value = serde_json::to_value(&self.config)
            .map_err(|error| EngineError::node_config(self.id.clone(), error.to_string()))?;

        if let Some(metadata) = connection_metadata.and_then(Value::as_object) {
            let Some(config_map) = config_value.as_object_mut() else {
                return Err(EngineError::node_config(
                    self.id.clone(),
                    "Bark Push 配置格式无效".to_owned(),
                ));
            };

            for key in ["server_url", "device_key", "request_timeout_ms"] {
                if let Some(value) = metadata.get(key) {
                    config_map.insert(key.to_owned(), value.clone());
                }
            }
        }

        serde_json::from_value(config_value)
            .map_err(|error| EngineError::node_config(self.id.clone(), error.to_string()))
    }
}

#[async_trait]
impl NodeTrait for BarkPushNode {
    nazh_core::impl_node_meta!("barkPush");

    async fn transform(
        &self,
        trace_id: Uuid,
        payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        let connection_id = self.config.connection_id.as_deref().ok_or_else(|| {
            EngineError::node_config(
                self.id.clone(),
                "Bark Push 节点需要在 Connection Studio 中绑定一个 Bark 连接",
            )
        })?;
        let mut guard = Some(self.connection_manager.acquire(connection_id).await?);
        let resolved_config =
            self.resolve_config(guard.as_ref().map(connections::ConnectionGuard::metadata))?;

        let endpoint =
            resolve_bark_endpoint(&resolved_config.server_url, &resolved_config.device_key)
                .map_err(|message| EngineError::node_config(self.id.clone(), message))?;
        let now = Utc::now();
        let requested_at = now.to_rfc3339();
        let event_timestamp = now.to_rfc3339();
        let content_mode = normalize_bark_content_mode(&resolved_config.content_mode);
        let level = normalize_bark_level(&resolved_config.level);
        let archive_mode = normalize_bark_archive_mode(&resolved_config.archive_mode);
        let request_timeout_ms = resolved_config.request_timeout_ms.max(500);

        let vars = TemplateVars {
            payload: &payload,
            trace_id: &trace_id,
            node_id: &self.id,
            timestamp: &event_timestamp,
            extras: &[("requested_at", requested_at.as_str())],
        };

        let request_body_value = build_bark_request_body(
            &resolved_config,
            &vars,
            content_mode,
            level,
            archive_mode,
            &self.id,
            trace_id,
        )?;

        let result = match send_bark_push(
            &self.client,
            &endpoint,
            &request_body_value,
            request_timeout_ms,
        )
        .await
        {
            Ok(result) => result,
            Err(message) => {
                if let Some(connection_guard) = &mut guard {
                    connection_guard.mark_failure(&message);
                }
                return Err(EngineError::stage_execution(
                    self.id.clone(),
                    trace_id,
                    message,
                ));
            }
        };

        if let Err(message) = validate_bark_response(result.status_code, &result.response_value) {
            if let Some(connection_guard) = &mut guard {
                connection_guard.mark_failure(&message);
            }
            return Err(EngineError::stage_execution(
                self.id.clone(),
                trace_id,
                message,
            ));
        }

        let mut payload_map = into_payload_map(payload);
        payload_map.insert("bark_response".to_owned(), result.response_value);

        let mut metadata = serde_json::Map::new();
        if let Some(connection_guard) = guard.as_ref() {
            let (key, value) = connection_metadata(&self.id, connection_guard.lease())?;
            metadata.insert(key, value);
        }
        let bark_meta = build_bark_metadata(&BarkMetadataParams {
            node_id: &self.id,
            endpoint: &endpoint,
            content_mode,
            level,
            status_code: result.status_code,
            request_timeout_ms,
            requested_at: &requested_at,
            request_body_preview: &result.request_body_preview,
        });
        metadata.extend(bark_meta);

        if let Some(connection_guard) = &mut guard {
            connection_guard.mark_success();
        }

        Ok(NodeExecution::broadcast(Value::Object(payload_map)).with_metadata(Some(metadata)))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[path = "tests.rs"]
mod tests;
