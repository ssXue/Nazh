use std::{
    collections::{HashMap, HashSet},
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::{DateTime, Utc};
use nazh_engine::{ExecutionEvent, WorkflowContext};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use tokio::sync::Mutex;
use uuid::Uuid;

const OBSERVABILITY_DIR: &str = "observability";
const EVENTS_FILE: &str = "events.jsonl";
const AUDIT_FILE: &str = "audit.jsonl";
const ALERTS_FILE: &str = "alerts.jsonl";

pub type SharedObservabilityStore = Arc<ObservabilityStore>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObservabilityContextInput {
    pub workspace_path: String,
    pub project_id: String,
    pub project_name: String,
    pub environment_id: String,
    pub environment_name: String,
    #[serde(default)]
    pub deployment_source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObservabilityEntry {
    pub id: String,
    pub timestamp: String,
    pub level: String,
    pub category: String,
    pub source: String,
    pub message: String,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub trace_id: Option<String>,
    #[serde(default)]
    pub node_id: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub project_name: Option<String>,
    #[serde(default)]
    pub environment_id: Option<String>,
    #[serde(default)]
    pub environment_name: Option<String>,
    #[serde(default)]
    pub data: Option<Value>,
    #[serde(default)]
    pub event_kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertDeliveryRecord {
    pub id: String,
    pub timestamp: String,
    pub trace_id: String,
    pub node_id: String,
    pub project_id: String,
    pub project_name: String,
    pub environment_id: String,
    pub environment_name: String,
    pub url: String,
    pub method: String,
    pub status: u16,
    pub success: bool,
    #[serde(default)]
    pub webhook_kind: Option<String>,
    #[serde(default)]
    pub body_mode: Option<String>,
    #[serde(default)]
    pub request_timeout_ms: Option<u64>,
    #[serde(default)]
    pub requested_at: Option<String>,
    #[serde(default)]
    pub request_body_preview: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObservabilityTraceSummary {
    pub trace_id: String,
    pub status: String,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub last_seen_at: Option<String>,
    pub total_events: usize,
    pub node_count: usize,
    pub output_count: usize,
    pub failure_count: usize,
    #[serde(default)]
    pub total_duration_ms: Option<u64>,
    #[serde(default)]
    pub last_node_id: Option<String>,
    #[serde(default)]
    pub project_name: Option<String>,
    #[serde(default)]
    pub environment_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObservabilityQueryResult {
    pub entries: Vec<ObservabilityEntry>,
    pub traces: Vec<ObservabilityTraceSummary>,
    pub alerts: Vec<AlertDeliveryRecord>,
    pub audits: Vec<ObservabilityEntry>,
}

#[derive(Debug, Clone)]
struct ObservabilitySession {
    project_id: String,
    project_name: String,
    environment_id: String,
    environment_name: String,
}

#[derive(Debug, Default)]
struct ObservabilityRuntimeState {
    active_spans: HashMap<String, DateTime<Utc>>,
}

#[derive(Debug)]
pub struct ObservabilityStore {
    root_dir: PathBuf,
    session: ObservabilitySession,
    state: Mutex<ObservabilityRuntimeState>,
}

struct ObservabilityEntryDraft {
    level: String,
    category: String,
    source: String,
    message: String,
    detail: Option<String>,
    trace_id: Option<String>,
    node_id: Option<String>,
    duration_ms: Option<u64>,
    data: Option<Value>,
    timestamp: DateTime<Utc>,
    event_kind: Option<String>,
}

fn elapsed_ms_since(now: DateTime<Utc>, started_at: DateTime<Utc>) -> u64 {
    (now - started_at).num_milliseconds().max(0).cast_unsigned()
}

impl ObservabilityEntryDraft {
    fn execution(
        level: &str,
        kind: &str,
        source: String,
        message: String,
        trace_id: String,
        timestamp: DateTime<Utc>,
    ) -> Self {
        Self {
            level: level.to_owned(),
            category: "execution".to_owned(),
            source,
            message,
            detail: None,
            trace_id: Some(trace_id),
            node_id: None,
            duration_ms: None,
            data: None,
            timestamp,
            event_kind: Some(kind.to_owned()),
        }
    }

    fn with_detail(mut self, detail: Option<String>) -> Self {
        self.detail = detail;
        self
    }

    fn with_node_id(mut self, node_id: Option<String>) -> Self {
        self.node_id = node_id;
        self
    }

    fn with_duration_ms(mut self, duration_ms: Option<u64>) -> Self {
        self.duration_ms = duration_ms;
        self
    }
}

impl ObservabilityStore {
    pub async fn new(
        workspace_dir: PathBuf,
        context: ObservabilityContextInput,
    ) -> Result<SharedObservabilityStore, String> {
        let root_dir = workspace_dir.join(OBSERVABILITY_DIR);
        tokio::task::spawn_blocking({
            let root_dir = root_dir.clone();
            move || fs::create_dir_all(&root_dir)
        })
        .await
        .map_err(|error| format!("创建观测目录失败: {error}"))?
        .map_err(|error| format!("创建观测目录失败: {error}"))?;

        Ok(Arc::new(Self {
            root_dir,
            session: ObservabilitySession {
                project_id: context.project_id,
                project_name: context.project_name,
                environment_id: context.environment_id,
                environment_name: context.environment_name,
            },
            state: Mutex::new(ObservabilityRuntimeState::default()),
        }))
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
                    let _ = append_jsonl(self.root_dir.join(ALERTS_FILE), &alert).await;
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
                append_jsonl(self.root_dir.join(EVENTS_FILE), &entry).await?;
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
            // VariableChanged 事件由 Task 4（Tauri shell 转发）单独处理，
            // 此处不写入可观测性日志，避免重复。
            ExecutionEvent::VariableChanged { .. } => {
                return Ok(self.build_entry(ObservabilityEntryDraft::execution(
                    "info",
                    "variable_changed",
                    "variables".to_owned(),
                    "工作流变量变更".to_owned(),
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
        append_jsonl(self.root_dir.join(EVENTS_FILE), &entry).await?;
        Ok(entry)
    }

    pub async fn record_result(&self, result: &WorkflowContext) -> Result<(), String> {
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
        append_jsonl(self.root_dir.join(EVENTS_FILE), &result_entry).await?;

        Ok(())
    }

    pub async fn record_audit(
        &self,
        level: &str,
        source: &str,
        message: &str,
        detail: Option<String>,
        trace_id: Option<String>,
        data: Option<Value>,
    ) -> Result<(), String> {
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
        append_jsonl(self.root_dir.join(AUDIT_FILE), &entry).await
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
}

pub async fn query_observability(
    workspace_dir: PathBuf,
    trace_id: Option<String>,
    search: Option<String>,
    limit: usize,
) -> Result<ObservabilityQueryResult, String> {
    let root_dir = workspace_dir.join(OBSERVABILITY_DIR);
    if !root_dir.exists() {
        return Ok(ObservabilityQueryResult {
            entries: Vec::new(),
            traces: Vec::new(),
            alerts: Vec::new(),
            audits: Vec::new(),
        });
    }

    let trace_filter = trace_id
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    let search_filter = search
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty());
    let limit = limit.clamp(20, 600);

    let mut events = read_jsonl::<ObservabilityEntry>(&root_dir.join(EVENTS_FILE)).await?;
    let mut audits = read_jsonl::<ObservabilityEntry>(&root_dir.join(AUDIT_FILE)).await?;
    let mut alerts = read_jsonl::<AlertDeliveryRecord>(&root_dir.join(ALERTS_FILE)).await?;

    events.retain(|entry| matches_entry(entry, trace_filter.as_deref(), search_filter.as_deref()));
    audits.retain(|entry| matches_entry(entry, trace_filter.as_deref(), search_filter.as_deref()));
    alerts
        .retain(|record| matches_alert(record, trace_filter.as_deref(), search_filter.as_deref()));

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

    let traces = build_trace_summaries(&events, &alerts, trace_filter.as_deref(), limit);

    Ok(ObservabilityQueryResult {
        entries: merged,
        traces,
        alerts,
        audits,
    })
}

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

fn alert_to_entry(alert: &AlertDeliveryRecord) -> ObservabilityEntry {
    ObservabilityEntry {
        id: alert.id.clone(),
        timestamp: alert.timestamp.clone(),
        level: if alert.success {
            "success".to_owned()
        } else {
            "error".to_owned()
        },
        category: "alert".to_owned(),
        source: alert.node_id.clone(),
        message: if alert.success {
            "告警投递成功".to_owned()
        } else {
            "告警投递失败".to_owned()
        },
        detail: Some(format!(
            "{} {} -> {} ({})",
            alert.method, alert.url, alert.status, alert.environment_name
        )),
        trace_id: Some(alert.trace_id.clone()),
        node_id: Some(alert.node_id.clone()),
        duration_ms: None,
        project_id: Some(alert.project_id.clone()),
        project_name: Some(alert.project_name.clone()),
        environment_id: Some(alert.environment_id.clone()),
        environment_name: Some(alert.environment_name.clone()),
        data: Some(json!({
            "url": alert.url,
            "method": alert.method,
            "status": alert.status,
            "success": alert.success,
            "webhook_kind": alert.webhook_kind,
            "body_mode": alert.body_mode,
            "request_timeout_ms": alert.request_timeout_ms,
            "requested_at": alert.requested_at,
            "request_body_preview": alert.request_body_preview,
        })),
        event_kind: None,
    }
}

fn build_alert_delivery(
    session: &ObservabilitySession,
    _stage: &str,
    trace_id: Uuid,
    http_meta: &serde_json::Map<String, Value>,
    timestamp: DateTime<Utc>,
) -> Option<AlertDeliveryRecord> {
    let node_id = http_meta.get("node_id")?.as_str()?.to_owned();
    let url = http_meta.get("url")?.as_str()?.to_owned();
    let method = http_meta
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("POST")
        .to_owned();
    let status = http_meta
        .get("status")
        .and_then(Value::as_u64)
        .and_then(|value| u16::try_from(value).ok())?;

    Some(AlertDeliveryRecord {
        id: build_record_id("alert", &timestamp),
        timestamp: timestamp.to_rfc3339(),
        trace_id: trace_id.to_string(),
        node_id,
        project_id: session.project_id.clone(),
        project_name: session.project_name.clone(),
        environment_id: session.environment_id.clone(),
        environment_name: session.environment_name.clone(),
        url,
        method,
        status,
        success: status < 400,
        webhook_kind: http_meta
            .get("webhook_kind")
            .and_then(Value::as_str)
            .map(str::to_owned),
        body_mode: http_meta
            .get("body_mode")
            .and_then(Value::as_str)
            .map(str::to_owned),
        request_timeout_ms: http_meta.get("request_timeout_ms").and_then(Value::as_u64),
        requested_at: http_meta
            .get("requested_at")
            .and_then(Value::as_str)
            .map(str::to_owned),
        request_body_preview: http_meta
            .get("request_body_preview")
            .and_then(Value::as_str)
            .map(str::to_owned),
    })
}

fn summarize_payload(payload: &Value) -> Value {
    match payload {
        Value::Object(map) => {
            let mut summary = serde_json::Map::new();
            for (key, value) in map.iter().take(8) {
                summary.insert(key.clone(), truncate_value(value, 220));
            }
            Value::Object(summary)
        }
        other => truncate_value(other, 220),
    }
}

fn truncate_value(value: &Value, max_text_len: usize) -> Value {
    match value {
        Value::String(text) => {
            if text.chars().count() <= max_text_len {
                Value::String(text.clone())
            } else {
                Value::String(text.chars().take(max_text_len).collect::<String>())
            }
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .take(12)
                .map(|item| truncate_value(item, max_text_len))
                .collect(),
        ),
        Value::Object(map) => {
            let mut next = serde_json::Map::new();
            for (key, nested) in map.iter().take(12) {
                next.insert(key.clone(), truncate_value(nested, max_text_len));
            }
            Value::Object(next)
        }
        other => other.clone(),
    }
}

fn matches_entry(
    entry: &ObservabilityEntry,
    trace_filter: Option<&str>,
    search_filter: Option<&str>,
) -> bool {
    if trace_filter.is_some_and(|filter| entry.trace_id.as_deref() != Some(filter)) {
        return false;
    }

    if let Some(search_filter) = search_filter {
        let haystack = format!(
            "{} {} {} {} {} {}",
            entry.source,
            entry.message,
            entry.detail.as_deref().unwrap_or_default(),
            entry.trace_id.as_deref().unwrap_or_default(),
            entry.node_id.as_deref().unwrap_or_default(),
            entry.project_name.as_deref().unwrap_or_default(),
        )
        .to_ascii_lowercase();
        if !haystack.contains(search_filter) {
            return false;
        }
    }

    true
}

fn matches_alert(
    record: &AlertDeliveryRecord,
    trace_filter: Option<&str>,
    search_filter: Option<&str>,
) -> bool {
    if trace_filter.is_some_and(|filter| record.trace_id != filter) {
        return false;
    }

    if let Some(search_filter) = search_filter {
        let haystack = format!(
            "{} {} {} {} {}",
            record.node_id, record.url, record.method, record.project_name, record.environment_name
        )
        .to_ascii_lowercase();
        if !haystack.contains(search_filter) {
            return false;
        }
    }

    true
}

fn min_timestamp(current: Option<String>, next: Option<String>) -> Option<String> {
    match (current, next) {
        (Some(left), Some(right)) => Some(if left <= right { left } else { right }),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}

fn max_timestamp(current: Option<String>, next: Option<String>) -> Option<String> {
    match (current, next) {
        (Some(left), Some(right)) => Some(if left >= right { left } else { right }),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}

async fn read_jsonl<T>(path: &Path) -> Result<Vec<T>, String>
where
    T: DeserializeOwned,
{
    if !path.exists() {
        return Ok(Vec::new());
    }

    let text = tokio::fs::read_to_string(path)
        .await
        .map_err(|error| format!("读取观测文件失败: {error}"))?;

    let mut items = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        if let Ok(item) = serde_json::from_str::<T>(line) {
            items.push(item);
        }
    }

    Ok(items)
}

async fn append_jsonl<T>(path: PathBuf, record: &T) -> Result<(), String>
where
    T: Serialize + Send + Sync,
{
    let line =
        serde_json::to_string(record).map_err(|error| format!("序列化观测记录失败: {error}"))?;

    tokio::task::spawn_blocking(move || -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("创建观测目录失败: {error}"))?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|error| format!("打开观测文件失败: {error}"))?;
        writeln!(file, "{line}").map_err(|error| format!("写入观测文件失败: {error}"))?;
        Ok(())
    })
    .await
    .map_err(|error| format!("写入观测记录任务失败: {error}"))?
}

fn span_key(trace_id: impl std::fmt::Display, stage: &str) -> String {
    format!("{trace_id}:{stage}")
}

fn build_record_id(prefix: &str, timestamp: &DateTime<Utc>) -> String {
    format!(
        "{prefix}-{}-{}",
        timestamp.timestamp_millis(),
        timestamp.timestamp_subsec_nanos()
    )
}

#[cfg(test)]
mod tests {
    use super::{ObservabilityContextInput, ObservabilityStore, span_key};
    use nazh_engine::{CompletedExecutionEvent, ExecutionEvent};
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
        let workspace =
            std::env::temp_dir().join(format!("nazh-observability-test-{}", Uuid::new_v4()));
        let Ok(store) = ObservabilityStore::new(workspace.clone(), test_context()).await else {
            panic!("观测存储应可创建");
        };
        let trace_id = Uuid::new_v4();
        let node_stage = "node_a";
        let span_key = span_key(trace_id, node_stage);

        let started = ExecutionEvent::Started {
            stage: node_stage.to_owned(),
            trace_id,
        };
        let Ok(_) = store.record_execution_event(&started).await else {
            panic!("started 事件应可记录");
        };
        {
            let runtime_state = store.state.lock().await;
            assert!(runtime_state.active_spans.contains_key(&span_key));
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
            assert!(!runtime_state.active_spans.contains_key(&span_key));
        }

        let _ = std::fs::remove_dir_all(workspace);
    }
}
