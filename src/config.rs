use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;

pub struct Config {
    pub openai_api_key: String,
    pub openai_base_url: String,
    pub llm_model: String,
    pub embedding_model: String,
    pub embedding_base_url: String,
    pub database_url: String,
    pub max_context: usize,

    // Dynamic timeout controls
    pub max_turns: u64,
    pub timeout_base_secs: u64,

    // Agent context controls
    pub max_context_messages: usize,

    // Filesystem/security controls
    pub yolo: bool,
    pub workspaces: Vec<PathBuf>,
}

#[derive(Debug, Default)]
struct FileConfig {
    values: HashMap<String, String>,
    workspaces: Vec<PathBuf>,
}

#[derive(Debug, Default)]
struct CliConfig {
    yolo: Option<bool>,
    add_workspaces: Vec<PathBuf>,
}

impl Config {
    pub fn config_dir() -> Option<PathBuf> {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join(".rustclaw"))
    }

    fn load_env() {
        if let Some(env_path) = Self::config_dir()
            .map(|config_dir| config_dir.join(".env"))
            .filter(|path| path.exists())
        {
            dotenvy::from_path(env_path).ok();
            return;
        }

        dotenvy::dotenv().ok();
    }

    fn load_file_config() -> Result<FileConfig> {
        let Some(config_path) = Self::config_dir()
            .map(|config_dir| config_dir.join("config.toml"))
            .filter(|path| path.exists())
        else {
            return Ok(FileConfig::default());
        };

        let raw_config = std::fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read config file: {}", config_path.display()))?;

        Ok(Self::parse_config_toml(&raw_config))
    }

    fn parse_config_toml(raw_config: &str) -> FileConfig {
        let mut config = FileConfig::default();
        let mut section = String::new();
        let mut pending_array_key: Option<String> = None;
        let mut pending_array_values = Vec::new();

        for raw_line in raw_config.lines() {
            let line = raw_line.trim();

            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some(key) = pending_array_key.as_deref() {
                if line.starts_with(']') {
                    if key == "workspace.workspaces" || key == "workspaces" {
                        config
                            .workspaces
                            .extend(pending_array_values.drain(..).map(PathBuf::from));
                    }
                    pending_array_key = None;
                    continue;
                }

                if let Some(value) = Self::parse_toml_string(line.trim_end_matches(',')) {
                    pending_array_values.push(value);
                }
                continue;
            }

            if line.starts_with('[') && line.ends_with(']') {
                section = line
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .trim()
                    .to_string();
                continue;
            }

            let Some((key, value)) = line.split_once('=') else {
                continue;
            };

            let key = key.trim();
            let value = value.trim();
            let full_key = if section.is_empty() {
                key.to_string()
            } else {
                format!("{section}.{key}")
            };

            if value == "[" {
                pending_array_key = Some(full_key);
                pending_array_values.clear();
                continue;
            }

            if (full_key == "workspace.workspaces" || full_key == "workspaces")
                && value.starts_with('[')
                && value.ends_with(']')
            {
                let values = value
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .split(',')
                    .filter_map(Self::parse_toml_string)
                    .map(PathBuf::from);
                config.workspaces.extend(values);
                continue;
            }

            config.values.insert(full_key, value.to_string());
        }

        config
    }

    fn parse_toml_string(value: &str) -> Option<String> {
        let value = value.trim().trim_end_matches(',').trim();

        if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
            return Some(value[1..value.len() - 1].to_string());
        }

        if value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2 {
            return Some(value[1..value.len() - 1].to_string());
        }

        None
    }

    fn parse_cli_config() -> CliConfig {
        let mut cli_config = CliConfig::default();
        let mut args = std::env::args().skip(1).peekable();

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--yolo" => cli_config.yolo = Some(true),
                "--no-yolo" => cli_config.yolo = Some(false),
                "--add-workspace" => {
                    if let Some(path) = args.next() {
                        cli_config.add_workspaces.push(PathBuf::from(path));
                    }
                }
                _ if arg.starts_with("--add-workspace=") => {
                    if let Some((_, path)) = arg.split_once('=') {
                        cli_config.add_workspaces.push(PathBuf::from(path));
                    }
                }
                _ => {}
            }
        }

        cli_config
    }

    fn env_or_file_string(
        file_config: &FileConfig,
        env_key: &str,
        file_keys: &[&str],
        default: &str,
    ) -> String {
        std::env::var(env_key)
            .ok()
            .or_else(|| {
                file_keys.iter().find_map(|key| {
                    file_config
                        .values
                        .get(*key)
                        .and_then(|value| Self::parse_toml_string(value))
                })
            })
            .unwrap_or_else(|| default.into())
    }

    fn env_or_file_parse<T>(
        file_config: &FileConfig,
        env_key: &str,
        file_keys: &[&str],
        default: T,
    ) -> T
    where
        T: std::str::FromStr,
    {
        std::env::var(env_key)
            .ok()
            .or_else(|| {
                file_keys
                    .iter()
                    .find_map(|key| file_config.values.get(*key).cloned())
            })
            .and_then(|value| value.parse().ok())
            .unwrap_or(default)
    }

    fn env_or_file_bool(
        file_config: &FileConfig,
        env_key: &str,
        file_keys: &[&str],
        default: bool,
    ) -> bool {
        std::env::var(env_key)
            .ok()
            .or_else(|| {
                file_keys
                    .iter()
                    .find_map(|key| file_config.values.get(*key).cloned())
            })
            .and_then(|value| match value.trim().to_ascii_lowercase().as_str() {
                "true" | "1" | "yes" | "y" | "on" => Some(true),
                "false" | "0" | "no" | "n" | "off" => Some(false),
                _ => None,
            })
            .unwrap_or(default)
    }

    fn normalize_database_url(database_url: &str) -> String {
        let Some(path_and_query) = database_url.strip_prefix("sqlite:") else {
            return database_url.to_string();
        };

        let (path, query) = path_and_query
            .split_once('?')
            .map_or((path_and_query, ""), |(path, query)| (path, query));

        let path = PathBuf::from(path);
        if path.is_absolute() {
            return database_url.to_string();
        }

        let Some(config_dir) = Self::config_dir() else {
            return database_url.to_string();
        };

        let absolute_path = config_dir.join(path);
        let absolute_path = absolute_path.to_string_lossy();

        if query.is_empty() {
            format!("sqlite:{absolute_path}")
        } else {
            format!("sqlite:{absolute_path}?{query}")
        }
    }

    pub fn load() -> Result<Self> {
        Self::load_env();

        let file_config = Self::load_file_config()?;
        let cli_config = Self::parse_cli_config();

        let mut workspaces = file_config.workspaces.clone();
        workspaces.extend(cli_config.add_workspaces);

        let database_url = Self::normalize_database_url(&Self::env_or_file_string(
            &file_config,
            "DATABASE_URL",
            &["database.url"],
            "sqlite:data/agent.db?mode=rwc",
        ));

        Ok(Self {
            openai_api_key: std::env::var("OPENAI_API_KEY")
                .or_else(|_| {
                    file_config
                        .values
                        .get("llm.openai_api_key")
                        .and_then(|value| Self::parse_toml_string(value))
                        .ok_or_else(|| std::env::VarError::NotPresent)
                })
                .context("OPENAI_API_KEY missing.")?,
            openai_base_url: Self::env_or_file_string(
                &file_config,
                "OPENAI_BASE_URL",
                &["llm.openai_base_url"],
                "https://api.openai.com/v1",
            ),
            llm_model: Self::env_or_file_string(
                &file_config,
                "LLM_MODEL",
                &["llm.model", "agent.model"],
                "nvidia/llama-3.1-70b-instruct",
            ),
            embedding_model: Self::env_or_file_string(
                &file_config,
                "EMBEDDING_MODEL",
                &["embedding.model"],
                "text-embedding-3-small",
            ),
            embedding_base_url: Self::env_or_file_string(
                &file_config,
                "EMBEDDING_BASE_URL",
                &["embedding.base_url"],
                "http://localhost:8080/v1",
            ),
            database_url,
            max_context: Self::env_or_file_parse(
                &file_config,
                "MAX_CONTEXT",
                &["agent.max_context"],
                3,
            ),
            max_turns: Self::env_or_file_parse(
                &file_config,
                "AGENT_MAX_TURNS",
                &["agent.max_turns"],
                25,
            ),
            timeout_base_secs: Self::env_or_file_parse(
                &file_config,
                "AGENT_TIMEOUT_BASE",
                &["agent.timeout_base_secs"],
                30,
            ),
            max_context_messages: Self::env_or_file_parse(
                &file_config,
                "MAX_CONTEXT_MESSAGES",
                &["agent.max_context_messages"],
                20,
            ),
            yolo: cli_config.yolo.unwrap_or_else(|| {
                Self::env_or_file_bool(&file_config, "RUSTCLAW_YOLO", &["security.yolo"], false)
            }),
            workspaces,
        })
    }
}
