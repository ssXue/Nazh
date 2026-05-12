//! Store 的 async 调用边界。

use crate::{
    AssetEmbedding, AssetEmbeddingSearchResult, CopilotConversation, CopilotMessage, HistoryEntry,
    Store, StoreError, StoredGlobalVariable, StoredVariable,
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

    // -- Embedding 向量存储 --

    /// 写入或更新一条 embedding 记录。
    pub async fn upsert_asset_embedding(&self, record: AssetEmbedding) -> Result<(), StoreError> {
        self.run_blocking(move |store| store.upsert_asset_embedding(&record))
            .await
    }

    /// 删除指定资产的所有 embedding。
    pub async fn delete_asset_embeddings(
        &self,
        asset_type: &str,
        asset_id: &str,
    ) -> Result<(), StoreError> {
        let asset_type = asset_type.to_owned();
        let asset_id = asset_id.to_owned();
        self.run_blocking(move |store| store.delete_asset_embeddings(&asset_type, &asset_id))
            .await
    }

    /// 删除所有 embedding 记录（用于全量重建索引）。
    pub async fn delete_all_asset_embeddings(&self) -> Result<(), StoreError> {
        self.run_blocking(super::Store::delete_all_asset_embeddings)
            .await
    }

    /// 基于查询向量检索最相似的 embedding 记录。
    pub async fn search_similar(
        &self,
        query: Vec<f32>,
        asset_type: Option<String>,
        limit: usize,
    ) -> Result<Vec<AssetEmbeddingSearchResult>, StoreError> {
        self.run_blocking(move |store| store.search_similar(&query, asset_type.as_deref(), limit))
            .await
    }

    /// 统计 embedding 记录数。
    pub async fn count_asset_embeddings(&self) -> Result<u64, StoreError> {
        self.run_blocking(Store::count_asset_embeddings).await
    }
}
