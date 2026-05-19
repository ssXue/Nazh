//! 可观测性 + 部署审计 async 句柄方法。

use std::sync::Arc;

use crate::{BatchWriter, DeploymentAuditRecord, Store, StoreError, StoredObservabilityRecord};
use super::StoreHandle;

/// 批量写入器的可观测性记录条目。
#[derive(Debug)]
pub struct ObservabilityBatchItem {
    pub id: String,
    pub record_kind: String,
    pub category: String,
    pub timestamp: String,
    pub trace_id: Option<String>,
    pub search_text: String,
    pub payload: serde_json::Value,
}

impl StoreHandle {
    /// 写入一条可观测性记录。
    #[allow(clippy::too_many_arguments)]
    pub async fn insert_observability_record(
        &self,
        id: &str,
        record_kind: &str,
        category: &str,
        timestamp: &str,
        trace_id: Option<&str>,
        search_text: &str,
        payload: &serde_json::Value,
    ) -> Result<(), StoreError> {
        let id = id.to_owned();
        let record_kind = record_kind.to_owned();
        let category = category.to_owned();
        let timestamp = timestamp.to_owned();
        let trace_id = trace_id.map(str::to_owned);
        let search_text = search_text.to_owned();
        let payload = payload.clone();
        self.run_blocking(move |store| {
            store.insert_observability_record(
                &id,
                &record_kind,
                &category,
                &timestamp,
                trace_id.as_deref(),
                &search_text,
                &payload,
            )
        })
        .await
    }

    /// 查询可观测性记录。
    pub async fn query_observability_records(
        &self,
        trace_id: Option<&str>,
        search: Option<&str>,
        limit: usize,
    ) -> Result<Vec<StoredObservabilityRecord>, StoreError> {
        let trace_id = trace_id.map(str::to_owned);
        let search = search.map(str::to_owned);
        self.run_blocking(move |store| {
            store.query_observability_records(trace_id.as_deref(), search.as_deref(), limit)
        })
        .await
    }

    /// 清空可观测性记录。
    pub async fn clear_observability_records(&self) -> Result<(), StoreError> {
        self.run_blocking(Store::clear_observability_records).await
    }

    /// 追加部署审计记录。
    pub async fn insert_deployment_audit(
        &self,
        record: DeploymentAuditRecord,
    ) -> Result<(), StoreError> {
        self.run_blocking(move |store| store.insert_deployment_audit(&record))
            .await
    }

    /// 查询指定工作流的部署审计记录。
    pub async fn list_deployment_audit(
        &self,
        workflow_id: &str,
        limit: usize,
    ) -> Result<Vec<DeploymentAuditRecord>, StoreError> {
        let workflow_id = workflow_id.to_owned();
        self.run_blocking(move |store| store.list_deployment_audit(&workflow_id, limit))
            .await
    }

    /// 创建可观测性记录的批量写入器。
    ///
    /// 后台 task 按 `flush_capacity` 条或 `flush_interval_ms` 毫秒批量写入。
    /// 返回的 [`BatchWriter`] 生命周期应与 [`ObservabilityStore`](super::super::ObservabilityStore) 一致。
    pub fn observability_batch_writer(
        &self,
        flush_capacity: usize,
        flush_interval_ms: u64,
    ) -> BatchWriter<ObservabilityBatchItem> {
        let store = Arc::clone(&self.store);
        BatchWriter::new(
            1024,
            flush_capacity,
            flush_interval_ms,
            store,
            |store: &Store, batch: Vec<ObservabilityBatchItem>| {
                let rows: Vec<_> = batch
                    .into_iter()
                    .map(|item: ObservabilityBatchItem| {
                        (
                            item.id,
                            item.record_kind,
                            item.category,
                            item.timestamp,
                            item.trace_id,
                            item.search_text,
                            item.payload,
                        )
                    })
                    .collect();
                store.insert_observability_record_batch(&rows)
            },
        )
    }
}
