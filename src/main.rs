use anyhow::Context;
use clap::Parser;
use std::sync::Arc;

mod agent;
mod config;
mod context;
mod memory;
mod providers;
mod tools;
mod utils;

use crate::{context::injector::ContextInjector, io::tui::session_select::select_session};
use agent::build_agent;
use config::Config;
use context::vector_store::AppVectorStore;
// use io::cli_loop::run_cli;
use memory::sqlite::MemoryDB;
use rig::client::EmbeddingsClient;
// use utils::tracing::init_tracing;
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
    // let cli = Cli::parse();
    // init_tracing(cli.verbose, true)?;

    let config = Config::load()?;

    // Resolve and normalize database path to avoid SQLite URI issues with "./"
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

    // Conectar con el URI exacto de la config (ya incluye ?mode=rwc)
    let memory_db = MemoryDB::init(&config.database_url)
        .await
        .context("Failed to connect to database")?;

    sqlx::migrate!("./migrations")
        .run(memory_db.pool())
        .await
        .context("Failed to run database migrations")?;

    tracing::info!("Database initialized and migrations applied");

    let memory_db_arc = Arc::new(memory_db);

    // let session_id = "test-session-001".to_string();
    let session_id = crate::io::tui::session_select::select_session(&memory_db_arc)?;

    let vector_store = AppVectorStore::new_in_memory();

    // Crear cliente de embeddings (llama.cpp local)
    let embed_client = rig::providers::openai::Client::builder()
        .api_key("") // llama.cpp ignora la key
        .base_url(&config.embedding_base_url)
        .build()
        .context("Failed to create embeddings client")?;

    // Crear el modelo de embeddings
    let embed_model = embed_client.embedding_model(&config.embedding_model);

    let context_injector = ContextInjector::new(
        memory_db_arc.clone(),
        embed_model.clone(),
        config.embedding_model.clone(),
    );

    let agent = build_agent(
        &config,
        vector_store,
        memory_db_arc.clone(),
        embed_model.clone(),
        "temp".to_string(),
    )?;

    // run_cli(
    //     agent,
    //     Some(context_injector),
    //     Some(memory_db_arc.clone()),
    //     session_id,
    // )
    // .await?;

    crate::io::tui::run_tui(
        agent,
        memory_db_arc.clone(),
        session_id.clone(),
        Some(context_injector),
    )?;

    Ok(())
}
