//! 可观测性存储：`SQLite` Store 持久化、告警投递、链路摘要。
//!
//! 提供 `ObservabilityStore` 供运行时记录节点执行事件，
//! 以及 `query_observability` / `clear_observability_store` 供 IPC 命令调用。

pub(crate) mod alerting;
pub(crate) mod store;
pub(crate) mod types;

pub(crate) use store::{clear_observability_store, query_observability};
pub(crate) use types::{ObservabilityStore, SharedObservabilityStore};

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::store::query_observability;
    use super::types::{ObservabilityStore, span_key};
    use nazh_engine::{CompletedExecutionEvent, ExecutionEvent};
    use store::{Store, StoreHandle};
    use tauri_bindings::ObservabilityContextInput;
    use uuid::Uuid;

    fn test_context() -> ObservabilityContextInput {
        ObservabilityContextInput {
            workspace_path: String::new(),
            project_id: "project-test".to_owned(),
            project_name: "测试项目".to_owned(),
            environment_id: "env-test".to_owned(),
            environment_name: "测试环境".to_owned(),
            deployment_source: "test".to_owned(),
        }
    }

    #[tokio::test]
    async fn completed_事件会清理_active_span() {
        let store = ObservabilityStore::new(test_context(), None);
        let trace_id = Uuid::new_v4();
        let node_stage = "node_a";
        let sk = span_key(trace_id, node_stage);

        let started = ExecutionEvent::Started {
            stage: node_stage.to_owned(),
            trace_id,
        };
        let Ok(_) = store.record_execution_event(&started).await else {
            panic!("started 事件应可记录");
        };
        {
            let runtime_state = store.state.lock().await;
            assert!(runtime_state.active_spans.contains_key(&sk));
        }

        let completed = ExecutionEvent::Completed(CompletedExecutionEvent {
            stage: node_stage.to_owned(),
            trace_id,
            metadata: None,
        });
        let Ok(_) = store.record_execution_event(&completed).await else {
            panic!("completed 事件应可记录");
        };
        {
            let runtime_state = store.state.lock().await;
            assert!(!runtime_state.active_spans.contains_key(&sk));
        }
    }

    #[tokio::test]
    async fn query_observability_从_store_读取记录() {
        let handle = StoreHandle::new(Store::open_unpersisted().expect("内存 Store 应可打开"));
        let store = ObservabilityStore::new(test_context(), Some(&handle));

        store.record_audit(
            "info",
            "workflow",
            "部署完成",
            Some("workflow_id=wf-store".to_owned()),
            Some("trace-store".to_owned()),
            None,
        );

        // 给 batch writer 一点时间 flush
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let result = query_observability(
            Some(handle),
            Some("trace-store".to_owned()),
            Some("部署".to_owned()),
            20,
        )
        .await
        .expect("Store 查询应成功");

        assert_eq!(result.audits.len(), 1);
        assert_eq!(result.audits[0].message, "部署完成");
    }
}
