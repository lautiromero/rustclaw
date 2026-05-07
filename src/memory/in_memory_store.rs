use crate::memory::session_store::{DbMessage, SessionMeta, SessionStore};
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use tokio::sync::RwLock;

pub struct InMemorySessionStore {
    sessions: RwLock<HashMap<String, Vec<DbMessage>>>,
}

impl InMemorySessionStore {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl SessionStore for InMemorySessionStore {
    async fn list_sessions(&self) -> Result<Vec<SessionMeta>> {
        let sessions = self.sessions.read().await;
        Ok(sessions
            .iter()
            .map(|(id, msgs)| SessionMeta {
                id: id.clone(),
                name: None,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                message_count: msgs.len() as u32,
            })
            .collect())
    }

    async fn load_session(&self, session_id: &str) -> Result<Vec<DbMessage>> {
        let sessions = self.sessions.read().await;
        Ok(sessions.get(session_id).cloned().unwrap_or_default())
    }

    async fn save_message(&self, session_id: &str, message: &DbMessage) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        sessions
            .entry(session_id.to_string())
            .or_default()
            .push(message.clone());
        Ok(())
    }

    async fn delete_session(&self, session_id: &str) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        sessions.remove(session_id);
        Ok(())
    }

    async fn update_session_name(&self, _session_id: &str, _name: &str) -> Result<()> {
        Ok(()) // No-op por ahora
    }
}
