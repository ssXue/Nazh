//! Store 的 async 调用边界。

use crate::{
    BatchWriter, CopilotConversation, CopilotMessage, DeploymentAuditRecord, HistoryEntry, Store,
    StoreError, StoredConnectionLocalOverride, StoredConnectionSecret, StoredGlobalVariable,
    StoredObservabilityRecord, StoredVariable,
};
use std::sync::Arc;

/// 面向 async 调用方的 Store 句柄。
///
/// `SQLite` CRUD 仍由同步 [`Store`] 执行；本句柄只负责把调用搬到 Tokio blocking
/// 线程池，避免 Tauri/运行时 async worker 直接等待 `SQLite` I/O 或 Store Mutex。
#[derive(Clone)]
pub struct StoreHandle {
    store: Arc<Store>,
}

impl StoreHandle {
    /// 用已有 Store 创建 async 句柄。
    pub fn new(store: Store) -> Self {
        Self {
            store: Arc::new(store),
        }
    }

    /// 用共享 Store 创建 async 句柄。
    pub fn from_arc(store: Arc<Store>) -> Self {
        Self { store }
    }

    /// 在 blocking 线程池中执行一次 Store 操作。
    pub async fn run_blocking<F, T>(&self, operation: F) -> Result<T, StoreError>
    where
        F: FnOnce(&Store) -> Result<T, StoreError> + Send + 'static,
        T: Send + 'static,
    {
        let store = Arc::clone(&self.store);
        tokio::task::spawn_blocking(move || operation(&store))
            .await
            .map_err(|error| StoreError::BlockingTask(error.to_string()))?
    }

    /// 加载指定工作流的所有持久化变量。
    pub async fn load_variables(
        &self,
        workflow_id: &str,
    ) -> Result<Vec<StoredVariable>, StoreError> {
        let workflow_id = workflow_id.to_owned();
        self.run_blocking(move |store| store.load_variables(&workflow_id))
            .await
    }

    /// 写入（或更新）一个变量。
    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_variable(
        &self,
        workflow_id: &str,
        key: &str,
        value: &serde_json::Value,
        var_type: &str,
        initial: &serde_json::Value,
        updated_at: &str,
        updated_by: Option<&str>,
    ) -> Result<(), StoreError> {
        let workflow_id = workflow_id.to_owned();
        let key = key.to_owned();
        let value = value.clone();
        let var_type = var_type.to_owned();
        let initial = initial.clone();
        let updated_at = updated_at.to_owned();
        let updated_by = updated_by.map(str::to_owned);
        self.run_blocking(move |store| {
            store.upsert_variable(
                &workflow_id,
                &key,
                &value,
                &var_type,
                &initial,
                &updated_at,
                updated_by.as_deref(),
            )
        })
        .await
    }

    /// 删除指定变量。
    pub async fn delete_variable(&self, workflow_id: &str, key: &str) -> Result<(), StoreError> {
        let workflow_id = workflow_id.to_owned();
        let key = key.to_owned();
        self.run_blocking(move |store| store.delete_variable(&workflow_id, &key))
            .await
    }

    /// 记录一条变量变更历史。
    pub async fn record_history(
        &self,
        workflow_id: &str,
        key: &str,
        value: &serde_json::Value,
        updated_at: &str,
        updated_by: Option<&str>,
    ) -> Result<(), StoreError> {
        let workflow_id = workflow_id.to_owned();
        let key = key.to_owned();
        let value = value.clone();
        let updated_at = updated_at.to_owned();
        let updated_by = updated_by.map(str::to_owned);
        self.run_blocking(move |store| {
            store.record_history(
                &workflow_id,
                &key,
                &value,
                &updated_at,
                updated_by.as_deref(),
            )
        })
        .await
    }

    /// 查询指定变量的最近 N 条历史。
    pub async fn query_latest(
        &self,
        workflow_id: &str,
        key: &str,
        limit: usize,
    ) -> Result<Vec<HistoryEntry>, StoreError> {
        let workflow_id = workflow_id.to_owned();
        let key = key.to_owned();
        self.run_blocking(move |store| store.query_latest(&workflow_id, &key, limit))
            .await
    }

    /// 写入（或更新）一个全局变量。
    pub async fn upsert_global(
        &self,
        namespace: &str,
        key: &str,
        value: &serde_json::Value,
        var_type: &str,
        updated_at: &str,
        updated_by: Option<&str>,
    ) -> Result<(), StoreError> {
        let namespace = namespace.to_owned();
        let key = key.to_owned();
        let value = value.clone();
        let var_type = var_type.to_owned();
        let updated_at = updated_at.to_owned();
        let updated_by = updated_by.map(str::to_owned);
        self.run_blocking(move |store| {
            store.upsert_global(
                &namespace,
                &key,
                &value,
                &var_type,
                &updated_at,
                updated_by.as_deref(),
            )
        })
        .await
    }

    /// 读取一个全局变量。
    pub async fn load_global(
        &self,
        namespace: &str,
        key: &str,
    ) -> Result<Option<StoredGlobalVariable>, StoreError> {
        let namespace = namespace.to_owned();
        let key = key.to_owned();
        self.run_blocking(move |store| store.load_global(&namespace, &key))
            .await
    }

    /// 列出全局变量。
    pub async fn list_globals(
        &self,
        namespace: Option<&str>,
    ) -> Result<Vec<StoredGlobalVariable>, StoreError> {
        let namespace = namespace.map(str::to_owned);
        self.run_blocking(move |store| store.list_globals(namespace.as_deref()))
            .await
    }

    /// 删除一个全局变量。
    pub async fn delete_global(&self, namespace: &str, key: &str) -> Result<(), StoreError> {
        let namespace = namespace.to_owned();
        let key = key.to_owned();
        self.run_blocking(move |store| store.delete_global(&namespace, &key))
            .await
    }

    // -- 连接私有配置 --

    /// 写入或更新一个连接密钥。
    pub async fn upsert_connection_secret(
        &self,
        connection_id: &str,
        secret_key: &str,
        value: &str,
        updated_at: &str,
        updated_by: Option<&str>,
    ) -> Result<(), StoreError> {
        let connection_id = connection_id.to_owned();
        let secret_key = secret_key.to_owned();
        let value = value.to_owned();
        let updated_at = updated_at.to_owned();
        let updated_by = updated_by.map(str::to_owned);
        self.run_blocking(move |store| {
            store.upsert_connection_secret(
                &connection_id,
                &secret_key,
                &value,
                &updated_at,
                updated_by.as_deref(),
            )
        })
        .await
    }

    /// 读取一个连接密钥。
    pub async fn load_connection_secret(
        &self,
        connection_id: &str,
        secret_key: &str,
    ) -> Result<Option<StoredConnectionSecret>, StoreError> {
        let connection_id = connection_id.to_owned();
        let secret_key = secret_key.to_owned();
        self.run_blocking(move |store| store.load_connection_secret(&connection_id, &secret_key))
            .await
    }

    /// 列出指定连接的所有密钥。
    pub async fn list_connection_secrets(
        &self,
        connection_id: &str,
    ) -> Result<Vec<StoredConnectionSecret>, StoreError> {
        let connection_id = connection_id.to_owned();
        self.run_blocking(move |store| store.list_connection_secrets(&connection_id))
            .await
    }

    /// 删除一个连接密钥。
    pub async fn delete_connection_secret(
        &self,
        connection_id: &str,
        secret_key: &str,
    ) -> Result<(), StoreError> {
        let connection_id = connection_id.to_owned();
        let secret_key = secret_key.to_owned();
        self.run_blocking(move |store| store.delete_connection_secret(&connection_id, &secret_key))
            .await
    }

    /// 写入或更新一个连接本机覆盖。
    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_connection_local_override(
        &self,
        connection_id: &str,
        environment_id: &str,
        key: &str,
        value: &serde_json::Value,
        updated_at: &str,
        updated_by: Option<&str>,
    ) -> Result<(), StoreError> {
        let connection_id = connection_id.to_owned();
        let environment_id = environment_id.to_owned();
        let key = key.to_owned();
        let value = value.clone();
        let updated_at = updated_at.to_owned();
        let updated_by = updated_by.map(str::to_owned);
        self.run_blocking(move |store| {
            store.upsert_connection_local_override(
                &connection_id,
                &environment_id,
                &key,
                &value,
                &updated_at,
                updated_by.as_deref(),
            )
        })
        .await
    }

    /// 读取一个连接本机覆盖。
    pub async fn load_connection_local_override(
        &self,
        connection_id: &str,
        environment_id: &str,
        key: &str,
    ) -> Result<Option<StoredConnectionLocalOverride>, StoreError> {
        let connection_id = connection_id.to_owned();
        let environment_id = environment_id.to_owned();
        let key = key.to_owned();
        self.run_blocking(move |store| {
            store.load_connection_local_override(&connection_id, &environment_id, &key)
        })
        .await
    }

    /// 列出连接本机覆盖。`environment_id` 为 `Some` 时按环境过滤。
    pub async fn list_connection_local_overrides(
        &self,
        connection_id: &str,
        environment_id: Option<&str>,
    ) -> Result<Vec<StoredConnectionLocalOverride>, StoreError> {
        let connection_id = connection_id.to_owned();
        let environment_id = environment_id.map(str::to_owned);
        self.run_blocking(move |store| {
            store.list_connection_local_overrides(&connection_id, environment_id.as_deref())
        })
        .await
    }

    /// 删除一个连接本机覆盖。
    pub async fn delete_connection_local_override(
        &self,
        connection_id: &str,
        environment_id: &str,
        key: &str,
    ) -> Result<(), StoreError> {
        let connection_id = connection_id.to_owned();
        let environment_id = environment_id.to_owned();
        let key = key.to_owned();
        self.run_blocking(move |store| {
            store.delete_connection_local_override(&connection_id, &environment_id, &key)
        })
        .await
    }

    // -- Copilot 对话持久化 --

    /// 列出所有 copilot 对话。
    pub async fn list_copilot_conversations(&self) -> Result<Vec<CopilotConversation>, StoreError> {
        self.run_blocking(Store::list_copilot_conversations).await
    }

    /// 创建新的 copilot 对话。
    pub async fn create_copilot_conversation(
        &self,
        id: &str,
        title: &str,
        now: &str,
    ) -> Result<CopilotConversation, StoreError> {
        let id = id.to_owned();
        let title = title.to_owned();
        let now = now.to_owned();
        self.run_blocking(move |store| store.create_copilot_conversation(&id, &title, &now))
            .await
    }

    /// 删除 copilot 对话。
    pub async fn delete_copilot_conversation(&self, id: &str) -> Result<(), StoreError> {
        let id = id.to_owned();
        self.run_blocking(move |store| store.delete_copilot_conversation(&id))
            .await
    }

    /// 重命名 copilot 对话。
    pub async fn rename_copilot_conversation(
        &self,
        id: &str,
        title: &str,
        now: &str,
    ) -> Result<(), StoreError> {
        let id = id.to_owned();
        let title = title.to_owned();
        let now = now.to_owned();
        self.run_blocking(move |store| store.rename_copilot_conversation(&id, &title, &now))
            .await
    }

    /// 加载指定对话的所有消息。
    pub async fn list_copilot_messages(
        &self,
        conversation_id: &str,
    ) -> Result<Vec<CopilotMessage>, StoreError> {
        let conversation_id = conversation_id.to_owned();
        self.run_blocking(move |store| store.list_copilot_messages(&conversation_id))
            .await
    }

    /// 追加一条消息到指定对话。
    pub async fn append_copilot_message(
        &self,
        conversation_id: &str,
        id: &str,
        role: &str,
        content: &str,
        thinking: Option<&str>,
        now: &str,
    ) -> Result<CopilotMessage, StoreError> {
        let conversation_id = conversation_id.to_owned();
        let id = id.to_owned();
        let role = role.to_owned();
        let content = content.to_owned();
        let thinking = thinking.map(std::borrow::ToOwned::to_owned);
        let now = now.to_owned();
        self.run_blocking(move |store| {
            store.append_copilot_message(
                &conversation_id,
                &id,
                &role,
                &content,
                thinking.as_deref(),
                &now,
            )
        })
        .await
    }

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
    /// 返回的 [`BatchWriter`] 生命周期应与 [`ObservabilityStore`](super::ObservabilityStore) 一致。
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

// -- AI 配置持久化 --

impl StoreHandle {
    /// 读取 AI 配置 JSON。
    pub async fn load_ai_config(&self) -> Result<Option<String>, StoreError> {
        self.run_blocking(Store::load_ai_config).await
    }

    /// 写入 AI 配置 JSON。
    pub async fn save_ai_config(&self, json: String) -> Result<(), StoreError> {
        self.run_blocking(move |store| store.save_ai_config(&json))
            .await
    }
}
