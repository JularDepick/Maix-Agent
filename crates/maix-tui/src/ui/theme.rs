//! UI theme colors and presets.

use ratatui::style::Color;

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
    #[allow(dead_code)]
    pub fn to_json(self) -> String {
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
pub(crate) fn parse_hex_color(s: &str) -> Option<Color> {
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
