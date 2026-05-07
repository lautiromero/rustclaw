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

    /// Buscar facts por similitud semántica (coseno)
    pub async fn search_facts_semantic(
        &self,
        query_embedding: &[f32],
        model_name: &str,
        limit: u32,
    ) -> Result<Vec<(String, String, f32)>, sqlx::Error> {
        // Query devuelve 3 columnas: text_hash (String), embedding (BLOB→Vec<u8>), fact_key (Option<String>)
        let rows = sqlx::query_as::<_, (String, Vec<u8>, Option<String>)>(
            // ← FIX: 3 tipos, no 4
            "SELECT text_hash, embedding, fact_key FROM embedding_cache 
             WHERE model_name = ? AND fact_key IS NOT NULL",
        )
        .bind(model_name)
        .fetch_all(&self.pool)
        .await?;

        let mut scored: Vec<(String, String, f32)> = Vec::new();

        for (_text_hash, embedding_blob, fact_key_opt) in rows {
            if let Some(fact_key) = fact_key_opt {
                // Recuperar el fact_value de la tabla facts
                if let Ok(Some(value)) = self.get_fact(&fact_key).await {
                    // Deserializar embedding de BLOB (f32: 4 bytes cada uno)
                    let stored_emb: Vec<f32> = embedding_blob
                        .chunks_exact(4)
                        .map(|chunk| f32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                        .collect();

                    // Calcular similitud coseno
                    let similarity = cosine_similarity(query_embedding, &stored_emb);
                    scored.push((fact_key, value, similarity));
                }
            }
        }

        // Ordenar por similitud descendente y limitar
        scored.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit as usize);
        Ok(scored)
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
