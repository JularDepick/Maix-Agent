//! UI rendering with ratatui.

use crate::app::{mode_name, App, ActivePanel, ChatMessage};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Tabs, Wrap},
    Frame,
};

/// UI theme colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub accent: Color,
    pub dim: Color,
    pub warn: Color,
    pub bg: Color,
    pub fg: Color,
    pub border: Color,
    pub user_msg: Color,
    pub assistant_msg: Color,
    pub system_msg: Color,
    pub error_msg: Color,
    pub success_msg: Color,
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            accent: Color::Cyan,
            dim: Color::DarkGray,
            warn: Color::Yellow,
            bg: Color::Reset,
            fg: Color::White,
            border: Color::DarkGray,
            user_msg: Color::Green,
            assistant_msg: Color::White,
            system_msg: Color::DarkGray,
            error_msg: Color::Red,
            success_msg: Color::Green,
        }
    }

    pub fn light() -> Self {
        Self {
            accent: Color::Blue,
            dim: Color::Gray,
            warn: Color::Rgb(200, 150, 0),
            bg: Color::White,
            fg: Color::Black,
            border: Color::Gray,
            user_msg: Color::Rgb(0, 100, 0),
            assistant_msg: Color::Black,
            system_msg: Color::Gray,
            error_msg: Color::Red,
            success_msg: Color::Rgb(0, 150, 0),
        }
    }

    pub fn solarized_dark() -> Self {
        Self {
            accent: Color::Rgb(38, 139, 210),
            dim: Color::Rgb(88, 110, 117),
            warn: Color::Rgb(181, 137, 0),
            bg: Color::Rgb(0, 43, 54),
            fg: Color::Rgb(131, 148, 150),
            border: Color::Rgb(88, 110, 117),
            user_msg: Color::Rgb(42, 161, 152),
            assistant_msg: Color::Rgb(131, 148, 150),
            system_msg: Color::Rgb(88, 110, 117),
            error_msg: Color::Rgb(220, 50, 47),
            success_msg: Color::Rgb(42, 161, 152),
        }
    }

    pub fn dracula() -> Self {
        Self {
            accent: Color::Rgb(189, 147, 249),
            dim: Color::Rgb(98, 114, 164),
            warn: Color::Rgb(241, 250, 140),
            bg: Color::Rgb(40, 42, 54),
            fg: Color::Rgb(248, 248, 242),
            border: Color::Rgb(98, 114, 164),
            user_msg: Color::Rgb(80, 250, 123),
            assistant_msg: Color::Rgb(248, 248, 242),
            system_msg: Color::Rgb(98, 114, 164),
            error_msg: Color::Rgb(255, 85, 85),
            success_msg: Color::Rgb(80, 250, 123),
        }
    }

    pub fn high_contrast() -> Self {
        Self {
            accent: Color::Yellow,
            dim: Color::White,
            warn: Color::Rgb(255, 165, 0),
            bg: Color::Black,
            fg: Color::White,
            border: Color::White,
            user_msg: Color::Cyan,
            assistant_msg: Color::White,
            system_msg: Color::White,
            error_msg: Color::Red,
            success_msg: Color::Green,
        }
    }

    pub fn from_name(name: &str) -> Self {
        match name.to_lowercase().as_str() {
            "light" => Self::light(),
            "solarized" | "solarized-dark" => Self::solarized_dark(),
            "dracula" => Self::dracula(),
            "high-contrast" | "hc" => Self::high_contrast(),
            "custom" => Self::load_custom(),
            _ => Self::dark(),
        }
    }

    /// Load custom theme from configuration file.
    fn load_custom() -> Self {
        let home = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("."));
        let theme_path = home.join(".maix").join("theme.json");

        if let Ok(content) = std::fs::read_to_string(&theme_path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                let parse_color = |key: &str, default: Color| -> Color {
                    if let Some(val) = json.get(key).and_then(|v| v.as_str()) {
                        parse_hex_color(val).unwrap_or(default)
                    } else {
                        default
                    }
                };

                return Self {
                    accent: parse_color("accent", Color::Cyan),
                    dim: parse_color("dim", Color::DarkGray),
                    warn: parse_color("warn", Color::Yellow),
                    bg: parse_color("bg", Color::Reset),
                    fg: parse_color("fg", Color::White),
                    border: parse_color("border", Color::DarkGray),
                    user_msg: parse_color("user_msg", Color::Green),
                    assistant_msg: parse_color("assistant_msg", Color::White),
                    system_msg: parse_color("system_msg", Color::DarkGray),
                    error_msg: parse_color("error_msg", Color::Red),
                    success_msg: parse_color("success_msg", Color::Green),
                };
            }
        }

        // Fallback to dark theme if custom config not found
        Self::dark()
    }

    /// Export current theme to JSON string.
    pub fn to_json(&self) -> String {
        let color_to_hex = |c: &Color| -> String {
            match c {
                Color::Reset => "#000000".to_string(),
                Color::Black => "#000000".to_string(),
                Color::Red => "#ff0000".to_string(),
                Color::Green => "#00ff00".to_string(),
                Color::Yellow => "#ffff00".to_string(),
                Color::Blue => "#0000ff".to_string(),
                Color::Magenta => "#ff00ff".to_string(),
                Color::Cyan => "#00ffff".to_string(),
                Color::White => "#ffffff".to_string(),
                Color::DarkGray => "#555555".to_string(),
                Color::Gray => "#aaaaaa".to_string(),
                Color::Rgb(r, g, b) => format!("#{:02x}{:02x}{:02x}", r, g, b),
                _ => "#000000".to_string(),
            }
        };

        serde_json::json!({
            "accent": color_to_hex(&self.accent),
            "dim": color_to_hex(&self.dim),
            "warn": color_to_hex(&self.warn),
            "bg": color_to_hex(&self.bg),
            "fg": color_to_hex(&self.fg),
            "border": color_to_hex(&self.border),
            "user_msg": color_to_hex(&self.user_msg),
            "assistant_msg": color_to_hex(&self.assistant_msg),
            "system_msg": color_to_hex(&self.system_msg),
            "error_msg": color_to_hex(&self.error_msg),
            "success_msg": color_to_hex(&self.success_msg),
        }).to_string()
    }

    pub fn available_themes() -> Vec<&'static str> {
        vec!["dark", "light", "solarized", "dracula", "high-contrast", "custom"]
    }
}

/// Parse hex color string like "#ff0000" or "ff0000" to Color.
fn parse_hex_color(s: &str) -> Option<Color> {
    let s = s.trim().trim_start_matches('#');
    if s.len() == 6 {
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        Some(Color::Rgb(r, g, b))
    } else {
        None
    }
}

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

    let spans = vec![
        Span::styled(
            format!(" {} ", spinner),
            Style::default().bg(ACCENT).fg(Color::Black),
        ),
        Span::styled(
            format!(" Maix-Agent │ {} │ 上下文:{}% │ token:{}{}{} │ ¥{:.4} │ {}{} ",
                app.model_name, ctx_pct, format_tokens(app.total_tokens), cache_display, rate_display, app.total_cost, mode_name(app.mode),
                if app.vim.enabled { format!(" │ VIM:{}", app.vim.mode) } else { String::new() }),
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
    let visible_height = chat_area.height as usize - 2;
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

            if line_idx >= msg_start && line_idx < msg_start + 1 {
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
    let bar_width = 20;
    let max_tokens = app.prompt_tokens.max(app.completion_tokens).max(app.cache_read_tokens).max(1);
    let prompt_bar_len = ((app.prompt_tokens as f64 / max_tokens as f64) * bar_width as f64) as usize;
    let completion_bar_len = ((app.completion_tokens as f64 / max_tokens as f64) * bar_width as f64) as usize;
    let cache_bar_len = ((app.cache_read_tokens as f64 / max_tokens as f64) * bar_width as f64) as usize;

    let prompt_bar = format!("{}{}", "█".repeat(prompt_bar_len), "░".repeat(bar_width - prompt_bar_len));
    let completion_bar = format!("{}{}", "█".repeat(completion_bar_len), "░".repeat(bar_width - completion_bar_len));
    let cache_bar = format!("{}{}", "█".repeat(cache_bar_len), "░".repeat(bar_width - cache_bar_len));

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

    // Render input box
    let focus_hint = if app.selected_index.is_some() {
        "↑↓ 导航 │ Enter 选择 │ Esc 取消"
    } else if has_completions {
        "↑↓ 选择 │ Tab 循环 │ Enter 确认 │ Esc 取消"
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
    let max_visible = (height - 3) as usize; // Subtract borders and query line

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

/// Render markdown text with basic formatting.
/// Supports: headings (#), code blocks (```), inline code (`), bold (**), bullet lists (-/*)
fn render_markdown(text: &str) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut in_code_block = false;
    let mut code_lang = String::new();
    let mut _code_line_num = 0;
    let mut code_block_lines: Vec<String> = Vec::new();
    let max_code_lines = 30; // Fold code blocks longer than this

    for line in text.lines() {
        // Code block toggle
        if line.starts_with("```") {
            if in_code_block {
                in_code_block = false;
                // Render collected code block with folding
                if code_block_lines.len() > max_code_lines {
                    // Show first N lines
                    for (i, code_line) in code_block_lines.iter().take(max_code_lines).enumerate() {
                        let language = crate::highlight::Language::from_extension(&code_lang);
                        let highlighter = crate::highlight::SimpleHighlighter::new(language);
                        let tokens = highlighter.highlight_line(code_line);

                        let line_num_str = format!("{:>3}│ ", i + 1);
                        let mut spans = vec![Span::styled(line_num_str, Style::default().fg(DIM))];
                        for token in tokens {
                            let color = match token.kind {
                                crate::highlight::TokenKind::Keyword => Color::Blue,
                                crate::highlight::TokenKind::String => Color::Green,
                                crate::highlight::TokenKind::Comment => Color::DarkGray,
                                crate::highlight::TokenKind::Number => Color::Cyan,
                                crate::highlight::TokenKind::Function => Color::Yellow,
                                crate::highlight::TokenKind::Type => Color::Magenta,
                                crate::highlight::TokenKind::Operator => Color::White,
                                crate::highlight::TokenKind::Punctuation => Color::White,
                                crate::highlight::TokenKind::Plain => Color::Green,
                            };
                            spans.push(Span::styled(token.text, Style::default().fg(color)));
                        }
                        lines.push(Line::from(spans));
                    }
                    // Show fold indicator
                    let remaining = code_block_lines.len() - max_code_lines;
                    lines.push(Line::from(vec![
                        Span::styled(format!("  │ ... {} 行已折叠 (输入 'unfold' 展开)", remaining), Style::default().fg(ACCENT)),
                    ]));
                } else {
                    // Render all lines
                    for (i, code_line) in code_block_lines.iter().enumerate() {
                        let language = crate::highlight::Language::from_extension(&code_lang);
                        let highlighter = crate::highlight::SimpleHighlighter::new(language);
                        let tokens = highlighter.highlight_line(code_line);

                        let line_num_str = format!("{:>3}│ ", i + 1);
                        let mut spans = vec![Span::styled(line_num_str, Style::default().fg(DIM))];
                        for token in tokens {
                            let color = match token.kind {
                                crate::highlight::TokenKind::Keyword => Color::Blue,
                                crate::highlight::TokenKind::String => Color::Green,
                                crate::highlight::TokenKind::Comment => Color::DarkGray,
                                crate::highlight::TokenKind::Number => Color::Cyan,
                                crate::highlight::TokenKind::Function => Color::Yellow,
                                crate::highlight::TokenKind::Type => Color::Magenta,
                                crate::highlight::TokenKind::Operator => Color::White,
                                crate::highlight::TokenKind::Punctuation => Color::White,
                                crate::highlight::TokenKind::Plain => Color::Green,
                            };
                            spans.push(Span::styled(token.text, Style::default().fg(color)));
                        }
                        lines.push(Line::from(spans));
                    }
                }
                code_block_lines.clear();
                code_lang.clear();
                lines.push(Line::from(vec![
                    Span::styled("  └───────────────────────────", Style::default().fg(DIM)),
                ]));
            } else {
                in_code_block = true;
                code_lang = line.strip_prefix("```").unwrap_or("").trim().to_string();
                _code_line_num = 0;
                code_block_lines.clear();
                let lang_display = if code_lang.is_empty() {
                    "code".to_string()
                } else {
                    code_lang.clone()
                };
                lines.push(Line::from(vec![
                    Span::styled(format!("  ┌─ {} ─", lang_display), Style::default().fg(DIM)),
                    Span::styled("  [C 复制]", Style::default().fg(ACCENT)),
                ]));
            }
            continue;
        }

        if in_code_block {
            // Collect code block lines for folding
            code_block_lines.push(line.to_string());
            continue;
        }

        // Heading
        if let Some(text) = line.strip_prefix("# ") {
            lines.push(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(
                    text.to_string(),
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
            ]));
            continue;
        }
        if let Some(text) = line.strip_prefix("## ") {
            lines.push(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(
                    text.to_string(),
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
            ]));
            continue;
        }
        if let Some(text) = line.strip_prefix("### ") {
            lines.push(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(
                    text.to_string(),
                    Style::default().fg(WARN).add_modifier(Modifier::BOLD),
                ),
            ]));
            continue;
        }

        // Bullet list
        if line.starts_with("- ") || line.starts_with("* ") {
            let mut spans = vec![
                Span::styled("  ", Style::default()),
                Span::styled("* ", Style::default().fg(ACCENT)),
            ];
            spans.extend(parse_inline(&line[2..]));
            lines.push(Line::from(spans));
            continue;
        }

        // Task list
        if line.starts_with("- [ ] ") || line.starts_with("* [ ] ") {
            let mut spans = vec![
                Span::styled("  ", Style::default()),
                Span::styled("☐ ", Style::default().fg(DIM)),
            ];
            spans.extend(parse_inline(&line[6..]));
            lines.push(Line::from(spans));
            continue;
        }
        if line.starts_with("- [x] ") || line.starts_with("* [x] ") {
            let mut spans = vec![
                Span::styled("  ", Style::default()),
                Span::styled("☑ ", Style::default().fg(Color::Green)),
            ];
            spans.extend(parse_inline(&line[6..]));
            lines.push(Line::from(spans));
            continue;
        }

        // Table row (contains |)
        if line.contains('|') && line.trim().starts_with('|') {
            let cells: Vec<&str> = line.split('|').filter(|s| !s.trim().is_empty()).collect();
            if !cells.is_empty() {
                // Check if separator row (---|---|---)
                if cells.iter().all(|c| c.trim().chars().all(|ch| ch == '-' || ch == ' ' || ch == ':')) {
                    lines.push(Line::from(vec![
                        Span::styled("  ───────────────────────────", Style::default().fg(DIM)),
                    ]));
                    continue;
                }
                // Regular table row
                let mut spans = vec![Span::styled("  ", Style::default())];
                for (i, cell) in cells.iter().enumerate() {
                    if i > 0 {
                        spans.push(Span::styled(" │ ", Style::default().fg(DIM)));
                    }
                    spans.extend(parse_inline(cell.trim()));
                }
                lines.push(Line::from(spans));
                continue;
            }
        }

        // Blockquote
        if line.starts_with("> ") {
            let mut spans = vec![
                Span::styled("  ", Style::default()),
                Span::styled("│ ", Style::default().fg(ACCENT)),
            ];
            spans.extend(parse_inline(&line[2..]));
            lines.push(Line::from(spans));
            continue;
        }

        // Regular line with inline formatting
        let mut spans = vec![Span::styled("  ", Style::default())];
        spans.extend(parse_inline(line));
        lines.push(Line::from(spans));
    }

    lines
}

/// Smart wrap text preserving indentation.
#[allow(dead_code)]
fn smart_wrap_line(text: &str, max_width: usize) -> Vec<String> {
    if text.len() <= max_width {
        return vec![text.to_string()];
    }

    // Calculate indentation
    let indent = text.len() - text.trim_start().len();
    let indent_str = &text[..indent];
    let content = text.trim_start();

    let mut result = Vec::new();
    let mut current_line = String::new();
    let mut current_len = 0;

    for word in content.split_whitespace() {
        if current_len + word.len() + 1 > max_width - indent && !current_line.is_empty() {
            result.push(current_line);
            current_line = format!("{}{}", indent_str, word);
            current_len = indent + word.len();
        } else {
            if !current_line.is_empty() {
                current_line.push(' ');
                current_len += 1;
            }
            current_line.push_str(word);
            current_len += word.len();
        }
    }

    if !current_line.is_empty() {
        result.push(current_line);
    }

    result
}

/// Parse inline markdown: `code`, **bold**, *italic*, and plain text.
/// Highlight search matches in text (100-002).
fn highlight_search_matches(text: &str, query: &str) -> Vec<Span<'static>> {
    if query.is_empty() || text.is_empty() {
        return vec![Span::raw(text.to_string())];
    }

    let lower_text = text.to_lowercase();
    let lower_query = query.to_lowercase();
    let mut spans = Vec::new();
    let mut last_end = 0;

    while let Some(pos) = lower_text[last_end..].find(&lower_query) {
        let abs_pos = last_end + pos;
        // Add text before match
        if abs_pos > last_end {
            spans.push(Span::raw(text[last_end..abs_pos].to_string()));
        }
        // Add highlighted match
        let match_end = abs_pos + query.len();
        spans.push(Span::styled(
            text[abs_pos..match_end].to_string(),
            Style::default().bg(Color::Yellow).fg(Color::Black),
        ));
        last_end = match_end;
    }

    // Add remaining text
    if last_end < text.len() {
        spans.push(Span::raw(text[last_end..].to_string()));
    }

    if spans.is_empty() {
        vec![Span::raw(text.to_string())]
    } else {
        spans
    }
}

fn parse_inline(text: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut plain = String::new();

    while i < len {
        // Inline code: `...`
        if chars[i] == '`' {
            if !plain.is_empty() {
                spans.push(Span::raw(plain.clone()));
                plain.clear();
            }
            let end = chars[i + 1..].iter().position(|&c| c == '`').map(|p| p + i + 1);
            if let Some(end_idx) = end {
                let code: String = chars[i + 1..end_idx].iter().collect();
                spans.push(Span::styled(
                    format!(" {} ", code),
                    Style::default().bg(Color::DarkGray).fg(Color::White),
                ));
                i = end_idx + 1;
                continue;
            }
        }

        // Bold: **...**
        if i + 1 < len && chars[i] == '*' && chars[i + 1] == '*' {
            if !plain.is_empty() {
                spans.push(Span::raw(plain.clone()));
                plain.clear();
            }
            let remaining: String = chars[i + 2..].iter().collect();
            if let Some(end_pos) = remaining.find("**") {
                let bold_text: String = chars[i + 2..i + 2 + end_pos].iter().collect();
                spans.push(Span::styled(
                    bold_text,
                    Style::default().add_modifier(Modifier::BOLD),
                ));
                i = i + 2 + end_pos + 2;
                continue;
            }
        }

        // Italic: *...* (single asterisk, not at word boundary)
        if chars[i] == '*' && (i + 1 < len && chars[i + 1] != '*') {
            if !plain.is_empty() {
                spans.push(Span::raw(plain.clone()));
                plain.clear();
            }
            let remaining: String = chars[i + 1..].iter().collect();
            if let Some(end_pos) = remaining.find('*') {
                let italic_text: String = chars[i + 1..i + 1 + end_pos].iter().collect();
                spans.push(Span::styled(
                    italic_text,
                    Style::default().fg(Color::Yellow),
                ));
                i = i + 1 + end_pos + 1;
                continue;
            }
        }

        // Link: [text](url) - render as underlined text
        if chars[i] == '[' {
            if !plain.is_empty() {
                spans.push(Span::raw(plain.clone()));
                plain.clear();
            }
            let remaining: String = chars[i..].iter().collect();
            if let Some(bracket_end) = remaining.find(']') {
                if i + bracket_end + 1 < len && chars[i + bracket_end + 1] == '(' {
                    if let Some(paren_end) = remaining[bracket_end + 2..].find(')') {
                        let link_text: String = chars[i + 1..i + bracket_end].iter().collect();
                        spans.push(Span::styled(
                            link_text,
                            Style::default().fg(Color::Blue).add_modifier(Modifier::UNDERLINED),
                        ));
                        i = i + bracket_end + 2 + paren_end + 1;
                        continue;
                    }
                }
            }
        }

        plain.push(chars[i]);
        i += 1;
    }

    if !plain.is_empty() {
        spans.push(Span::raw(plain));
    }

    spans
}
