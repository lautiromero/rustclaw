use anyhow::Context;
use clap::Parser;
use std::sync::Arc;

// ← AGREGAR: Trait para embedding_model()
use rig::client::EmbeddingsClient;

mod agent;
mod config;
mod context;
mod memory;
mod providers;
mod tools;
mod utils;

use agent::build_agent;
use config::Config;
use context::vector_store::AppVectorStore;
use memory::sqlite::MemoryDB;

mod io {
    pub mod tui;
}
mod state {
    pub mod conversation;
}

#[derive(Parser)]
#[command(name = "lite-agent")]
struct Cli {
    #[arg(short, long)]
    doc_url: Option<String>,
    #[arg(short, long, default_value_t = false)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::load()?;

    // Resolve and normalize database path
    let raw_path = config
        .database_url
        .split('?')
        .next()
        .unwrap_or(&config.database_url);
    let physical_path = raw_path
        .strip_prefix("sqlite:file://")
        .or_else(|| raw_path.strip_prefix("sqlite:///"))
        .or_else(|| raw_path.strip_prefix("sqlite:"))
        .unwrap_or(raw_path);

    if let Some(parent) = std::path::Path::new(physical_path).parent() {
        std::fs::create_dir_all(parent).context("Failed to create database directory")?;
    }

    let memory_db = MemoryDB::init(&config.database_url)
        .await
        .context("Failed to connect to database")?;

    sqlx::migrate!("./migrations")
        .run(memory_db.pool())
        .await
        .context("Failed to run database migrations")?;

    tracing::info!("Database initialized and migrations applied");

    let memory_db_arc = Arc::new(memory_db);
    let session_id = crate::io::tui::session_select::select_session(&memory_db_arc)?;
    let vector_store = AppVectorStore::new_in_memory();

    // Crear cliente de embeddings (para índice vectorial de docs/código)
    let embed_client = rig::providers::openai::Client::builder()
        .api_key("") // llama.cpp ignora la key
        .base_url(&config.embedding_base_url)
        .build()
        .context("Failed to create embeddings client")?;

    let embed_model = embed_client.embedding_model(&config.embedding_model);

    let (ui_tx, ui_rx) = tokio::sync::mpsc::unbounded_channel();

    // Construir agente (ahora es async)
    let agent = build_agent(
        &config,
        vector_store,
        memory_db_arc.clone(),
        embed_model,
        session_id.clone(),
        Some(ui_tx.clone()),
    )
    .await?;

    crate::io::tui::run_tui(
        agent,
        memory_db_arc.clone(),
        session_id.clone(),
        ui_tx.clone(),
        ui_rx,
    )?;

    Ok(())
}
