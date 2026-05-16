use anyhow::{Context, Result};

pub struct Config {
    pub nvidia_api_key: String,
    pub nvidia_base_url: String,
    pub llm_model: String,
    pub embedding_model: String,
    pub embedding_base_url: String,
    pub database_url: String,
    pub max_context: usize,

    // Control de timeout dinámico
    pub max_turns: u64, // Máx. iteraciones internas por mensaje del usuario
    pub timeout_base_secs: u64, // Timeout base por iteración (en segundos)

    // Control de contexto enviado al agente
    pub max_context_messages: usize, // Últimos N mensajes a enviar al agente (excluyendo system prompt)
}

impl Config {
    fn load_env() {
        if let Some(env_path) = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .map(|home| home.join(".config").join("rustclaw").join(".env"))
            .filter(|path| path.exists())
        {
            dotenvy::from_path(env_path).ok();
            return;
        }

        dotenvy::dotenv().ok();
    }

    pub fn load() -> Result<Self> {
        Self::load_env();

        Ok(Self {
            nvidia_api_key: std::env::var("NVIDIA_API_KEY").context("Falta NVIDIA_API_KEY")?,
            nvidia_base_url: std::env::var("NVIDIA_BASE_URL")
                .unwrap_or_else(|_| "https://integrate.api.nvidia.com/v1".into()),
            llm_model: std::env::var("LLM_MODEL")
                .unwrap_or_else(|_| "nvidia/llama-3.1-70b-instruct".into()),
            embedding_model: std::env::var("EMBEDDING_MODEL")
                .unwrap_or_else(|_| "text-embedding-3-small".into()),
            embedding_base_url: std::env::var("EMBEDDING_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:8080/v1".into()),
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite:./data/agent.db?mode=rwc".into()),
            max_context: std::env::var("MAX_CONTEXT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(3),

            max_turns: std::env::var("AGENT_MAX_TURNS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(25), // Default iteraciones máx. por mensaje

            timeout_base_secs: std::env::var("AGENT_TIMEOUT_BASE")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(30), // Default por iteración

            max_context_messages: std::env::var("MAX_CONTEXT_MESSAGES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(20), // Default: últimos 20 mensajes
        })
    }
}
