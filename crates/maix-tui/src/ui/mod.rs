//! UI rendering with ratatui.

pub mod markdown;
pub mod theme;

pub use theme::Theme;

use crate::app::{mode_name, App, ActivePanel, ChatMessage};
use markdown::{highlight_search_matches, render_markdown};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Tabs, Wrap},
    Frame,
};

// Default theme constants (dark)
const ACCENT: Color = Color::Cyan;
const DIM: Color = Color::DarkGray;
const WARN: Color = Color::Yellow;

/// Truncate a string to max_chars characters, safe on UTF-8 boundaries.
fn truncate_str(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let truncated: String = s.chars().take(max_chars.saturating_sub(3)).collect();
    format!("{truncated}...")
}

/// Format token count with smart unit: K / M / B.
fn format_tokens(n: u64) -> String {
    if n >= 1_000_000_000 {
        format!("{:.1}B", n as f64 / 1_000_000_000.0)
    } else if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        format!("{}", n)
    }
}

pub fn render(f: &mut Frame, app: &App) {
    let area = f.area();

    // Calculate input area height based on completions and multi-line input
    let has_completions = !app.input.completions.is_empty();
    let input_lines = app.input.line_count().max(1) as u16;
    let max_input_height = (area.height / 3).max(3); // Max 1/3 of screen, min 3
    let input_box_height = (input_lines + 2).min(max_input_height); // +2 for borders
    let input_height = if has_completions {
        let completions_count = app.input.completions.len().min(6) as u16;
        input_box_height + completions_count + 2 // input + completions list + borders(2)
    } else {
        input_box_height
    };

    // Vertical split: session tabs + status bar + main area + input
    let has_session_tabs = app.sessions.len() > 1;
    let v_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            if has_session_tabs { Constraint::Length(1) } else { Constraint::Length(0) },
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(input_height),
        ])
        .split(area);

    if has_session_tabs {
        render_session_tabs(f, v_chunks[0], app);
    }
    render_status_bar(f, v_chunks[1], app);
    render_main_area(f, v_chunks[2], app);
    render_input(f, v_chunks[3], app);

    // Render command palette overlay
    if app.palette.is_visible() {
        render_palette(f, area, app);
    }

    // Render search overlay
    if app.search_mode {
        render_search(f, area, app);
    }

    // Render快捷键提示栏
    render_shortcut_bar(f, v_chunks[2], app);
}

fn render_session_tabs(f: &mut Frame, area: Rect, app: &App) {
    if app.sessions.len() <= 1 {
        return;
    }

    let tabs: Vec<String> = app.sessions.iter().enumerate().map(|(i, s)| {
        let active = if i == app.active_session { "●" } else { "○" };
        format!("{} {}", active, s.name)
    }).collect();

    let tab_widget = Tabs::new(tabs)
        .select(app.active_session)
        .style(Style::default().fg(Color::White))
        .highlight_style(Style::default().fg(ACCENT))
        .block(Block::default().borders(Borders::BOTTOM));
    f.render_widget(tab_widget, area);
}

fn render_status_bar(f: &mut Frame, area: Rect, app: &App) {
    let ctx_pct = if app.provider_caps.max_context > 0 {
        (app.total_tokens as f64 / app.provider_caps.max_context as f64 * 100.0) as u32
    } else {
        0
    };

    // Animated spinner using braille dots, cycles every ~500ms (10 ticks at 50ms each)
    const SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    let is_active = app.is_streaming.load(std::sync::atomic::Ordering::SeqCst);
    let spinner = if is_active {
        SPINNER[(app.tick_count as usize / 2) % SPINNER.len()]
    } else {
        '●'
    };

    // Build status detail text
    let detail = if let Some(ref d) = app.status_detail {
        if is_active {
            // Truncate long tool names for the status bar
            let truncated: String = d.chars().take(20).collect();
            format!(" {} ", truncated)
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    let cache_pct = if app.cache_read_tokens + app.cache_write_tokens > 0 {
        app.cache_read_tokens as f64 / (app.cache_read_tokens + app.cache_write_tokens) as f64 * 100.0
    } else {
        0.0
    };
    let cache_display = if app.cache_read_tokens > 0 {
        format!(" │ cache:{:.0}%", cache_pct)
    } else {
        String::new()
    };

    // Token rate display
    let rate_display = if is_active && app.token_rate > 0.0 {
        format!(" │ {:.1}t/s", app.token_rate)
    } else {
        String::new()
    };

    // Progress bar for streaming
    let progress_bar = if is_active {
        let progress_chars = ['⣾', '⣽', '⣻', '⢿', '⡿', '⣟', '⣯', '⣷'];
        let progress_char = progress_chars[(app.tick_count as usize / 3) % progress_chars.len()];
        format!(" {} ", progress_char)
    } else {
        String::new()
    };

    let git_display = app.git_status.as_ref()
        .map(|g| format!(" │ {}", g.display()))
        .unwrap_or_default();

    let spans = vec![
        Span::styled(
            format!(" {} ", spinner),
            Style::default().bg(ACCENT).fg(Color::Black),
        ),
        Span::styled(
            format!(" Maix-Agent │ {} │ 上下文:{}% │ token:{}{}{} │ ¥{:.4} │ {}{}{} ",
                app.model_name, ctx_pct, format_tokens(app.total_tokens), cache_display, rate_display, app.total_cost, mode_name(app.mode),
                if app.vim.enabled { format!(" │ VIM:{}", app.vim.mode) } else { String::new() },
                git_display),
            Style::default().bg(ACCENT).fg(Color::Black),
        ),
    ];

    let mut all_spans = spans;
    if !progress_bar.is_empty() {
        all_spans.push(Span::styled(
            progress_bar,
            Style::default().fg(Color::Green),
        ));
    }
    if !detail.is_empty() {
        all_spans.push(Span::styled(
            format!("│{}", detail),
            Style::default().bg(Color::DarkGray).fg(Color::White),
        ));
    }

    let status = Paragraph::new(Line::from(all_spans));
    f.render_widget(status, area);
}

fn render_main_area(f: &mut Frame, area: Rect, app: &App) {
    if app.fullscreen {
        // Fullscreen: only chat panel
        render_chat(f, area, app);
    } else {
        // Main split: chat panel + side panel
        let panel_pct = app.panel_width;
        let chat_pct = 100 - panel_pct;
        let h_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(chat_pct), Constraint::Percentage(panel_pct)])
            .split(area);

        render_chat(f, h_chunks[0], app);
        render_side_panel(f, h_chunks[1], app);
    }
}

fn render_chat(f: &mut Frame, area: Rect, app: &App) {
    // Split area for chat and mini map (if enough space)
    let show_minimap = area.width > 80 && app.messages.len() > 20;
    let (chat_area, minimap_area) = if show_minimap {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(40), Constraint::Length(12)])
            .split(area);
        (chunks[0], Some(chunks[1]))
    } else {
        (area, None)
    };

    let mut all_lines: Vec<Line> = Vec::new();

    for (msg_idx, msg) in app.messages.iter().enumerate() {
        // 099-007: Check if this message is focused
        let is_focused = app.focused_message == Some(msg_idx);
        let focus_prefix = if is_focused { "▶ " } else { "  " };
        let focus_color = if is_focused { Color::Yellow } else { DIM };

        // Show divider before user messages (except first)
        if app.show_dividers && matches!(msg, ChatMessage::User(_)) && msg_idx > 0 {
            all_lines.push(Line::from(vec![
                Span::styled("─".repeat(50), Style::default().fg(DIM)),
            ]));
        }

        // 099-007: Focused message header
        if is_focused {
            all_lines.push(Line::from(vec![
                Span::styled("┌─── 焦点消息 ───", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            ]));
        }

        match msg {
            ChatMessage::User(text) => {
                if app.show_timestamps {
                    all_lines.push(Line::from(vec![
                        Span::styled(focus_prefix, Style::default().fg(focus_color)),
                        Span::styled(format!("[M{}] ", msg_idx + 1), Style::default().fg(DIM)),
                        Span::styled(chrono::Local::now().format("[%H:%M:%S] ").to_string(), Style::default().fg(DIM)),
                    ]));
                } else {
                    all_lines.push(Line::from(vec![
                        Span::styled(focus_prefix, Style::default().fg(focus_color)),
                        Span::styled(format!("[M{}] ", msg_idx + 1), Style::default().fg(DIM)),
                    ]));
                }
                // User message with icon and accent border
                let border_char = if is_focused { "┃ " } else { "│ " };
                all_lines.push(Line::from(vec![
                    Span::styled(focus_prefix, Style::default().fg(focus_color)),
                    Span::styled("  ", Style::default()),
                    Span::styled("▸ ", Style::default().fg(app.theme.user_msg).add_modifier(Modifier::BOLD)),
                    Span::styled("You", Style::default().fg(app.theme.user_msg).add_modifier(Modifier::BOLD)),
                ]));
                for line in text.lines() {
                    let mut spans = vec![
                        Span::styled(focus_prefix, Style::default().fg(focus_color)),
                        Span::styled(border_char, Style::default().fg(if is_focused { Color::Yellow } else { app.theme.user_msg })),
                    ];
                    // 100-002: Highlight search matches
                    if app.search_mode && !app.search_query.is_empty() {
                        spans.extend(highlight_search_matches(line, &app.search_query));
                    } else {
                        spans.push(Span::styled(line, Style::default().fg(app.theme.user_msg)));
                    }
                    all_lines.push(Line::from(spans));
                }
            }
            ChatMessage::Assistant(text) => {
                if app.show_timestamps {
                    all_lines.push(Line::from(vec![
                        Span::styled(focus_prefix, Style::default().fg(focus_color)),
                        Span::styled(chrono::Local::now().format("[%H:%M:%S] ").to_string(), Style::default().fg(DIM)),
                    ]));
                }
                // Assistant message with icon
                let _border_char = if is_focused { "┃ " } else { "  " };
                all_lines.push(Line::from(vec![
                    Span::styled(focus_prefix, Style::default().fg(focus_color)),
                    Span::styled("  ", Style::default()),
                    Span::styled("◆ ", Style::default().fg(app.theme.accent).add_modifier(Modifier::BOLD)),
                    Span::styled("Maix", Style::default().fg(app.theme.accent).add_modifier(Modifier::BOLD)),
                ]));
                // Fold long messages (> 50 lines)
                let line_count = text.lines().count();
                if line_count > 50 && app.folded_messages.contains(&msg_idx) {
                    let preview: String = text.lines().take(5).collect::<Vec<_>>().join("\n");
                    all_lines.extend(render_markdown(&preview));
                    all_lines.push(Line::from(vec![
                        Span::styled(format!("  [展开 {} 行]", line_count - 5), Style::default().fg(ACCENT)),
                    ]));
                } else {
                    all_lines.extend(render_markdown(text));
                }
            }
            ChatMessage::Reasoning(text) => {
                if app.show_reasoning {
                    for line in text.lines() {
                        all_lines.push(Line::from(vec![
                            Span::styled(focus_prefix, Style::default().fg(focus_color)),
                            Span::styled("  ", Style::default()),
                            Span::styled(line, Style::default().fg(DIM).add_modifier(Modifier::ITALIC)),
                        ]));
                    }
                } else {
                    let char_count = text.chars().count();
                    all_lines.push(Line::from(vec![
                        Span::styled(focus_prefix, Style::default().fg(focus_color)),
                        Span::styled("  ", Style::default()),
                        Span::styled(
                            format!("推理过程 ({} 字符) [R 展开]", char_count),
                            Style::default().fg(DIM),
                        ),
                    ]));
                }
            }
            ChatMessage::ToolCall { name, args } => {
                // 100-007: Expandable tool calls
                let is_expanded = app.expanded_tool_calls.contains(&msg_idx);
                let expand_icon = if is_expanded { "▼" } else { "▶" };
                let short_args = truncate_str(args, 80);
                all_lines.push(Line::from(vec![
                    Span::styled(focus_prefix, Style::default().fg(focus_color)),
                    Span::styled("  ", Style::default()),
                    Span::styled(format!("{} ", expand_icon), Style::default().fg(ACCENT)),
                    Span::styled("⚙ ", Style::default().fg(WARN)),
                    Span::styled("tool:", Style::default().fg(WARN)),
                    Span::styled(name.as_str(), Style::default().fg(WARN).add_modifier(Modifier::BOLD)),
                    Span::styled(format!("({})", short_args), Style::default().fg(WARN)),
                ]));
                if is_expanded && args.len() > 80 {
                    for line in args.lines() {
                        all_lines.push(Line::from(vec![
                            Span::styled(focus_prefix, Style::default().fg(focus_color)),
                            Span::styled("      │ ", Style::default().fg(DIM)),
                            Span::styled(line, Style::default().fg(DIM)),
                        ]));
                    }
                }
            }
            ChatMessage::ToolResult { result } => {
                // 100-007: Expandable tool results
                let is_expanded = app.expanded_tool_calls.contains(&msg_idx);
                let has_error = result.to_lowercase().contains("error") || result.to_lowercase().contains("failed");
                let icon = if has_error { "✗" } else { "✓" };
                let color = if has_error { app.theme.error_msg } else { app.theme.success_msg };
                if result.len() > 100 && !is_expanded {
                    let short = truncate_str(result, 100);
                    all_lines.push(Line::from(vec![
                        Span::styled(focus_prefix, Style::default().fg(focus_color)),
                        Span::styled("  ", Style::default()),
                        Span::styled(format!("{} ", icon), Style::default().fg(color)),
                        Span::styled(short, Style::default().fg(DIM)),
                        Span::styled(" [Enter 展开]", Style::default().fg(ACCENT)),
                    ]));
                } else {
                    all_lines.push(Line::from(vec![
                        Span::styled(focus_prefix, Style::default().fg(focus_color)),
                        Span::styled("  ", Style::default()),
                        Span::styled(format!("{} ", icon), Style::default().fg(color)),
                    ]));
                    for line in result.lines() {
                        all_lines.push(Line::from(vec![
                            Span::styled(focus_prefix, Style::default().fg(focus_color)),
                            Span::styled("    ", Style::default()),
                            Span::styled(line, Style::default().fg(DIM)),
                        ]));
                    }
                }
            }
            ChatMessage::System(text) => {
                all_lines.push(Line::from(vec![
                    Span::styled(focus_prefix, Style::default().fg(focus_color)),
                    Span::styled("  ", Style::default()),
                    Span::styled("ℹ ", Style::default().fg(DIM)),
                    Span::styled(text, Style::default().fg(DIM)),
                ]));
            }
            ChatMessage::Timestamped { time, inner } => {
                all_lines.push(Line::from(vec![
                    Span::styled(focus_prefix, Style::default().fg(focus_color)),
                    Span::styled(format!("[{}] ", time), Style::default().fg(DIM)),
                ]));
                // Render inner message recursively (simplified)
                match inner.as_ref() {
                    ChatMessage::User(text) => {
                        for line in text.lines() {
                            all_lines.push(Line::from(vec![
                                Span::styled(focus_prefix, Style::default().fg(focus_color)),
                                Span::styled("> ", Style::default().fg(ACCENT)),
                                Span::raw(line),
                            ]));
                        }
                    }
                    ChatMessage::Assistant(text) => {
                        all_lines.extend(render_markdown(text));
                    }
                    _ => {}
                }
            }
        }

        // 099-007: Focused message footer
        if is_focused {
            all_lines.push(Line::from(vec![
                Span::styled("└───────────────", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            ]));
        }
    }

    // Virtual scrolling: only render visible lines
    let visible_height = (chat_area.height as usize).saturating_sub(2);
    let total_lines = all_lines.len();
    let scroll = app.chat_scroll.min(total_lines.saturating_sub(visible_height));
    let start = total_lines.saturating_sub(visible_height + scroll);
    let end = total_lines.saturating_sub(scroll);

    // Use slice instead of clone for better performance
    let lines: Vec<Line> = if start < end && start < total_lines {
        all_lines[start..end].to_vec()
    } else {
        Vec::new()
    };

    let title = if app.chat_scroll > 0 {
        format!(" 对话 [↑{}行] ", app.chat_scroll)
    } else {
        " 对话 ".to_string()
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL);
    let paragraph = Paragraph::new(Text::from(lines))
        .block(block)
        .wrap(Wrap { trim: true });
    f.render_widget(paragraph, chat_area);

    // Render mini map if enabled
    if let Some(minimap_area) = minimap_area {
        render_minimap(f, minimap_area, app, total_lines, start, end);
    }
}

fn render_minimap(f: &mut Frame, area: Rect, app: &App, total_lines: usize, visible_start: usize, visible_end: usize) {
    let height = area.height as usize;
    if height == 0 || total_lines == 0 {
        return;
    }

    let mut minimap_lines = Vec::new();

    // Title
    minimap_lines.push(Line::from(vec![
        Span::styled("地图", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
    ]));

    // Calculate scale
    let scale = total_lines as f64 / height as f64;

    // Render minimap dots
    for i in 0..height.saturating_sub(1) {
        let line_idx = (i as f64 * scale) as usize;
        let is_visible = line_idx >= visible_start && line_idx < visible_end;

        // Determine message type at this position
        let msg_type = app.messages.iter().enumerate().find_map(|(idx, msg)| {
            let msg_start = app.messages.iter().take(idx).map(|m| match m {
                ChatMessage::User(t) => t.lines().count() + 1,
                ChatMessage::Assistant(t) => t.lines().count() + 2,
                _ => 1,
            }).sum::<usize>();

            let msg_lines = match msg {
                ChatMessage::User(t) => t.lines().count() + 1,
                ChatMessage::Assistant(t) => t.lines().count() + 2,
                _ => 1,
            };
            if line_idx >= msg_start && line_idx < msg_start + msg_lines {
                Some(msg)
            } else {
                None
            }
        });

        let dot = if is_visible {
            "█"
        } else {
            "░"
        };

        let color = match msg_type {
            Some(ChatMessage::User(_)) => app.theme.user_msg,
            Some(ChatMessage::Assistant(_)) => app.theme.accent,
            Some(ChatMessage::ToolCall { .. }) => WARN,
            Some(ChatMessage::ToolResult { .. }) => DIM,
            _ => DIM,
        };

        minimap_lines.push(Line::from(vec![
            Span::styled(dot, Style::default().fg(color)),
        ]));
    }

    let minimap_block = Block::default()
        .borders(Borders::ALL)
        .title("缩略");
    let minimap_paragraph = Paragraph::new(minimap_lines)
        .block(minimap_block);
    f.render_widget(minimap_paragraph, area);
}

fn render_side_panel(f: &mut Frame, area: Rect, app: &App) {
    let v_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(1)])
        .split(area);

    // Tabs
    let tabs = Tabs::new(vec!["记忆", "工具", "统计", "工作台"])
        .select(match app.active_panel {
            ActivePanel::Memory => 0,
            ActivePanel::Tools => 1,
            ActivePanel::Stats => 2,
            ActivePanel::Desk => 3,
        })
        .style(Style::default().fg(Color::White))
        .highlight_style(Style::default().fg(ACCENT))
        .block(Block::default().borders(Borders::BOTTOM));
    f.render_widget(tabs, v_chunks[0]);

    match app.active_panel {
        ActivePanel::Memory => render_memory_panel(f, v_chunks[1], app),
        ActivePanel::Tools => render_tools_panel(f, v_chunks[1], app),
        ActivePanel::Stats => render_stats_panel(f, v_chunks[1], app),
        ActivePanel::Desk => render_desk_panel(f, v_chunks[1], app),
    }
}

fn render_memory_panel(f: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .memories
        .iter()
        .map(|m| {
            let truncated: String = m.content.chars().take(60).collect();
            let display = format!(" kind={} {}", m.kind, truncated);
            ListItem::new(display)
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL))
        .highlight_style(Style::default().bg(ACCENT).fg(Color::Black))
        .highlight_symbol("> ");

    let mut state = ListState::default();
    if app.active_panel == ActivePanel::Memory {
        state.select(app.selected_index);
    }
    f.render_stateful_widget(list, area, &mut state);
}

fn render_tools_panel(f: &mut Frame, area: Rect, app: &App) {
    // Split area for list and tooltip
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(4)])
        .split(area);

    let items: Vec<ListItem> = app
        .tool_defs
        .iter()
        .map(|t| {
            let risk_icon = match t.risk_level {
                0 => " ",
                1 => " ",
                2 => " ",
                _ => " ",
            };
            ListItem::new(format!(
                "{} {} risk={} {}",
                risk_icon,
                t.name,
                t.risk_level,
                truncate_str(&t.description, 30)
            ))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("工具"))
        .highlight_style(Style::default().bg(ACCENT).fg(Color::Black))
        .highlight_symbol("> ");

    let mut state = ListState::default();
    if app.active_panel == ActivePanel::Tools {
        state.select(app.selected_index);
    }
    f.render_stateful_widget(list, chunks[0], &mut state);

    // Show tooltip for selected tool
    if let Some(idx) = app.selected_index {
        if let Some(tool) = app.tool_defs.get(idx) {
            let risk_text = match tool.risk_level {
                0 => "无风险",
                1 => "低风险",
                2 => "中风险",
                _ => "高风险",
            };
            let tooltip_lines = vec![
                Line::from(vec![
                    Span::styled(format!("工具: {}", tool.name), Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
                ]),
                Line::from(vec![
                    Span::styled(format!("风险: {}", risk_text), Style::default().fg(WARN)),
                ]),
                Line::from(vec![
                    Span::raw(truncate_str(&tool.description, 60)),
                ]),
            ];
            let tooltip = Paragraph::new(tooltip_lines)
                .block(Block::default().borders(Borders::ALL).title("详情"))
                .wrap(Wrap { trim: true });
            f.render_widget(tooltip, chunks[1]);
        }
    }
}

fn render_desk_panel(f: &mut Frame, area: Rect, app: &App) {
    let desk = &app.desk;
    let mut lines: Vec<Line> = Vec::new();

    // Sticky Notes
    if !desk.sticky_notes.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("  便签", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        ]));
        for note in &desk.sticky_notes {
            let pin = if note.pinned { " [固定]" } else { "" };
            let color = match note.color {
                crate::desk::NoteColor::Yellow => Color::Yellow,
                crate::desk::NoteColor::Blue => Color::Blue,
                crate::desk::NoteColor::Green => Color::Green,
                crate::desk::NoteColor::Red => Color::Red,
                crate::desk::NoteColor::Purple => Color::Magenta,
            };
            lines.push(Line::from(vec![
                Span::styled(format!("  {} ", note.id), Style::default().fg(DIM)),
                Span::styled(truncate_str(&note.content, 30), Style::default().fg(color)),
                Span::styled(pin, Style::default().fg(WARN)),
            ]));
        }
        lines.push(Line::from(""));
    }

    // Pinned Files
    if !desk.pinned_files.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("  固定文件", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        ]));
        for pf in &desk.pinned_files {
            let dirty = if pf.dirty { " *" } else { "" };
            lines.push(Line::from(vec![
                Span::styled(format!("  {} ", pf.path.file_name().unwrap_or_default().to_string_lossy()), Style::default()),
                Span::styled(format!("({}行{})", pf.line_count, dirty), Style::default().fg(DIM)),
            ]));
        }
        lines.push(Line::from(""));
    }

    // Task Board
    if !desk.task_board.tasks.is_empty() {
        let done = desk.task_board.done_count();
        let total = desk.task_board.tasks.len();
        lines.push(Line::from(vec![
            Span::styled(format!("  任务 ({}/{})", done, total), Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        ]));
        for task in &desk.task_board.tasks {
            let (checkbox, color) = match task.status {
                crate::desk::TaskStatus::Todo => ("☐", DIM),
                crate::desk::TaskStatus::InProgress => ("~", WARN),
                crate::desk::TaskStatus::Done => ("☑", Color::Green),
            };
            lines.push(Line::from(vec![
                Span::styled(format!("  {} ", checkbox), Style::default().fg(color)),
                Span::raw(truncate_str(&task.title, 30)),
            ]));
        }
    }

    if lines.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("  空工作台", Style::default().fg(DIM)),
        ]));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  /note add <内容>", Style::default().fg(ACCENT)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  /pin <文件>", Style::default().fg(ACCENT)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  /task_add <标题>", Style::default().fg(ACCENT)),
        ]));
    }

    let paragraph = Paragraph::new(Text::from(lines))
        .block(Block::default().borders(Borders::ALL).title(" 工作台 "));
    f.render_widget(paragraph, area);
}

fn render_stats_panel(f: &mut Frame, area: Rect, app: &App) {
    let ctx_pct = if app.provider_caps.max_context > 0 {
        (app.total_tokens as f64 / app.provider_caps.max_context as f64 * 100.0) as u32
    } else {
        0
    };
    let avg_cost = if app.round_count > 0 {
        app.total_cost / app.round_count as f64
    } else {
        0.0
    };

    let cache_pct = if app.cache_read_tokens + app.cache_write_tokens > 0 {
        app.cache_read_tokens as f64 / (app.cache_read_tokens + app.cache_write_tokens) as f64 * 100.0
    } else {
        0.0
    };

    // Generate token usage bar chart
    let bar_width: usize = 20;
    let max_tokens = app.prompt_tokens.max(app.completion_tokens).max(app.cache_read_tokens).max(1);
    let prompt_bar_len = ((app.prompt_tokens as f64 / max_tokens as f64) * bar_width as f64) as usize;
    let completion_bar_len = ((app.completion_tokens as f64 / max_tokens as f64) * bar_width as f64) as usize;
    let cache_bar_len = ((app.cache_read_tokens as f64 / max_tokens as f64) * bar_width as f64) as usize;

    let prompt_bar = format!("{}{}", "█".repeat(prompt_bar_len), "░".repeat(bar_width.saturating_sub(prompt_bar_len)));
    let completion_bar = format!("{}{}", "█".repeat(completion_bar_len), "░".repeat(bar_width.saturating_sub(completion_bar_len)));
    let cache_bar = format!("{}{}", "█".repeat(cache_bar_len), "░".repeat(bar_width.saturating_sub(cache_bar_len)));

    let lines = vec![
        Line::from(vec![Span::styled(" 会话", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))]),
        Line::from(vec![
            Span::styled("  ID: ", Style::default().fg(DIM)),
            Span::raw(&app.session_id[..8.min(app.session_id.len())]),
        ]),
        Line::from(vec![
            Span::styled("  轮次: ", Style::default().fg(DIM)),
            Span::raw(format!("{}", app.round_count)),
        ]),
        Line::from(vec![
            Span::styled("  消息: ", Style::default().fg(DIM)),
            Span::raw(format!("{}", app.messages.len())),
        ]),
        Line::from(vec![]),
        Line::from(vec![Span::styled(" Token 分布", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))]),
        Line::from(vec![
            Span::styled("  输入: ", Style::default().fg(DIM)),
            Span::styled(prompt_bar, Style::default().fg(Color::Green)),
            Span::styled(format!(" {}", format_tokens(app.prompt_tokens)), Style::default()),
        ]),
        Line::from(vec![
            Span::styled("  输出: ", Style::default().fg(DIM)),
            Span::styled(completion_bar, Style::default().fg(Color::Blue)),
            Span::styled(format!(" {}", format_tokens(app.completion_tokens)), Style::default()),
        ]),
        Line::from(vec![
            Span::styled("  缓存: ", Style::default().fg(DIM)),
            Span::styled(cache_bar, Style::default().fg(Color::Yellow)),
            Span::styled(format!(" {}", format_tokens(app.cache_read_tokens)), Style::default()),
        ]),
        Line::from(vec![]),
        Line::from(vec![Span::styled(" 上下文", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))]),
        Line::from(vec![
            Span::styled("  用量: ", Style::default().fg(DIM)),
            Span::raw(format!("{}%", ctx_pct)),
        ]),
        Line::from(vec![
            Span::styled("  上限: ", Style::default().fg(DIM)),
            Span::raw(format_tokens(app.provider_caps.max_context)),
        ]),
        Line::from(vec![
            Span::styled("  缓存命中: ", Style::default().fg(DIM)),
            Span::raw(format!("{:.1}%", cache_pct)),
        ]),
        Line::from(vec![]),
        Line::from(vec![Span::styled(" 费用", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))]),
        Line::from(vec![
            Span::styled("  总计: ", Style::default().fg(DIM)),
            Span::styled(format!("¥{:.4}", app.total_cost), Style::default().fg(Color::Yellow)),
        ]),
        Line::from(vec![
            Span::styled("  平均/轮: ", Style::default().fg(DIM)),
            Span::raw(format!("¥{:.6}", avg_cost)),
        ]),
        Line::from(vec![]),
        Line::from(vec![Span::styled(" 环境", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))]),
        Line::from(vec![
            Span::styled("  模型: ", Style::default().fg(DIM)),
            Span::raw(&app.model_name),
        ]),
        Line::from(vec![
            Span::styled("  服务: ", Style::default().fg(DIM)),
            Span::raw(&app.server_addr),
        ]),
        Line::from(vec![
            Span::styled("  模式: ", Style::default().fg(DIM)),
            Span::raw(mode_name(app.mode)),
        ]),
        Line::from(vec![]),
        Line::from(vec![Span::styled(" 内存", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))]),
        Line::from(vec![
            Span::styled("  消息数: ", Style::default().fg(DIM)),
            Span::raw(format!("{}", app.messages.len())),
        ]),
        Line::from(vec![
            Span::styled("  会话数: ", Style::default().fg(DIM)),
            Span::raw(format!("{}", app.sessions.len())),
        ]),
        Line::from(vec![
            Span::styled("  记忆数: ", Style::default().fg(DIM)),
            Span::raw(format!("{}", app.memories.len())),
        ]),
        Line::from(vec![
            Span::styled("  工具数: ", Style::default().fg(DIM)),
            Span::raw(format!("{}", app.tool_defs.len())),
        ]),
        Line::from(vec![
            Span::styled("  补全项: ", Style::default().fg(DIM)),
            Span::raw(format!("{}", app.input.completions.len())),
        ]),
    ];

    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(" 统计 "));
    f.render_widget(paragraph, area);
}

fn render_input(f: &mut Frame, area: Rect, app: &App) {
    let has_completions = !app.input.completions.is_empty();

    // When completions are shown, split area: completions on top, input on bottom
    let input_lines = app.input.line_count().max(1) as u16;
    let input_box_height = (input_lines + 2).min(8); // +2 for borders, max 8 lines
    let (completions_area, input_area) = if has_completions {
        let completions_count = app.input.completions.len().min(6) as u16;
        let completions_height = completions_count + 2; // +2 for borders
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(completions_height),
                Constraint::Length(input_box_height),
            ])
            .split(area);
        (Some(chunks[0]), chunks[1])
    } else {
        (None, area)
    };

    // Render completions dropdown above input with scrolling
    if let Some(comp_area) = completions_area {
        let visible_height = 6;  // Max visible completions
        let offset = app.input.completion_offset;
        let total = app.input.completions.len();

        // Only show visible portion of completions
        let visible_completions: Vec<ListItem> = app.input.completions
            .iter()
            .enumerate()
            .skip(offset)
            .take(visible_height)
            .map(|(i, cmd)| {
                let style = if i == app.input.completion_index {
                    Style::default().bg(ACCENT).fg(Color::Black)
                } else {
                    Style::default()
                };
                // Show name and description
                let text = format!("{:<15} {}", cmd.name, cmd.description);
                ListItem::new(text).style(style)
            })
            .collect();

        // Show scroll indicators in title
        let scroll_indicator = if offset > 0 && offset + visible_height < total {
            " ▲▼ "
        } else if offset > 0 {
            " ▲ "
        } else if offset + visible_height < total {
            " ▼ "
        } else {
            " "
        };
        let title = format!("{}补全 ({}/{})", scroll_indicator, app.input.completion_index + 1, total);
        let list = List::new(visible_completions)
            .block(Block::default().borders(Borders::ALL).title(title));

        f.render_widget(list, comp_area);
    }

    // Build focus hint with context suggestions when input is empty
    let context_hint;
    let focus_hint = if app.selected_index.is_some() {
        "↑↓ 导航 │ Enter 选择 │ Esc 取消"
    } else if has_completions {
        "↑↓ 选择 │ Tab 循环 │ Enter 确认 │ Esc 取消"
    } else if app.input.buffer.is_empty() {
        let ctx = app.get_context_suggestions();
        let time = app.get_smart_history_suggestions();
        let mut all: Vec<String> = ctx.into_iter().chain(time).collect();
        all.dedup();
        if all.is_empty() {
            "/help 帮助 │ Shift+Enter 换行"
        } else {
            context_hint = all.join(" │ ");
            context_hint.as_str()
        }
    } else {
        "/help 帮助 │ Shift+Enter 换行"
    };
    let block = Block::default()
        .title(format!(
            " 输入 │ {} │ Ctrl+Q 退出 │ {focus_hint} ",
            mode_name(app.mode),
        ))
        .borders(Borders::ALL)
        .border_style(if app.is_streaming.load(std::sync::atomic::Ordering::SeqCst) {
            Style::default().fg(WARN)
        } else {
            Style::default().fg(ACCENT)
        });

    let cursor_pos = app.input.cursor.min(app.input.buffer.len());

    // Build lines from buffer, handling newlines with line numbers
    let mut input_lines: Vec<Line> = Vec::new();
    let lines: Vec<&str> = app.input.buffer.split('\n').collect();
    let total_lines = lines.len();
    let line_num_width = if total_lines > 9 { 2 } else { 1 };
    let mut char_offset = 0;

    for (line_idx, line_text) in lines.iter().enumerate() {
        let line_start = char_offset;
        let line_end = char_offset + line_text.len();

        // Line number prefix for multi-line input
        let line_num = if total_lines > 1 {
            let num = format!("{:>width$}| ", line_idx + 1, width = line_num_width);
            if line_idx + 1 == total_lines {
                // Current line highlighted
                Span::styled(num, Style::default().fg(ACCENT))
            } else {
                Span::styled(num, Style::default().fg(DIM))
            }
        } else {
            Span::raw("> ")
        };

        // Check if cursor is in this line
        if cursor_pos >= line_start && cursor_pos <= line_end {
            let cursor_in_line = cursor_pos - line_start;
            let before = &line_text[..cursor_in_line];
            let cursor_char = line_text[cursor_in_line..]
                .chars()
                .next()
                .unwrap_or(' ');
            let after_start = cursor_in_line + cursor_char.len_utf8();
            let after = if after_start <= line_text.len() {
                &line_text[after_start..]
            } else {
                ""
            };

            let prefix = if line_idx == 0 && total_lines == 1 { Span::raw("> ") } else { line_num };
            input_lines.push(Line::from(vec![
                prefix,
                Span::raw(before),
                Span::styled(
                    cursor_char.to_string(),
                    Style::default().bg(Color::White).fg(Color::Black),
                ),
                Span::raw(after),
            ]));
        } else {
            let prefix = if line_idx == 0 && total_lines == 1 { Span::raw("> ") } else { line_num };
            input_lines.push(Line::from(vec![
                prefix,
                Span::raw(*line_text),
            ]));
        }

        char_offset = line_end + 1; // +1 for the newline character
    }

    // If buffer is empty, show cursor
    if app.input.buffer.is_empty() {
        input_lines.push(Line::from(vec![
            Span::raw("> "),
            Span::styled(" ", Style::default().bg(Color::White).fg(Color::Black)),
        ]));
    }

    f.render_widget(Paragraph::new(Text::from(input_lines)).block(block), input_area);
}

fn render_shortcut_bar(f: &mut Frame, area: Rect, app: &App) {
    if area.height < 1 {
        return;
    }
    let shortcuts = if !app.input.completions.is_empty() {
        "Tab/↑↓ 选择 │ Enter 确认 │ 1-9 直选 │ Esc 取消"
    } else if app.search_mode {
        "Enter 下一个 │ Esc 关闭搜索"
    } else if app.palette.is_visible() {
        "↑↓ 导航 │ Enter 执行 │ Esc 关闭"
    } else if app.is_streaming.load(std::sync::atomic::Ordering::SeqCst) {
        "Esc 中断 │ 输入可继续编辑"
    } else if app.selected_index.is_some() {
        "↑↓ 导航 │ Enter 选择 │ Esc 返回"
    } else {
        "/ 命令 │ Ctrl+P 面板 │ Ctrl+F 搜索 │ Ctrl+L 清屏"
    };

    let shortcut_area = Rect::new(area.x, area.y + area.height - 1, area.width, 1);
    let line = Line::from(vec![
        Span::styled(format!(" {} ", shortcuts), Style::default().fg(DIM)),
    ]);
    f.render_widget(Paragraph::new(line), shortcut_area);
}

fn render_palette(f: &mut Frame, area: Rect, app: &App) {
    let palette = &app.palette;

    // Center the palette on screen
    let width = 50.min(area.width - 4);
    let height = 15.min(area.height - 4);
    let x = (area.width - width) / 2;
    let y = (area.height - height) / 2;
    let palette_area = Rect::new(x, y, width, height);

    // Clear background
    let clear = Block::default()
        .style(Style::default().bg(Color::Black));
    f.render_widget(clear, palette_area);

    // Get filtered entries
    let entries = palette.filtered_entries();
    let selected = palette.selected_index();
    let max_visible = (height.saturating_sub(3)) as usize; // Subtract borders and query line

    let mut items: Vec<ListItem> = Vec::new();

    // Query line
    items.push(ListItem::new(Line::from(vec![
        Span::styled("> ", Style::default().fg(ACCENT)),
        Span::raw(palette.query()),
        Span::styled("█", Style::default().fg(Color::White)),
    ])));

    // Separator
    items.push(ListItem::new("─".repeat(width as usize - 2)));

    // Entries
    let start = if selected >= max_visible {
        selected - max_visible + 1
    } else {
        0
    };

    for (display_idx, entry) in entries.iter().skip(start).take(max_visible).enumerate() {
        let is_selected = start + display_idx == selected;
        let style = if is_selected {
            Style::default().bg(ACCENT).fg(Color::Black)
        } else {
            Style::default()
        };

        let icon = entry.category.icon();
        let line = Line::from(vec![
            Span::styled(format!(" {} ", icon), Style::default().fg(ACCENT)),
            Span::styled(&entry.label, style.add_modifier(Modifier::BOLD)),
            Span::raw(" "),
            Span::styled(&entry.description, Style::default().fg(DIM)),
        ]);
        items.push(ListItem::new(line).style(style));
    }

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" 命令面板 (Ctrl+P) ")
                .border_style(Style::default().fg(ACCENT))
        );

    f.render_widget(list, palette_area);
}

fn render_search(f: &mut Frame, area: Rect, app: &App) {
    let search = &app.search_query;
    let results = &app.search_results;
    let current = app.search_result_index;

    // Search bar at the top
    let search_area = Rect::new(area.x, area.y, area.width, 3);
    let result_text = if results.is_empty() {
        "无结果".to_string()
    } else {
        format!("{}/{}", current + 1, results.len())
    };

    let search_line = Line::from(vec![
        Span::styled(" 搜索: ", Style::default().fg(ACCENT)),
        Span::raw(search),
        Span::styled("█", Style::default().fg(Color::White)),
        Span::raw("  "),
        Span::styled(result_text, Style::default().fg(DIM)),
        Span::raw("  "),
        Span::styled("Enter 下一个 │ Esc 关闭", Style::default().fg(DIM)),
    ]);

    let search_block = Paragraph::new(search_line)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ACCENT))
        );

    f.render_widget(search_block, search_area);
}
