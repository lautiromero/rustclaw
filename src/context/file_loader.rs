//! File loading utilities: read files, discover paths, resolve @-references
//! Reusable by both tools and UI autocomplete

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::utils::debug_log;

/// Resolves the base directory for relative paths: the cwd where the agent was launched.
pub fn get_project_root() -> Result<PathBuf> {
    std::env::current_dir().context("Failed to get current working directory")
}

/// Reads a file and returns its content as a String.
/// - Resolves relative paths against the project root.
/// - Rejects binary or very large files (>1MB for now).
pub fn read_file_text(relative_path: &str) -> Result<String> {
    let project_root = get_project_root()?;
    let absolute_path = project_root.join(relative_path);

    debug_log(&format!(
        "READ_FILE | project_root: {}",
        project_root.display()
    ));
    debug_log(&format!("READ_FILE | relative_path: [{}]", relative_path));
    debug_log(&format!(
        "READ_FILE | absolute_path: {}",
        absolute_path.display()
    ));

    // Security checks.
    validate_file_path(&absolute_path)?;

    // Read content.
    fs::read_to_string(&absolute_path)
        .with_context(|| format!("Failed to read file: {}", absolute_path.display()))
}

/// Validates that a file is safe to read: text, reasonable size, and non-binary.
fn validate_file_path(path: &Path) -> Result<()> {
    // Ensure the path exists and is a file.
    if !path.exists() {
        anyhow::bail!("File not found: {}", path.display());
    }

    let metadata = fs::metadata(path)?;
    if !metadata.is_file() {
        anyhow::bail!("Not a file: {}", path.display());
    }

    // Size limit: 1MB for now, configurable later.
    const MAX_SIZE: u64 = 1024 * 1024; // 1MB
    if metadata.len() > MAX_SIZE {
        anyhow::bail!("File too large (>1MB): {}", path.display());
    }

    // Detect common binary extensions.
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let binary_exts = [
            "exe", "dll", "so", "dylib", "bin", "o", "a", "lib", "pdf", "zip", "tar", "gz",
        ];
        if binary_exts.contains(&ext.to_lowercase().as_str()) {
            anyhow::bail!("Binary file not supported: {}", path.display());
        }
    }

    Ok(())
}

/// Discovers files that match a fuzzy query for autocomplete.
/// - Searches recursively from project_root.
/// - Excludes noisy directories: node_modules, target, .git, etc.
/// - Returns paths relative to project_root.
pub fn discover_files(prefix: &str) -> Result<Vec<String>> {
    let project_root = get_project_root()?;
    let mut matches = Vec::new();

    let search_query = prefix.strip_prefix('@').unwrap_or(prefix).to_lowercase();

    // Hardcoded exclusions; can become configurable later.
    let excluded_dirs = [
        "node_modules",
        "target",
        "dist",
        "build",
        ".git",
        ".vscode",
        ".idea",
    ];

    // Walk the project tree.
    for entry in walkdir::WalkDir::new(&project_root)
        .into_iter()
        .filter_entry(|e| {
            // Exclude noisy directories.
            let name = e.file_name().to_string_lossy();
            !excluded_dirs.contains(&name.as_ref()) && !name.starts_with('.')
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
    {
        // Compute path relative to project_root.
        if let Ok(rel_path) = entry.path().strip_prefix(&project_root) {
            let rel_str = rel_path.to_string_lossy().replace('\\', "/");

            if fuzzy_score(&rel_str.to_lowercase(), &search_query).is_some() {
                matches.push(rel_str);
            }
        }
    }

    matches.sort_by_key(|path| fuzzy_score(&path.to_lowercase(), &search_query).unwrap_or(usize::MAX));
    matches.truncate(50);

    Ok(matches)
}

fn fuzzy_score(candidate: &str, query: &str) -> Option<usize> {
    if query.is_empty() {
        return Some(candidate.len());
    }

    let mut score = 0;
    let mut last_match = 0;
    let mut chars = candidate.char_indices();

    for query_char in query.chars() {
        let Some((idx, _)) = chars.find(|(_, candidate_char)| *candidate_char == query_char) else {
            return None;
        };
        score += idx.saturating_sub(last_match);
        last_match = idx;
    }

    Some(score + candidate.len().saturating_sub(last_match))
}

/// Parses input containing @-references and extracts paths.
/// Example: "review @src/main.rs and @Cargo.toml" -> ["src/main.rs", "Cargo.toml"]
pub fn extract_file_references(input: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let mut current_ref = String::new();
    let mut in_ref = false;

    for ch in input.chars() {
        match ch {
            '@' => {
                push_file_ref(&mut refs, &current_ref);
                current_ref.clear();
                in_ref = true;
            }
            ch if in_ref && ch.is_whitespace() => {
                push_file_ref(&mut refs, &current_ref);
                current_ref.clear();
                in_ref = false;
            }
            ch if in_ref => current_ref.push(ch),
            _ => {}
        }
    }

    push_file_ref(&mut refs, &current_ref);
    refs
}

fn push_file_ref(refs: &mut Vec<String>, current_ref: &str) {
    let path = current_ref.trim_matches(|ch: char| matches!(ch, ',' | '.' | ':' | ';' | ')' | ']'));

    if !path.is_empty() {
        refs.push(path.to_string());
    }
}
