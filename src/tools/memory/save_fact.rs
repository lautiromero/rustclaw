use crate::memory::sqlite::MemoryDB;
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(thiserror::Error, Debug)]
pub enum SaveFactError {
    #[error("Database error: {0}")]
    DbError(#[from] sqlx::Error),
}

#[derive(Serialize, Clone)]
pub struct SaveFactTool {
    #[serde(skip)]
    db: Arc<MemoryDB>,
}

impl SaveFactTool {
    pub fn new(db: Arc<MemoryDB>) -> Self {
        Self { db }
    }
}

impl Tool for SaveFactTool {
    const NAME: &'static str = "save_fact";
    type Error = SaveFactError;
    type Args = SaveFactArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.into(),
            description: "Saves a user rule or preference to persistent memory. Use logical keys (e.g., 'coding_style', 'language_prefs', 'betting_rules')".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string", "description": "Category or topic key (e.g., 'coding_style', 'user_language')" },
                    "value": { "type": "string", "description": "The rule or preference to persist" },
                    "context": { "type": "string", "description": "Optional context or source", "default": null }
                },
                "required": ["key", "value"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        self.db
            .save_fact(&args.key, &args.value, args.context.as_deref())
            .await
            .map_err(SaveFactError::DbError)?;

        Ok(format!("Saved: {} = '{}'", args.key, args.value))
    }
}

#[derive(Deserialize, Serialize, Clone)]
pub struct SaveFactArgs {
    pub key: String,
    pub value: String,
    #[serde(default)]
    pub context: Option<String>,
}
