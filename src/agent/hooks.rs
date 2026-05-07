use anyhow::Result;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::mpsc;

// Evento que el agente emite hacia la UI/DB
pub enum AgentEvent {
    PromptSent(String),
    StreamingChunk(String),
    ResponseComplete(String),
    ToolCalled { name: String, args: String },
    ToolResult { result: String },
    Error(String),
}

// Evento interno de la app (UI, comandos, selección)
pub enum AppEvent {
    Agent(AgentEvent),
    UserCommand(String),
    SelectOption {
        prompt: String,
        options: Vec<String>,
    },
    CopyPath(String), // @path completado
    CancelPrompt,
    Exit,
}

// Hook unificado que se adjunta al agente
pub struct AgentHooks {
    pub cancel_flag: Arc<AtomicBool>,
    pub tx: mpsc::UnboundedSender<AgentEvent>,
}

impl AgentHooks {
    pub fn new(tx: mpsc::UnboundedSender<AgentEvent>) -> Self {
        Self {
            cancel_flag: Arc::new(AtomicBool::new(false)),
            tx,
        }
    }

    // TODO: Implementar trait de hooks de Rig 0.36.0 aquí
    // on_prompt, on_response, on_tool_call, etc.
    // Cada callback envía por self.tx y chequea cancel_flag
}
