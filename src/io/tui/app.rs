use crate::io::tui::commands::{Command, parse};
use crate::state::conversation::{ConversationState, MessageRole, UiMessage};
use ratatui::text::Line;
use ratatui::widgets::ScrollbarState;
use ratatui_textarea::TextArea;
use rig::completion::Chat;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use tokio::sync::mpsc;

const FILE_PICKER_LIMIT: usize = 50;

pub enum UiEvent {
    AgentResponse(String),
    AgentError(String),
    NewRigMessage(rig::completion::Message),
    ToolStatus { name: String, message: String },
}

pub struct TuiApp {
    pub input: TextArea<'static>,
    pub conversation: ConversationState,
    pub quit: bool,

    pub agent: Option<Arc<Mutex<crate::agent::AppAgent>>>,
    pub ui_tx: mpsc::UnboundedSender<UiEvent>,
    pub ui_rx: mpsc::UnboundedReceiver<UiEvent>,
    pub pending_response: bool,
    pub response_started_at: Option<Instant>,
    pub last_response_elapsed_secs: Option<u64>,
    pub current_context_tokens: Option<usize>,
    pub current_attachment_count: usize,
    pub chat_history: Vec<rig::completion::Message>,

    pub mouse_capture_enabled: bool,
    pub session_id: String,
    pub memory_db: Arc<crate::memory::sqlite::MemoryDB>,

    pub vertical_scroll: ScrollbarState,
    pub file_picker: FilePickerState,
    pub markdown_cache: HashMap<u64, Vec<Line<'static>>>,
}

impl TuiApp {
    pub fn new(
        agent: crate::agent::AppAgent,
        initial_messages: Vec<crate::state::conversation::UiMessage>,
        initial_rig_history: Vec<rig::completion::Message>,
        session_id: String,
        memory_db: Arc<crate::memory::sqlite::MemoryDB>,
        ui_tx: mpsc::UnboundedSender<UiEvent>,
        ui_rx: mpsc::UnboundedReceiver<UiEvent>,
    ) -> Self {
        Self {
            input: TextArea::default(),
            conversation: ConversationState {
                messages: initial_messages,
                scroll_offset: 0,
                auto_scroll: true,
                cursor: None,
                pending_selection: None,
            },
            quit: false,
            agent: Some(Arc::new(Mutex::new(agent))),
            ui_tx,
            ui_rx,
            pending_response: false,
            response_started_at: None,
            last_response_elapsed_secs: None,
            current_context_tokens: None,
            current_attachment_count: 0,
            mouse_capture_enabled: true,
            chat_history: initial_rig_history,
            session_id,
            memory_db,
            vertical_scroll: ScrollbarState::new(0),
            file_picker: FilePickerState::default(),
            markdown_cache: HashMap::new(),
        }
    }

    pub fn sync_scrollbar(&mut self, content_height: usize, viewport_height: usize) {
        let max_pos = content_height.saturating_sub(viewport_height);

        if self.conversation.auto_scroll {
            self.vertical_scroll = ScrollbarState::new(content_height)
                .position(max_pos)
                .viewport_content_length(viewport_height);
        } else {
            let current = self.vertical_scroll.get_position().min(max_pos);
            self.vertical_scroll = ScrollbarState::new(content_height)
                .position(current)
                .viewport_content_length(viewport_height);
        }
    }

    pub fn open_file_picker(&mut self) {
        self.file_picker.visible = true;
        self.file_picker.query.clear();
        self.file_picker.selected = 0;
        self.refresh_file_picker();
    }

    pub fn close_file_picker(&mut self) {
        self.file_picker.visible = false;
    }

    pub fn file_picker_input(&mut self, ch: char) {
        self.file_picker.query.push(ch);
        self.file_picker.selected = 0;
        self.refresh_file_picker();
    }

    pub fn file_picker_backspace(&mut self) {
        self.file_picker.query.pop();
        self.file_picker.selected = 0;
        self.refresh_file_picker();
    }

    pub fn file_picker_next(&mut self) {
        if !self.file_picker.matches.is_empty() {
            self.file_picker.selected =
                (self.file_picker.selected + 1).min(self.file_picker.matches.len() - 1);
        }
    }

    pub fn file_picker_prev(&mut self) {
        self.file_picker.selected = self.file_picker.selected.saturating_sub(1);
    }

    pub fn insert_selected_file(&mut self) {
        if let Some(path) = self.file_picker.matches.get(self.file_picker.selected) {
            self.input.insert_str(&format!("{} ", path));
        }
        self.close_file_picker();
    }

    fn refresh_file_picker(&mut self) {
        self.file_picker.matches =
            crate::context::file_loader::discover_files(&self.file_picker.query)
                .unwrap_or_default()
                .into_iter()
                .take(FILE_PICKER_LIMIT)
                .collect();
        if self.file_picker.selected >= self.file_picker.matches.len() {
            self.file_picker.selected = self.file_picker.matches.len().saturating_sub(1);
        }
    }

    pub fn handle_submit(&mut self) -> Option<Command> {
        let text = self.input.lines().join("\n");
        self.input = TextArea::default();

        if text.eq_ignore_ascii_case("exit") {
            return Some(Command::Exit);
        }

        Some(parse(&text))
    }

    pub fn send_to_agent(&mut self, raw_input: String) {
        if self.pending_response {
            return;
        }

        let msg = raw_input.trim().to_string();
        if msg.is_empty() {
            return;
        }

        // 1. Push user message
        self.conversation.messages.push(UiMessage {
            role: MessageRole::User,
            content: msg.clone(),
            raw: Some(msg.clone()),
            timestamp: chrono::Utc::now(),
        });
        self.conversation.auto_scroll = true;
        self.conversation.scroll_offset = 0;

        // 2. Handle @file references as one-shot attachments for this prompt.
        let file_refs = crate::context::file_loader::extract_file_references(&msg);
        self.current_attachment_count = 0;
        let mut enriched_prompt = msg.clone();
        let mut attached_files: Vec<String> = Vec::new();
        for file_path in &file_refs {
            let cached = {
                let db = self.memory_db.clone();
                let session = self.session_id.clone();
                let path = file_path.clone();
                tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(
                        crate::context::file_cache::try_get_cached(&db, &session, &path),
                    )
                })
                .unwrap_or(None)
            };

            let content = match cached {
                Some(c) => c,
                None => {
                    let clean_path = file_path.trim_start_matches('@');
                    match crate::context::file_loader::read_file_text(clean_path) {
                        Ok(c) => {
                            let db = self.memory_db.clone();
                            let s = self.session_id.clone();
                            let p = file_path.clone();
                            let c_clone = c.clone();
                            tokio::spawn(async move {
                                crate::context::file_cache::store_in_cache(&db, &s, &p, &c_clone)
                                    .await;
                            });
                            c
                        }
                        Err(e) => {
                            let _ = self.ui_tx.send(UiEvent::ToolStatus {
                                name: "system".into(),
                                message: format!("Failed to read {}: {}", file_path, e),
                            });
                            continue;
                        }
                    }
                }
            };
            attached_files.push(file_path.clone());
            enriched_prompt.push_str(&format!("\n\n[FILE: {}]\n{}\n[/FILE]", file_path, content));
        }

        self.current_attachment_count = attached_files.len();

        // 3. Set pending + show "Thinking..."
        self.pending_response = true;
        self.response_started_at = Some(Instant::now());
        self.last_response_elapsed_secs = None;
        self.conversation.messages.push(UiMessage {
            role: MessageRole::System,
            content: "Thinking...".to_string(),
            raw: None,
            timestamp: chrono::Utc::now(),
        });
        self.conversation.auto_scroll = true;

        // 4. Clone deps
        let agent = self.agent.clone().unwrap();
        let ui_tx = self.ui_tx.clone();
        let session_id = self.session_id.clone();
        let memory_db = self.memory_db.clone();
        let user_text_for_db = msg.clone();
        let config = crate::config::Config::load().expect("Failed to load config");
        let timeout_duration =
            std::time::Duration::from_secs(config.timeout_base_secs * config.max_turns);

        let max_ctx = config.max_context_messages;
        let chat_history: Vec<rig::completion::Message> = if self.chat_history.len() > max_ctx {
            self.chat_history[self.chat_history.len().saturating_sub(max_ctx)..].to_vec()
        } else {
            self.chat_history.clone()
        };
        self.current_context_tokens =
            Some(estimate_context_tokens(&enriched_prompt, &chat_history));

        // 5. Persist User message to history and DB
        let user_rig_msg = rig::completion::Message::User {
            content: rig::OneOrMany::one(rig::message::UserContent::Text(rig::message::Text {
                text: enriched_prompt.clone(),
            })),
        };

        // Save to local history (for future turns)
        self.chat_history.push(user_rig_msg.clone());

        // Save to DB
        let db_mem = self.memory_db.clone();
        let db_sid = self.session_id.clone();
        tokio::spawn(async move {
            if let Ok(db_msg) = crate::memory::serialization::rig_to_db(&db_sid, &user_rig_msg) {
                use crate::memory::session_store::SessionStore;
                let _ = db_mem.save_message(&db_sid, &db_msg).await;
            }
        });

        // 6. Spawn async task WITH IN-CHAT DEBUG
        tokio::spawn(async move {
            let agent_locked = agent.lock().await;

            let _ = ui_tx.send(UiEvent::ToolStatus {
                name: "agent".into(),
                message: "Processing...".into(),
            });

            let result = tokio::time::timeout(
                timeout_duration,
                agent_locked.chat(&enriched_prompt, chat_history),
            )
            .await;

            // let status_msg = match &result {
            //     // Ok(Ok(resp)) => format!("LLM responded ({} chars)", resp.len()),
            //     Ok(Err(e)) => format!("LLM error: {}", e),
            //     Err(_) => format!("Timeout after {}s", timeout_duration.as_secs()),
            // };
            // let _ = ui_tx.send(UiEvent::ToolStatus {
            //     name: "agent".into(),
            //     message: status_msg,
            // });

            match result {
                Ok(Ok(resp)) => {
                    let _ = ui_tx.send(UiEvent::AgentResponse(resp.clone()));
                    let _ = ui_tx.send(UiEvent::NewRigMessage(
                        rig::completion::Message::Assistant {
                            content: rig::OneOrMany::one(rig::completion::AssistantContent::Text(
                                rig::message::Text { text: resp },
                            )),
                            id: None,
                        },
                    ));
                }
                Ok(Err(e)) => {
                    let _ = ui_tx.send(UiEvent::AgentError(e.to_string()));
                }
                Err(_) => {
                    let _ = ui_tx.send(UiEvent::AgentError(format!(
                        "Request timed out ({}s)",
                        timeout_duration.as_secs()
                    )));
                }
            }
        });
    }

    pub fn poll_agent_events(&mut self) {
        while let Ok(event) = self.ui_rx.try_recv() {
            match event {
                UiEvent::AgentResponse(text) => {
                    // Remove pending "Thinking..." if present
                    if let Some(last) = self.conversation.messages.last() {
                        if last.role == MessageRole::System && last.content == "Thinking..." {
                            self.conversation.messages.pop();
                        }
                    }

                    self.conversation.messages.push(UiMessage {
                        role: MessageRole::Assistant,
                        content: text.clone(),
                        raw: Some(text.clone()),
                        timestamp: chrono::Utc::now(),
                    });
                    self.conversation.auto_scroll = true;
                    self.conversation.scroll_offset = 0;

                    if let Ok(db_msg) = crate::memory::serialization::rig_to_db(
                        &self.session_id,
                        &rig::completion::Message::Assistant {
                            content: rig::OneOrMany::one(rig::completion::AssistantContent::Text(
                                rig::message::Text { text: text.clone() },
                            )),
                            id: None,
                        },
                    ) {
                        let mem = self.memory_db.clone();
                        let sid = self.session_id.clone();
                        tokio::spawn(async move {
                            use crate::memory::session_store::SessionStore;
                            let _ = mem.save_message(&sid, &db_msg).await;
                        });
                    }
                    self.finish_response_timer();
                    self.pending_response = false;
                }
                UiEvent::AgentError(err) => {
                    if let Some(last) = self.conversation.messages.last() {
                        if last.role == MessageRole::System && last.content == "Thinking..." {
                            self.conversation.messages.pop();
                        }
                    }
                    self.conversation.messages.push(UiMessage {
                        role: MessageRole::System,
                        content: format!("Error: {}", err),
                        raw: None,
                        timestamp: chrono::Utc::now(),
                    });
                    self.finish_response_timer();
                    self.pending_response = false;
                }
                UiEvent::NewRigMessage(rig_msg) => {
                    self.chat_history.push(rig_msg);
                }
                UiEvent::ToolStatus { name, message } => {
                    // Tool logs overwrite "Thinking..." if it's on screen
                    if let Some(last) = self.conversation.messages.last() {
                        if last.role == MessageRole::System && last.content == "Thinking..." {
                            self.conversation.messages.pop();
                        }
                    }
                    self.conversation.messages.push(UiMessage {
                        role: MessageRole::System,
                        content: format!("[{}] {}", name, message),
                        raw: None,
                        timestamp: chrono::Utc::now(),
                    });
                    self.conversation.auto_scroll = true;
                }
            }
        }
    }

    pub fn execute(&mut self, cmd: Command) {
        match cmd {
            Command::Message(msg) => {
                self.send_to_agent(msg);
            }
            Command::Exit => self.quit = true,
            Command::Help => {
                self.conversation.messages.push(UiMessage {
                    role: MessageRole::System,
                    content: "Enter=send | Shift/Alt+Enter=newline | Ctrl+C=exit".to_string(),
                    raw: None,
                    timestamp: chrono::Utc::now(),
                });
                self.conversation.auto_scroll = true;
            }
            Command::Copy { target } => {
                let last_assistant_msg = self
                    .conversation
                    .messages
                    .iter()
                    .rev()
                    .find(|m| m.role == MessageRole::Assistant);

                if let Some(msg) = last_assistant_msg {
                    if let Some(block) =
                        crate::utils::clipboard::get_code_block(&msg.content, &target)
                    {
                        match crate::utils::clipboard::copy_to_clipboard(&block) {
                            Ok(_) => {
                                self.conversation.messages.push(UiMessage {
                                    role: MessageRole::System,
                                    content: format!(
                                        "Block {} copied to clipboard",
                                        if target == "last" { "last" } else { &target }
                                    ),
                                    raw: None,
                                    timestamp: chrono::Utc::now(),
                                });
                            }
                            Err(e) => {
                                self.conversation.messages.push(UiMessage {
                                    role: MessageRole::System,
                                    content: format!("Copy failed: {}", e),
                                    raw: None,
                                    timestamp: chrono::Utc::now(),
                                });
                            }
                        }
                    } else {
                        let count =
                            crate::utils::clipboard::extract_code_blocks(&msg.content).len();
                        self.conversation.messages.push(UiMessage {
                            role: MessageRole::System,
                            content: if count == 0 {
                                "No code blocks found".to_string()
                            } else {
                                format!("Invalid index. Available: 1-{}", count)
                            },
                            raw: None,
                            timestamp: chrono::Utc::now(),
                        });
                    }
                } else {
                    self.conversation.messages.push(UiMessage {
                        role: MessageRole::System,
                        content: "No assistant message to copy from".to_string(),
                        raw: None,
                        timestamp: chrono::Utc::now(),
                    });
                }
                self.conversation.auto_scroll = true;
            }
            Command::RenameSession(name) => {
                if name.trim().is_empty() {
                    return;
                }
                let new_name = name.trim().to_string();
                let mem = self.memory_db.clone();
                let sid = self.session_id.clone();
                let n = new_name.clone();
                tokio::spawn(async move {
                    use crate::memory::session_store::SessionStore;
                    let _ = mem.update_session_name(&sid, &n).await;
                });
                self.conversation.messages.push(UiMessage {
                    role: MessageRole::System,
                    content: format!("Session renamed: '{}'", new_name),
                    raw: None,
                    timestamp: chrono::Utc::now(),
                });
                self.conversation.auto_scroll = true;
            }
            _ => {}
        }
    }

    pub fn response_elapsed_secs(&self) -> Option<u64> {
        self.response_started_at
            .map(|started| started.elapsed().as_secs())
            .or(self.last_response_elapsed_secs)
    }

    fn finish_response_timer(&mut self) {
        self.last_response_elapsed_secs = self
            .response_started_at
            .map(|started| started.elapsed().as_secs());
        self.response_started_at = None;
    }

    pub fn export_conversation_to_markdown(&self) -> String {
        crate::utils::visual_mode::export_conversation_to_markdown(&self.conversation.messages)
    }
}

#[derive(Default)]
pub struct FilePickerState {
    pub visible: bool,
    pub query: String,
    pub matches: Vec<String>,
    pub selected: usize,
}

fn estimate_context_tokens(prompt: &str, history: &[rig::completion::Message]) -> usize {
    let history_chars: usize = history
        .iter()
        .map(|msg| format!("{:?}", msg).chars().count())
        .sum();
    (prompt.chars().count() + history_chars).div_ceil(4)
}
