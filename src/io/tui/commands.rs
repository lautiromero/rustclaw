#[derive(Debug, Clone)]
pub enum Command {
    NewSession,
    ListSessions { limit: Option<usize> },
    SwitchSession { id: String },
    Clear,
    Copy { target: String },
    Accept,
    Deny,
    Help,
    Exit,
    RenameSession(String),
    Message(String),
    Unknown(String),
}

pub fn parse(input: &str) -> Command {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Command::Message("".to_string());
    }

    if trimmed.starts_with('/') {
        let parts: Vec<&str> = trimmed[1..].split_whitespace().collect();
        match parts.first().copied() {
            Some("new") => Command::NewSession,
            Some("list") => Command::ListSessions { limit: None },
            Some("switch") if parts.len() > 1 => Command::SwitchSession {
                id: parts[1].to_string(),
            },
            Some("clear") => Command::Clear,
            Some("copy") if parts.len() > 1 => Command::Copy {
                target: parts[1..].join(" "),
            },
            Some("accept") => Command::Accept,
            Some("deny") => Command::Deny,
            Some("help") => Command::Help,
            Some("exit") => Command::Exit,
            Some("rename") if parts.len() > 1 => {
                let name = if parts[1].starts_with('"') && parts.last().unwrap().ends_with('"') {
                    parts[1..].join(" ").trim_matches('"').to_string()
                } else {
                    parts[1..].join(" ")
                };
                Command::RenameSession(name)
            }
            _ => Command::Unknown(trimmed.to_string()),
        }
    } else {
        Command::Message(trimmed.to_string())
    }
}
