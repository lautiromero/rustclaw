pub mod app;
pub mod commands;
pub mod events;
pub mod render;
pub mod session_select;

use crate::agent::AppAgent;
use crate::memory::sqlite::MemoryDB;
use anyhow::Result;
use std::sync::Arc;

pub fn run_tui<M>(
    agent: AppAgent,
    memory_db: Arc<MemoryDB>,
    session_id: String,
    context_injector: Option<crate::context::injector::ContextInjector<M>>,
) -> Result<()>
where
    M: rig::embeddings::EmbeddingModel + Clone + Send + Sync + 'static,
{
    events::run_tui(agent, memory_db, session_id, context_injector)
}
