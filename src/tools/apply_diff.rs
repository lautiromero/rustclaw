use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(thiserror::Error, Debug)]
pub enum ApplyDiffError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Apply error: {0}")]
    Apply(String),
}

#[derive(Deserialize, Serialize, Clone)]
pub struct ApplyDiffTool;

impl Tool for ApplyDiffTool {
    const NAME: &'static str = "apply_diff";
    type Error = ApplyDiffError;
    type Args = ApplyDiffArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.into(),
            description: "Applies a SEARCH/REPLACE diff to a file. Format:\n<<<<<<< SEARCH\n<exact original lines>\n=======\n<new lines>\n>>>>>>> REPLACE\nSupports multiple blocks. Creates .bak backup on success.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative path to target file" },
                    "diff": { "type": "string", "description": "SEARCH/REPLACE blocks" }
                },
                "required": ["path", "diff"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let project_root = crate::context::file_loader::get_project_root()
            .map_err(|e| ApplyDiffError::Apply(e.to_string()))?;

        let file_path = project_root.join(&args.path);

        if !file_path.exists() || !file_path.is_file() {
            return Err(ApplyDiffError::Apply(format!(
                "File not found: {}",
                args.path
            )));
        }

        let original = fs::read_to_string(&file_path)?;
        let modified = apply_search_replace_blocks(&original, &args.diff)?;

        if modified == original {
            return Ok(
                "⚠️ No changes applied. SEARCH blocks didn't match or diff was empty.".into(),
            );
        }

        let backup_path = file_path.with_extension("bak");
        fs::write(&backup_path, &original)?;
        fs::write(&file_path, &modified)?;

        Ok(format!(
            "✅ Applied diff to '{}'. Backup: '{}'.\n\nChanges preview:\n{}",
            args.path,
            backup_path.display(),
            generate_preview(&original, &modified)
        ))
    }
}

fn apply_search_replace_blocks(content: &str, diff: &str) -> Result<String, ApplyDiffError> {
    let mut current = content.to_string();
    let blocks = parse_diff_blocks(diff)?;

    for (search, replace) in blocks {
        let search_trimmed = search.trim();
        if search_trimmed.is_empty() {
            return Err(ApplyDiffError::Parse("Empty SEARCH block".into()));
        }

        current = current.replace(search_trimmed, replace.trim());
    }

    if current == content {
        return Err(ApplyDiffError::Apply(
            "SEARCH blocks not found in file. Ensure exact match.".into(),
        ));
    }

    Ok(current)
}

fn parse_diff_blocks(diff: &str) -> Result<Vec<(String, String)>, ApplyDiffError> {
    let mut blocks = Vec::new();
    let mut search = String::new();
    let mut replace = String::new();
    let mut in_block = false;
    let mut is_search = true;

    for line in diff.lines() {
        if line.starts_with("<<<<<<< SEARCH") {
            if in_block {
                return Err(ApplyDiffError::Parse("Unclosed SEARCH block".into()));
            }
            in_block = true;
            is_search = true;
            continue;
        }
        if line.starts_with("=======") {
            if !in_block || !is_search {
                return Err(ApplyDiffError::Parse("Invalid block structure".into()));
            }
            is_search = false;
            continue;
        }
        if line.starts_with(">>>>>>> REPLACE") {
            if !in_block || is_search {
                return Err(ApplyDiffError::Parse("Invalid block structure".into()));
            }
            blocks.push((search.clone(), replace.clone()));
            search.clear();
            replace.clear();
            in_block = false;
            continue;
        }
        if !in_block {
            continue;
        }

        if is_search {
            search.push_str(line);
            search.push('\n');
        } else {
            replace.push_str(line);
            replace.push('\n');
        }
    }

    if in_block {
        return Err(ApplyDiffError::Parse("Unclosed diff block".into()));
    }
    if blocks.is_empty() {
        return Err(ApplyDiffError::Parse(
            "No valid SEARCH/REPLACE blocks found".into(),
        ));
    }

    Ok(blocks)
}

fn generate_preview(original: &str, modified: &str) -> String {
    let orig_lines: Vec<&str> = original.lines().collect();
    let mod_lines: Vec<&str> = modified.lines().collect();

    let mut preview = String::new();
    for (i, (o, m)) in orig_lines.iter().zip(mod_lines.iter()).enumerate() {
        if o != m {
            preview.push_str(&format!("L{}: - {}\nL{}: + {}\n", i + 1, o, i + 1, m));
            if preview.lines().count() > 8 {
                preview.push_str("... (truncated)\n");
                break;
            }
        }
    }
    preview
}

#[derive(Deserialize, Serialize, Clone)]
pub struct ApplyDiffArgs {
    pub path: String,
    pub diff: String,
}
