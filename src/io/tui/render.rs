use crate::io::tui::app::TuiApp;
use crate::state::conversation::MessageRole;
use ratatui::layout::Margin;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Padding, Paragraph, Scrollbar, ScrollbarOrientation, Wrap};

pub fn render(frame: &mut Frame, app: &mut TuiApp) {
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

    let chat_block = Block::new().padding(Padding::new(2, 2, 1, 1));

    let mut lines: Vec<Line<'static>> = Vec::new();
    for (i, msg) in app.conversation.messages.iter().enumerate() {
        if i > 0 {
            lines.push(Line::from(""));
        }

        let style = match msg.role {
            MessageRole::User => Style::default().fg(Color::Cyan),
            MessageRole::Assistant => Style::default().fg(Color::White),
            MessageRole::System => Style::default()
                .fg(Color::Rgb(90, 90, 90))
                .add_modifier(ratatui::style::Modifier::ITALIC),
        };

        for line in msg.content.lines() {
            // ← .to_string() rompe el préstamo con app
            lines.push(Line::from(Span::styled(line.to_string(), style)));
        }
    }

    // Ahora podemos tomar app prestada mutablemente sin conflictos
    let inner_area = chat_block.inner(chat_area);
    let viewport_height = inner_area.height as usize;
    let bounding_width = inner_area.width as usize;

    let paragraph = Paragraph::new(lines)
        .block(chat_block)
        .wrap(Wrap { trim: true });

    // Calcular líneas reales (requiere feature unstable-rendered-line-info)
    let content_height = paragraph.line_count(bounding_width as u16);

    // Sincronizar estado del scrollbar (préstamo mutable)
    app.sync_scrollbar(content_height, viewport_height);

    // Aplicar scroll usando la posición actualizada
    let scroll_row = app.vertical_scroll.get_position() as u16;
    let scrolled_paragraph = paragraph.scroll((scroll_row, 0));

    frame.render_widget(scrolled_paragraph, chat_area);

    // Renderizar widget Scrollbar solo si hay overflow
    if content_height > viewport_height {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(Some("│"))
            .thumb_symbol("█")
            .track_style(Style::new().fg(Color::DarkGray))
            .thumb_style(Style::new().fg(Color::Rgb(100, 180, 220)));

        frame.render_stateful_widget(
            scrollbar,
            inner_area.inner(Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut app.vertical_scroll,
        );
    }

    // Input
    let input_block = Block::new()
        .title(" > ")
        .borders(Borders::ALL)
        .border_style(Style::new().fg(Color::DarkGray));
    frame.render_widget(&input_block, input_area);
    frame.render_widget(&app.input, input_block.inner(input_area));

    // Status bar
    let mode_text = if app.mouse_capture_enabled {
        "Scroll Mode"
    } else {
        "Select Mode"
    };

    let line = Line::from(vec![
        Span::styled("Active: ", Style::new().fg(Color::Rgb(150, 150, 150))),
        Span::styled(
            mode_text,
            Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" │ ", Style::new().fg(Color::Rgb(80, 80, 80))),
        Span::styled("Click/Esc ", Style::new().fg(Color::Rgb(100, 180, 220))),
        Span::styled(" │ ", Style::new().fg(Color::Rgb(80, 80, 80))),
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
