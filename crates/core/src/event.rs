//! 统一的执行生命周期事件与事件发射辅助。
//!
//! [`ExecutionEvent`] 覆盖 DAG 工作流和线性流水线两种执行模式，
//! 替代原先独立的 `WorkflowEvent` 和 `PipelineEvent`。
//!
//! 事件发射使用 `try_send`（非阻塞），确保可观测性不会拖慢数据通路。
//! 通道满或关闭时通过 `tracing::error!` 报告——事件丢失即丢帧，不可接受。

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tokio::sync::mpsc;
#[cfg(feature = "ts-export")]
use ts_rs::TS;
use uuid::Uuid;

use crate::pin::PinKind;

use crate::error::EngineError;

/// 统一的执行生命周期事件。
///
/// DAG 工作流和线性流水线共享同一事件类型，
/// 前端只需注册一个事件监听器即可处理所有执行模式。
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
pub enum ExecutionEvent {
    /// 阶段/节点开始执行。
    Started { stage: String, trace_id: Uuid },
    /// 阶段/节点执行完成，附带该节点的执行元数据。
    Completed(CompletedExecutionEvent),
    /// 阶段/节点执行失败。
    Failed {
        stage: String,
        trace_id: Uuid,
        error: String,
    },
    /// 叶节点产出最终结果（仅 DAG 工作流模式下发出）。
    Output { stage: String, trace_id: Uuid },
    /// 整条流水线执行完毕（仅线性流水线模式下发出）。
    Finished { trace_id: Uuid },
    /// 工作流变量值变更（ADR-0012 Phase 2，write-on-change 语义）。
    ///
    /// 仅当 `set` / `compare_and_swap` 检测到 `entry.value != new` 时 emit；
    /// 写入相同值不触发本事件（避免轮询脚本制造事件刷屏）。
    /// `updated_at` 是 RFC3339 字符串，保持与 [`TypedVariableSnapshot`](crate::TypedVariableSnapshot) 一致；
    /// `updated_by` 是写入方 `node_id`（IPC 写入时为 `Some("ipc")` / 类似哨兵）。
    /// `workflow_id` 由 emit 路径在事件构造时注入——`WorkflowVariables` 自身不持有
    /// 所属 workflow 的 id，调用方（`set` / `compare_and_swap` 的 emit 闭包）负责传入。
    VariableChanged {
        workflow_id: String,
        name: String,
        value: serde_json::Value,
        updated_at: String,
        #[cfg_attr(feature = "ts-export", ts(optional))]
        updated_by: Option<String>,
    },
    /// 工作流变量被删除（ADR-0012 Phase 3）。
    VariableDeleted { workflow_id: String, name: String },
    /// 边传输汇总（ADR-0016，默认 100ms 窗口）。
    EdgeTransmitSummary(EdgeTransmitSummary),
    /// 背压告警（ADR-0016，下游 channel 接近容量上限）。
    BackpressureDetected(BackpressureDetected),
}

/// 阶段/节点执行完成事件的详细载荷。
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
pub struct CompletedExecutionEvent {
    pub stage: String,
    pub trace_id: Uuid,
    /// 节点执行元数据（协议参数、连接信息等），与业务 payload 完全分离。
    /// 无元数据时为 `None`，序列化时省略该字段。
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub metadata: Option<Map<String, Value>>,
}

/// 边传输汇总事件（ADR-0016）。
///
/// Runner 在每次向下游 channel 发送数据后累计窗口内统计，每 100ms 刷新一条汇总。
/// 前端用 `from_node + from_pin → to_node + to_pin` 标识一条边，
/// 叠加到画布线条上实现热力图。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub struct EdgeTransmitSummary {
    pub from_node: String,
    pub from_pin: String,
    pub to_node: String,
    pub to_pin: String,
    pub edge_kind: PinKind,
    pub transmit_count: usize,
    pub max_queue_depth: usize,
    pub window_started_at: String,
    pub window_ended_at: String,
}

/// 背压告警事件（ADR-0016）。
///
/// 下游 channel 深度接近容量上限时发出。
/// 发射逻辑暂未实施（类型就位，`#[allow(dead_code)]` 抑制警告）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub struct BackpressureDetected {
    pub at_node: String,
    pub incoming_pin: String,
    pub channel_capacity: usize,
    pub channel_depth: usize,
    pub policy: BackpressurePolicy,
    pub dropped_since_last_report: u64,
    pub detected_at: String,
}

/// 背压处理策略（ADR-0016）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum BackpressurePolicy {
    Block,
    DropNewest,
    DropOldest,
    Sample,
    Overflow,
}

#[derive(Serialize, Deserialize)]
enum ExecutionEventSerde {
    Started {
        stage: String,
        trace_id: Uuid,
    },
    Completed {
        stage: String,
        trace_id: Uuid,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        metadata: Option<Map<String, Value>>,
    },
    Failed {
        stage: String,
        trace_id: Uuid,
        error: String,
    },
    Output {
        stage: String,
        trace_id: Uuid,
    },
    Finished {
        trace_id: Uuid,
    },
    VariableChanged {
        workflow_id: String,
        name: String,
        value: serde_json::Value,
        updated_at: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        updated_by: Option<String>,
    },
    EdgeTransmitSummary(EdgeTransmitSummary),
    BackpressureDetected(BackpressureDetected),
    VariableDeleted {
        workflow_id: String,
        name: String,
    },
}

impl From<&ExecutionEvent> for ExecutionEventSerde {
    fn from(value: &ExecutionEvent) -> Self {
        match value {
            ExecutionEvent::Started { stage, trace_id } => Self::Started {
                stage: stage.clone(),
                trace_id: *trace_id,
            },
            ExecutionEvent::Completed(completed) => Self::Completed {
                stage: completed.stage.clone(),
                trace_id: completed.trace_id,
                metadata: completed.metadata.clone(),
            },
            ExecutionEvent::Failed {
                stage,
                trace_id,
                error,
            } => Self::Failed {
                stage: stage.clone(),
                trace_id: *trace_id,
                error: error.clone(),
            },
            ExecutionEvent::Output { stage, trace_id } => Self::Output {
                stage: stage.clone(),
                trace_id: *trace_id,
            },
            ExecutionEvent::Finished { trace_id } => Self::Finished {
                trace_id: *trace_id,
            },
            ExecutionEvent::VariableChanged {
                workflow_id,
                name,
                value,
                updated_at,
                updated_by,
            } => Self::VariableChanged {
                workflow_id: workflow_id.clone(),
                name: name.clone(),
                value: value.clone(),
                updated_at: updated_at.clone(),
                updated_by: updated_by.clone(),
            },
            ExecutionEvent::EdgeTransmitSummary(summary) => {
                Self::EdgeTransmitSummary(summary.clone())
            }
            ExecutionEvent::BackpressureDetected(detected) => {
                Self::BackpressureDetected(detected.clone())
            }
            ExecutionEvent::VariableDeleted { workflow_id, name } => Self::VariableDeleted {
                workflow_id: workflow_id.clone(),
                name: name.clone(),
            },
        }
    }
}

impl From<ExecutionEventSerde> for ExecutionEvent {
    fn from(value: ExecutionEventSerde) -> Self {
        match value {
            ExecutionEventSerde::Started { stage, trace_id } => Self::Started { stage, trace_id },
            ExecutionEventSerde::Completed {
                stage,
                trace_id,
                metadata,
            } => Self::Completed(CompletedExecutionEvent {
                stage,
                trace_id,
                metadata,
            }),
            ExecutionEventSerde::Failed {
                stage,
                trace_id,
                error,
            } => Self::Failed {
                stage,
                trace_id,
                error,
            },
            ExecutionEventSerde::Output { stage, trace_id } => Self::Output { stage, trace_id },
            ExecutionEventSerde::Finished { trace_id } => Self::Finished { trace_id },
            ExecutionEventSerde::VariableChanged {
                workflow_id,
                name,
                value,
                updated_at,
                updated_by,
            } => Self::VariableChanged {
                workflow_id,
                name,
                value,
                updated_at,
                updated_by,
            },
            ExecutionEventSerde::EdgeTransmitSummary(summary) => Self::EdgeTransmitSummary(summary),
            ExecutionEventSerde::BackpressureDetected(detected) => {
                Self::BackpressureDetected(detected)
            }
            ExecutionEventSerde::VariableDeleted { workflow_id, name } => {
                Self::VariableDeleted { workflow_id, name }
            }
        }
    }
}

impl Serialize for ExecutionEvent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        ExecutionEventSerde::from(self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ExecutionEvent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(ExecutionEventSerde::deserialize(deserializer)?.into())
    }
}

/// 非阻塞发送执行事件。
///
/// 使用 `try_send` 而非 `.await`，保证事件发射不阻塞节点的数据处理循环。
/// 通道满或关闭时记录 `error!`——事件丢失即丢帧，属于系统异常。
pub fn emit_event(tx: &mpsc::Sender<ExecutionEvent>, event: ExecutionEvent) {
    if let Err(err) = tx.try_send(event) {
        match err {
            mpsc::error::TrySendError::Full(dropped) => {
                tracing::error!(?dropped, "事件通道已满，事件被丢弃");
            }
            mpsc::error::TrySendError::Closed(dropped) => {
                tracing::error!(?dropped, "事件通道已关闭，事件消费者可能已崩溃");
            }
        }
    }
}

/// 发送失败事件并记录结构化日志。
pub fn emit_failure(
    tx: &mpsc::Sender<ExecutionEvent>,
    stage: &str,
    trace_id: Uuid,
    error: &EngineError,
) {
    tracing::warn!(stage, trace_id = %trace_id, error = %error, "阶段执行失败");
    emit_event(
        tx,
        ExecutionEvent::Failed {
            stage: stage.to_owned(),
            trace_id,
            error: error.to_string(),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn started_event() -> ExecutionEvent {
        ExecutionEvent::Started {
            stage: "test-node".to_owned(),
            trace_id: Uuid::new_v4(),
        }
    }

    #[test]
    fn 正常发送事件进入通道() {
        let (tx, mut rx) = mpsc::channel(4);
        let event = started_event();
        let expected = event.clone();

        emit_event(&tx, event);

        let received = rx.try_recv();
        assert_eq!(received.ok(), Some(expected));
    }

    #[test]
    fn 通道满时事件被丢弃且不崩溃() {
        let (tx, _rx) = mpsc::channel(1);

        emit_event(&tx, started_event());
        // 通道容量为 1，第二次应触发 Full 分支
        emit_event(&tx, started_event());
    }

    #[test]
    fn 通道关闭时事件被丢弃且不崩溃() {
        let (tx, rx) = mpsc::channel(4);
        drop(rx);

        // 接收端已 drop，应触发 Closed 分支
        emit_event(&tx, started_event());
    }

    #[test]
    fn emit_failure_构造正确的失败事件() {
        let (tx, mut rx) = mpsc::channel(4);
        let trace_id = Uuid::new_v4();
        let error = EngineError::invalid_graph("测试错误");

        emit_failure(&tx, "fail-node", trace_id, &error);

        let received = rx.try_recv();
        match received {
            Ok(ExecutionEvent::Failed {
                stage,
                trace_id: tid,
                error: msg,
            }) => {
                assert_eq!(stage, "fail-node");
                assert_eq!(tid, trace_id);
                assert!(msg.contains("测试错误"));
            }
            other => panic!("应收到 Failed 事件，实际收到: {other:?}"),
        }
    }

    #[test]
    fn completed_事件在无元数据时省略_metadata_字段() {
        let trace_id = Uuid::from_bytes([0; 16]);
        let event = ExecutionEvent::Completed(CompletedExecutionEvent {
            stage: "test-node".to_owned(),
            trace_id,
            metadata: None,
        });

        let Ok(value) = serde_json::to_value(event) else {
            panic!("事件应可序列化");
        };

        assert_eq!(
            value,
            json!({
                "Completed": {
                    "stage": "test-node",
                    "trace_id": trace_id,
                }
            })
        );
    }

    #[test]
    fn completed_事件可反序列化缺省_metadata_字段() {
        let trace_id = Uuid::from_bytes([1; 16]);
        let value = json!({
            "Completed": {
                "stage": "test-node",
                "trace_id": trace_id,
            }
        });

        let Ok(event) = serde_json::from_value(value) else {
            panic!("事件应可反序列化");
        };
        let event: ExecutionEvent = event;

        assert_eq!(
            event,
            ExecutionEvent::Completed(CompletedExecutionEvent {
                stage: "test-node".to_owned(),
                trace_id,
                metadata: None,
            })
        );
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod variable_changed_tests {
    use super::*;

    #[test]
    fn variable_changed_往返序列化() {
        let event = ExecutionEvent::VariableChanged {
            workflow_id: "wf-1".to_owned(),
            name: "setpoint".to_owned(),
            value: serde_json::json!(25.5),
            updated_at: "2026-04-27T10:00:00+00:00".to_owned(),
            updated_by: Some("node-A".to_owned()),
        };
        let json = serde_json::to_string(&event).unwrap();
        let restored: ExecutionEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, restored);
    }

    #[test]
    fn variable_changed_value_为嵌套对象时往返序列化() {
        let event = ExecutionEvent::VariableChanged {
            workflow_id: "wf-1".to_owned(),
            name: "config".to_owned(),
            value: serde_json::json!({"threshold": 10, "tags": ["a", "b"]}),
            updated_at: "2026-04-27T10:00:00+00:00".to_owned(),
            updated_by: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        // 顺带验证 skip_serializing_if = "Option::is_none"：updated_by 字段不应出现
        assert!(
            !json.contains("updated_by"),
            "updated_by = None 时不应出现在序列化输出，实际：{json}"
        );
        let restored: ExecutionEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, restored);
    }

    #[test]
    fn variable_changed_updated_by_缺省时反序列化为_none() {
        let json = serde_json::json!({
            "VariableChanged": {
                "workflow_id": "wf-1",
                "name": "x",
                "value": 1,
                "updated_at": "2026-04-27T10:00:00+00:00"
            }
        });
        let restored: ExecutionEvent = serde_json::from_value(json).unwrap();
        match restored {
            ExecutionEvent::VariableChanged { updated_by, .. } => {
                assert!(updated_by.is_none(), "updated_by 缺省应为 None");
            }
            other => panic!("expected VariableChanged, got {other:?}"),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod edge_event_tests {
    use super::*;

    #[test]
    fn edge_transmit_summary_往返序列化() {
        let summary = EdgeTransmitSummary {
            from_node: "timer-1".to_owned(),
            from_pin: "out".to_owned(),
            to_node: "debug-1".to_owned(),
            to_pin: "in".to_owned(),
            edge_kind: PinKind::Exec,
            transmit_count: 5,
            max_queue_depth: 3,
            window_started_at: "2026-04-30T10:00:00+00:00".to_owned(),
            window_ended_at: "2026-04-30T10:00:00.100+00:00".to_owned(),
        };
        let event = ExecutionEvent::EdgeTransmitSummary(summary.clone());
        let json = serde_json::to_string(&event).unwrap();
        let restored: ExecutionEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, restored);
    }

    #[test]
    fn backpressure_detected_往返序列化() {
        let detected = BackpressureDetected {
            at_node: "debug-1".to_owned(),
            incoming_pin: "in".to_owned(),
            channel_capacity: 16,
            channel_depth: 14,
            policy: BackpressurePolicy::Block,
            dropped_since_last_report: 0,
            detected_at: "2026-04-30T10:00:01+00:00".to_owned(),
        };
        let event = ExecutionEvent::BackpressureDetected(detected);
        let json = serde_json::to_string(&event).unwrap();
        let restored: ExecutionEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, restored);
    }

    #[test]
    fn backpressure_policy_序列化为_camel_case() {
        assert_eq!(
            serde_json::to_string(&BackpressurePolicy::DropNewest).unwrap(),
            "\"dropNewest\""
        );
        assert_eq!(
            serde_json::to_string(&BackpressurePolicy::DropOldest).unwrap(),
            "\"dropOldest\""
        );
    }
}
