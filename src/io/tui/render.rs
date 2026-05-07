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
    let status_text = app.status.chars().take(100).collect::<String>();
    let status_paragraph = Paragraph::new(format!(" {}", status_text))
        .style(Style::new().bg(Color::DarkGray).fg(Color::White))
        .wrap(ratatui::widgets::Wrap { trim: true });

    frame.render_widget(status_paragraph, status_area);
}
