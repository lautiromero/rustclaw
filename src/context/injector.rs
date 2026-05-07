use crate::memory::sqlite::MemoryDB;
use rig::embeddings::EmbeddingsBuilder;
use rig::embeddings::EmbeddingModel;
use std::sync::Arc;

pub struct ContextInjector<M: EmbeddingModel + Clone + Send + Sync + 'static> {
    db: Arc<MemoryDB>,
    embed_model: M,
    model_name: String,
}

impl<M: EmbeddingModel + Clone + Send + Sync + 'static> ContextInjector<M> { 
    pub fn new(db: Arc<MemoryDB>, embed_model: M, model_name: String) -> Self {
        Self { db, embed_model, model_name }
    }

    pub async fn build_context(&self, query: &str, max_facts: u32) -> String {
        // 1. Generar embedding de la query
        let builder = match EmbeddingsBuilder::new(self.embed_model.clone())
            .document(query.to_string())
        {
            Ok(b) => b,
            Err(_) => return String::new(),
        };

        let embs = match builder.build().await {
            Ok(e) => e,
            Err(_) => return String::new(),
        };

        // Extraer el Vec<f32> del embedding con tipo explícito
        let query_emb_vec: Vec<f32> = match embs.into_iter().next() {
            Some((_, one_or_many)) => {
                match one_or_many.into_iter().next() {
                    Some(embedding) => {
                        // embedding.vec es Vec<f64>, convertir a Vec<f32> con tipo explícito
                        embedding.vec.iter().map(|x| *x as f32).collect::<Vec<f32>>()
                    }
                    None => return String::new(),
                }
            }
            None => return String::new(),
        };

        // 2. Buscar facts relevantes por similitud
        let relevant = match self.db
            .search_facts_semantic(&query_emb_vec, &self.model_name, max_facts)
            .await
        {
            Ok(facts) => facts,
            Err(_) => return String::new(),
        };

        if relevant.is_empty() {
            return String::new();
        }

        // 3. Formatear como bloque de contexto
        let mut block = String::from("\n--- USER CONTEXT ---\n");
        for (key, value, _score) in relevant {
            block.push_str(&format!("• {}: {}\n", key, value));
        }
        block.push_str("--- END CONTEXT ---\n");
        block
    }
}
