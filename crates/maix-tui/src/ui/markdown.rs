//! Markdown rendering for TUI chat messages.

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

// Default theme constants (dark)
const ACCENT: Color = Color::Cyan;
const DIM: Color = Color::DarkGray;
const WARN: Color = Color::Yellow;

/// Render markdown text into styled lines for the TUI.
pub fn render_markdown(text: &str) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut in_code_block = false;
    let mut code_lang = String::new();
    let mut code_block_lines: Vec<String> = Vec::new();
    let max_code_lines = 30;

    for line in text.lines() {
        // Code block toggle
        if line.starts_with("```") {
            if in_code_block {
                in_code_block = false;
                render_code_block(&mut lines, &code_block_lines, &code_lang, max_code_lines);
                code_block_lines.clear();
                code_lang.clear();
                lines.push(Line::from(vec![
                    Span::styled("  └───────────────────────────", Style::default().fg(DIM)),
                ]));
            } else {
                in_code_block = true;
                code_lang = line.strip_prefix("```").unwrap_or("").trim().to_string();
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
            code_block_lines.push(line.to_string());
            continue;
        }

        // Heading
        if let Some(text) = line.strip_prefix("# ") {
            lines.push(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(text.to_string(), Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            ]));
            continue;
        }
        if let Some(text) = line.strip_prefix("## ") {
            lines.push(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(text.to_string(), Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            ]));
            continue;
        }
        if let Some(text) = line.strip_prefix("### ") {
            lines.push(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(text.to_string(), Style::default().fg(WARN).add_modifier(Modifier::BOLD)),
            ]));
            continue;
        }

        // Task list (must be checked before bullet list since "- [ ] " starts with "- ")
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
        if let Some(stripped) = line.strip_prefix("> ") {
            let mut spans = vec![
                Span::styled("  ", Style::default()),
                Span::styled("│ ", Style::default().fg(ACCENT)),
            ];
            spans.extend(parse_inline(stripped));
            lines.push(Line::from(spans));
            continue;
        }

        // Regular line with inline formatting
        let mut spans = vec![Span::styled("  ", Style::default())];
        spans.extend(parse_inline(line));
        lines.push(Line::from(spans));
    }

    // Handle unclosed code blocks
    if in_code_block && !code_block_lines.is_empty() {
        render_code_block(&mut lines, &code_block_lines, &code_lang, max_code_lines);
    }

    lines
}

/// Render a code block with syntax highlighting and folding.
fn render_code_block(lines: &mut Vec<Line<'static>>, code_lines: &[String], lang: &str, max_lines: usize) {
    let render_lines = if code_lines.len() > max_lines {
        &code_lines[..max_lines]
    } else {
        code_lines
    };

    let language = crate::highlight::Language::from_extension(lang);
    let highlighter = crate::highlight::SimpleHighlighter::new(language);
    for (i, code_line) in render_lines.iter().enumerate() {
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

    if code_lines.len() > max_lines {
        let remaining = code_lines.len() - max_lines;
        lines.push(Line::from(vec![
            Span::styled(format!("  │ ... {} 行已折叠 (输入 'unfold' 展开)", remaining), Style::default().fg(ACCENT)),
        ]));
    }
}

/// Smart wrap text preserving indentation.
#[allow(dead_code)]
pub fn smart_wrap_line(text: &str, max_width: usize) -> Vec<String> {
    if text.chars().count() <= max_width {
        return vec![text.to_string()];
    }

    let indent_chars = text.chars().take_while(|c| c.is_whitespace()).count();
    let indent_str: String = text.chars().take(indent_chars).collect();
    let content = text.trim_start();

    let mut result = Vec::new();
    let mut current_line = String::new();
    let mut current_len = 0;

    for word in content.split_whitespace() {
        let word_char_len = word.chars().count();
        if current_len + word_char_len + 1 > max_width - indent_chars && !current_line.is_empty() {
            result.push(current_line);
            current_line = format!("{}{}", indent_str, word);
            current_len = indent_chars + word_char_len;
        } else {
            if !current_line.is_empty() {
                current_line.push(' ');
                current_len += 1;
            }
            current_line.push_str(word);
            current_len += word_char_len;
        }
    }

    if !current_line.is_empty() {
        result.push(current_line);
    }

    result
}

/// Highlight search matches in text.
pub fn highlight_search_matches(text: &str, query: &str) -> Vec<Span<'static>> {
    if query.is_empty() || text.is_empty() {
        return vec![Span::raw(text.to_string())];
    }

    let chars: Vec<char> = text.chars().collect();
    let lower_chars: Vec<char> = text.to_lowercase().chars().collect();
    let lower_query: Vec<char> = query.to_lowercase().chars().collect();
    let text_len = chars.len();
    let query_len = lower_query.len();
    let mut spans = Vec::new();
    let mut last_end = 0;

    if query_len == 0 || text_len == 0 {
        return vec![Span::raw(text.to_string())];
    }

    let mut i = 0;
    while i + query_len <= text_len {
        if lower_chars[i..i + query_len] == *lower_query {
            if i > last_end {
                let before: String = chars[last_end..i].iter().collect();
                spans.push(Span::raw(before));
            }
            let matched: String = chars[i..i + query_len].iter().collect();
            spans.push(Span::styled(
                matched,
                Style::default().bg(Color::Yellow).fg(Color::Black),
            ));
            last_end = i + query_len;
            i = last_end;
        } else {
            i += 1;
        }
    }

    if last_end < text_len {
        let remaining: String = chars[last_end..].iter().collect();
        spans.push(Span::raw(remaining));
    }

    if spans.is_empty() {
        vec![Span::raw(text.to_string())]
    } else {
        spans
    }
}

/// Parse inline markdown: `code`, **bold**, *italic*, links, and plain text.
pub fn parse_inline(text: &str) -> Vec<Span<'static>> {
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
            if let Some(byte_pos) = remaining.find("**") {
                let end_pos = remaining[..byte_pos].chars().count();
                let bold_text: String = chars[i + 2..i + 2 + end_pos].iter().collect();
                spans.push(Span::styled(bold_text, Style::default().add_modifier(Modifier::BOLD)));
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
            if let Some(byte_pos) = remaining.find('*') {
                let end_pos = remaining[..byte_pos].chars().count();
                let italic_text: String = chars[i + 1..i + 1 + end_pos].iter().collect();
                spans.push(Span::styled(italic_text, Style::default().fg(Color::Yellow)));
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
            if let Some(byte_pos) = remaining.find(']') {
                let bracket_end = remaining[..byte_pos].chars().count();
                if i + bracket_end + 1 < len && chars[i + bracket_end + 1] == '(' {
                    let after_bracket: String = remaining[byte_pos + ']'.len_utf8()..].to_string();
                    if let Some(paren_byte) = after_bracket.find(')') {
                        let paren_end = after_bracket[..paren_byte].chars().count();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_markdown_heading() {
        let lines = render_markdown("# Hello");
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_render_markdown_bullet() {
        let lines = render_markdown("- item 1\n- item 2");
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_render_markdown_code_block() {
        let lines = render_markdown("```rust\nfn main() {}\n```");
        // Opening + code + closing = 3 lines
        assert!(lines.len() >= 3);
    }

    #[test]
    fn test_parse_inline_bold() {
        let spans = parse_inline("hello **world**");
        assert!(spans.iter().any(|s| s.content.contains("world")));
    }

    #[test]
    fn test_parse_inline_code() {
        let spans = parse_inline("use `println!` here");
        assert!(spans.iter().any(|s| s.content.contains("println!")));
    }

    #[test]
    fn test_highlight_search_matches() {
        let spans = highlight_search_matches("hello world", "world");
        assert!(spans.len() >= 2); // "hello " + highlighted "world"
    }

    #[test]
    fn test_highlight_empty_query() {
        let spans = highlight_search_matches("hello", "");
        assert_eq!(spans.len(), 1);
    }

    #[test]
    fn test_smart_wrap_short() {
        let lines = smart_wrap_line("short", 80);
        assert_eq!(lines.len(), 1);
    }
}
