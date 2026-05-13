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
        description: "Saves a fact, rule, or preference to persistent memory. KEY NAMING IS CRITICAL: Use 'general_<name>' for user-specific rules that apply to EVERY session (e.g., 'general_name', 'general_language', 'general_coding_style'). Use '<topic>_<detail>' for domain-specific rules (e.g., 'coding_style', 'linux_shell_prefs'). The system automatically loads all 'general_*' facts into the preamble of every new session.".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "key": { 
                    "type": "string", 
                    "description": "MUST follow convention: 'general_*' for global/user-wide preferences, or '<topic>_<detail>' for domain-specific rules. Examples: 'general_name', 'general_language', 'coding_style', 'betting_odds_format'." 
                },
                "value": { 
                    "type": "string", 
                    "description": "The exact rule, preference, or fact to persist." 
                },
                "context": { 
                    "type": "string", 
                    "description": "Optional source or context (e.g., 'user_stated', 'inferred', 'project_specific')", 
                    "default": null 
                }
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
