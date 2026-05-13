use crate::memory::sqlite::MemoryDB;
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(thiserror::Error, Debug)]
pub enum RecallError {
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Database error: {0}")]
    DbError(#[from] sqlx::Error),
}

fn _dummy_memory_db() -> Arc<MemoryDB> {
    panic!("MemoryDB must be injected via RecallTool::new(), not deserialized")
}

#[derive(Deserialize, Serialize, Clone)]
pub struct RecallTool {
    #[serde(skip, default = "_dummy_memory_db")]
    db: Arc<MemoryDB>,
}

impl RecallTool {
    pub fn new(db: Arc<MemoryDB>) -> Self {
        Self { db }
    }
}

impl Tool for RecallTool {
    const NAME: &'static str = "recall_memory";
    type Error = RecallError;
    type Args = RecallArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.into(),
            description: "Loads ALL rules for a topic domain. Use broad topics: coding, cooking, betting, medicine, linux, music, etc. (open-ended). Returns all facts matching 'topic_*' pattern. Call multiple times if multiple topics apply (e.g., coding + linux).".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { 
                        "type": "string", 
                        "description": "Broad topic domain: coding, cooking, betting, medicine, linux, music, etc. Returns ALL facts for that topic."
                    }
                },
                "required": ["query"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        if args.query.eq_ignore_ascii_case("general") || args.query.starts_with("general_") {
            return Ok(
                "General rules are pre-loaded in the system prompt. No recall needed.".to_string(),
            );
        }

        let results = sqlx::query_as::<_, (String, String)>(
            "SELECT fact_key, fact_value FROM facts 
             WHERE fact_key LIKE ? || '_%' 
             ORDER BY fact_key",
        )
        .bind(&args.query)
        .fetch_all(self.db.pool()) // ← Sin & adelante
        .await
        .map_err(RecallError::DbError)?;

        if results.is_empty() {
            return Err(RecallError::NotFound(format!(
                "No facts found for topic '{}'",
                args.query
            )));
        }

        let formatted = results
            .iter()
            .map(|(k, v)| format!("• {}: {}", k, v))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(format!("📝 Topic '{}':\n{}", args.query, formatted))
    }
}

#[derive(Deserialize, Serialize, Clone)]
pub struct RecallArgs {
    pub query: String,
}
