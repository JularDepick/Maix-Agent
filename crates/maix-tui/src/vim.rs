//! Vim mode — Normal/Insert mode with basic motions.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Vim mode state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VimMode {
    Normal,
    Insert,
    Visual,
}

impl std::fmt::Display for VimMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VimMode::Normal => write!(f, "NORMAL"),
            VimMode::Insert => write!(f, "INSERT"),
            VimMode::Visual => write!(f, "VISUAL"),
        }
    }
}

/// Action to take after processing a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VimAction {
    /// Do nothing (key was consumed by vim)
    None,
    /// Fall through to normal input handling
    Passthrough,
    /// Submit the input
    Submit,
    /// Yank (copy) selected/last action text
    Yank(String),
    /// Selection updated (for Visual mode rendering)
    SelectionChanged,
}

/// Vim state machine.
pub struct VimState {
    pub mode: VimMode,
    pub enabled: bool,
    pending_d: bool,
    pending_y: bool,
    pending_r: bool,
    register: String,
    /// Visual mode selection start (byte offset).
    selection_start: Option<usize>,
}

impl VimState {
    pub fn new() -> Self {
        Self {
            mode: VimMode::Insert,
            enabled: false,
            pending_d: false,
            pending_y: false,
            pending_r: false,
            register: String::new(),
            selection_start: None,
        }
    }

    /// Get the current selection range (start, end) in Visual mode.
    pub fn selection(&self, cursor: usize) -> Option<(usize, usize)> {
        self.selection_start.map(|start| {
            if start <= cursor {
                (start, cursor)
            } else {
                (cursor, start)
            }
        })
    }

    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
        if self.enabled {
            self.mode = VimMode::Normal;
        } else {
            self.mode = VimMode::Insert;
        }
        self.pending_d = false;
        self.pending_y = false;
        self.pending_r = false;
        self.selection_start = None;
    }

    /// Process a key event. Returns the action to take.
    pub fn handle_key(
        &mut self,
        key: KeyEvent,
        cursor: &mut usize,
        text: &mut String,
    ) -> VimAction {
        if !self.enabled {
            return VimAction::Passthrough;
        }

        // Ctrl+C always resets to insert mode and clears pending state
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.mode = VimMode::Insert;
            self.pending_d = false;
            self.pending_y = false;
            self.pending_r = false;
            self.selection_start = None;
            return VimAction::None;
        }

        match self.mode {
            VimMode::Insert => self.handle_insert(key, cursor, text),
            VimMode::Normal => self.handle_normal(key, cursor, text),
            VimMode::Visual => self.handle_visual(key, cursor, text),
        }
    }

    fn handle_insert(&mut self, key: KeyEvent, _cursor: &mut usize, _text: &mut String) -> VimAction {
        match key.code {
            KeyCode::Esc => {
                self.mode = VimMode::Normal;
                VimAction::None
            }
            _ => VimAction::Passthrough,
        }
    }

    fn handle_normal(&mut self, key: KeyEvent, cursor: &mut usize, text: &mut String) -> VimAction {
        // Handle pending operators (dd, yy)
        if self.pending_d {
            self.pending_d = false;
            match key.code {
                KeyCode::Char('d') => {
                    // dd — delete current line (or all text in single-line)
                    self.register = text.clone();
                    text.clear();
                    *cursor = 0;
                    return VimAction::None;
                }
                _ => {
                    // Unknown key after 'd' — reset and re-process through normal match
                    return self.handle_normal(key, cursor, text);
                }
            }
        }
        if self.pending_y {
            self.pending_y = false;
            match key.code {
                KeyCode::Char('y') => {
                    // yy — yank entire line
                    self.register = text.clone();
                    return VimAction::None;
                }
                _ => {
                    // Unknown key after 'y' — reset and re-process through normal match
                    return self.handle_normal(key, cursor, text);
                }
            }
        }
        if self.pending_r {
            self.pending_r = false;
            // Replace character under cursor with the typed character
            if let KeyCode::Char(c) = key.code {
                if *cursor < text.len() {
                    let ch_len = text[*cursor..].chars().next().map_or(0, |c| c.len_utf8());
                    text.drain(*cursor..*cursor + ch_len);
                    text.insert(*cursor, c);
                    // Don't advance cursor (Vim behavior)
                }
            }
            return VimAction::None;
        }

        match key.code {
            // Mode switching
            KeyCode::Char('i') => {
                self.mode = VimMode::Insert;
                VimAction::None
            }
            KeyCode::Char('a') => {
                self.mode = VimMode::Insert;
                if *cursor < text.len() {
                    *cursor += text[*cursor..].chars().next().map_or(0, |c| c.len_utf8());
                }
                VimAction::None
            }
            KeyCode::Char('A') => {
                self.mode = VimMode::Insert;
                *cursor = text.len();
                VimAction::None
            }
            KeyCode::Char('I') => {
                self.mode = VimMode::Insert;
                *cursor = 0;
                VimAction::None
            }
            KeyCode::Char('o') => {
                self.mode = VimMode::Insert;
                // In single-line input, o just goes to end
                *cursor = text.len();
                VimAction::None
            }

            // Movement
            KeyCode::Char('h') | KeyCode::Left => {
                move_cursor_left(cursor, text);
                VimAction::None
            }
            KeyCode::Char('l') | KeyCode::Right => {
                move_cursor_right(cursor, text);
                VimAction::None
            }
            KeyCode::Char('0') | KeyCode::Home => {
                *cursor = 0;
                VimAction::None
            }
            KeyCode::Char('$') | KeyCode::End => {
                *cursor = text.len();
                VimAction::None
            }
            KeyCode::Char('w') => {
                // Next word
                move_to_next_word(cursor, text);
                VimAction::None
            }
            KeyCode::Char('b') => {
                // Previous word
                move_to_prev_word(cursor, text);
                VimAction::None
            }
            KeyCode::Char('^') => {
                // First non-whitespace
                *cursor = text.len() - text.trim_start().len();
                VimAction::None
            }

            // Editing
            KeyCode::Char('x') => {
                // Delete character under cursor
                if *cursor < text.len() {
                    let ch_len = text[*cursor..].chars().next().map_or(0, |c| c.len_utf8());
                    self.register = text[*cursor..*cursor + ch_len].to_string();
                    text.drain(*cursor..*cursor + ch_len);
                }
                VimAction::None
            }
            KeyCode::Char('d') => {
                self.pending_d = true;
                VimAction::None
            }
            KeyCode::Char('y') => {
                self.pending_y = true;
                VimAction::None
            }
            KeyCode::Char('p') => {
                // Paste
                if !self.register.is_empty() {
                    let insert_pos = *cursor;
                    text.insert_str(insert_pos, &self.register);
                    *cursor = insert_pos + self.register.len();
                }
                VimAction::None
            }
            KeyCode::Char('u') => {
                // Undo — not supported in single-line, just ignore
                VimAction::None
            }
            KeyCode::Char('v') => {
                // Enter Visual mode
                self.mode = VimMode::Visual;
                self.selection_start = Some(*cursor);
                VimAction::SelectionChanged
            }
            KeyCode::Char('r') => {
                // Enter replace-pending state
                self.pending_r = true;
                VimAction::None
            }

            // Submit
            KeyCode::Enter => VimAction::Submit,

            // Unknown normal mode key — ignore
            _ => VimAction::None,
        }
    }

    fn handle_visual(&mut self, key: KeyEvent, cursor: &mut usize, text: &mut String) -> VimAction {
        match key.code {
            KeyCode::Esc => {
                self.mode = VimMode::Normal;
                self.selection_start = None;
                VimAction::SelectionChanged
            }
            KeyCode::Char('h') | KeyCode::Left => {
                move_cursor_left(cursor, text);
                VimAction::SelectionChanged
            }
            KeyCode::Char('l') | KeyCode::Right => {
                move_cursor_right(cursor, text);
                VimAction::SelectionChanged
            }
            KeyCode::Char('0') | KeyCode::Home => {
                *cursor = 0;
                VimAction::SelectionChanged
            }
            KeyCode::Char('$') | KeyCode::End => {
                *cursor = text.len();
                VimAction::SelectionChanged
            }
            KeyCode::Char('w') => {
                move_to_next_word(cursor, text);
                VimAction::SelectionChanged
            }
            KeyCode::Char('b') => {
                move_to_prev_word(cursor, text);
                VimAction::SelectionChanged
            }
            KeyCode::Char('y') => {
                // Yank selection (inclusive of cursor position)
                let selected = if let Some((start, end)) = self.selection(*cursor) {
                    let inclusive_end = end + text[end..].chars().next().map_or(0, |c| c.len_utf8());
                    text[start..inclusive_end].to_string()
                } else {
                    String::new()
                };
                self.register = selected.clone();
                self.mode = VimMode::Normal;
                self.selection_start = None;
                VimAction::Yank(selected)
            }
            KeyCode::Char('d') | KeyCode::Char('x') => {
                // Delete selection (inclusive of cursor position)
                if let Some((start, end)) = self.selection(*cursor) {
                    let inclusive_end = end + text[end..].chars().next().map_or(0, |c| c.len_utf8());
                    self.register = text[start..inclusive_end].to_string();
                    text.drain(start..inclusive_end);
                    *cursor = start;
                }
                self.mode = VimMode::Normal;
                self.selection_start = None;
                VimAction::SelectionChanged
            }
            _ => VimAction::None,
        }
    }
}

fn move_cursor_left(cursor: &mut usize, text: &str) {
    if *cursor > 0 {
        // Find the previous char boundary
        let mut pos = *cursor - 1;
        while pos > 0 && !text.is_char_boundary(pos) {
            pos -= 1;
        }
        *cursor = pos;
    }
}

fn move_cursor_right(cursor: &mut usize, text: &str) {
    if *cursor < text.len() {
        let ch_len = text[*cursor..].chars().next().map_or(0, |c| c.len_utf8());
        *cursor += ch_len;
    }
}

fn move_to_next_word(cursor: &mut usize, text: &str) {
    let remaining = &text[*cursor..];
    // Skip current word
    let after_word = remaining.trim_start_matches(|c: char| !c.is_whitespace());
    // Skip whitespace
    let after_space = after_word.trim_start_matches(|c: char| c.is_whitespace());
    let moved = remaining.len() - after_space.len();
    if moved > 0 {
        *cursor += moved;
    } else {
        *cursor = text.len();
    }
}

fn move_to_prev_word(cursor: &mut usize, text: &str) {
    if *cursor == 0 {
        return;
    }
    let before = &text[..*cursor];
    // Trim trailing whitespace
    let trimmed = before.trim_end_matches(|c: char| c.is_whitespace());
    if trimmed.is_empty() {
        *cursor = 0;
        return;
    }
    // Find start of the word we're now inside
    let word_part = trimmed.trim_end_matches(|c: char| !c.is_whitespace());
    *cursor = word_part.len();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vim_toggle() {
        let mut vim = VimState::new();
        assert!(!vim.enabled);
        assert_eq!(vim.mode, VimMode::Insert);

        vim.toggle();
        assert!(vim.enabled);
        assert_eq!(vim.mode, VimMode::Normal);

        vim.toggle();
        assert!(!vim.enabled);
        assert_eq!(vim.mode, VimMode::Insert);
    }

    #[test]
    fn test_normal_mode_movements() {
        let mut vim = VimState::new();
        vim.enabled = true;
        vim.mode = VimMode::Normal;

        let mut text = String::from("hello world");
        let mut cursor = 0;

        // l moves right
        vim.handle_key(KeyCode::Char('l').into(), &mut cursor, &mut text);
        assert_eq!(cursor, 1);

        // $ moves to end
        vim.handle_key(KeyCode::Char('$').into(), &mut cursor, &mut text);
        assert_eq!(cursor, 11);

        // 0 moves to start
        vim.handle_key(KeyCode::Char('0').into(), &mut cursor, &mut text);
        assert_eq!(cursor, 0);
    }

    #[test]
    fn test_insert_mode_switch() {
        let mut vim = VimState::new();
        vim.enabled = true;
        vim.mode = VimMode::Normal;

        let mut text = String::from("hello");
        let mut cursor = 2;

        // i enters insert mode
        let action = vim.handle_key(KeyCode::Char('i').into(), &mut cursor, &mut text);
        assert_eq!(action, VimAction::None);
        assert_eq!(vim.mode, VimMode::Insert);

        // Esc returns to normal
        let action = vim.handle_key(KeyCode::Esc.into(), &mut cursor, &mut text);
        assert_eq!(action, VimAction::None);
        assert_eq!(vim.mode, VimMode::Normal);
    }

    #[test]
    fn test_delete_char() {
        let mut vim = VimState::new();
        vim.enabled = true;
        vim.mode = VimMode::Normal;

        let mut text = String::from("hello");
        let mut cursor = 1;

        // x deletes char under cursor
        vim.handle_key(KeyCode::Char('x').into(), &mut cursor, &mut text);
        assert_eq!(text, "hllo");
    }

    #[test]
    fn test_word_movement() {
        let mut vim = VimState::new();
        vim.enabled = true;
        vim.mode = VimMode::Normal;

        let mut text = String::from("hello world foo");
        let mut cursor = 0;

        // w moves to next word
        vim.handle_key(KeyCode::Char('w').into(), &mut cursor, &mut text);
        assert_eq!(cursor, 6); // "world" starts at index 6

        vim.handle_key(KeyCode::Char('w').into(), &mut cursor, &mut text);
        assert_eq!(cursor, 12); // "foo" starts at index 12

        // b moves back
        vim.handle_key(KeyCode::Char('b').into(), &mut cursor, &mut text);
        assert_eq!(cursor, 6); // back to "world"
    }

    #[test]
    fn test_paste() {
        let mut vim = VimState::new();
        vim.enabled = true;
        vim.mode = VimMode::Normal;
        vim.register = "xyz".to_string();

        let mut text = String::from("hello");
        let mut cursor = 5;

        vim.handle_key(KeyCode::Char('p').into(), &mut cursor, &mut text);
        assert_eq!(text, "helloxyz");
    }

    #[test]
    fn test_dd_delete_line() {
        let mut vim = VimState::new();
        vim.enabled = true;
        vim.mode = VimMode::Normal;

        let mut text = String::from("hello world");
        let mut cursor = 3;

        // First d sets pending
        vim.handle_key(KeyCode::Char('d').into(), &mut cursor, &mut text);
        assert!(vim.pending_d);
        assert_eq!(text, "hello world"); // not deleted yet

        // Second d deletes line
        vim.handle_key(KeyCode::Char('d').into(), &mut cursor, &mut text);
        assert!(text.is_empty());
        assert_eq!(cursor, 0);
        assert_eq!(vim.register, "hello world");
    }

    #[test]
    fn test_disabled_passthrough() {
        let mut vim = VimState::new();
        // Not enabled — should passthrough
        let mut text = String::new();
        let mut cursor = 0;

        let action = vim.handle_key(KeyCode::Char('i').into(), &mut cursor, &mut text);
        assert_eq!(action, VimAction::Passthrough);
    }
}
