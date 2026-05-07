#[allow(dead_code)] // se usará cuando implementes carga dinámica de docs
pub fn chunk_text(text: &str, max_len: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }

        if current.len() + trimmed.len() > max_len && !current.is_empty() {
            chunks.push(std::mem::take(&mut current));
        }
        if !current.is_empty() { current.push('\n'); }
        current.push_str(trimmed);
    }
    if !current.is_empty() { chunks.push(current); }
    chunks
}
