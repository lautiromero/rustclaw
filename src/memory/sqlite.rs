use crate::memory::session_store::{DbMessage, SessionMeta, SessionStore};
use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use hex;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

pub struct MemoryDB {
    pool: SqlitePool,
}

impl MemoryDB {
    pub async fn init(db_url: &str) -> Result<Self> {
        let pool = SqlitePool::connect(db_url).await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn save_conversation(
        &self,
        _session: &str,
        _user: &str,
        _assistant: &str,
    ) -> Result<()> {
        Ok(())
    }

    pub async fn save_fact(
        &self,
        key: &str,
        value: &str,
        source_context: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        tracing::info!("💾 Saving fact: key={}, value={}", key, value);
        sqlx::query(
            "INSERT INTO facts (fact_key, fact_value, source_context, confidence)
             VALUES (?, ?, ?, 1.0)
             ON CONFLICT(fact_key) DO UPDATE SET 
                 fact_value = excluded.fact_value,
                 updated_at = CURRENT_TIMESTAMP,
                 source_context = COALESCE(excluded.source_context, facts.source_context)",
        )
        .bind(key)
        .bind(value)
        .bind(source_context)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn search_facts(
        &self,
        query: &str,
        limit: i32,
    ) -> Result<Vec<(String, String)>, sqlx::Error> {
        sqlx::query_as::<_, (String, String)>(
            "SELECT fact_key, fact_value FROM facts
             WHERE fact_key LIKE ? OR fact_value LIKE ?
             ORDER BY updated_at DESC LIMIT ?",
        )
        .bind(format!("%{}%", query))
        .bind(format!("%{}%", query))
        .bind(limit)
        .fetch_all(&self.pool)
        .await
    }

    pub async fn get_fact(&self, key: &str) -> Result<Option<String>, sqlx::Error> {
        let result: Option<(String,)> =
            sqlx::query_as("SELECT fact_value FROM facts WHERE fact_key = ?")
                .bind(key)
                .fetch_optional(&self.pool)
                .await?;
        Ok(result.map(|(v,)| v))
    }

    pub async fn save_fact_embedding(
        &self,
        fact_key: &str,
        text: &str,
        embedding: &[f32],
        model_name: &str,
    ) -> Result<(), sqlx::Error> {
        // Hash del texto para cache key
        let mut hasher = Sha256::new();
        hasher.update(text.as_bytes());
        let text_hash = hex::encode(hasher.finalize());

        // Serializar embedding a BLOB (f32 -> bytes)
        let embedding_bytes: Vec<u8> = embedding.iter().flat_map(|f| f.to_ne_bytes()).collect();

        sqlx::query(
            "INSERT INTO embedding_cache (text_hash, embedding, model_name, fact_key)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(text_hash) DO UPDATE SET 
                 embedding = excluded.embedding,
                 model_name = excluded.model_name,
                 fact_key = excluded.fact_key",
        )
        .bind(text_hash)
        .bind(embedding_bytes)
        .bind(model_name)
        .bind(fact_key)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_fact_categories(&self) -> Result<Vec<String>, sqlx::Error> {
        sqlx::query_scalar("SELECT DISTINCT fact_key FROM facts ORDER BY fact_key")
            .fetch_all(&self.pool)
            .await
    }

    pub async fn cache_file(
        &self,
        session_id: &str,
        file_path: &str,
        content: &str,
    ) -> Result<(), sqlx::Error> {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());

        let hash: String = hasher
            .finalize()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();

        let now = chrono::Utc::now().timestamp();

        sqlx::query(
            r#"
        INSERT INTO file_cache (session_id, file_path, content_hash, content, last_accessed)
        VALUES (?, ?, ?, ?, ?)
        ON CONFLICT(session_id, file_path) DO UPDATE SET
            content_hash = excluded.content_hash,
            content = excluded.content,
            last_accessed = excluded.last_accessed
        "#,
        )
        .bind(session_id)
        .bind(file_path)
        .bind(hash)
        .bind(content)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Intenta obtener un archivo del cache, verificando que no haya cambiado en disco
    pub async fn get_cached_file(
        &self,
        session_id: &str,
        file_path: &str,
    ) -> Result<Option<String>, anyhow::Error> {
        use sha2::{Digest, Sha256};
        use std::fs;
        use std::path::Path;

        // 1. Buscar en DB
        let cached: Option<(String, String)> = sqlx::query_as(
        r#"SELECT content_hash, content FROM file_cache WHERE session_id = ? AND file_path = ?"#,
    )
    .bind(session_id)
    .bind(file_path)
    .fetch_optional(&self.pool)
    .await
    .ok()
    .flatten();

        let (cached_hash, cached_content) = match cached {
            Some(c) => c,
            None => return Ok(None),
        };

        // 2. Verificar que el archivo en disco no haya cambiado
        let project_root = crate::context::file_loader::get_project_root()?;
        let absolute_path = project_root.join(file_path);

        if !Path::new(&absolute_path).exists() {
            return Ok(None);
        }

        let current_content = fs::read_to_string(&absolute_path)?;

        let mut hasher = Sha256::new();
        hasher.update(current_content.as_bytes());
        let current_hash: String = hasher
            .finalize()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();

        if current_hash == cached_hash {
            Ok(Some(cached_content))
        } else {
            // Archivo cambió: invalidar cache y retornar None para forzar re-lectura
            self.invalidate_cached_file(session_id, file_path)
                .await
                .ok();
            Ok(None)
        }
    }

    /// Invalida una entrada del cache
    pub async fn invalidate_cached_file(
        &self,
        session_id: &str,
        file_path: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(r#"DELETE FROM file_cache WHERE session_id = ? AND file_path = ?"#)
            .bind(session_id)
            .bind(file_path)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

// Función auxiliar: similitud coseno entre dos vectores f32
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a * norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

#[async_trait]
impl SessionStore for MemoryDB {
    async fn list_sessions(&self) -> anyhow::Result<Vec<SessionMeta>> {
        let rows = sqlx::query_as::<_, (String, Option<String>, String, String, i64)>(
            "SELECT id, name, created_at, updated_at, message_count FROM sessions ORDER BY updated_at DESC"
        )
        .fetch_all(&self.pool)
        .await?;

        let sessions = rows
            .into_iter()
            .map(|(id, name, created, updated, count)| SessionMeta {
                id,
                name,
                created_at: chrono::DateTime::parse_from_rfc3339(&created)
                    .unwrap_or_else(|_| {
                        chrono::DateTime::parse_from_rfc3339("1970-01-01T00:00:00Z").unwrap()
                    })
                    .with_timezone(&Utc),
                updated_at: chrono::DateTime::parse_from_rfc3339(&updated)
                    .unwrap_or_else(|_| {
                        chrono::DateTime::parse_from_rfc3339("1970-01-01T00:00:00Z").unwrap()
                    })
                    .with_timezone(&Utc),
                message_count: count as u32,
            })
            .collect();

        Ok(sessions)
    }

    async fn load_session(&self, session_id: &str) -> anyhow::Result<Vec<DbMessage>> {
        let rows = sqlx::query_as::<_, (i64, String, String, String, Option<String>, String)>(
            "SELECT id, session_id, role, content, tool_calls, timestamp FROM messages 
             WHERE session_id = ? ORDER BY timestamp ASC",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        let messages = rows
            .into_iter()
            .map(|(id, sid, role, content, tool_calls, ts)| DbMessage {
                id: Some(id),
                session_id: sid,
                role,
                content,
                tool_calls,
                timestamp: chrono::DateTime::parse_from_rfc3339(&ts)
                    .unwrap_or_else(|_| {
                        chrono::DateTime::parse_from_rfc3339("1970-01-01T00:00:00Z").unwrap()
                    })
                    .with_timezone(&Utc),
            })
            .collect();

        Ok(messages)
    }

    async fn save_message(&self, session_id: &str, message: &DbMessage) -> anyhow::Result<()> {
        // Extraer texto del JSON para usar como nombre de sesión (solo si es user y primer mensaje)
        let session_name = if message.role == "user" && !message.content.is_empty() {
            // Intentar extraer el campo "text" del JSON guardado en content
            let raw_text = if message.content.starts_with('{') {
                // Buscar "text":" en el JSON y extraer el valor
                if let Some(start) = message.content.find("\"text\":\"") {
                    let start = start + 8; // saltar "text":"
                    if let Some(end) = message.content[start..].find('"') {
                        message.content[start..start + end].to_string()
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                }
            } else {
                message.content.clone()
            };

            // Limpiar y chunkear a 50 caracteres
            let clean = raw_text
                .replace('\n', " ")
                .replace('\r', "")
                .trim()
                .to_string();
            let mut name = clean.chars().take(50).collect::<String>();
            if clean.chars().count() > 50 {
                name.push_str("...");
            }
            name
        } else {
            // Fallback: UUID corto si no es user o está vacío
            format!("Session {}", session_id.chars().take(8).collect::<String>())
        };

        sqlx::query(
            "INSERT INTO sessions (id, name, message_count, updated_at)
         VALUES (?, ?, 1, CURRENT_TIMESTAMP)
         ON CONFLICT(id) DO UPDATE SET 
             message_count = sessions.message_count + 1,
             updated_at = CURRENT_TIMESTAMP",
        )
        .bind(session_id)
        .bind(&session_name)
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "INSERT INTO messages (session_id, role, content, tool_calls, timestamp)
         VALUES (?, ?, ?, ?, ?)",
        )
        .bind(session_id)
        .bind(&message.role)
        .bind(&message.content)
        .bind(&message.tool_calls)
        .bind(message.timestamp.to_rfc3339())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn delete_session(&self, session_id: &str) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM sessions WHERE id = ?")
            .bind(session_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn update_session_name(&self, session_id: &str, name: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE sessions SET name = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?")
            .bind(name)
            .bind(session_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
