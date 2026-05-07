use anyhow::{Context, Result};
use arboard::Clipboard;
use regex::Regex;

/// Copia texto al portapapeles del sistema
pub fn copy_to_clipboard(text: &str) -> Result<()> {
    let mut ctx = Clipboard::new().context("Failed to open clipboard")?;
    ctx.set_text(text).context("Failed to set clipboard text")?;

    // ← FIX: Dar más tiempo al WM para procesar la selección (silencia warning)
    #[cfg(target_os = "linux")]
    std::thread::sleep(std::time::Duration::from_millis(200));

    Ok(())
}

/// Extrae bloques de código de un mensaje (markdown ``` blocks)
/// Retorna Vec<String> con el contenido limpio de cada bloque
pub fn extract_code_blocks(content: &str) -> Vec<String> {
    // (?s) = dotall, (?:...) = non-capturing, \s* = permite espacios tras ```
    // Maneja bloques cerrados ```...``` y abiertos ```... (fin de respuesta)
    let re = Regex::new(r"(?s)```[^\n]*\n(.*?)(?:```|$)").unwrap();

    re.captures_iter(content)
        .filter_map(|cap| {
            cap.get(1).map(|m| {
                // Solo trim de saltos al inicio, preservar contenido final
                let text = m.as_str();
                text.trim_start_matches('\n')
                    .trim_start_matches('\r')
                    .to_string()
            })
        })
        .collect()
}

/// Obtiene el bloque de código por índice (1-based) o "last"
pub fn get_code_block(content: &str, index: &str) -> Option<String> {
    let blocks = extract_code_blocks(content);

    if blocks.is_empty() {
        return None;
    }

    match index {
        "last" => blocks.last().cloned(),
        _ => {
            // Parsear índice numérico (1-based)
            index
                .parse::<usize>()
                .ok()
                .and_then(|n| blocks.get(n.saturating_sub(1)).cloned())
        }
    }
}
