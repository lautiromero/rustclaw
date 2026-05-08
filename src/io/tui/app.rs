use crate::io::tui::commands::{Command, parse};
use crate::state::conversation::{ConversationState, MessageRole, UiMessage};
use ratatui_textarea::TextArea;
use rig::completion::Chat;
use rig::message::Message as RigMessage;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::mpsc;

// Eventos que el agente envía a la UI
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

    // Agent communication (MVP)
    pub agent: Option<Arc<Mutex<crate::agent::AppAgent>>>,
    pub ui_tx: mpsc::UnboundedSender<UiEvent>,
    pub ui_rx: mpsc::UnboundedReceiver<UiEvent>,
    pub pending_response: bool,
    pub chat_history: Vec<rig::completion::Message>,

    pub mouse_capture_enabled: bool,
    pub session_id: String,
    pub memory_db: Arc<crate::memory::sqlite::MemoryDB>,
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

    /// Spawn agent request in background (no context injection yet)
    pub fn send_to_agent(&mut self, enriched_prompt: String) {
        if self.pending_response {
            self.status = "⏳ Waiting for response".to_string();
            return;
        }

        let original_message = enriched_prompt
            .split("\n\n--- USER CONTEXT ---")
            .next()
            .unwrap_or(&enriched_prompt)
            .to_string();

        // 1. Mostrar mensaje del usuario en UI
        self.conversation.messages.push(UiMessage {
            role: MessageRole::User,
            content: original_message.clone(),
            raw: Some(original_message.clone()),
            timestamp: chrono::Utc::now(),
        });

        // 2. Guardar mensaje del usuario en DB (async, en background)
        let original_text = original_message.clone();
        let session_id = self.session_id.clone();
        let memory_db = self.memory_db.clone();

        tokio::spawn(async move {
            if let Ok(db_msg) = crate::memory::serialization::rig_to_db(
                &session_id,
                &rig::completion::Message::User {
                    content: rig::OneOrMany::one(rig::message::UserContent::Text(
                        rig::message::Text {
                            text: original_text,
                        },
                    )),
                },
            ) {
                use crate::memory::session_store::SessionStore;
                let _ = memory_db.save_message(&session_id, &db_msg).await;
            }
        });

        self.pending_response = true;
        self.status = "🤖 Pensando...".to_string();

        // 3. Spawn agent request
        let agent = self.agent.clone().unwrap();
        let ui_tx = self.ui_tx.clone();
        let chat_history = self.chat_history.clone();

        tokio::spawn(async move {
            let agent_guard = agent.lock().await;

            match agent_guard
                .chat(&enriched_prompt, chat_history.clone())
                .await
            {
                Ok(resp) => {
                    let new_msg = RigMessage::Assistant {
                        content: rig::OneOrMany::one(rig::completion::AssistantContent::Text(
                            rig::message::Text { text: resp.clone() },
                        )),
                        id: None,
                    };
                    let _ = ui_tx.send(UiEvent::AgentResponse(resp));
                    let _ = ui_tx.send(UiEvent::NewRigMessage(new_msg));
                }
                Err(e) => {
                    let _ = ui_tx.send(UiEvent::AgentError(e.to_string()));
                }
            }
        });
    }

    /// Poll for agent responses (call this in event loop)
    pub fn poll_agent_events(&mut self) {
        while let Ok(event) = self.ui_rx.try_recv() {
            match event {
                UiEvent::AgentResponse(text) => {
                    // 1. Actualizar UI
                    self.conversation.messages.push(UiMessage {
                        role: MessageRole::Assistant,
                        content: text.clone(),
                        raw: Some(text.clone()),
                        timestamp: chrono::Utc::now(),
                    });
                    self.pending_response = false;
                    self.status = "✅ Ready".to_string();
                    self.conversation.auto_scroll = true;

                    // 2. Guardar respuesta del agente en DB (async, en background)
                    let memory_db = self.memory_db.clone();
                    let session_id = self.session_id.clone();
                    let response_text = text.clone();

                    tokio::spawn(async move {
                        if let Ok(db_msg) = crate::memory::serialization::rig_to_db(
                            &session_id,
                            &rig::completion::Message::Assistant {
                                content: rig::OneOrMany::one(
                                    rig::completion::AssistantContent::Text(rig::message::Text {
                                        text: response_text,
                                    }),
                                ),
                                id: None,
                            },
                        ) {
                            use crate::memory::session_store::SessionStore;
                            let _ = memory_db.save_message(&session_id, &db_msg).await;
                        }
                    });
                }
                UiEvent::AgentError(err) => {
                    self.pending_response = false;
                    self.status = format!("❌ Error: {}", err);
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
                // Obtener el último mensaje del agente
                let last_assistant_msg = self
                    .conversation
                    .messages
                    .iter()
                    .rev()
                    .find(|m| m.role == MessageRole::Assistant);

                if let Some(msg) = last_assistant_msg {
                    // Extraer bloque de código según índice
                    let code_block = crate::utils::clipboard::get_code_block(&msg.content, &target);

                    match code_block {
                        Some(block) => {
                            // Copiar al portapapeles
                            match crate::utils::clipboard::copy_to_clipboard(&block) {
                                Ok(_) => {
                                    let block_num =
                                        if target == "last" { "último" } else { &target };
                                    self.status =
                                        format!("Block {} copied to clipboard", block_num);
                                }
                                Err(e) => self.status = format!("❌ Copy failed: {}", e),
                            }
                        }
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
                // Validar que no esté vacío
                if name.trim().is_empty() {
                    self.status = "❌ Rename: nombre vacío".to_string();
                    return;
                }

                let new_name = name.trim().to_string();
                let new_name_for_db = new_name.clone();

                let memory_db = self.memory_db.clone();
                let session_id = self.session_id.clone();

                // Actualizar en DB (background)
                tokio::spawn(async move {
                    use crate::memory::session_store::SessionStore;
                    let _ = memory_db
                        .update_session_name(&session_id, &new_name_for_db)
                        .await;
                });

                // Feedback inmediato en UI
                self.status = format!("✅ New session name: '{}'", new_name);
            }
            _ => {}
        }
    }

    pub fn export_conversation_to_markdown(&self) -> String {
        crate::utils::visual_mode::export_conversation_to_markdown(&self.conversation.messages)
    }
}
