use crate::io::tui::app::TuiApp;
use crate::io::tui::commands::Command;
use crate::io::tui::render::render;
use crate::memory::session_store::SessionStore;
use crate::memory::sqlite::MemoryDB;
use crate::state::conversation::{MessageRole, UiMessage};
use anyhow::Result;
use crossterm::event::EnableMouseCapture;
use crossterm::event::{
    self, DisableBracketedPaste, EnableBracketedPaste, KeyboardEnhancementFlags,
    PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::event::{DisableMouseCapture, KeyCode};
use crossterm::execute;
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui_textarea::Input;
use std::io;
use std::sync::Arc;

pub fn run_tui<M>(
    agent: crate::agent::AppAgent,
    memory_db: Arc<crate::memory::sqlite::MemoryDB>,
    session_id: String,
    context_injector: Option<crate::context::injector::ContextInjector<M>>,
) -> Result<()>
where
    M: rig::embeddings::EmbeddingModel + Clone + Send + Sync + 'static,
{
    let mut stdout = io::stdout();

    execute!(
        stdout,
        EnableMouseCapture,
        EnterAlternateScreen,
        EnableBracketedPaste,
        PushKeyboardEnhancementFlags(
            KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
                | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
        )
    )?;

    terminal::enable_raw_mode()?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (initial_ui, initial_rig) = load_session_messages(&memory_db, &session_id);
    let mut app = TuiApp::new(
        agent,
        initial_ui,
        initial_rig,
        session_id.clone(),
        memory_db.clone(),
    );

    loop {
        // ← IMPORTANTE: render ahora toma &mut app
        terminal.draw(|f| render(f, &mut app))?;

        if event::poll(std::time::Duration::from_millis(50))? {
            let crossterm_event = event::read()?;

            if let crossterm::event::Event::Paste(text) = crossterm_event {
                app.input.insert_str(&text);
                continue;
            }

            if let crossterm::event::Event::Mouse(mouse_event) = &crossterm_event {
                use crossterm::event::{MouseButton, MouseEventKind};
                if app.mouse_capture_enabled {
                    match mouse_event.kind {
                        MouseEventKind::ScrollUp => {
                            app.conversation.auto_scroll = false;
                            // ← FIX: Usar métodos de ScrollbarState (mismo ritmo que antes)
                            app.vertical_scroll.prev();
                            app.vertical_scroll.prev();
                            app.vertical_scroll.prev();
                        }
                        MouseEventKind::ScrollDown => {
                            app.vertical_scroll.next();
                            app.vertical_scroll.next();
                            app.vertical_scroll.next();
                        }
                        MouseEventKind::Down(MouseButton::Left) => {
                            app.mouse_capture_enabled = false;
                            let _ = execute!(std::io::stdout(), DisableMouseCapture);
                        }
                        _ => {}
                    }
                }
                continue;
            }

            if let crossterm::event::Event::Key(key_event) = &crossterm_event
                && key_event.kind == crossterm::event::KeyEventKind::Press
                && !app.mouse_capture_enabled
                && key_event.code == KeyCode::Esc
            {
                app.mouse_capture_enabled = true;
                let _ = execute!(std::io::stdout(), EnableMouseCapture);
                continue;
            }

            match crossterm_event.into() {
                Input {
                    key: ratatui_textarea::Key::Char('c'),
                    ctrl: true,
                    shift: false,
                    ..
                } => {
                    app.quit = true;
                }
                Input {
                    key: ratatui_textarea::Key::Char('e'),
                    ctrl: true,
                    shift: false,
                    alt: false,
                    ..
                } => {
                    let md_content = app.export_conversation_to_markdown();
                    let _ = crate::utils::visual_mode::run_visual_mode(
                        &mut terminal,
                        &md_content,
                        None,
                    );
                    app.mouse_capture_enabled = true;
                    let _ = execute!(std::io::stdout(), EnableMouseCapture);
                    continue;
                }
                Input {
                    key: ratatui_textarea::Key::Char('c'),
                    ctrl: true,
                    shift: true,
                    ..
                } => {}
                Input {
                    key: ratatui_textarea::Key::Enter,
                    shift: false,
                    alt: false,
                    ctrl: false,
                    ..
                } => {
                    if app.pending_response {
                        continue;
                    }

                    if let Some(cmd) = app.handle_submit() {
                        match cmd {
                            Command::Message(msg) => {
                                let enriched = if let Some(ref injector) = context_injector {
                                    let ctx = tokio::task::block_in_place(|| {
                                        tokio::runtime::Handle::current()
                                            .block_on(injector.build_context(&msg, 3))
                                    });
                                    if !ctx.trim().is_empty() {
                                        format!("{}\n\n{}", msg, ctx)
                                    } else {
                                        msg
                                    }
                                } else {
                                    msg
                                };
                                app.send_to_agent(enriched);
                            }
                            cmd => app.execute(cmd),
                        }
                    }
                }
                input => {
                    if !app.pending_response {
                        app.input.input(input);
                    }
                }
            }

            app.poll_agent_events();

            if app.quit {
                break;
            }
        }

        app.poll_agent_events();
    }

    terminal::disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        LeaveAlternateScreen,
        DisableBracketedPaste,
        PopKeyboardEnhancementFlags
    )?;
    Ok(())
}

fn load_session_messages(
    memory_db: &MemoryDB,
    session_id: &str,
) -> (Vec<UiMessage>, Vec<rig::completion::Message>) {
    use crate::memory::serialization::db_to_rig;

    let mut ui_messages = Vec::new();
    let mut rig_messages = Vec::new();

    let db_msgs = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(memory_db.load_session(session_id))
    });

    if let Ok(db_msgs) = db_msgs {
        for db_msg in db_msgs {
            if let Ok(rig_msg) = db_to_rig(&db_msg) {
                let content = extract_text_from_rig(&rig_msg);

                ui_messages.push(UiMessage {
                    role: match rig_msg {
                        rig::completion::Message::User { .. } => MessageRole::User,
                        rig::completion::Message::Assistant { .. } => MessageRole::Assistant,
                        _ => MessageRole::System,
                    },
                    content,
                    raw: Some(db_msg.content.clone()),
                    timestamp: db_msg.timestamp,
                });
                rig_messages.push(rig_msg.clone());
            }
        }
    }

    (ui_messages, rig_messages)
}

fn extract_text_from_rig(msg: &rig::completion::Message) -> String {
    use rig::completion::Message as RigMessage;

    match msg {
        RigMessage::User { content, .. } => content
            .iter()
            .filter_map(|c| {
                if let rig::message::UserContent::Text(rig::message::Text { text }) = c {
                    Some(text.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        RigMessage::Assistant { content, .. } => content
            .iter()
            .filter_map(|c| {
                if let rig::completion::AssistantContent::Text(rig::message::Text { text }) = c {
                    Some(text.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}
