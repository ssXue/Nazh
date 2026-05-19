//! Copilot 对话持久化 + AI 配置 async 句柄方法。

use crate::{CopilotConversation, CopilotMessage, Store, StoreError};
use super::StoreHandle;

impl StoreHandle {
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
