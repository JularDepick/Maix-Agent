//! Input handling for TUI.

/// Tracks multi-line input state.
pub struct InputState {
    pub buffer: String,
    pub cursor: usize,
    pub history: Vec<String>,
    pub history_index: Option<usize>,
}

impl InputState {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            cursor: 0,
            history: Vec::new(),
            history_index: None,
        }
    }

    pub fn insert_char(&mut self, c: char) {
        self.buffer.insert(self.cursor, c);
        self.cursor += 1;
    }

    pub fn delete_before(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.buffer.remove(self.cursor);
        }
    }

    pub fn delete_after(&mut self) {
        if self.cursor < self.buffer.len() {
            self.buffer.remove(self.cursor);
        }
    }

    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn move_right(&mut self) {
        if self.cursor < self.buffer.len() {
            self.cursor += 1;
        }
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.buffer.len();
    }

    /// Submit: returns the text if non-empty, saves to history.
    pub fn submit(&mut self) -> Option<String> {
        let text = std::mem::take(&mut self.buffer).trim().to_string();
        self.cursor = 0;
        if text.is_empty() {
            return None;
        }
        self.history.push(text.clone());
        self.history_index = None;
        Some(text)
    }

    /// Navigate history up (older).
    pub fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let idx = match self.history_index {
            None => self.history.len().saturating_sub(1),
            Some(i) => i.saturating_sub(1),
        };
        self.history_index = Some(idx);
        self.buffer = self.history[idx].clone();
        self.cursor = self.buffer.len();
    }

    /// Navigate history down (newer).
    pub fn history_next(&mut self) {
        match self.history_index {
            Some(i) if i + 1 < self.history.len() => {
                self.history_index = Some(i + 1);
                self.buffer = self.history[i + 1].clone();
                self.cursor = self.buffer.len();
            }
            _ => {
                self.history_index = None;
                self.buffer.clear();
                self.cursor = 0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_delete() {
        let mut input = InputState::new();
        input.insert_char('a');
        input.insert_char('b');
        assert_eq!(input.buffer, "ab");
        input.delete_before();
        assert_eq!(input.buffer, "a");
    }

    #[test]
    fn test_cursor_movement() {
        let mut input = InputState::new();
        input.insert_char('a');
        input.insert_char('b');
        input.move_left();
        assert_eq!(input.cursor, 1);
        input.insert_char('x');
        assert_eq!(input.buffer, "axb"); // inserted at cursor
    }

    #[test]
    fn test_submit() {
        let mut input = InputState::new();
        input.insert_char('h');
        input.insert_char('i');
        let text = input.submit();
        assert_eq!(text, Some("hi".into()));
        assert!(input.buffer.is_empty());
        assert_eq!(input.history.len(), 1);
    }

    #[test]
    fn test_empty_submit() {
        let mut input = InputState::new();
        let text = input.submit();
        assert_eq!(text, None);
    }

    #[test]
    fn test_history_navigation() {
        let mut input = InputState::new();
        // Add two entries to history
        input.insert_char('a');
        input.submit();
        input.insert_char('b');
        input.submit();

        input.history_prev();
        assert_eq!(input.buffer, "b");
        input.history_prev();
        assert_eq!(input.buffer, "a");
        input.history_next();
        assert_eq!(input.buffer, "b");
        input.history_next();
        assert!(input.buffer.is_empty());
    }

    #[test]
    fn test_home_end() {
        let mut input = InputState::new();
        input.insert_char('a');
        input.insert_char('b');
        input.insert_char('c');
        input.move_home();
        assert_eq!(input.cursor, 0);
        input.move_end();
        assert_eq!(input.cursor, 3);
    }
}
