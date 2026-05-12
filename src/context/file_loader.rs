//! File loading utilities: read files, discover paths, resolve @-references
//! Reusable by both tools and UI autocomplete

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::utils::debug_log;

/// Resuelve el directorio base para paths relativos: el cwd donde se ejecutó el agente
pub fn get_project_root() -> Result<PathBuf> {
    std::env::current_dir().context("Failed to get current working directory")
}

/// Lee un archivo y retorna su contenido como String
/// - Resuelve paths relativos contra el project root
/// - Rechaza archivos binarios o muy grandes (>1MB por ahora)
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

    // Validaciones de seguridad
    validate_file_path(&absolute_path)?;

    // Leer contenido
    fs::read_to_string(&absolute_path)
        .with_context(|| format!("Failed to read file: {}", absolute_path.display()))
}

/// Valida que un archivo sea seguro para leer (texto, tamaño razonable, no binario)
fn validate_file_path(path: &Path) -> Result<()> {
    // Verificar que existe y es archivo
    if !path.exists() {
        anyhow::bail!("File not found: {}", path.display());
    }

    let metadata = fs::metadata(path)?;
    if !metadata.is_file() {
        anyhow::bail!("Not a file: {}", path.display());
    }

    // Límite de tamaño: 1MB por ahora (configurable después)
    const MAX_SIZE: u64 = 1024 * 1024; // 1MB
    if metadata.len() > MAX_SIZE {
        anyhow::bail!("File too large (>1MB): {}", path.display());
    }

    // Intentar detectar binarios por extensión común
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

/// Descubre archivos que matcheen un prefix para autocomplete
/// - Busca recursivamente desde project_root
/// - Excluye directorios ruidosos: node_modules, target, .git, etc.
/// - Retorna paths relativos al project root
pub fn discover_files(prefix: &str) -> Result<Vec<String>> {
    let project_root = get_project_root()?;
    let mut matches = Vec::new();

    // Normalizar prefix: quitar @ inicial si existe
    let search_prefix = prefix.strip_prefix('@').unwrap_or(prefix);

    // Exclusiones hardcodeadas (después se puede hacer configurable)
    let excluded_dirs = [
        "node_modules",
        "target",
        "dist",
        "build",
        ".git",
        ".vscode",
        ".idea",
    ];

    // Recorrer árbol de directorios
    for entry in walkdir::WalkDir::new(&project_root)
        .into_iter()
        .filter_entry(|e| {
            // Excluir directorios ruidosos
            let name = e.file_name().to_string_lossy();
            !excluded_dirs.contains(&name.as_ref()) && !name.starts_with('.')
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
    {
        // Calcular path relativo al project root
        if let Ok(rel_path) = entry.path().strip_prefix(&project_root) {
            let rel_str = rel_path.to_string_lossy().replace('\\', "/"); // Normalizar separadores

            // Si matchea el prefix, agregarlo
            if rel_str.starts_with(search_prefix) {
                matches.push(rel_str);
            }
        }
    }

    // Limitar resultados para no saturar la UI (configurable)
    matches.truncate(50);
    matches.sort();

    Ok(matches)
}

/// Parsea un input que puede contener @-references y extrae los paths
/// Ej: "revisá @src/main.rs y @Cargo.toml" → ["src/main.rs", "Cargo.toml"]
pub fn extract_file_references(input: &str) -> Vec<String> {
    let mut refs = Vec::new();

    // Buscar patrones @path/to/file (soporta espacios si están entre comillas)
    let mut in_quotes = false;
    let mut current_ref = String::new();

    for ch in input.chars() {
        match ch {
            '@' if !in_quotes => {
                if !current_ref.is_empty() {
                    refs.push(current_ref.clone());
                    current_ref.clear();
                }
            }
            '"' => {
                in_quotes = !in_quotes;
                if !in_quotes && !current_ref.is_empty() {
                    refs.push(current_ref.clone());
                    current_ref.clear();
                }
            }
            ' ' if !in_quotes => {
                if current_ref.starts_with('@') && current_ref.len() > 1 {
                    refs.push(current_ref[1..].to_string());
                }
                current_ref.clear();
            }
            _ => current_ref.push(ch),
        }
    }

    // Capturar último reference si existe
    if current_ref.starts_with('@') && current_ref.len() > 1 {
        refs.push(current_ref[1..].to_string());
    }

    refs.into_iter().filter(|p| !p.is_empty()).collect()
}
