use crate::io::tui::app::TuiApp;
use crate::state::conversation::MessageRole;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Padding, Paragraph, Wrap};

pub fn render(frame: &mut Frame, app: &TuiApp) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(4),
            Constraint::Length(1),
        ])
        .split(frame.area());

    let chat_area = chunks[0];
    let input_area = chunks[1];
    let status_area = chunks[2];

    // Render actual conversation messages
    let chat_block = Block::new()
        // .title(" Lorem ")
        // .borders(Borders::ALL)
        // .border_style(Style::new().fg(Color::DarkGray))
        .padding(Padding::new(2, 2, 1, 1));

    // Build text from messages
    let mut lines = Vec::new();

    for (i, msg) in app.conversation.messages.iter().enumerate() {
        if i > 0 {
            lines.push(Line::from(""));
        }

        let style = match msg.role {
            MessageRole::User => Style::default().fg(Color::Cyan),
            MessageRole::Assistant => Style::default().fg(Color::White),
            MessageRole::System => Style::default().fg(Color::DarkGray),
        };

        // Cada línea del contenido con el estilo aplicado
        for line in msg.content.lines() {
            lines.push(Line::from(Span::styled(line, style)));
        }
    }

    // Add "thinking" indicator if pending
    if app.pending_response {
        lines.push(Line::from(""));
        lines.push(Line::from("Thinking ..."));
    }

    let total_lines = lines.len();
    let visible_lines = chat_area.height.saturating_sub(2) as usize;

    let scroll_row = if app.conversation.auto_scroll && total_lines > visible_lines {
        (total_lines.saturating_sub(visible_lines)) as u16
    } else {
        app.conversation.scroll_offset as u16
    };

    let paragraph = Paragraph::new(lines)
        .block(chat_block)
        .wrap(Wrap { trim: true })
        .scroll((scroll_row, 0));

    frame.render_widget(paragraph, chat_area);

    // Input
    let input_block = Block::new()
        .title(" > ")
        .borders(Borders::ALL)
        .border_style(Style::new().fg(Color::DarkGray));
    frame.render_widget(&input_block, input_area);
    frame.render_widget(&app.input, input_block.inner(input_area));

    // Status bar
    // Status bar estructurado: Active: [Mode] │ hints estáticos
    let mode_text = if app.mouse_capture_enabled {
        "Scroll Mode"
    } else {
        "Select Mode"
    };

    // Construir línea con segmentos estilizados
    let line = Line::from(vec![
        // "Active: " estático en gris medio
        Span::styled("Active: ", Style::new().fg(Color::Rgb(150, 150, 150))),
        // Modo actual (dinámico) en blanco
        Span::styled(
            mode_text,
            Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
        // Separador
        Span::styled(" │ ", Style::new().fg(Color::Rgb(80, 80, 80))),
        // Hint Click/Esc estático en cyan suave
        Span::styled("Click/Esc ", Style::new().fg(Color::Rgb(100, 180, 220))),
        // Separador
        Span::styled(" │ ", Style::new().fg(Color::Rgb(80, 80, 80))),
        // Hint Visual Mode estático en verde suave
        Span::styled(
            "Ctrl+E > Visual",
            Style::new().fg(Color::Rgb(120, 200, 150)),
        ),
    ]);

    let status_paragraph = Paragraph::new(line)
        .style(Style::new().bg(Color::Rgb(35, 35, 35)))
        .wrap(ratatui::widgets::Wrap { trim: true });

    frame.render_widget(status_paragraph, status_area);
}
