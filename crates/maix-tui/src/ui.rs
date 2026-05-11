//! UI rendering with ratatui.

use crate::app::{mode_name, App, ActivePanel, ChatMessage};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Tabs, Wrap},
    Frame,
};

const ACCENT: Color = Color::Cyan;
const DIM: Color = Color::DarkGray;
const WARN: Color = Color::Yellow;

pub fn render(f: &mut Frame, app: &App) {
    let area = f.area();

    // Vertical split: status bar + main area
    let v_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1), Constraint::Length(3)])
        .split(area);

    render_status_bar(f, v_chunks[0], app);
    render_main_area(f, v_chunks[1], app);
    render_input(f, v_chunks[2], app);
}

fn render_status_bar(f: &mut Frame, area: Rect, app: &App) {
    let ctx_pct = if app.provider_caps.max_context > 0 {
        (app.total_tokens as f64 / app.provider_caps.max_context as f64 * 100.0) as u32
    } else {
        0
    };

    let text = format!(
        " Maix-Agent │ {} │ ctx:{}% │ tokens:{:.1}K │ ¥{:.4} │ {} mode │ {}",
        app.model_name,
        ctx_pct,
        app.total_tokens as f64 / 1000.0,
        app.total_cost,
        mode_name(app.mode),
        app.agent_state.as_deref().unwrap_or("init"),
    );

    let status = Paragraph::new(text)
        .style(Style::default().bg(ACCENT).fg(Color::Black));
    f.render_widget(status, area);
}

fn render_main_area(f: &mut Frame, area: Rect, app: &App) {
    // Main split: chat panel + side panel
    let h_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(area);

    render_chat(f, h_chunks[0], app);
    render_side_panel(f, h_chunks[1], app);
}

fn render_chat(f: &mut Frame, area: Rect, app: &App) {
    let mut lines: Vec<Line> = Vec::new();
    let max_lines = area.height as usize - 2;

    for msg in app.messages.iter().rev().take(max_lines).rev() {
        match msg {
            ChatMessage::User(text) => {
                for line in text.lines() {
                    lines.push(Line::from(vec![
                        Span::styled("> ", Style::default().fg(ACCENT)),
                        Span::raw(line),
                    ]));
                }
            }
            ChatMessage::Assistant(text) => {
                for line in text.lines() {
                    lines.push(Line::from(vec![
                        Span::styled("  ", Style::default()),
                        Span::raw(line),
                    ]));
                }
            }
            ChatMessage::ToolCall { name, args } => {
                let short_args = if args.len() > 80 {
                    format!("{}...", &args[..77])
                } else {
                    args.clone()
                };
                lines.push(Line::from(vec![
                    Span::styled("[tool:", Style::default().fg(WARN)),
                    Span::styled(name.as_str(), Style::default().fg(WARN).add_modifier(Modifier::BOLD)),
                    Span::styled(format!("({})]", short_args), Style::default().fg(WARN)),
                ]));
            }
            ChatMessage::ToolResult { result } => {
                let short = if result.len() > 100 {
                    format!("{}...", &result[..97])
                } else {
                    result.clone()
                };
                lines.push(Line::from(vec![
                    Span::styled("  => ", Style::default().fg(DIM)),
                    Span::styled(short, Style::default().fg(DIM)),
                ]));
            }
            ChatMessage::System(text) => {
                lines.push(Line::from(vec![
                    Span::styled("-- ", Style::default().fg(DIM)),
                    Span::styled(text, Style::default().fg(DIM)),
                ]));
            }
        }
    }

    let block = Block::default()
        .title(" Chat ")
        .borders(Borders::ALL);
    let paragraph = Paragraph::new(Text::from(lines))
        .block(block)
        .wrap(Wrap { trim: true });
    f.render_widget(paragraph, area);
}

fn render_side_panel(f: &mut Frame, area: Rect, app: &App) {
    let v_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(1)])
        .split(area);

    // Tabs
    let tabs = Tabs::new(vec!["Memory", "Tools", "Stats"])
        .select(match app.active_panel {
            ActivePanel::Memory => 0,
            ActivePanel::Tools => 1,
            ActivePanel::Stats => 2,
        })
        .style(Style::default().fg(Color::White))
        .highlight_style(Style::default().fg(ACCENT))
        .block(Block::default().borders(Borders::BOTTOM));
    f.render_widget(tabs, v_chunks[0]);

    match app.active_panel {
        ActivePanel::Memory => render_memory_panel(f, v_chunks[1], app),
        ActivePanel::Tools => render_tools_panel(f, v_chunks[1], app),
        ActivePanel::Stats => render_stats_panel(f, v_chunks[1], app),
    }
}

fn render_memory_panel(f: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .memories
        .iter()
        .map(|m| {
            let display = format!(
                " kind={} {}",
                m.kind,
                &m.content[..m.content.len().min(60)]
            );
            ListItem::new(display)
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL))
        .style(Style::default());
    f.render_widget(list, area);
}

fn render_tools_panel(f: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .tool_defs
        .iter()
        .map(|t| {
            ListItem::new(format!(
                " {}  risk={}  {}",
                t.name,
                t.risk_level,
                &t.description[..t.description.len().min(40)]
            ))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(list, area);
}

fn render_stats_panel(f: &mut Frame, area: Rect, app: &App) {
    let stats = format!(
        "\n Session: {}\n Rounds: {}\n Tokens: {}\n Cost: ¥{:.4}\n\n Model: {}\n Provider caps:\n  context: {}K\n  reasoning: {}\n  tool_use: {}",
        &app.session_id[..8.min(app.session_id.len())],
        app.round_count,
        app.total_tokens,
        app.total_cost,
        app.model_name,
        app.provider_caps.max_context / 1000,
        app.provider_caps.supports_reasoning,
        app.provider_caps.supports_tool_use,
    );

    let paragraph = Paragraph::new(stats)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(paragraph, area);
}

fn render_input(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title(format!(
            " Input │ {} mode │ Ctrl+S save │ Ctrl+Q quit │ Tab switch ",
            mode_name(app.mode),
        ))
        .borders(Borders::ALL)
        .border_style(if app.is_streaming.load(std::sync::atomic::Ordering::SeqCst) {
            Style::default().fg(WARN)
        } else {
            Style::default().fg(ACCENT)
        });

    let cursor_pos = app.input.cursor.min(app.input.buffer.len());
    let before = &app.input.buffer[..cursor_pos];
    let cursor_char = app.input.buffer.chars().nth(cursor_pos).unwrap_or(' ');
    let after = &app.input.buffer[cursor_pos + cursor_char.len_utf8()..];

    let line = Line::from(vec![
        Span::raw("> "),
        Span::raw(before),
        Span::styled(
            cursor_char.to_string(),
            Style::default().bg(Color::White).fg(Color::Black),
        ),
        Span::raw(after),
    ]);

    f.render_widget(Paragraph::new(line).block(block), area);
}
