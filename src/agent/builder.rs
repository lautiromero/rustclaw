use anyhow::Result;
use serde_json::json;
use std::sync::Arc;

use crate::config::Config;
use crate::context::vector_store::AppVectorStore;
use crate::memory::sqlite::MemoryDB;
use crate::providers::nvidia;
use crate::tools::ReadFileTool;
use crate::tools::memory::{recall::RecallTool, save_fact::SaveFactTool};

use super::AppAgent;

pub async fn build_agent<M>(
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
        let rules = global_facts
            .iter()
            .map(|(k, v)| format!("• {}: {}", k, v))
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "\nGLOBAL RULES (apply to EVERY response, never override):\n{}\n",
            rules
        )
    };

    // 2. Preamble definitivo
    let preamble = format!(
        "You are a highly adaptive technical assistant.{}
MEMORY PROTOCOL:
1. GLOBAL RULES above apply to EVERY response. Apply them silently.
2. Identify the main topic(s) of the conversation: coding, cooking, betting, medicine, linux, music, etc. (open-ended).
3. For each relevant topic, call recall_memory(query=\"topic\") to load ALL rules for that domain.
   - Example: recall_memory(\"coding\") returns coding_language, coding_style, etc.
   - You can call recall_memory multiple times if multiple topics apply (e.g., coding + linux).
4. Apply loaded topic rules silently. Do not ask the user about them.
5. When saving new facts: use key=\"topic_subkey\" format.
   - Topics: coding, cooking, betting, medicine, linux, music, general, etc.
   - Examples: \"coding_language\", \"linux_shell\", \"general_name\", \"betting_odds_format\".
   - Use \"general_*\" for rules that apply everywhere (they will appear in GLOBAL RULES).

DEFAULTS (if no relevant facts found):
- Chat language: match user's language
- Code/comments/logs: English
- No emojis in code or logs

RESPONSE FORMAT:
- Be concise and technical when appropriate.
- Stop calling tools once you have necessary context.",
        global_block
    );

    // 3. Crear tools
    let save_tool = SaveFactTool::new(memory_db.clone());
    let file_reader_tool = ReadFileTool::new(memory_db.clone(), session_id.clone());
    let recall_tool = RecallTool::new(memory_db.clone());

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
        .default_max_turns(5)
        .build();

    tracing::info!(
        "🔧 Agent built | model: {} | tools: 3 | session: {}",
        config.llm_model,
        session_id
    );

    Ok(agent)
}
