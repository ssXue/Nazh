//! Copilot 对话持久化（对话 + 消息 CRUD）。

use crate::Store;
use crate::StoreError;
use rusqlite::params;

/// 持久化 copilot 对话记录。
#[derive(Debug, Clone)]
pub struct CopilotConversation {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
}

/// 持久化 copilot 消息记录。
#[derive(Debug, Clone)]
pub struct CopilotMessage {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    /// 助手消息携带的推理过程（DeepSeek 等模型的 `reasoning_content`）。
    /// 多轮对话时必须回传给 API，否则会触发 API 错误。
    pub thinking: Option<String>,
    pub created_at: String,
}

impl Store {
    /// 列出所有 copilot 对话，按最近更新排列。
    pub fn list_copilot_conversations(&self) -> Result<Vec<CopilotConversation>, StoreError> {
        let db = self.db();
        let mut stmt = db.prepare(
            "SELECT id, title, created_at, updated_at
             FROM copilot_conversations
             ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(CopilotConversation {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// 创建新的 copilot 对话。
    pub fn create_copilot_conversation(
        &self,
        id: &str,
        title: &str,
        now: &str,
    ) -> Result<CopilotConversation, StoreError> {
        self.db().execute(
            "INSERT INTO copilot_conversations (id, title, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?3)",
            params![id, title, now],
        )?;
        Ok(CopilotConversation {
            id: id.to_owned(),
            title: title.to_owned(),
            created_at: now.to_owned(),
            updated_at: now.to_owned(),
        })
    }

    /// 删除 copilot 对话及其所有消息。
    pub fn delete_copilot_conversation(&self, id: &str) -> Result<(), StoreError> {
        self.db().execute(
            "DELETE FROM copilot_conversations WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// 重命名 copilot 对话。
    pub fn rename_copilot_conversation(
        &self,
        id: &str,
        title: &str,
        now: &str,
    ) -> Result<(), StoreError> {
        self.db().execute(
            "UPDATE copilot_conversations SET title = ?1, updated_at = ?2 WHERE id = ?3",
            params![title, now, id],
        )?;
        Ok(())
    }

    /// 加载指定对话的所有消息（按时间升序）。
    pub fn list_copilot_messages(
        &self,
        conversation_id: &str,
    ) -> Result<Vec<CopilotMessage>, StoreError> {
        let db = self.db();
        let mut stmt = db.prepare(
            "SELECT id, conversation_id, role, content, thinking, created_at
             FROM copilot_messages
             WHERE conversation_id = ?1
             ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map(params![conversation_id], |row| {
            Ok(CopilotMessage {
                id: row.get(0)?,
                conversation_id: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                thinking: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// 追加一条消息到指定对话，并更新对话的 `updated_at`。
    pub fn append_copilot_message(
        &self,
        conversation_id: &str,
        id: &str,
        role: &str,
        content: &str,
        thinking: Option<&str>,
        now: &str,
    ) -> Result<CopilotMessage, StoreError> {
        self.db().execute(
            "INSERT INTO copilot_messages (id, conversation_id, role, content, thinking, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, conversation_id, role, content, thinking, now],
        )?;
        self.db().execute(
            "UPDATE copilot_conversations SET updated_at = ?1 WHERE id = ?2",
            params![now, conversation_id],
        )?;
        Ok(CopilotMessage {
            id: id.to_owned(),
            conversation_id: conversation_id.to_owned(),
            role: role.to_owned(),
            content: content.to_owned(),
            thinking: thinking.map(std::borrow::ToOwned::to_owned),
            created_at: now.to_owned(),
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn test_store() -> Store {
        Store::open_in_memory().expect("内存数据库应可打开")
    }

    #[test]
    fn create_and_list_conversations() {
        let store = test_store();
        let conv = store
            .create_copilot_conversation("c1", "测试对话", "2026-01-01T00:00:00")
            .unwrap();
        assert_eq!(conv.id, "c1");
        assert_eq!(conv.title, "测试对话");

        let list = store.list_copilot_conversations().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "c1");
    }

    #[test]
    fn delete_conversation_cascades_messages() {
        let store = test_store();
        store
            .create_copilot_conversation("c1", "对话", "2026-01-01T00:00:00")
            .unwrap();
        store
            .append_copilot_message("c1", "m1", "user", "你好", None, "2026-01-01T00:00:01")
            .unwrap();

        store.delete_copilot_conversation("c1").unwrap();
        assert!(store.list_copilot_conversations().unwrap().is_empty());
    }

    #[test]
    fn append_and_list_messages() {
        let store = test_store();
        store
            .create_copilot_conversation("c1", "对话", "2026-01-01T00:00:00")
            .unwrap();
        store
            .append_copilot_message("c1", "m1", "user", "你好", None, "2026-01-01T00:00:01")
            .unwrap();
        store
            .append_copilot_message(
                "c1",
                "m2",
                "assistant",
                "你好！",
                None,
                "2026-01-01T00:00:02",
            )
            .unwrap();

        let msgs = store.list_copilot_messages("c1").unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "user");
        assert_eq!(msgs[0].thinking, None);
        assert_eq!(msgs[1].role, "assistant");
        assert_eq!(msgs[1].thinking, None);
    }

    #[test]
    fn thinking_round_trip() {
        let store = test_store();
        store
            .create_copilot_conversation("c1", "对话", "2026-01-01T00:00:00")
            .unwrap();
        store
            .append_copilot_message("c1", "m1", "user", "你好", None, "2026-01-01T00:00:01")
            .unwrap();
        store
            .append_copilot_message(
                "c1",
                "m2",
                "assistant",
                "你好！",
                Some("让我想想..."),
                "2026-01-01T00:00:02",
            )
            .unwrap();

        let msgs = store.list_copilot_messages("c1").unwrap();
        assert_eq!(msgs[1].thinking.as_deref(), Some("让我想想..."));
    }

    #[test]
    fn rename_conversation() {
        let store = test_store();
        store
            .create_copilot_conversation("c1", "旧标题", "2026-01-01T00:00:00")
            .unwrap();
        store
            .rename_copilot_conversation("c1", "新标题", "2026-01-01T00:01:00")
            .unwrap();

        let conv = &store.list_copilot_conversations().unwrap()[0];
        assert_eq!(conv.title, "新标题");
        assert_eq!(conv.updated_at, "2026-01-01T00:01:00");
    }
}
