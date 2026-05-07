// src/providers/local.rs
//! Wrapper para llama.cpp local (OpenAI-compatible)

use rig::providers::openai;

pub type Client = openai::Client;
pub type EmbeddingModel = openai::embedding::EmbeddingModel;

/// Crea un cliente local sin API key
pub fn new_client(base_url: &str) -> anyhow::Result<Client> {
    openai::Client::builder()
        .api_key("")  // llama.cpp ignora la key en local
        .base_url(base_url)
        .build()
        .map_err(|e| anyhow::anyhow!("Error creando cliente local: {}", e))
}

pub fn embedding_model(client: &Client, model_name: &str) -> EmbeddingModel {
    client.embedding_model(model_name)
}
