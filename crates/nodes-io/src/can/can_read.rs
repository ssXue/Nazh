//! CAN 帧接收节点。
//!
//! 通过连接级共享 CAN 会话接收单帧 CAN 数据，将帧内容转换为 JSON payload。
//! 无连接时自动回退到 `MockBackend` 生成模拟帧。
//!
//! 节点级 CAN ID 过滤在接收到帧后本地执行，不过滤共享总线。

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use connections::{SharedConnectionManager, connection_metadata};
use nazh_core::{
    EngineError, LifecycleGuard, NodeExecution, NodeLifecycleContext, NodeTrait, PinDefinition,
    PinType, into_payload_map,
};

use crate::can::{
    CanFilter, CanFrame, hex,
    session::{self, CanBusRuntime},
    validate_can_id,
};

/// CAN 读节点配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanReadNodeConfig {
    /// 连接 ID（引用 `ConnectionManager` 中的连接定义）。
    #[serde(default)]
    pub connection_id: Option<String>,
    /// 可选：只接收指定 CAN ID 的帧。
    #[serde(default)]
    pub can_id: Option<u32>,
    /// 目标帧是否为扩展帧（29-bit）。
    #[serde(default)]
    pub is_extended: bool,
    /// 接收超时（毫秒），默认 1000。
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
}

fn default_timeout_ms() -> u64 {
    1000
}

/// CAN 帧接收节点。
pub struct CanReadNode {
    id: String,
    config: CanReadNodeConfig,
    connection_manager: SharedConnectionManager,
}

impl CanReadNode {
    pub fn new(
        id: impl Into<String>,
        config: CanReadNodeConfig,
        connection_manager: SharedConnectionManager,
    ) -> Self {
        Self {
            id: id.into(),
            config,
            connection_manager,
        }
    }

    /// 构造节点级过滤器（用于本地帧过滤，不设置到共享总线）。
    fn node_filter(&self) -> Option<CanFilter> {
        self.config.can_id.map(|can_id| {
            if self.config.is_extended {
                CanFilter::extended(can_id, 0x1FFF_FFFF)
            } else {
                CanFilter::standard(can_id, 0x7FF)
            }
        })
    }

    /// 检查帧是否通过节点级过滤。
    fn matches_filter(&self, frame: &CanFrame) -> bool {
        match self.node_filter() {
            Some(filter) => filter.matches(frame.id, frame.is_extended),
            None => true, // 无过滤器则接收所有帧
        }
    }
}

#[async_trait]
impl NodeTrait for CanReadNode {
    nazh_core::impl_node_meta!("canRead");

    fn output_pins(&self) -> Vec<PinDefinition> {
        vec![PinDefinition::output(
            PinType::Json,
            "接收到的 CAN 帧（id / data / dlc / is_extended / timestamp）",
        )]
    }

    async fn transform(
        &self,
        trace_id: Uuid,
        payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        if let Some(can_id) = self.config.can_id {
            validate_can_id(can_id, self.config.is_extended)
                .map_err(|error| EngineError::node_config(self.id.clone(), error.to_string()))?;
        }

        let connection_id = self
            .config
            .connection_id
            .clone()
            .unwrap_or_else(|| "mock-can".to_owned());

        let runtime = CanBusRuntime::new(self.connection_manager.clone(), connection_id);
        let session = runtime
            .ensure_session(&self.id, |_| Ok(()))
            .await
            .map_err(|error| {
                EngineError::stage_execution(self.id.clone(), trace_id, error.to_string())
            })?;

        let timeout = std::time::Duration::from_millis(self.config.timeout_ms);
        let bus_guard = session.bus(&self.id)?;
        let frame_result = match bus_guard.as_ref() {
            Some(bus) => bus.recv(timeout).await,
            None => {
                return Err(EngineError::stage_execution(
                    self.id.clone(),
                    trace_id,
                    "CAN 总线会话已被清理".to_owned(),
                ));
            }
        };
        drop(bus_guard);

        let frame = match frame_result {
            Ok(frame) => frame,
            Err(error) => {
                let reason = error.to_string();
                // 错误时关闭共享会话，所有共享节点下次 ensure_session 重建
                runtime.shutdown().await;
                if let Some(conn_id) = &self.config.connection_id {
                    let _ = self
                        .connection_manager
                        .record_connect_failure(conn_id, &reason)
                        .await;
                }
                return Err(EngineError::stage_execution(
                    self.id.clone(),
                    trace_id,
                    reason,
                ));
            }
        };

        // 本地过滤：按节点级 can_id 筛选
        let filtered_frame = frame.filter(|f| self.matches_filter(f));

        // 构建 payload
        let mut payload_map = into_payload_map(payload);
        if let Some(ref f) = filtered_frame {
            payload_map.insert(
                "can".to_owned(),
                json!({
                    "id": f.id,
                    "id_hex": format!("0x{:03X}", f.id),
                    "data": f.data,
                    "data_hex": hex::encode(&f.data).to_ascii_uppercase(),
                    "dlc": f.dlc,
                    "is_extended": f.is_extended,
                    "is_remote": f.is_remote,
                    "timestamp": f.timestamp.map(|t| t.to_rfc3339()),
                }),
            );
        } else {
            payload_map.insert("can".to_owned(), Value::Null);
        }

        // 构建 metadata
        let simulated = session.simulated();
        let channel_info = session.channel_info().to_owned();
        let connection_meta = session
            .lease()
            .map(|lease| connection_metadata(&self.id, lease))
            .transpose()?;

        let mut can_meta = Map::from_iter([
            ("simulated".to_owned(), json!(simulated)),
            ("channel_info".to_owned(), json!(channel_info)),
            ("sampled_at".to_owned(), json!(Utc::now().to_rfc3339())),
        ]);

        if let Some((key, value)) = connection_meta {
            can_meta.insert(key, value);
        }

        let metadata = Map::from_iter([("can".to_owned(), Value::Object(can_meta))]);
        Ok(NodeExecution::broadcast(Value::Object(payload_map)).with_metadata(Some(metadata)))
    }

    async fn on_deploy(&self, ctx: NodeLifecycleContext) -> Result<LifecycleGuard, EngineError> {
        let connection_id = match &self.config.connection_id {
            Some(id) => id.clone(),
            None => return Ok(LifecycleGuard::noop()),
        };

        // 验证连接并预热共享会话
        let runtime = CanBusRuntime::new(self.connection_manager.clone(), connection_id.clone());
        let session = runtime.ensure_session(&self.id, |_| Ok(())).await?;
        drop(session);

        Ok(session::lifecycle_guard(
            ctx,
            self.connection_manager.clone(),
            connection_id,
        ))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use connections::shared_connection_manager;

    fn make_node(config: CanReadNodeConfig) -> CanReadNode {
        CanReadNode::new("can-read-1", config, shared_connection_manager())
    }

    #[test]
    fn output_pin_是_json() {
        let node = make_node(CanReadNodeConfig {
            connection_id: None,
            can_id: None,
            is_extended: false,
            timeout_ms: 1000,
        });

        let pins = node.output_pins();
        assert_eq!(pins.len(), 1);
        assert_eq!(pins[0].id, "out");
        assert_eq!(pins[0].pin_type, PinType::Json);
    }

    #[tokio::test]
    async fn 标准帧_can_id_超限会失败() {
        let node = make_node(CanReadNodeConfig {
            connection_id: None,
            can_id: Some(0x800),
            is_extended: false,
            timeout_ms: 1,
        });

        let err = node
            .transform(Uuid::new_v4(), Value::Null)
            .await
            .unwrap_err();
        assert!(matches!(err, EngineError::NodeConfig { .. }));
    }
}
