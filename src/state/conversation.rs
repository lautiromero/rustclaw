use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Default, Hash, PartialEq)]
pub enum MessageRole {
    #[default]
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, Default)]
pub struct UiMessage {
    pub role: MessageRole,
    pub content: String,
    pub raw: Option<String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Default)]
pub struct ConversationState {
    pub messages: Vec<UiMessage>,
    pub scroll_offset: usize,
    pub auto_scroll: bool,
    pub cursor: Option<usize>,
    pub pending_selection: Option<String>,
}
