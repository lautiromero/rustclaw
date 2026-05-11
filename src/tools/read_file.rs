//! Tool: read_file - Allows the agent to read file contents on demand

use anyhow::Result;
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};

use crate::context::file_loader;

#[derive(thiserror::Error, Debug)]
pub enum ReadFileError {
    #[error("File error: {0}")]
    FileError(#[from] anyhow::Error),
}

#[derive(Clone)]
pub struct ReadFileTool;

impl ReadFileTool {
    pub fn new() -> Self {
        Self
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
            description: "Reads the content of a file from the project. Use this to inspect code, configs, or docs. Paths are relative to the project root.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative path to the file (e.g., 'src/main.rs', 'Cargo.toml')"
                    }
                },
                "required": ["path"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        // Usar el core reutilizable
        let content = file_loader::read_file_text(&args.path)?;

        // Formatear respuesta para el agente (incluye path para referencia)
        Ok(format!("📄 {}:\n\n```text\n{}\n```", args.path, content))
    }
}

#[derive(Deserialize, Serialize, Clone)]
pub struct ReadFileArgs {
    pub path: String,
}
