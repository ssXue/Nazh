//! `ObservabilityStore` 核心实现与查询。

use std::{collections::HashMap, collections::HashSet, sync::Arc};

use chrono::Utc;
use nazh_engine::{ExecutionEvent, WorkflowContext};
use serde_json::Value;
use store::{ObservabilityBatchItem, StoreHandle};
use tauri_bindings::{
    AlertDeliveryRecord, ObservabilityContextInput, ObservabilityEntry, ObservabilityQueryResult,
    ObservabilityTraceSummary,
};
use tokio::sync::Mutex;

use super::alerting::{alert_to_entry, build_alert_delivery};
use super::types::{
    ObservabilityEntryDraft, ObservabilityRuntimeState, ObservabilitySession, ObservabilityStore,
    SharedObservabilityStore, build_record_id, elapsed_ms_since, entry_search_text, max_timestamp,
    min_timestamp, span_key, summarize_payload,
};

impl ObservabilityStore {
    pub fn new(
        context: ObservabilityContextInput,
        store_handle: Option<&StoreHandle>,
    ) -> SharedObservabilityStore {
        let batch_writer = store_handle.map(|s| s.observability_batch_writer(100, 100));
        Arc::new(Self {
            session: ObservabilitySession {
                project_id: context.project_id,
                project_name: context.project_name,
                environment_id: context.environment_id,
                environment_name: context.environment_name,
            },
            state: Mutex::new(ObservabilityRuntimeState::default()),
            batch_writer,
        })
    }

    #[allow(clippy::too_many_lines)]
    pub async fn record_execution_event(
        &self,
        event: &ExecutionEvent,
    ) -> Result<ObservabilityEntry, String> {
        let now = Utc::now();
        let mut runtime_state = self.state.lock().await;

        runtime_state
            .active_spans
            .retain(|_, started_at| (now - *started_at).num_seconds() < 3600);

        let (draft, clear_span) = match event {
            ExecutionEvent::Started { stage, trace_id } => {
                runtime_state
                    .active_spans
                    .insert(span_key(trace_id, stage), now);
                (
                    ObservabilityEntryDraft::execution(
                        "info",
                        "started",
                        stage.clone(),
                        "节点开始执行".to_owned(),
                        trace_id.to_string(),
                        now,
                    )
                    .with_node_id(Some(stage.clone())),
                    false,
                )
            }
            ExecutionEvent::Completed(completed) => {
                let node_stage = &completed.stage;
                let trace_id = completed.trace_id;
                let metadata = &completed.metadata;
                let duration_ms = runtime_state
                    .active_spans
                    .get(&span_key(trace_id, node_stage))
                    .map(|started_at| elapsed_ms_since(now, *started_at));

                let pending_alert = metadata
                    .as_ref()
                    .and_then(|m| m.get("http"))
                    .and_then(Value::as_object)
                    .and_then(|meta| {
                        build_alert_delivery(&self.session, node_stage, trace_id, meta, now)
                    });
                runtime_state
                    .active_spans
                    .remove(&span_key(trace_id, node_stage));

                drop(runtime_state);

                if let Some(alert) = pending_alert {
                    self.persist_alert(&alert);
                }

                let draft = ObservabilityEntryDraft::execution(
                    "success",
                    "completed",
                    node_stage.clone(),
                    "节点执行完成".to_owned(),
                    trace_id.to_string(),
                    now,
                )
                .with_detail(duration_ms.map(|ms| format!("节点耗时 {ms} ms")))
                .with_node_id(Some(node_stage.clone()))
                .with_duration_ms(duration_ms);
                let entry = self.build_entry(draft);
                self.persist_entry("event", &entry);
                return Ok(entry);
            }
            ExecutionEvent::Failed {
                stage,
                trace_id,
                error,
            } => {
                let duration_ms = runtime_state
                    .active_spans
                    .get(&span_key(trace_id, stage))
                    .map(|started_at| elapsed_ms_since(now, *started_at));
                (
                    ObservabilityEntryDraft::execution(
                        "error",
                        "failed",
                        stage.clone(),
                        "节点执行失败".to_owned(),
                        trace_id.to_string(),
                        now,
                    )
                    .with_detail(Some(error.clone()))
                    .with_node_id(Some(stage.clone()))
                    .with_duration_ms(duration_ms),
                    true,
                )
            }
            ExecutionEvent::Output { stage, trace_id } => (
                ObservabilityEntryDraft::execution(
                    "success",
                    "output",
                    stage.clone(),
                    "节点产生输出".to_owned(),
                    trace_id.to_string(),
                    now,
                )
                .with_node_id(Some(stage.clone())),
                false,
            ),
            ExecutionEvent::Finished { trace_id } => (
                ObservabilityEntryDraft::execution(
                    "success",
                    "finished",
                    "workflow".to_owned(),
                    "执行链路完成".to_owned(),
                    trace_id.to_string(),
                    now,
                ),
                false,
            ),
            // 变量事件已从 ExecutionEvent 中拆出到独立的 WorkflowVariableEvent 通道
            // （B1-R0-01/B1-R0-05），不再经过可观测性日志路径。
            // ADR-0016：边级观测事件不持久化到可观测性日志——
            // 它们通过 workflow://node-status 实时流向前端。
            ExecutionEvent::EdgeTransmitSummary(_) | ExecutionEvent::BackpressureDetected(_) => {
                return Ok(self.build_entry(ObservabilityEntryDraft::execution(
                    "info",
                    "edge_event_skip",
                    "edge".to_owned(),
                    "边级事件不持久化（实时流转发）".to_owned(),
                    String::new(),
                    now,
                )));
            }
        };

        if clear_span {
            match event {
                ExecutionEvent::Completed(completed) => {
                    runtime_state
                        .active_spans
                        .remove(&span_key(completed.trace_id, &completed.stage));
                }
                ExecutionEvent::Failed {
                    stage, trace_id, ..
                } => {
                    runtime_state
                        .active_spans
                        .remove(&span_key(trace_id, stage));
                }
                _ => {}
            }
        }
        drop(runtime_state);

        let entry = self.build_entry(draft);
        self.persist_entry("event", &entry);
        Ok(entry)
    }

    pub fn record_result(&self, result: &WorkflowContext) {
        let now = Utc::now();
        let result_entry = self.build_entry(ObservabilityEntryDraft {
            level: "success".to_owned(),
            category: "result".to_owned(),
            source: "result".to_owned(),
            message: "结果载荷输出".to_owned(),
            detail: None,
            trace_id: Some(result.trace_id.to_string()),
            node_id: None,
            duration_ms: None,
            data: Some(summarize_payload(&result.payload)),
            timestamp: now,
            event_kind: None,
        });
        self.persist_entry("event", &result_entry);
    }

    pub fn record_audit(
        &self,
        level: &str,
        source: &str,
        message: &str,
        detail: Option<String>,
        trace_id: Option<String>,
        data: Option<Value>,
    ) {
        let entry = self.build_entry(ObservabilityEntryDraft {
            level: level.to_owned(),
            category: "audit".to_owned(),
            source: source.to_owned(),
            message: message.to_owned(),
            detail,
            trace_id,
            node_id: None,
            duration_ms: None,
            data,
            timestamp: Utc::now(),
            event_kind: None,
        });
        self.persist_entry("audit", &entry);
    }

    fn build_entry(&self, draft: ObservabilityEntryDraft) -> ObservabilityEntry {
        ObservabilityEntry {
            id: build_record_id(&draft.category, &draft.timestamp),
            timestamp: draft.timestamp.to_rfc3339(),
            level: draft.level,
            category: draft.category,
            source: draft.source,
            message: draft.message,
            detail: draft.detail,
            trace_id: draft.trace_id,
            node_id: draft.node_id,
            duration_ms: draft.duration_ms,
            project_id: Some(self.session.project_id.clone()),
            project_name: Some(self.session.project_name.clone()),
            environment_id: Some(self.session.environment_id.clone()),
            environment_name: Some(self.session.environment_name.clone()),
            data: draft.data,
            event_kind: draft.event_kind,
        }
    }

    fn persist_entry(&self, record_kind: &str, entry: &ObservabilityEntry) {
        let Some(batch_writer) = &self.batch_writer else {
            return;
        };
        let payload = match serde_json::to_value(entry) {
            Ok(payload) => payload,
            Err(error) => {
                tracing::warn!(?error, "序列化可观测条目入库 payload 失败");
                return;
            }
        };
        let search_text = entry_search_text(entry);
        batch_writer.enqueue(ObservabilityBatchItem {
            id: entry.id.clone(),
            record_kind: record_kind.to_owned(),
            category: entry.category.clone(),
            timestamp: entry.timestamp.clone(),
            trace_id: entry.trace_id.clone(),
            search_text,
            payload,
        });
    }

    fn persist_alert(&self, alert: &AlertDeliveryRecord) {
        let Some(batch_writer) = &self.batch_writer else {
            return;
        };
        let payload = match serde_json::to_value(alert) {
            Ok(payload) => payload,
            Err(error) => {
                tracing::warn!(?error, "序列化告警记录入库 payload 失败");
                return;
            }
        };
        let search_text = super::alerting::alert_search_text(alert);
        batch_writer.enqueue(ObservabilityBatchItem {
            id: alert.id.clone(),
            record_kind: "alert".to_owned(),
            category: "alert".to_owned(),
            timestamp: alert.timestamp.clone(),
            trace_id: Some(alert.trace_id.clone()),
            search_text,
            payload,
        });
    }
}

// ---- 查询与清理 ----

pub async fn query_observability(
    store: Option<StoreHandle>,
    trace_id: Option<String>,
    search: Option<String>,
    limit: usize,
) -> Result<ObservabilityQueryResult, String> {
    let Some(store) = store else {
        return Ok(ObservabilityQueryResult {
            entries: Vec::new(),
            traces: Vec::new(),
            alerts: Vec::new(),
            audits: Vec::new(),
        });
    };

    let trace_filter = trace_id
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    let search_filter = search
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty());
    let limit = limit.clamp(20, 600);

    query_observability_from_store(
        &store,
        trace_filter.as_deref(),
        search_filter.as_deref(),
        limit,
    )
    .await
}

pub async fn clear_observability_store(store: Option<StoreHandle>) {
    if let Some(store) = store
        && let Err(error) = store.clear_observability_records().await
    {
        tracing::warn!(?error, "清空 SQLite 可观测性记录失败");
    }
}

// ---- 查询内部辅助 ----

async fn query_observability_from_store(
    store: &StoreHandle,
    trace_filter: Option<&str>,
    search_filter: Option<&str>,
    limit: usize,
) -> Result<ObservabilityQueryResult, String> {
    let records = store
        .query_observability_records(trace_filter, search_filter, limit)
        .await
        .map_err(|error| error.to_string())?;

    let mut events = Vec::new();
    let mut audits = Vec::new();
    let mut alerts = Vec::new();

    for record in records {
        match record.record_kind.as_str() {
            "alert" => {
                let alert = serde_json::from_value::<AlertDeliveryRecord>(record.payload)
                    .map_err(|error| format!("解析 SQLite 告警记录失败: {error}"))?;
                alerts.push(alert);
            }
            "audit" => {
                let entry = serde_json::from_value::<ObservabilityEntry>(record.payload)
                    .map_err(|error| format!("解析 SQLite 审计记录失败: {error}"))?;
                audits.push(entry);
            }
            _ => {
                let entry = serde_json::from_value::<ObservabilityEntry>(record.payload)
                    .map_err(|error| format!("解析 SQLite 观测记录失败: {error}"))?;
                events.push(entry);
            }
        }
    }

    let mut merged = events.clone();
    merged.extend(audits.clone());
    merged.extend(alerts.iter().map(alert_to_entry));
    merged.sort_by(|left, right| {
        right
            .timestamp
            .cmp(&left.timestamp)
            .then_with(|| right.id.cmp(&left.id))
    });
    merged.truncate(limit);

    audits.sort_by(|left, right| right.timestamp.cmp(&left.timestamp));
    audits.truncate(limit.min(120));
    alerts.sort_by(|left, right| right.timestamp.cmp(&left.timestamp));
    alerts.truncate(limit.min(120));

    let traces = build_trace_summaries(&events, &alerts, trace_filter, limit);

    Ok(ObservabilityQueryResult {
        entries: merged,
        traces,
        alerts,
        audits,
    })
}

// ---- Trace 摘要 ----

#[derive(Default)]
struct TraceAccumulator {
    status: String,
    started_at: Option<String>,
    last_seen_at: Option<String>,
    total_events: usize,
    node_ids: HashSet<String>,
    output_count: usize,
    failure_count: usize,
    total_duration_ms: u64,
    has_duration: bool,
    last_node_id: Option<String>,
    project_name: Option<String>,
    environment_name: Option<String>,
}

impl TraceAccumulator {
    fn apply_entry(&mut self, entry: &ObservabilityEntry) {
        self.total_events += 1;
        self.last_seen_at = max_timestamp(self.last_seen_at.take(), Some(entry.timestamp.clone()));
        self.started_at = min_timestamp(self.started_at.take(), Some(entry.timestamp.clone()));
        self.project_name = self
            .project_name
            .clone()
            .or_else(|| entry.project_name.clone());
        self.environment_name = self
            .environment_name
            .clone()
            .or_else(|| entry.environment_name.clone());

        if let Some(node_id) = &entry.node_id {
            self.node_ids.insert(node_id.clone());
            self.last_node_id = Some(node_id.clone());
        }
        if entry.event_kind.as_deref() == Some("output") {
            self.output_count += 1;
        }
        if entry.level == "error" {
            self.failure_count += 1;
            "failed".clone_into(&mut self.status);
        } else if self.status.is_empty() {
            self.status = if entry.event_kind.as_deref() == Some("completed") {
                "completed".to_owned()
            } else {
                "running".to_owned()
            };
        }
        if let Some(duration_ms) = entry.duration_ms {
            self.total_duration_ms = self.total_duration_ms.saturating_add(duration_ms);
            self.has_duration = true;
        }
    }

    fn apply_alert(&mut self, alert: &AlertDeliveryRecord) {
        self.project_name = Some(alert.project_name.clone());
        self.environment_name = Some(alert.environment_name.clone());
        self.last_seen_at = max_timestamp(self.last_seen_at.take(), Some(alert.timestamp.clone()));
    }

    fn finish(self, trace_id: String) -> ObservabilityTraceSummary {
        ObservabilityTraceSummary {
            trace_id,
            status: if self.status.is_empty() {
                "observed".to_owned()
            } else {
                self.status
            },
            started_at: self.started_at,
            last_seen_at: self.last_seen_at,
            total_events: self.total_events,
            node_count: self.node_ids.len(),
            output_count: self.output_count,
            failure_count: self.failure_count,
            total_duration_ms: self.has_duration.then_some(self.total_duration_ms),
            last_node_id: self.last_node_id,
            project_name: self.project_name,
            environment_name: self.environment_name,
        }
    }
}

fn build_trace_summaries(
    events: &[ObservabilityEntry],
    alerts: &[AlertDeliveryRecord],
    trace_filter: Option<&str>,
    limit: usize,
) -> Vec<ObservabilityTraceSummary> {
    let mut traces: HashMap<String, TraceAccumulator> = HashMap::new();

    for entry in events {
        let Some(trace_id) = entry.trace_id.as_ref() else {
            continue;
        };
        if trace_filter.is_some_and(|filter| filter != trace_id) {
            continue;
        }
        traces
            .entry(trace_id.clone())
            .or_default()
            .apply_entry(entry);
    }

    for alert in alerts {
        if trace_filter.is_some_and(|filter| filter != alert.trace_id) {
            continue;
        }
        traces
            .entry(alert.trace_id.clone())
            .or_default()
            .apply_alert(alert);
    }

    let mut result = traces
        .into_iter()
        .map(|(trace_id, accumulator)| accumulator.finish(trace_id))
        .collect::<Vec<_>>();

    result.sort_by(|left, right| {
        right
            .last_seen_at
            .cmp(&left.last_seen_at)
            .then_with(|| right.trace_id.cmp(&left.trace_id))
    });
    result.truncate(limit.min(80));
    result
}
