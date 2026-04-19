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
use ts_rs::TS;
use uuid::Uuid;

use crate::error::EngineError;

/// 统一的执行生命周期事件。
///
/// DAG 工作流和线性流水线共享同一事件类型，
/// 前端只需注册一个事件监听器即可处理所有执行模式。
#[derive(Debug, Clone, PartialEq, TS)]
#[ts(export)]
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
}

/// 阶段/节点执行完成事件的详细载荷。
#[derive(Debug, Clone, PartialEq, TS)]
#[ts(export)]
pub struct CompletedExecutionEvent {
    pub stage: String,
    pub trace_id: Uuid,
    /// 节点执行元数据（协议参数、连接信息等），与业务 payload 完全分离。
    /// 无元数据时为 `None`，序列化时省略该字段。
    #[ts(optional)]
    pub metadata: Option<Map<String, Value>>,
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

        let value = serde_json::to_value(event).expect("事件应可序列化");

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

        let event: ExecutionEvent = serde_json::from_value(value).expect("事件应可反序列化");

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
