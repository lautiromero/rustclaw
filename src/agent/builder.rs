use anyhow::Result;
use serde_json::json;
use std::sync::Arc;

use super::AppAgent;
use crate::config::Config;
use crate::context::vector_store::AppVectorStore;
use crate::memory::sqlite::MemoryDB;
use crate::providers::nvidia;
use crate::tools::ReadFileTool;
use crate::tools::memory::{recall::RecallTool, save_fact::SaveFactTool};

pub async fn build_agent<M>(
    config: &Config,
    vector_store: AppVectorStore,
    memory_db: Arc<MemoryDB>,
    embed_model: M,
    session_id: String,
    ui_tx: Option<tokio::sync::mpsc::UnboundedSender<crate::io::tui::app::UiEvent>>,
) -> Result<AppAgent>
where
    M: rig::embeddings::EmbeddingModel + Clone + Send + Sync + 'static,
{
    // Custom openai-compatible client so we can inspect the final HTTP body in the chat.
    let client = nvidia::Client::new(&config.openai_api_key)
        .base_url(&config.openai_base_url)
        .with_ui_tx(ui_tx.clone());
    let llm_model = nvidia::CompletionModel::new(client, &config.llm_model);

    // Índice para docs/código (rig.dynamic_context)
    let index = vector_store.index(embed_model);

    // 1. Cargar facts GLOBALES (general_*) → se inyectan en el preamble, aplican SIEMPRE
    let global_facts = sqlx::query_as::<_, (String, String)>(
        "SELECT fact_key, fact_value FROM facts WHERE fact_key LIKE 'general_%' ORDER BY fact_key",
    )
    .fetch_all(memory_db.pool())
    .await
    .unwrap_or_default();

    let global_block = if global_facts.is_empty() {
        String::new()
    } else {
        global_facts
            .iter()
            .map(|(k, v)| format!("[{}] {}", k, v))
            .collect::<Vec<_>>()
            .join(" | ")
    };

    // 2. Preamble
    let preamble = if global_block.is_empty() {
        "Technical assistant.
- Recall: specific topics only, once. Never 'general'.
- Language: match user.
- Output: direct answer only. No meta-commentary. No process explanations."
            .to_string()
    } else {
        format!(
            "Technical assistant.

Persistent global facts:
{}

- Recall: specific topics only, once. Never 'general'.
- Language: match user.
- Output: direct answer only. No meta-commentary. No process explanations.",
            global_block
        )
    };

    // 3. Crear tools
    let save_tool = SaveFactTool::new(memory_db.clone());
    let file_reader_tool = ReadFileTool::new(memory_db.clone(), session_id.clone());
    let recall_tool = RecallTool::new(memory_db.clone());
    let read_dir_tool = crate::tools::read_dir::ReadDirTool::new(ui_tx.clone());
    let apply_diff_tool = crate::tools::ApplyDiffTool::new(ui_tx.clone());
    let write_file_tool = crate::tools::WriteFileTool;

    let agent = rig::agent::AgentBuilder::new(llm_model)
        .preamble(&preamble)
        .dynamic_context(config.max_context, index)
        .temperature(0.2)
        .max_tokens(8000)
        .additional_params(json!({"top_p": 0.9}))
        .tool(crate::tools::web::katana::KatanaTool::new())
        .tool(save_tool)
        .tool(recall_tool)
        .tool(file_reader_tool)
        .tool(read_dir_tool)
        .tool(apply_diff_tool)
        .tool(write_file_tool)
        .default_max_turns(config.max_turns as usize)
        .build();

    Ok(agent)
}
