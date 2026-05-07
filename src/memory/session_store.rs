use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Metadata público de una sesión (para list/render)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub id: String,
    pub name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub message_count: u32,
}

/// Representación serializable de un mensaje para DB
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbMessage {
    pub id: Option<i64>,
    pub session_id: String,
    pub role: String,               // "user" | "assistant"
    pub content: String,            // JSON de rig::completion::Message content
    pub tool_calls: Option<String>, // JSON si hubo tool_calls
    pub timestamp: DateTime<Utc>,
}

/// Trait mínimo para gestión de sesiones.
/// Permite mockear con InMemoryStore mientras terminás SQLite.
#[async_trait::async_trait]
pub trait SessionStore: Send + Sync {
    async fn list_sessions(&self) -> Result<Vec<SessionMeta>>;
    async fn load_session(&self, session_id: &str) -> Result<Vec<DbMessage>>;
    async fn save_message(&self, session_id: &str, message: &DbMessage) -> Result<()>;
    async fn delete_session(&self, session_id: &str) -> Result<()>;
    async fn update_session_name(&self, session_id: &str, name: &str) -> Result<()>;
}
