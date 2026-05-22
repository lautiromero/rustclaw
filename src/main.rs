use anyhow::Context;
use clap::Parser;
use std::sync::Arc;

// Embeddings are intentionally disabled for now.
// Do not delete the code below; it is kept for future RAG/dynamic context support.
// use rig::client::EmbeddingsClient;

mod agent;
mod config;
mod context;
mod memory;
mod providers;
mod tools;
mod utils;

use agent::build_agent;
use config::Config;
// Embeddings are intentionally disabled for now.
// Do not delete the code below; it is kept for future RAG/dynamic context support.
// use context::vector_store::AppVectorStore;
use memory::sqlite::MemoryDB;

mod io {
    pub mod tui;
}
mod state {
    pub mod conversation;
}

#[derive(Parser)]
#[command(name = "rustclaw")]
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

    let migrations_path = Config::config_dir()
        .map(|dir| dir.join("migrations"))
        .unwrap_or_else(|| std::path::PathBuf::from("./migrations"));

    let mut conn = memory_db.pool().acquire().await?;
    // Migrator::new(migrations_path)
    //     .run(&mut conn)
    //     .await
    //     .context("Failed to run database migrations")?;

    let migrator = sqlx::migrate::Migrator::new(migrations_path)
        .await
        .context("Failed to create migrator")?;
    migrator
        .run(&mut conn)
        .await
        .context("Failed to run database migrations")?;

    // tracing::info!("Database initialized and migrations applied");

    let memory_db_arc = Arc::new(memory_db);
    let session_id = crate::io::tui::session_select::select_session(&memory_db_arc)?;
    // Embeddings are intentionally disabled for now.
    // Do not delete the code below; it is kept for future RAG/dynamic context support.
    // let vector_store = AppVectorStore::new_in_memory();

    // Embeddings are intentionally disabled for now.
    // Do not delete the code below; it is kept for future RAG/dynamic context support.
    // Create embeddings client for vector document/code indexing.
    // let embed_client = rig::providers::openai::Client::builder()
    //     .api_key("")
    //     .base_url(&config.embedding_base_url)
    //     .build()
    //     .context("Failed to create embeddings client")?;

    // Embeddings are intentionally disabled for now.
    // Do not delete the code below; it is kept for future RAG/dynamic context support.
    // let embed_model = embed_client.embedding_model(&config.embedding_model);

    let (ui_tx, ui_rx) = tokio::sync::mpsc::unbounded_channel();

    // Construir agente (ahora es async)
    let agent = build_agent(
        &config,
        // Embeddings are intentionally disabled for now.
        // Do not delete the code below; it is kept for future RAG/dynamic context support.
        // vector_store,
        memory_db_arc.clone(),
        // Embeddings are intentionally disabled for now.
        // Do not delete the code below; it is kept for future RAG/dynamic context support.
        // embed_model,
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
