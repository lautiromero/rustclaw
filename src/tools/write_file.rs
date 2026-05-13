use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(thiserror::Error, Debug)]
pub enum WriteFileError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Safety check: {0}")]
    Safety(String),
}

#[derive(Deserialize, Serialize, Clone)]
pub struct WriteFileTool;

impl Tool for WriteFileTool {
    const NAME: &'static str = "write_file";
    type Error = WriteFileError;
    type Args = WriteFileArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.into(),
            description: "Creates a new file or overwrites an existing one. If file exists, the agent should inform the user and ask for confirmation before proceeding. Use for initial file creation or full rewrites.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative path to target file" },
                    "content": { "type": "string", "description": "Full content to write" },
                    "confirm_overwrite": { 
                        "type": "boolean", 
                        "description": "Must be true if file already exists. Agent should ask user first.",
                        "default": false
                    }
                },
                "required": ["path", "content", "confirm_overwrite"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let project_root = crate::context::file_loader::get_project_root()
            .map_err(|e| WriteFileError::Safety(e.to_string()))?;
        let file_path = project_root.join(&args.path);

        // Seguridad: si el archivo existe y no hay confirmación explícita, rechazar
        if file_path.exists() && !args.confirm_overwrite {
            return Err(WriteFileError::Safety(format!(
                "File '{}' already exists. Agent must set confirm_overwrite=true after user approval.",
                args.path
            )));
        }

        // Crear directorios padre si no existen
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Backup si está sobrescribiendo
        if file_path.exists() {
            let backup = file_path.with_extension("bak");
            fs::copy(&file_path, &backup)?;
        }

        // Escribir contenido
        fs::write(&file_path, &args.content)?;

        Ok(format!(
            "Wrote {} bytes to '{}'.{}",
            args.content.len(),
            args.path,
            if file_path.exists() && args.confirm_overwrite {
                format!(
                    " Backup saved to '{}.bak'",
                    file_path.with_extension("bak").display()
                )
            } else {
                " (new file)".to_string()
            }
        ))
    }
}

#[derive(Deserialize, Serialize, Clone)]
pub struct WriteFileArgs {
    pub path: String,
    pub content: String,
    pub confirm_overwrite: bool,
}
