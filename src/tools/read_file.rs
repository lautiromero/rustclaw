//! Tool: read_file - Allows the agent to read file contents on demand
//! Uses persistent cache to avoid redundant disk reads

use anyhow::Result;
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};

use crate::context::file_loader;
use crate::memory::sqlite::MemoryDB;

#[derive(thiserror::Error, Debug)]
pub enum ReadFileError {
    #[error("File error: {0}")]
    FileError(#[from] anyhow::Error),
}

#[derive(Clone)]
pub struct ReadFileTool {
    memory_db: std::sync::Arc<MemoryDB>,
    session_id: String,
}

impl ReadFileTool {
    pub fn new(memory_db: std::sync::Arc<MemoryDB>, session_id: String) -> Self {
        Self {
            memory_db,
            session_id,
        }
    }
}

impl Tool for ReadFileTool {
    const NAME: &'static str = "read_file";
    type Error = ReadFileError;
    type Args = ReadFileArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.into(),
            description: "Reads the content of a file from the project. Paths are relative to the project root. Uses cache to avoid redundant reads.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative path to the file (e.g., 'src/main.rs')"
                    }
                },
                "required": ["path"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        // 1. Intentar obtener del cache
        let content = match crate::context::file_cache::try_get_cached(
            &self.memory_db,
            &self.session_id,
            &args.path,
        )
        .await
        {
            Ok(Some(cached)) => {
                crate::utils::debug_log(&format!("🗃️ Tool cache HIT for {}", args.path));
                cached
            }
            _ => {
                // 2. Cache miss: leer de disco
                let content = file_loader::read_file_text(&args.path)?;

                // 3. Guardar en cache (fire-and-forget)
                let db = self.memory_db.clone();
                let session = self.session_id.clone();
                let path = args.path.clone();
                let content_clone = content.clone();

                tokio::spawn(async move {
                    crate::context::file_cache::store_in_cache(
                        &db,
                        &session,
                        &path,
                        &content_clone,
                    )
                    .await;
                });

                crate::utils::debug_log(&format!("💾 Tool cache STORED for {}", args.path));
                content
            }
        };

        Ok(format!("📄 {}:\n\n```text\n{}\n```", args.path, content))
    }
}

#[derive(Deserialize, Serialize, Clone)]
pub struct ReadFileArgs {
    pub path: String,
}
