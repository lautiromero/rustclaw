use crate::io::tui::app::TuiApp;
use crate::state::conversation::MessageRole;
use ratatui::layout::Margin;
use ratatui::prelude::*;
use ratatui::widgets::{
    Block, Borders, Clear, List, ListItem, ListState, Padding, Paragraph, Scrollbar,
    ScrollbarOrientation, Wrap,
};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

const MARKDOWN_CACHE_LIMIT: usize = 256;

pub fn render(frame: &mut Frame, app: &mut TuiApp) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(1),
            Constraint::Length(4),
            Constraint::Length(1),
        ])
        .split(frame.area());

    let chat_area = chunks[0];
    let help_area = chunks[1];
    let input_area = chunks[2];
    let status_area = chunks[3];

    let chat_block = Block::new().padding(Padding::new(2, 3, 1, 1));

    let mut lines: Vec<Line<'static>> = Vec::new();
    for (i, msg) in app.conversation.messages.iter().enumerate() {
        if i > 0 {
            lines.push(Line::from(""));
        }

        lines.extend(render_message_lines(
            msg.role.clone(),
            &msg.content,
            &mut app.markdown_cache,
        ));
    }

    if app.markdown_cache.len() > MARKDOWN_CACHE_LIMIT {
        app.markdown_cache.clear();
    }

    // From here on, app can be borrowed mutably without conflicting with message rendering.
    let inner_area = chat_block.inner(chat_area);
    let viewport_height = inner_area.height as usize;
    let bounding_width = inner_area.width as usize;

    let paragraph = Paragraph::new(lines)
        .block(chat_block)
        .wrap(Wrap { trim: true });

    // Compute rendered line count. Requires the unstable-rendered-line-info feature.
    let content_height = paragraph.line_count(bounding_width as u16);

    // Keep scrollbar state in sync with the rendered content height.
    app.sync_scrollbar(content_height, viewport_height);

    // Apply scroll using the synchronized position.
    let scroll_row = app.vertical_scroll.get_position() as u16;
    let scrolled_paragraph = paragraph.scroll((scroll_row, 0));

    frame.render_widget(scrolled_paragraph, chat_area);

    // Render the scrollbar only when content overflows.
    if content_height > viewport_height {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(Some("│"))
            .thumb_symbol("█")
            .track_style(Style::new().fg(Color::DarkGray))
            .thumb_style(Style::new().fg(Color::Rgb(100, 180, 220)));

        let scrollbar_area = Rect {
            x: chat_area.x + chat_area.width.saturating_sub(1),
            y: inner_area.y.saturating_add(1),
            width: 1,
            height: inner_area.height.saturating_sub(2),
        };

        frame.render_stateful_widget(scrollbar, scrollbar_area, &mut app.vertical_scroll);
    }

    // Help line
    let help_line = Line::from(vec![
        Span::styled("Click/Esc", Style::new().fg(Color::Rgb(95, 95, 95))),
        Span::styled(" toggle selection", Style::new().fg(Color::Rgb(70, 70, 70))),
        Span::styled("  |  ", Style::new().fg(Color::Rgb(55, 55, 55))),
        Span::styled("@", Style::new().fg(Color::Rgb(95, 95, 95))),
        Span::styled(" files", Style::new().fg(Color::Rgb(70, 70, 70))),
        Span::styled("  |  ", Style::new().fg(Color::Rgb(55, 55, 55))),
        Span::styled("Ctrl+E", Style::new().fg(Color::Rgb(95, 95, 95))),
        Span::styled(" visual mode", Style::new().fg(Color::Rgb(70, 70, 70))),
    ]);
    let help_paragraph = Paragraph::new(help_line);
    let help_inner_area = Rect {
        x: help_area.x.saturating_add(1),
        y: help_area.y,
        width: help_area.width.saturating_sub(1),
        height: help_area.height,
    };
    frame.render_widget(help_paragraph, help_inner_area);

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

    let context_label = format_context_usage(app.current_context_tokens);
    let elapsed_label = format_elapsed(app.response_elapsed_secs());

    let line = Line::from(vec![
        Span::styled("Active: ", Style::new().fg(Color::Rgb(150, 150, 150))),
        Span::styled(
            mode_text,
            Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" │ ", Style::new().fg(Color::Rgb(80, 80, 80))),
        Span::styled("Ctx ", Style::new().fg(Color::Rgb(150, 150, 150))),
        Span::styled(
            context_label,
            Style::new()
                .fg(Color::Rgb(210, 210, 210))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" │ ", Style::new().fg(Color::Rgb(80, 80, 80))),
        Span::styled("Files ", Style::new().fg(Color::Rgb(150, 150, 150))),
        Span::styled(
            format_attachment_count(app.current_attachment_count),
            Style::new()
                .fg(Color::Rgb(210, 210, 210))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" │ ", Style::new().fg(Color::Rgb(80, 80, 80))),
        Span::styled("Time ", Style::new().fg(Color::Rgb(150, 150, 150))),
        Span::styled(
            elapsed_label,
            Style::new()
                .fg(Color::Rgb(245, 190, 120))
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    let status_paragraph = Paragraph::new(line)
        .style(Style::new().bg(Color::Rgb(35, 35, 35)))
        .wrap(ratatui::widgets::Wrap { trim: true });

    frame.render_widget(status_paragraph, status_area);

    if app.file_picker.visible {
        render_file_picker(frame, app);
    }
}

fn render_message_lines(
    role: MessageRole,
    content: &str,
    markdown_cache: &mut HashMap<u64, Vec<Line<'static>>>,
) -> Vec<Line<'static>> {
    match role {
        MessageRole::Assistant => {
            let key = markdown_cache_key(&role, content);
            markdown_cache
                .entry(key)
                .or_insert_with(|| markdown_to_owned_lines(tui_markdown::from_str(content)))
                .clone()
        }
        MessageRole::User => render_plain_message(content, Style::default().fg(Color::Cyan)),
        MessageRole::System => render_plain_message(
            content,
            Style::default()
                .fg(Color::Rgb(90, 90, 90))
                .add_modifier(ratatui::style::Modifier::ITALIC),
        ),
    }
}

fn markdown_cache_key(role: &MessageRole, content: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    role.hash(&mut hasher);
    content.hash(&mut hasher);
    hasher.finish()
}

fn markdown_to_owned_lines(text: Text<'_>) -> Vec<Line<'static>> {
    text.lines
        .into_iter()
        .map(|line| {
            let owned_spans = line
                .spans
                .into_iter()
                .map(|span| Span::styled(span.content.into_owned(), span.style))
                .collect::<Vec<_>>();

            Line::from(owned_spans)
        })
        .collect()
}

fn render_plain_message(content: &str, style: Style) -> Vec<Line<'static>> {
    content
        .lines()
        .map(|line| Line::from(Span::styled(line.to_string(), style)))
        .collect()
}

fn render_file_picker(frame: &mut Frame, app: &mut TuiApp) {
    let input_top = frame.area().height.saturating_sub(6);
    let list_height = (app.file_picker.matches.len() as u16).clamp(3, 8);
    let popup_height = list_height + 2;
    let popup_width = frame.area().width.saturating_sub(8).min(92).max(24);
    let area = Rect {
        x: frame.area().x + 4,
        y: input_top.saturating_sub(popup_height),
        width: popup_width,
        height: popup_height,
    };
    let block = Block::new()
        .title(format!(" @ files: {}", app.file_picker.query))
        .borders(Borders::ALL)
        .border_style(Style::new().fg(Color::Rgb(65, 65, 65)))
        .style(Style::new().bg(Color::Rgb(16, 16, 16)))
        .padding(Padding::new(1, 1, 0, 0));

    let items: Vec<ListItem> = if app.file_picker.matches.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "No files found",
            Style::new()
                .fg(Color::Rgb(90, 90, 90))
                .add_modifier(Modifier::ITALIC),
        )))]
    } else {
        app.file_picker
            .matches
            .iter()
            .map(|path| {
                ListItem::new(Line::from(Span::styled(
                    path.clone(),
                    Style::new().fg(Color::Rgb(190, 190, 190)),
                )))
            })
            .collect()
    };

    let mut state = ListState::default().with_selected(if app.file_picker.matches.is_empty() {
        None
    } else {
        Some(app.file_picker.selected)
    });

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::new()
                .fg(Color::Rgb(235, 235, 235))
                .bg(Color::Rgb(45, 55, 58))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    frame.render_widget(Clear, area);
    frame.render_stateful_widget(list, area, &mut state);

    let hint_area = Rect {
        x: area.x + 2,
        y: area.y + area.height.saturating_sub(2),
        width: area.width.saturating_sub(4),
        height: 1,
    };
    let hint = Line::from(vec![
        Span::styled("Type", Style::new().fg(Color::Rgb(95, 95, 95))),
        Span::styled(" filter  ", Style::new().fg(Color::Rgb(70, 70, 70))),
        Span::styled("Enter", Style::new().fg(Color::Rgb(95, 95, 95))),
        Span::styled(" insert  ", Style::new().fg(Color::Rgb(70, 70, 70))),
        Span::styled("Esc", Style::new().fg(Color::Rgb(95, 95, 95))),
        Span::styled(" close", Style::new().fg(Color::Rgb(70, 70, 70))),
    ]);
    frame.render_widget(Paragraph::new(hint), hint_area);
}

fn format_context_usage(current: Option<usize>) -> String {
    match current {
        Some(current) => format!("[{}]", current),
        None => "[--]".to_string(),
    }
}

fn format_attachment_count(count: usize) -> String {
    format!("[{}]", count)
}

fn format_elapsed(seconds: Option<u64>) -> String {
    match seconds {
        Some(seconds) => format!("{}s", seconds),
        None => "--s".to_string(),
    }
}
