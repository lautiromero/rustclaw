use anyhow::Result;
use serde_json::json;
use std::sync::Arc;

use crate::config::Config;
use crate::context::vector_store::AppVectorStore;
use crate::memory::sqlite::MemoryDB;
use crate::providers::nvidia;
use crate::tools::memory::{recall::RecallTool, save_fact::SaveFactTool};

use super::AppAgent;

pub fn build_agent<M>(
    config: &Config,
    vector_store: AppVectorStore,
    memory_db: Arc<MemoryDB>,
    embed_model: M,
    session_id: String,
) -> Result<AppAgent>
where
    M: rig::embeddings::EmbeddingModel + Clone + Send + Sync + 'static,
{
    // NVIDIA client for LLM
    let client = nvidia::Client::new(&config.nvidia_api_key).base_url(&config.nvidia_base_url);

    let llm_model = nvidia::CompletionModel::new(client, &config.llm_model);

    let index = vector_store.index(embed_model.clone());

    // Create tools with DB access
    let save_tool = SaveFactTool::new(
        memory_db.clone(),
        embed_model.clone(),
        config.embedding_model.clone(),
    );

    let recall_tool = RecallTool::new(memory_db);

    let agent = rig::agent::AgentBuilder::new(llm_model)
        .preamble("You are a helpful technical assistant.
- If the user shares personal facts or preferences, use `save_fact` to persist them.
- If the user asks about stored information, use `recall_memory` to search.
- CRITICAL: Once a tool returns a result, STOP calling tools. Answer the user directly using the retrieved information.
- NEVER call the same tool repeatedly if it has already returned a valid result.
- If a tool returns an error or no results, inform the user politely. Do not retry.
- Respond naturally to greetings and general questions without using tools.")
        .dynamic_context(config.max_context, index)
        .temperature(0.2)
        .max_tokens(400)
        .additional_params(json!({"top_p": 0.95}))
        .tool(crate::tools::web::katana::KatanaTool::new())
        .tool(save_tool)
        .tool(recall_tool)
        .default_max_turns(3)
        .build();

    tracing::info!(
        "🔧 Agent built | model: {} | tools: 3 | session: {}",
        config.llm_model,
        session_id
    );

    Ok(agent)
}
