use crate::io::tui::commands::{Command, parse};
use crate::state::conversation::{ConversationState, MessageRole, UiMessage};
use ratatui::widgets::ScrollbarState;
use ratatui_textarea::TextArea;
use rig::completion::Chat;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::mpsc;

pub enum UiEvent {
    AgentResponse(String),
    AgentError(String),
    NewRigMessage(rig::completion::Message),
}

pub struct TuiApp {
    pub input: TextArea<'static>,
    pub conversation: ConversationState,
    pub status: String,
    pub quit: bool,

    pub agent: Option<Arc<Mutex<crate::agent::AppAgent>>>,
    pub ui_tx: mpsc::UnboundedSender<UiEvent>,
    pub ui_rx: mpsc::UnboundedReceiver<UiEvent>,
    pub pending_response: bool,
    pub chat_history: Vec<rig::completion::Message>,

    pub mouse_capture_enabled: bool,
    pub session_id: String,
    pub memory_db: Arc<crate::memory::sqlite::MemoryDB>,

    // ← NUEVO: Estado del scrollbar (fuente de verdad para scroll vertical)
    pub vertical_scroll: ScrollbarState,
}

impl TuiApp {
    pub fn new(
        agent: crate::agent::AppAgent,
        initial_messages: Vec<crate::state::conversation::UiMessage>,
        initial_rig_history: Vec<rig::completion::Message>,
        session_id: String,
        memory_db: Arc<crate::memory::sqlite::MemoryDB>,
    ) -> Self {
        let (ui_tx, ui_rx) = mpsc::unbounded_channel();

        Self {
            input: TextArea::default(),
            conversation: ConversationState {
                messages: initial_messages,
                scroll_offset: 0,
                auto_scroll: true,
                cursor: None,
                pending_selection: None,
            },
            status: "Mouse Scroll ON(alt+M)".to_string(),
            quit: false,
            agent: Some(Arc::new(Mutex::new(agent))),
            ui_tx,
            ui_rx,
            pending_response: false,
            mouse_capture_enabled: true,
            chat_history: initial_rig_history,
            session_id,
            memory_db,
            vertical_scroll: ScrollbarState::new(0),
        }
    }

    // ← NUEVO: Sincroniza el scrollbar con el contenido real y el viewport
    pub fn sync_scrollbar(&mut self, content_height: usize, viewport_height: usize) {
        let max_pos = content_height.saturating_sub(viewport_height);

        if self.conversation.auto_scroll {
            // Auto-scroll: fuerza al fondo
            self.vertical_scroll = ScrollbarState::new(content_height)
                .position(max_pos)
                .viewport_content_length(viewport_height);
        } else {
            // Scroll manual: mantiene posición actual, clampa al límite
            let current = self.vertical_scroll.get_position().min(max_pos);
            self.vertical_scroll = ScrollbarState::new(content_height)
                .position(current)
                .viewport_content_length(viewport_height);
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

        let trimmed = raw_input.trim().to_string();
        if trimmed.is_empty() {
            return;
        }

        let clean_message = trimmed
            .split("--- USER CONTEXT ---")
            .next()
            .unwrap_or(&trimmed)
            .split("--- END CONTEXT ---")
            .next()
            .unwrap_or(&trimmed)
            .trim()
            .to_string();

        self.conversation.messages.push(UiMessage {
            role: MessageRole::User,
            content: clean_message.clone(),
            raw: Some(clean_message.clone()),
            timestamp: chrono::Utc::now(),
        });

        self.conversation.auto_scroll = true;
        self.conversation.scroll_offset = 0;

        let file_refs = crate::context::file_loader::extract_file_references(&clean_message);
        let mut enriched_prompt = trimmed.clone();
        let mut files_loaded = 0;

        for file_path in &file_refs {
            if let Ok(content) = crate::context::file_loader::read_file_text(file_path) {
                enriched_prompt
                    .push_str(&format!("\n\n[FILE: {}]\n{}\n[/FILE]", file_path, content));
                files_loaded += 1;
            }
        }

        self.conversation.messages.push(UiMessage {
            role: MessageRole::System,
            content: if files_loaded > 0 {
                "Reading files...".to_string()
            } else {
                "Thinking...".to_string()
            },
            raw: None,
            timestamp: chrono::Utc::now(),
        });

        self.pending_response = true;

        let agent = self.agent.clone().unwrap();
        let ui_tx = self.ui_tx.clone();
        let chat_history = self.chat_history.clone();
        let session_id = self.session_id.clone();
        let memory_db = self.memory_db.clone();
        let user_text_for_db = clean_message.clone();

        let config = crate::config::Config::load().expect("Failed to load config");
        let timeout_secs = config.timeout_base_secs * config.max_turns;
        let timeout_duration = std::time::Duration::from_secs(timeout_secs);

        tokio::spawn(async move {
            let result = tokio::time::timeout(
                timeout_duration,
                agent.lock().await.chat(&enriched_prompt, chat_history),
            )
            .await;

            match result {
                Ok(Ok(resp)) => {
                    if let Ok(db_msg) = crate::memory::serialization::rig_to_db(
                        &session_id,
                        &rig::completion::Message::User {
                            content: rig::OneOrMany::one(rig::message::UserContent::Text(
                                rig::message::Text {
                                    text: user_text_for_db,
                                },
                            )),
                        },
                    ) {
                        use crate::memory::session_store::SessionStore;
                        let _ = memory_db.save_message(&session_id, &db_msg).await;
                    }

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
                        timeout_secs
                    )));
                }
            }
        });
    }

    pub fn poll_agent_events(&mut self) {
        while let Ok(event) = self.ui_rx.try_recv() {
            match event {
                UiEvent::AgentResponse(text) => {
                    if let Some(last) = self.conversation.messages.last() {
                        if last.role == MessageRole::System {
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

                    self.pending_response = false;
                }
                UiEvent::AgentError(err) => {
                    if let Some(last) = self.conversation.messages.last() {
                        if last.role == MessageRole::System {
                            self.conversation.messages.pop();
                        }
                    }
                    self.conversation.messages.push(UiMessage {
                        role: MessageRole::System,
                        content: format!("Error: {}", err),
                        raw: None,
                        timestamp: chrono::Utc::now(),
                    });
                    self.pending_response = false;
                }
                UiEvent::NewRigMessage(rig_msg) => {
                    self.chat_history.push(rig_msg);
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
                self.status = "Enter=enviar | Shift/Alt+Enter=newline | Ctrl+C=salir".to_string()
            }
            Command::Copy { target } => {
                let last_assistant_msg = self
                    .conversation
                    .messages
                    .iter()
                    .rev()
                    .find(|m| m.role == MessageRole::Assistant);

                if let Some(msg) = last_assistant_msg {
                    let code_block = crate::utils::clipboard::get_code_block(&msg.content, &target);

                    match code_block {
                        Some(block) => match crate::utils::clipboard::copy_to_clipboard(&block) {
                            Ok(_) => {
                                let block_num = if target == "last" { "último" } else { &target };
                                self.status = format!("Block {} copied to clipboard", block_num);
                            }
                            Err(e) => self.status = format!("❌ Copy failed: {}", e),
                        },
                        None => {
                            let blocks_count =
                                crate::utils::clipboard::extract_code_blocks(&msg.content).len();
                            self.status = if blocks_count == 0 {
                                "⚠️ No code blocks found in last message".to_string()
                            } else {
                                format!("Invalid index. Available: 1-{}", blocks_count)
                            };
                        }
                    }
                } else {
                    self.status = "No assistant message to copy from".to_string();
                }
            }

            Command::RenameSession(name) => {
                if name.trim().is_empty() {
                    self.status = "❌ Rename: nombre vacío".to_string();
                    return;
                }

                let new_name = name.trim().to_string();
                let new_name_for_db = new_name.clone();

                let memory_db = self.memory_db.clone();
                let session_id = self.session_id.clone();

                tokio::spawn(async move {
                    use crate::memory::session_store::SessionStore;
                    let _ = memory_db
                        .update_session_name(&session_id, &new_name_for_db)
                        .await;
                });

                self.status = format!("✅ New session name: '{}'", new_name);
            }
            _ => {}
        }
    }

    pub fn export_conversation_to_markdown(&self) -> String {
        crate::utils::visual_mode::export_conversation_to_markdown(&self.conversation.messages)
    }
}
