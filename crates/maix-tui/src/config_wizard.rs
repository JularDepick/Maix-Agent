//! Quick-config wizard for first-launch (double-click) setup.

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::Span,
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use std::path::PathBuf;

const ACCENT: Color = Color::Cyan;

pub struct ConfigWizard {
    pub provider: String,
    pub api_key: String,
    pub api_base: String,
    pub model: String,
    active_field: usize,
    done: bool,
}

impl ConfigWizard {
    pub fn new() -> Self {
        Self {
            provider: "deepseek".into(),
            api_key: String::new(),
            api_base: "https://api.deepseek.com".into(),
            model: "deepseek-chat".into(),
            active_field: 0,
            done: false,
        }
    }

    fn active_field_mut(&mut self) -> &mut String {
        match self.active_field {
            0 => &mut self.provider,
            1 => &mut self.api_key,
            2 => &mut self.api_base,
            3 => &mut self.model,
            _ => &mut self.provider,
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Tab | KeyCode::Down => {
                self.active_field = (self.active_field + 1) % 4;
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.active_field = if self.active_field == 0 { 3 } else { self.active_field - 1 };
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.active_field_mut().push(c);
            }
            KeyCode::Backspace => {
                self.active_field_mut().pop();
            }
            KeyCode::Enter
                if !self.api_key.is_empty() => {
                    self.done = true;
                }
            KeyCode::Esc => {
                self.done = true;
            }
            _ => {}
        }
    }

    pub fn run(&mut self, mut terminal: ratatui::Terminal<impl ratatui::backend::Backend>) -> std::io::Result<()> {
        loop {
            terminal.draw(|f| render_wizard(f, self))?;

            if self.done {
                break;
            }

            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    self.handle_key(key);
                }
            }
        }
        Ok(())
    }

    /// Save config to ~/.maix/config.toml
    pub fn save_config(&self) -> std::io::Result<PathBuf> {
        let home = std::env::var("MAIX_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let mut p = dirs_next();
                p.push(".maix");
                p
            });

        std::fs::create_dir_all(&home)?;
        let config_path = home.join("config.toml");

        let content = format!(
            r#"# Maix-Agent 配置文件（快速配置向导自动生成）
[providers.{provider}]
api_key = "{api_key}"
api_base = "{api_base}"
model = "{model}"

[agent]
max_tool_rounds = 16
context_threshold = 0.9
mode = "agent"

[memory]
dir = ""

[tools]
shell_enabled = false
"#,
            provider = self.provider,
            api_key = self.api_key,
            api_base = self.api_base,
            model = self.model,
        );

        std::fs::write(&config_path, content)?;
        Ok(config_path)
    }
}

fn dirs_next() -> PathBuf {
    if let Ok(home) = std::env::var("USERPROFILE") {
        PathBuf::from(home).join(".maix")
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".maix")
    } else {
        PathBuf::from(".").join(".maix")
    }
}

fn render_wizard(f: &mut Frame, wizard: &ConfigWizard) {
    let area = f.area();
    let centered = centered_rect(60, 14, area);

    let block = Block::default()
        .title(" Maix-Agent 快速配置 ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT));
    f.render_widget(block, centered);

    let inner = centered_inner(centered, 1);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(inner);

    render_field(f, chunks[0], "服务商", &wizard.provider, wizard.active_field == 0);
    render_field(f, chunks[1], "API Key", &masked_key(&wizard.api_key), wizard.active_field == 1);
    render_field(f, chunks[2], "API 地址", &wizard.api_base, wizard.active_field == 2);
    render_field(f, chunks[3], "模型", &wizard.model, wizard.active_field == 3);

    let hint = if wizard.active_field == 3 {
        "Enter 确认 | Esc 跳过"
    } else {
        "Tab/↓ 下一项 | Shift+Tab/↑ 上一项"
    };
    let hint_p = Paragraph::new(hint)
        .style(Style::default().fg(Color::DarkGray));
    f.render_widget(hint_p, chunks[4]);
}

fn render_field(f: &mut Frame, area: Rect, label: &str, value: &str, active: bool) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(12), Constraint::Min(1)])
        .split(area);

    let label_span = Span::styled(
        format!(" {label}: "),
        Style::default().fg(if active { ACCENT } else { Color::White }),
    );
    f.render_widget(Paragraph::new(label_span), chunks[0]);

    let value_style = if active {
        Style::default().fg(Color::White).bg(Color::DarkGray)
    } else {
        Style::default().fg(Color::Gray)
    };
    let display = if value.is_empty() && !active {
        "(未设置)".into()
    } else {
        format!("{value} ")
    };
    let value_p = Paragraph::new(display).style(value_style);
    f.render_widget(value_p, chunks[1]);
}

fn masked_key(key: &str) -> String {
    if key.len() <= 8 {
        return "***".to_string();
    }
    let prefix: String = key.chars().take(4).collect();
    let suffix: String = key.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
    format!("{prefix}****{suffix}")
}

fn centered_rect(percent_x: u16, height: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length((r.height.saturating_sub(height)) / 2),
            Constraint::Length(height),
            Constraint::Length((r.height.saturating_sub(height)) / 2),
        ])
        .split(r);

    let h_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1]);

    h_layout[1]
}

fn centered_inner(rect: Rect, margin: u16) -> Rect {
    Rect {
        x: rect.x + margin,
        y: rect.y + margin,
        width: rect.width.saturating_sub(margin * 2),
        height: rect.height.saturating_sub(margin * 2),
    }
}
