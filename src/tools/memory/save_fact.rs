use rig::tool::Tool;
use rig::completion::ToolDefinition;
use rig::embeddings::EmbeddingsBuilder;
use rig::embeddings::EmbeddingModel;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::memory::sqlite::MemoryDB;

#[derive(thiserror::Error, Debug)]
pub enum SaveFactError {
    #[error("Database error: {0}")]
    DbError(#[from] sqlx::Error),
    #[error("Embedding error: {0}")]
    EmbeddingError(String),
}

#[derive(Serialize, Clone)]
pub struct SaveFactTool<M: EmbeddingModel + Clone + Send + Sync + 'static> { 
    #[serde(skip)]
    db: Arc<MemoryDB>,
    #[serde(skip)]
    embed_model: M,
    model_name: String,
}

impl<M: EmbeddingModel + Clone + Send + Sync + 'static> SaveFactTool<M> {
    pub fn new(db: Arc<MemoryDB>, embed_model: M, model_name: String) -> Self {
        Self { db, embed_model, model_name }
    }
}

impl<M: EmbeddingModel + Clone + Send + Sync + 'static> Tool for SaveFactTool<M> {
    const NAME: &'static str = "save_fact";
    type Error = SaveFactError;
    type Args = SaveFactArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.into(),
            description: "Saves a user fact or preference to persistent memory".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string", "description": "Identifier key (e.g., 'setup', 'editor')" },
                    "value": { "type": "string", "description": "Value to store" },
                    "context": { "type": "string", "description": "Optional context", "default": null }
                },
                "required": ["key", "value"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        // 1. Guardar el fact en SQLite
        self.db.save_fact(&args.key, &args.value, args.context.as_deref())
            .await
            .map_err(SaveFactError::DbError)?;

        // 2. Generar embedding del valor (para búsqueda semántica)
        let embeddings = EmbeddingsBuilder::new(self.embed_model.clone())
            .document(&args.value)  // ← Cambiar simple_document → document
            .map_err(|e| SaveFactError::EmbeddingError(format!("Document error: {}", e)))?
            .build()
            .await
            .map_err(|e| SaveFactError::EmbeddingError(e.to_string()))?;

        // 3. Extraer el embedding y guardar en cache
        if let Some((_, one_or_many)) = embeddings.into_iter().next() {
            // Opción A: usar into_iter().next() que devuelve Option<&Embedding> (más seguro)
            if let Some(embedding) = one_or_many.into_iter().next() {
                let emb_f32: Vec<f32> = embedding.vec.iter().map(|x| *x as f32).collect();
        
                self.db.save_fact_embedding(
                    &args.key,
                    &args.value,
                    &emb_f32,
                    &self.model_name,
                )
                .await
                .map_err(SaveFactError::DbError)?;
            }
        }

        Ok(format!("✅ Saved: {} = '{}'", args.key, args.value))
    }
}

#[derive(Deserialize, Serialize, Clone)]
pub struct SaveFactArgs {
    pub key: String,
    pub value: String,
    #[serde(default)]
    pub context: Option<String>,
}
