//! Store 的 async 调用边界。

mod connection;
mod copilot;
mod observability;

pub use observability::ObservabilityBatchItem;

use crate::{HistoryEntry, Store, StoreError, StoredGlobalVariable, StoredVariable};
use std::sync::Arc;

/// 面向 async 调用方的 Store 句柄。
///
/// `SQLite` CRUD 仍由同步 [`Store`] 执行；本句柄只负责把调用搬到 Tokio blocking
/// 线程池，避免 Tauri/运行时 async worker 直接等待 `SQLite` I/O 或 Store Mutex。
#[derive(Clone)]
pub struct StoreHandle {
    pub(crate) store: Arc<Store>,
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

    // -- 工作流变量 --

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

    // -- 全局变量 --

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
}
