//! 连接私有配置 async 句柄方法。

use crate::{StoreError, StoredConnectionLocalOverride, StoredConnectionSecret};
use super::StoreHandle;

impl StoreHandle {
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
}
