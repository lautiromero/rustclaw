use rig::tool::Tool;
use rig::completion::ToolDefinition;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::memory::sqlite::MemoryDB;

#[derive(thiserror::Error, Debug)]
pub enum RecallError {
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Database error: {0}")]
    DbError(#[from] sqlx::Error),
}

fn _dummy_memory_db() -> Arc<MemoryDB> {
    // Este valor NUNCA se usa en runtime porque el campo se inyecta vía new()
    // Pero serde lo necesita para compilar la derivación de Deserialize
    panic!("MemoryDB must be injected via RecallTool::new(), not deserialized")
}

#[derive(Deserialize, Serialize, Clone)]
pub struct RecallTool {
    #[serde(skip, default = "_dummy_memory_db")]
    db: Arc<MemoryDB>,
}

impl RecallTool {
    pub fn new(db: Arc<MemoryDB>) -> Self { Self { db } }
}

impl Tool for RecallTool {
    const NAME: &'static str = "recall_memory";
    type Error = RecallError;
    type Args = RecallArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.into(),
            description: "Searches persistent memory by key or content".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Key or term to search for" }
                },
                "required": ["query"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        tracing::info!("🔍 Searching memory for query: '{}'", args.query);

        if let Some(value) = self.db.get_fact(&args.query).await? {
            return Ok(format!("📝 {} = '{}'", args.query, value));
        }
        
        let results = self.db.search_facts(&args.query, 5).await?;
        if results.is_empty() {
            return Err(RecallError::NotFound(format!("No results found for '{}' in memory", args.query)));
        }
        
        let formatted = results.iter()
            .map(|(k, v)| format!("• {}: {}", k, v))
            .collect::<Vec<_>>()
            .join("\n");
        
        Ok(format!("📝 Results for '{}':\n{}", args.query, formatted))
    }
}

#[derive(Deserialize, Serialize, Clone)]
pub struct RecallArgs { pub query: String }
