use crate::memory::session_store::{SessionMeta as DbSessionMeta, SessionStore};
use crate::memory::sqlite::MemoryDB;
use anyhow::Result;
use crossterm::event::{self, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};
use ratatui::{Frame, Terminal};
use std::io;
use std::sync::Arc;
use std::time::Duration;

// ← UI-only struct con nombre único para evitar colisiones
struct SessionListItem {
    id: String,
    display_name: String,
}

/// Muestra menú de selección de sesiones y retorna el session_id seleccionado
pub fn select_session(memory_db: &Arc<MemoryDB>) -> Result<String> {
    // 1. Cargar sesiones existentes y mapear a UI struct
    let sessions = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let raw_result = memory_db.list_sessions().await;

            raw_result
                .unwrap_or_default()
                .into_iter()
                .map(|s: DbSessionMeta| {
                    let id_prefix: String = s.id.chars().take(8).collect();
                    SessionListItem {
                        id: s.id,
                        display_name: s
                            .name
                            .filter(|n| !n.is_empty())
                            .unwrap_or_else(|| format!("Session {}", id_prefix)),
                    }
                })
                .collect::<Vec<_>>()
        })
    });

    // 2. Setup terminal minimal
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    terminal::enable_raw_mode()?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // 3. Estado del selector
    let mut state = ListState::default();
    state.select(Some(0));

    // 4. Loop de render + input (SOLO FLECHAS + ENTER)
    let result = loop {
        terminal.draw(|f| render_session_menu(f, &sessions, &state))?;

        if event::poll(Duration::from_millis(100))?
            && let Ok(event::Event::Key(key)) = event::read()
            && key.kind == KeyEventKind::Press
        {
            match key.code {
                KeyCode::Enter => {
                    if let Some(idx) = state.selected() {
                        if idx == 0 {
                            // "New session"
                            break Ok(uuid::Uuid::new_v4().to_string());
                        } else if idx >= 1 && idx <= sessions.len() {
                            // Sesión existente
                            break Ok(sessions[idx - 1].id.clone());
                        }
                    }
                    // Fallback por seguridad
                    break Ok(uuid::Uuid::new_v4().to_string());
                }
                KeyCode::Esc | KeyCode::Char('q') => {
                    break Ok(uuid::Uuid::new_v4().to_string());
                }
                KeyCode::Up => {
                    let current = state.selected().unwrap_or(0);
                    state.select(Some(current.saturating_sub(1)));
                }
                KeyCode::Down => {
                    let current = state.selected().unwrap_or(0);
                    let max = sessions.len();
                    state.select(Some((current + 1).min(max)));
                }
                _ => {}
            }
        }
    };

    // 5. Cleanup
    terminal::disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    result
}

fn render_session_menu(frame: &mut Frame, sessions: &[SessionListItem], state: &ListState) {
    use ratatui::prelude::*;
    use ratatui::style::{Color, Style};

    let area = frame.area();

    let mut items =
        vec![ListItem::new("0) New session (press Enter)").style(Style::default().fg(Color::Cyan))];
    for (i, session) in sessions.iter().enumerate() {
        items.push(ListItem::new(format!(
            "{}) {}",
            i + 1,
            session.display_name
        )));
    }

    let list = List::new(items)
        .block(
            Block::default()
                .title(" Select Session ")
                .borders(Borders::ALL),
        )
        .highlight_style(Style::default().bg(Color::DarkGray).fg(Color::White))
        .highlight_symbol("▶ ");

    let list_area = area.inner(Margin {
        vertical: 1,
        horizontal: 2,
    });

    frame.render_stateful_widget(list, list_area, &mut state.clone());
}
