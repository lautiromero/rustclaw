use anyhow::Result;
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::io::tui::app::UiEvent;

#[derive(thiserror::Error, Debug)]
pub enum ReadDirError {
    #[error("Directory error: {0}")]
    DirError(#[from] anyhow::Error),
}

#[derive(Deserialize, Serialize, Clone)]
pub struct ReadDirTool {
    #[serde(skip)]
    status_tx: Option<mpsc::UnboundedSender<UiEvent>>,
}

impl ReadDirTool {
    pub fn new(status_tx: Option<mpsc::UnboundedSender<UiEvent>>) -> Self {
        Self { status_tx }
    }
}

impl Tool for ReadDirTool {
    const NAME: &'static str = "read_dir";
    type Error = ReadDirError;
    type Args = ReadDirArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.into(),
            description: "Lists files and directories in a project path. Respects .gitignore and excludes noisy folders. Returns a flat, sorted structure optimized for LLM context.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { 
                        "type": "string", 
                        "description": "Relative path to directory (defaults to project root)", 
                        "default": "." 
                    },
                    "max_depth": { 
                        "type": "integer", 
                        "description": "Maximum depth to traverse from the target path (default: 3)", 
                        "default": 3 
                    }
                },
                "required": []
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let path_label = args.path.as_deref().unwrap_or(".");

        if let Some(tx) = &self.status_tx {
            let _ = tx.send(UiEvent::ToolStatus {
                name: Self::NAME.into(),
                message: format!("Reading directory: {}", path_label),
            });
        }

        // Capturar valores necesarios para la closure
        let path_label = path_label.to_string();
        let max_depth = args.max_depth.unwrap_or(3);
        let max_lines = 200;

        let result = tokio::task::spawn_blocking(move || {
            let project_root = crate::context::file_loader::get_project_root()
                .map_err(|e| ReadDirError::DirError(e))?;
            let target_path = project_root.join(&path_label);

            if !target_path.exists() || !target_path.is_dir() {
                return Err(ReadDirError::DirError(anyhow::anyhow!(
                    "Directory not found: {}",
                    path_label
                )));
            }

            let gitignore_path = project_root.join(".gitignore");
            let mut gitignore_patterns: Vec<String> = Vec::new();

            #[allow(clippy::collapsible_if)]
            if gitignore_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&gitignore_path) {
                    for line in content.lines() {
                        let trimmed = line.trim();
                        if trimmed.is_empty() || trimmed.starts_with('#') {
                            continue;
                        }
                        let pattern = trimmed.trim_start_matches('/').to_string();
                        let first_component =
                            pattern.split('/').next().unwrap_or(&pattern).to_string();
                        if !first_component.is_empty()
                            && !gitignore_patterns.contains(&first_component)
                        {
                            gitignore_patterns.push(first_component);
                        }
                    }
                }
            }

            let excluded_dirs = [
                "node_modules",
                "target",
                "dist",
                "build",
                ".git",
                ".vscode",
                ".idea",
                ".next",
                "out",
                "__pycache__",
                "venv",
                ".venv",
                "vendor",
                "env",
                ".env",
                "deps",
                "elm-stuff",
                "gradle",
                ".gradle",
                "bower_components",
                "jspm_packages",
                "packages",
                "tmp",
                "temp",
                "logs",
                "coverage",
                ".nyc_output",
                "site-packages",
                ".tox",
                ".eggs",
                "egg-info",
                "Pods",
                ".build",
                "DerivedData",
                "Carthage",
                "Buckets",
            ];

            let all_excluded: Vec<&str> = excluded_dirs
                .iter()
                .copied()
                .chain(gitignore_patterns.iter().map(|s| s.as_str()))
                .collect();

            let mut lines = Vec::new();

            for entry in walkdir::WalkDir::new(&target_path)
                .into_iter()
                .filter_entry(|e| {
                    let name = e.file_name().to_string_lossy();
                    if all_excluded.iter().any(|d| name.eq_ignore_ascii_case(d)) {
                        return false;
                    }
                    if name.starts_with('.') && !name.eq_ignore_ascii_case(".gitignore") {
                        return false;
                    }
                    true
                })
                .filter_map(|e| e.ok())
                .filter(|e| e.depth() <= max_depth)
            {
                let rel = entry
                    .path()
                    .strip_prefix(&target_path)
                    .unwrap_or(entry.path());
                let rel_str = rel.to_string_lossy().replace('\\', "/");
                if rel_str.is_empty() {
                    continue;
                }

                let prefix = if entry.file_type().is_dir() {
                    "[DIR] "
                } else {
                    "[FILE] "
                };
                lines.push(format!("{}{}", prefix, rel_str));

                if lines.len() >= max_lines {
                    lines.push(format!("... (truncated at {} entries)", max_lines));
                    break;
                }
            }

            lines.sort();
            Ok(if lines.is_empty() {
                "Directory is empty or contains only excluded files.".to_string()
            } else {
                format!("Contents of '{}':\n{}", path_label, lines.join("\n"))
            })
        })
        .await
        .map_err(|e| ReadDirError::DirError(e.into()))??;

        if let Some(tx) = &self.status_tx {
            let _ = tx.send(UiEvent::ToolStatus {
                name: Self::NAME.into(),
                message: "Done.".into(),
            });
        }

        Ok(result)
    }
}

#[derive(Deserialize, Serialize, Clone)]
pub struct ReadDirArgs {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub max_depth: Option<usize>,
}
