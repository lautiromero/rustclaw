//! File cache utilities: persistent cache with disk-change detection
//! Used by both UI (@-files) and tools (read_file)

use crate::memory::sqlite::MemoryDB;
use anyhow::Result;
use std::sync::Arc;

/// Intenta obtener un archivo del cache persistente.
/// Retorna Some(content) si está cacheado y no cambió en disco.
/// Retorna None si no está en cache o cambió.
pub async fn try_get_cached(
    db: &Arc<MemoryDB>,
    session_id: &str,
    file_path: &str,
) -> Result<Option<String>> {
    // ← db.get_cached_file() ahora es pub, así que compila
    match db.get_cached_file(session_id, file_path).await {
        Ok(content) => Ok(content),
        Err(e) => {
            crate::utils::debug_log(&format!("⚠️ Cache read error for @{}: {}", file_path, e));
            Ok(None)
        }
    }
}

pub async fn store_in_cache(db: &Arc<MemoryDB>, session_id: &str, file_path: &str, content: &str) {
    if let Err(e) = db.cache_file(session_id, file_path, content).await {
        crate::utils::debug_log(&format!("⚠️ Cache write error for @{}: {}", file_path, e));
    }
}

pub async fn invalidate_cache(db: &Arc<MemoryDB>, session_id: &str, file_path: &str) {
    if let Err(e) = db.invalidate_cached_file(session_id, file_path).await {
        crate::utils::debug_log(&format!(
            "⚠️ Cache invalidate error for @{}: {}",
            file_path, e
        ));
    }
}
