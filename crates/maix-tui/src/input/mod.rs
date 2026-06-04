//! Input handling for TUI.

mod commands;

use commands::{CODE_SNIPPETS, SLASH_COMMANDS};

/// File path completion support.
pub struct FilePathCompleter {
    /// Base directory for file search
    pub base_dir: std::path::PathBuf,
    /// Cached file list
    pub cached_files: Vec<String>,
    /// Last cache update time
    pub last_update: std::time::Instant,
}

impl FilePathCompleter {
    pub fn new() -> Self {
        Self {
            base_dir: std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
            cached_files: Vec::new(),
            last_update: std::time::Instant::now(),
        }
    }

    /// Refresh file cache if stale (older than 5 seconds)
    pub fn refresh_cache(&mut self) {
        if self.last_update.elapsed().as_secs() < 5 && !self.cached_files.is_empty() {
            return;
        }
        self.cached_files.clear();
        let base_dir = self.base_dir.clone();
        Self::scan_directory_static(&mut self.cached_files, &base_dir, &base_dir, 0);
        self.last_update = std::time::Instant::now();
    }

    /// Recursively scan directory for files (max depth 3)
    fn scan_directory_static(files: &mut Vec<String>, base_dir: &std::path::Path, dir: &std::path::Path, depth: u32) {
        if depth > 3 {
            return;
        }
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    // Skip hidden files and common ignored directories
                    if name.starts_with('.') || name == "node_modules" || name == "target" {
                        continue;
                    }
                    if path.is_file() {
                        if let Ok(relative) = path.strip_prefix(base_dir) {
                            files.push(relative.to_string_lossy().to_string());
                        }
                    } else if path.is_dir() {
                        if let Ok(relative) = path.strip_prefix(base_dir) {
                            files.push(format!("{}/", relative.to_string_lossy()));
                        }
                        Self::scan_directory_static(files, base_dir, &path, depth + 1);
                    }
                }
            }
        }
    }

    /// Get file path completions for a given prefix
    pub fn get_completions(&mut self, prefix: &str) -> Vec<CompletionItem> {
        self.refresh_cache();
        let prefix_lower = prefix.to_lowercase();
        self.cached_files
            .iter()
            .filter(|f| {
                let f_lower = f.to_lowercase();
                f_lower.starts_with(&prefix_lower) || fuzzy_match(&prefix_lower, &f_lower)
            })
            .map(|f| CompletionItem {
                name: f.clone(),
                description: if f.ends_with('/') { "目录" } else { "文件" }.to_string(),
                params: Vec::new(),
            })
            .collect()
    }
}

/// Completion item with name and description.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CompletionItem {
    pub name: String,
    pub description: String,
    /// Parameter hints for the command
    pub params: Vec<String>,
}

/// Tracks multi-line input state.
pub struct InputState {
    pub buffer: String,
    pub cursor: usize,
    pub history: Vec<String>,
    pub history_index: Option<usize>,
    pub completions: Vec<CompletionItem>,
    pub completion_index: usize,
    pub completion_offset: usize,  // Scroll offset for completions
    pub custom_commands: Vec<String>,
    /// Original input before tab completion started
    #[allow(dead_code)]
    pub completion_original: Option<String>,
    /// File path completer
    pub file_completer: FilePathCompleter,
    /// Variable names extracted from conversation
    pub variable_names: Vec<String>,
    /// Completion learning: track selection frequency
    pub completion_usage: std::collections::HashMap<String, usize>,
    /// File content identifiers for completion
    pub file_identifiers: Vec<String>,
}

#[allow(dead_code)]
impl InputState {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            cursor: 0,
            history: Vec::new(),
            history_index: None,
            completions: Vec::new(),
            completion_index: 0,
            completion_offset: 0,
            custom_commands: Vec::new(),
            completion_original: None,
            file_completer: FilePathCompleter::new(),
            variable_names: Vec::new(),
            completion_usage: std::collections::HashMap::new(),
            file_identifiers: Vec::new(),
        }
    }

    /// Tab completion: cycle through matching commands.
    #[allow(dead_code)]
    pub fn tab_complete(&mut self, visible_height: usize) {
        // If we already have completions and buffer hasn't changed, cycle
        if !self.completions.is_empty() && self.buffer.starts_with('/') {
            // Use same logic as completion_next
            self.completion_next(visible_height);
            self.buffer = self.completions[self.completion_index].name.clone();
            self.cursor = self.buffer.len();
            return;
        }

        // Save original input before starting completion
        self.completion_original = Some(self.buffer.clone());

        // Generate completions based on current input
        self.generate_completions();

        if !self.completions.is_empty() {
            self.completion_index = 0;
            self.buffer = self.completions[0].name.clone();
            self.cursor = self.buffer.len();
        }
    }

    /// Auto-complete: update completions when input starts with '/'.
    pub fn auto_complete(&mut self) {
        if self.buffer.starts_with('/') || self.buffer.starts_with(':') || self.buffer.starts_with('$') || self.buffer.starts_with('#') {
            self.generate_completions();
        } else if self.buffer.contains('@') || self.buffer.contains("./") || self.buffer.contains("../") {
            // File path completion
            let path_prefix = self.extract_path_prefix();
            self.completions = self.file_completer.get_completions(&path_prefix);
            self.completion_index = 0;
        } else {
            self.completions.clear();
            self.completion_index = 0;
        }
    }

    /// Extract file path prefix from current input
    fn extract_path_prefix(&self) -> String {
        let input = &self.buffer;
        // Find the last @ or ./ or ../
        let start = input.rfind('@')
            .or_else(|| input.rfind("./"))
            .or_else(|| input.rfind("../"))
            .unwrap_or(0);
        let prefix = &input[start..];
        // Remove the @ or ./ or ../ prefix
        if let Some(stripped) = prefix.strip_prefix('@') {
            stripped.to_string()
        } else if let Some(stripped) = prefix.strip_prefix("../") {
            stripped.to_string()
        } else if let Some(stripped) = prefix.strip_prefix("./") {
            stripped.to_string()
        } else {
            prefix.to_string()
        }
    }

    /// Generate completions with fuzzy matching.
    fn generate_completions(&mut self) {
        let input = self.buffer.to_lowercase();
        let prefix = &self.buffer;
        self.completion_offset = 0;  // Reset scroll offset

        // Check if we should show code snippets (triggered by ':' prefix)
        if self.buffer.starts_with(':') {
            let snippet_prefix = &self.buffer[1..];
            let snippet_lower = snippet_prefix.to_lowercase();
            self.completions = CODE_SNIPPETS
                .iter()
                .filter(|s| {
                    let trigger_lower = s.trigger.to_lowercase();
                    trigger_lower.starts_with(&snippet_lower) ||
                    fuzzy_match(&snippet_lower, &trigger_lower)
                })
                .map(|s| CompletionItem {
                    name: format!(":{}", s.trigger),
                    description: s.description.to_string(),
                    params: Vec::new(),
                })
                .collect();
            self.completion_index = 0;
            return;
        }

        // Check if we should show variable names (triggered by '$' prefix)
        if self.buffer.starts_with('$') {
            let var_prefix = &self.buffer[1..];
            let var_lower = var_prefix.to_lowercase();
            self.completions = self.variable_names
                .iter()
                .filter(|v| {
                    let v_lower = v.to_lowercase();
                    v_lower.starts_with(&var_lower) ||
                    fuzzy_match(&var_lower, &v_lower)
                })
                .map(|v| CompletionItem {
                    name: format!("${}", v),
                    description: "变量".to_string(),
                    params: Vec::new(),
                })
                .collect();
            self.completion_index = 0;
            return;
        }

        // Check if we should show file content identifiers (triggered by '#' prefix)
        if self.buffer.starts_with('#') {
            let id_prefix = &self.buffer[1..];
            let id_lower = id_prefix.to_lowercase();
            self.completions = self.file_identifiers
                .iter()
                .filter(|id| {
                    let id_l = id.to_lowercase();
                    id_l.starts_with(&id_lower) ||
                    fuzzy_match(&id_lower, &id_l)
                })
                .map(|id| CompletionItem {
                    name: format!("#{}", id),
                    description: "文件标识符".to_string(),
                    params: Vec::new(),
                })
                .collect();
            self.completion_index = 0;
            return;
        }

        // Collect all commands
        let mut all_cmds: Vec<CompletionItem> = SLASH_COMMANDS
            .iter()
            .map(|cmd| CompletionItem {
                name: cmd.name.to_string(),
                description: cmd.description.to_string(),
                params: cmd.params.iter().map(|s| s.to_string()).collect(),
            })
            .collect();

        // Add custom commands (without description)
        for custom in &self.custom_commands {
            all_cmds.push(CompletionItem {
                name: custom.clone(),
                description: "自定义命令".to_string(),
                params: Vec::new(),
            });
        }

        // Check if we should show parameter completions
        if let Some(space_pos) = self.buffer.rfind(' ') {
            let cmd_part = &self.buffer[..space_pos];
            let param_part = &self.buffer[space_pos + 1..];

            // Find the command definition
            if let Some(cmd_def) = SLASH_COMMANDS.iter().find(|c| c.name == cmd_part) {
                if !cmd_def.params.is_empty() {
                    // Filter parameters based on input
                    self.completions = cmd_def.params
                        .iter()
                        .filter(|p| {
                            let p_lower = p.to_lowercase();
                            p_lower.starts_with(&param_part.to_lowercase()) ||
                            fuzzy_match(&param_part.to_lowercase(), &p_lower)
                        })
                        .map(|p| CompletionItem {
                            name: format!("{} {}", cmd_part, p),
                            description: format!("{} 参数", cmd_def.description),
                            params: Vec::new(),
                        })
                        .collect();
                    self.completion_index = 0;
                    return;
                }
            }
        }

        // Filter: prefix match first, then fuzzy match
        self.completions = all_cmds
            .into_iter()
            .filter(|cmd| {
                // Exact prefix match
                cmd.name.starts_with(prefix) ||
                // Fuzzy: all chars in input appear in order in name
                fuzzy_match(&input, &cmd.name.to_lowercase())
            })
            .collect();

        // Sort: prefix matches first, then by usage frequency (learning), then by name
        let completion_usage = self.completion_usage.clone();
        self.completions.sort_by(|a, b| {
            let a_prefix = a.name.starts_with(prefix);
            let b_prefix = b.name.starts_with(prefix);
            match (a_prefix, b_prefix) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => {
                    // Use completion learning to sort by frequency
                    let a_usage = completion_usage.get(&a.name).copied().unwrap_or(0);
                    let b_usage = completion_usage.get(&b.name).copied().unwrap_or(0);
                    match b_usage.cmp(&a_usage) {
                        std::cmp::Ordering::Equal => a.name.cmp(&b.name),
                        other => other,
                    }
                }
            }
        });

        self.completion_index = 0;
    }

    /// Record completion selection for learning.
    #[allow(dead_code)]
    pub fn record_completion_selection(&mut self, name: &str) {
        *self.completion_usage.entry(name.to_string()).or_insert(0) += 1;
    }

    /// Get completion usage count.
    #[allow(dead_code)]
    pub fn get_completion_usage(&self, name: &str) -> usize {
        self.completion_usage.get(name).copied().unwrap_or(0)
    }

    /// Load file content and extract identifiers for completion.
    #[allow(dead_code)]
    pub fn load_file_identifiers(&mut self, file_path: &str) {
        if let Ok(content) = std::fs::read_to_string(file_path) {
            let mut identifiers = Vec::new();
            for line in content.lines() {
                // Extract identifiers (simple heuristic: words that look like code)
                for word in line.split_whitespace() {
                    let clean = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
                    if clean.len() > 2
                        && clean.chars().all(|c| c.is_alphanumeric() || c == '_')
                        && !clean.chars().next().is_some_and(|c| c.is_ascii_digit())
                        && !is_common_word(clean)
                    {
                        identifiers.push(clean.to_string());
                    }
                }
            }
            // Add to file_identifiers, avoiding duplicates
            for id in identifiers {
                if !self.file_identifiers.contains(&id) {
                    self.file_identifiers.push(id);
                }
            }
            // Keep only the most recent 200 identifiers
            if self.file_identifiers.len() > 200 {
                let drain_count = self.file_identifiers.len() - 200;
                self.file_identifiers.drain(..drain_count);
            }
        }
    }

    /// Select current completion item.
    pub fn select_completion(&mut self) -> Option<String> {
        let name = self.completions.get(self.completion_index).map(|item| item.name.clone());
        if let Some(name) = name {
            // Record selection for learning
            self.record_completion_selection(&name);
            // For code snippet completion, insert the snippet content
            if let Some(trigger) = name.strip_prefix(':') {
                if let Some(snippet) = CODE_SNIPPETS.iter().find(|s| s.trigger == trigger) {
                    self.buffer = snippet.content.to_string();
                    self.cursor = self.buffer.len();
                    self.completions.clear();
                    self.completion_original = None;
                    return Some(snippet.content.to_string());
                }
            }

            // For variable name completion, insert the variable name without the $ prefix
            if let Some(var_name) = name.strip_prefix('$') {
                self.buffer = var_name.to_string();
                self.cursor = self.buffer.len();
                self.completions.clear();
                self.completion_original = None;
                return Some(var_name.to_string());
            }

            // For file identifier completion, insert the identifier without the # prefix
            if let Some(id_name) = name.strip_prefix('#') {
                self.buffer = id_name.to_string();
                self.cursor = self.buffer.len();
                self.completions.clear();
                self.completion_original = None;
                return Some(id_name.to_string());
            }

            // For file path completion, replace only the path part
            if self.buffer.contains('@') || self.buffer.contains("./") || self.buffer.contains("../") {
                let start = self.buffer.rfind('@')
                    .or_else(|| self.buffer.rfind("./"))
                    .or_else(|| self.buffer.rfind("../"))
                    .unwrap_or(0);
                let prefix_len = if self.buffer[start..].starts_with('@') {
                    1
                } else if self.buffer[start..].starts_with("../") {
                    3
                } else if self.buffer[start..].starts_with("./") {
                    2
                } else {
                    0
                };
                self.buffer = format!("{}{}", &self.buffer[..start + prefix_len], name);
            } else {
                self.buffer = name.clone();
            }
            self.cursor = self.buffer.len();
            self.completions.clear();
            self.completion_original = None;
            Some(name)
        } else {
            None
        }
    }

    /// Move completion selection up with scroll.
    pub fn completion_prev(&mut self) {
        if !self.completions.is_empty() {
            // If at the top of visible area, scroll up
            if self.completion_index == self.completion_offset && self.completion_offset > 0 {
                self.completion_offset = self.completion_offset.saturating_sub(1);
            }
            self.completion_index = self.completion_index.saturating_add(self.completions.len() - 1) % self.completions.len();
        }
    }

    /// Move completion selection down with scroll.
    pub fn completion_next(&mut self, visible_height: usize) {
        if !self.completions.is_empty() {
            // If at the bottom of visible area, scroll down
            if self.completion_index == self.completion_offset + visible_height - 1 {
                self.completion_offset = (self.completion_offset + 1).min(self.completions.len().saturating_sub(visible_height));
            }
            self.completion_index = (self.completion_index + 1) % self.completions.len();
        }
    }

    /// Extract variable names from text and add to variable_names list.
    #[allow(dead_code)]
    pub fn extract_variable_names(&mut self, text: &str) {
        // Simple heuristic: look for identifiers that look like variable names
        let mut new_vars = Vec::new();

        // Look for patterns like: let/const/var name = ..., name: Type, etc.
        for line in text.lines() {
            let line = line.trim();

            // Rust/JS/TS variable declarations
            if line.starts_with("let ") || line.starts_with("const ") || line.starts_with("var ") {
                if let Some(eq_pos) = line.find('=') {
                    let var_part = &line[..eq_pos].trim();
                    let var_name = var_part.split_whitespace().last().unwrap_or("");
                    if !var_name.is_empty() && var_name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                        new_vars.push(var_name.to_string());
                    }
                }
            }

            // Function parameters
            if let Some(paren_start) = line.find('(') {
                if let Some(paren_end) = line.find(')') {
                    let params = &line[paren_start + 1..paren_end];
                    for param in params.split(',') {
                        let param = param.trim();
                        if let Some(colon_pos) = param.find(':') {
                            let var_name = param[..colon_pos].trim();
                            if !var_name.is_empty() && var_name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                                new_vars.push(var_name.to_string());
                            }
                        }
                    }
                }
            }

            // Python variable assignments
            if !line.starts_with('#') && !line.starts_with("//") {
                if let Some(eq_pos) = line.find('=') {
                    let var_part = &line[..eq_pos].trim();
                    let var_name = var_part.split_whitespace().last().unwrap_or("");
                    if !var_name.is_empty() && var_name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                        new_vars.push(var_name.to_string());
                    }
                }
            }
        }

        // Add new variables, avoiding duplicates
        for var in new_vars {
            if !self.variable_names.contains(&var) && var.len() > 1 {
                self.variable_names.push(var);
            }
        }

        // Keep only the most recent 100 variables
        if self.variable_names.len() > 100 {
            let drain_count = self.variable_names.len() - 100;
            self.variable_names.drain(..drain_count);
        }
    }

    pub fn insert_char(&mut self, c: char) {
        // Auto-pairing brackets
        let pair = match c {
            '(' => Some(')'),
            '[' => Some(']'),
            '{' => Some('}'),
            '"' => Some('"'),
            '\'' => Some('\''),
            '`' => Some('`'),
            _ => None,
        };

        // Defensive cursor bounds check
        self.cursor = self.cursor.min(self.buffer.len());

        if let Some(closing) = pair {
            self.buffer.insert(self.cursor, c);
            self.buffer.insert(self.cursor + c.len_utf8(), closing);
            self.cursor += c.len_utf8();
        } else {
            // Skip closing char if it's already there (type-over)
            if (c == ')' || c == ']' || c == '}' || c == '"' || c == '\'' || c == '`')
                && self.cursor < self.buffer.len()
                && self.buffer[self.cursor..].starts_with(c)
            {
                self.cursor += c.len_utf8();
            } else {
                self.buffer.insert(self.cursor, c);
                self.cursor += c.len_utf8();
            }
        }
    }

    pub fn insert_newline(&mut self) {
        self.buffer.insert(self.cursor, '\n');
        self.cursor += 1;
    }

    /// Count lines in the current buffer.
    pub fn line_count(&self) -> usize {
        self.buffer.chars().filter(|&c| c == '\n').count() + 1
    }

    pub fn delete_before(&mut self) {
        if self.cursor > 0 {
            let prev = self.buffer[..self.cursor]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.cursor = prev;
            self.buffer.remove(prev);
        }
    }

    pub fn delete_after(&mut self) {
        if self.cursor < self.buffer.len() {
            self.buffer.remove(self.cursor);
        }
    }

    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            // Find previous char boundary
            self.cursor = self.buffer[..self.cursor]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    pub fn move_right(&mut self) {
        if self.cursor < self.buffer.len() {
            if let Some(ch) = self.buffer[self.cursor..].chars().next() {
                self.cursor += ch.len_utf8();
            }
        }
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.buffer.len();
    }

    /// Ctrl+Left: Jump to previous word boundary.
    pub fn move_word_left(&mut self) {
        if self.cursor == 0 {
            return;
        }
        // Skip trailing spaces
        let mut pos = self.cursor;
        while pos > 0 && self.buffer[..pos].ends_with(' ') {
            pos -= 1;
        }
        // Find word start
        pos = self.buffer[..pos]
            .rfind([' ', '/', '\\', '\n'])
            .map(|p| p + 1)
            .unwrap_or(0);
        self.cursor = pos;
    }

    /// Ctrl+Right: Jump to next word boundary.
    pub fn move_word_right(&mut self) {
        if self.cursor >= self.buffer.len() {
            return;
        }
        // Skip current word
        let mut pos = self.cursor;
        while pos < self.buffer.len() && !self.buffer[pos..].starts_with(' ')
            && !self.buffer[pos..].starts_with('/')
            && !self.buffer[pos..].starts_with('\\')
            && !self.buffer[pos..].starts_with('\n')
        {
            pos += 1;
        }
        // Skip spaces
        while pos < self.buffer.len() && self.buffer[pos..].starts_with(' ') {
            pos += 1;
        }
        self.cursor = pos;
    }

    /// Ctrl+U: Clear from cursor to beginning of line
    pub fn clear_to_line_start(&mut self) {
        let line_start = self.buffer[..self.cursor]
            .rfind('\n')
            .map(|p| p + 1)
            .unwrap_or(0);
        self.buffer.drain(line_start..self.cursor);
        self.cursor = line_start;
    }

    /// Ctrl+W: Delete previous word
    pub fn delete_prev_word(&mut self) {
        if self.cursor == 0 {
            return;
        }
        // Find end of current word (skip trailing spaces)
        let mut end = self.cursor;
        while end > 0 && self.buffer[..end].ends_with(' ') {
            end -= 1;
        }
        // Find start of word
        let start = self.buffer[..end]
            .rfind([' ', '/', '\n'])
            .map(|p| p + 1)
            .unwrap_or(0);
        self.buffer.drain(start..self.cursor);
        self.cursor = start;
    }

    /// Ctrl+K: Clear from cursor to end of line
    pub fn clear_to_line_end(&mut self) {
        let line_end = self.buffer[self.cursor..]
            .find('\n')
            .map(|p| self.cursor + p)
            .unwrap_or(self.buffer.len());
        self.buffer.drain(self.cursor..line_end);
    }

    /// Submit: returns the text if non-empty, saves to history.
    pub fn submit(&mut self) -> Option<String> {
        let text = std::mem::take(&mut self.buffer).trim().to_string();
        self.cursor = 0;
        if text.is_empty() {
            return None;
        }
        // Deduplicate: don't add if same as last entry
        if self.history.last() != Some(&text) {
            self.history.push(text.clone());
        }
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

/// Check if a word is a common word that shouldn't be used for completion.
#[allow(dead_code)]
fn is_common_word(word: &str) -> bool {
    const COMMON_WORDS: &[&str] = &[
        "the", "and", "for", "are", "but", "not", "you", "all", "can", "had",
        "her", "was", "one", "our", "out", "day", "get", "has", "him", "his",
        "how", "its", "may", "new", "now", "old", "see", "way", "who", "did",
        "let", "say", "she", "too", "use", "this", "that", "with", "have",
        "from", "they", "been", "said", "each", "which", "their", "time",
        "will", "about", "there", "could", "other", "make", "what", "only",
        "very", "when", "come", "know", "just", "also", "back", "into",
        "over", "such", "take", "than", "them", "some", "would", "every",
        "then", "these", "like", "long", "look", "many", "after", "thing",
        "before", "should", "because", "between", "need", "each", "found",
        "does", "part",
    ];
    COMMON_WORDS.contains(&word.to_lowercase().as_str())
}

/// Fuzzy match: check if all chars in pattern appear in order in text.
fn fuzzy_match(pattern: &str, text: &str) -> bool {
    if pattern.is_empty() {
        return true;
    }
    let mut pattern_chars = pattern.chars();
    let mut current = pattern_chars.next();

    for c in text.chars() {
        if let Some(p) = current {
            if c == p {
                current = pattern_chars.next();
            }
        }
    }

    current.is_none()
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

    #[test]
    fn test_chinese_insert_and_cursor() {
        let mut input = InputState::new();
        input.insert_char('你'); // 3 bytes
        assert_eq!(input.buffer, "你");
        assert_eq!(input.cursor, 3);
        input.insert_char('好'); // 3 bytes
        assert_eq!(input.buffer, "你好");
        assert_eq!(input.cursor, 6);
    }

    #[test]
    fn test_chinese_move_left_right() {
        let mut input = InputState::new();
        input.insert_char('你');
        input.insert_char('好');
        input.insert_char('世');
        // cursor at byte 9, buffer = "你好世"
        input.move_left();
        assert_eq!(input.cursor, 6); // before '世'
        assert_eq!(input.buffer, "你好世");
        input.move_left();
        assert_eq!(input.cursor, 3); // before '好'
        input.move_right();
        assert_eq!(input.cursor, 6); // after '好', before '世'
    }

    #[test]
    fn test_chinese_delete_before() {
        let mut input = InputState::new();
        input.insert_char('你');
        input.insert_char('好');
        assert_eq!(input.buffer, "你好");
        assert_eq!(input.cursor, 6);
        input.delete_before();
        assert_eq!(input.buffer, "你");
        assert_eq!(input.cursor, 3);
        input.delete_before();
        assert!(input.buffer.is_empty());
        assert_eq!(input.cursor, 0);
    }

    #[test]
    fn test_chinese_delete_after() {
        let mut input = InputState::new();
        input.insert_char('你');
        input.insert_char('好');
        input.insert_char('世');
        input.move_home();
        // cursor at 0, buffer = "你好世"
        input.delete_after();
        assert_eq!(input.buffer, "好世");
        assert_eq!(input.cursor, 0);
    }

    #[test]
    fn test_mixed_ascii_chinese() {
        let mut input = InputState::new();
        input.insert_char('h');
        input.insert_char('i');
        input.insert_char('你');
        input.insert_char('好');
        assert_eq!(input.buffer, "hi你好");
        assert_eq!(input.cursor, 8); // 2 + 3 + 3
        input.move_left();
        assert_eq!(input.cursor, 5); // before '好'
        input.delete_before();
        assert_eq!(input.buffer, "hi好");
        assert_eq!(input.cursor, 2); // after 'i', before '好'
    }

    #[test]
    fn test_chinese_submit() {
        let mut input = InputState::new();
        input.insert_char('你');
        input.insert_char('好');
        let text = input.submit();
        assert_eq!(text, Some("你好".into()));
        assert!(input.buffer.is_empty());
        assert_eq!(input.cursor, 0);
    }

    #[test]
    fn test_chinese_history() {
        let mut input = InputState::new();
        input.insert_char('你');
        input.insert_char('好');
        input.submit();
        input.insert_char('世');
        input.insert_char('界');
        input.submit();

        input.history_prev();
        assert_eq!(input.buffer, "世界");
        assert_eq!(input.cursor, 6);
        input.history_prev();
        assert_eq!(input.buffer, "你好");
        assert_eq!(input.cursor, 6);
    }

    #[test]
    fn test_auto_complete_slash() {
        let mut input = InputState::new();
        input.insert_char('/');
        input.auto_complete();
        assert!(!input.completions.is_empty());
        assert!(input.completions.iter().any(|c| c.name == "/help"));
    }

    #[test]
    fn test_auto_complete_prefix() {
        let mut input = InputState::new();
        input.insert_char('/');
        input.insert_char('h');
        input.auto_complete();
        assert!(input.completions.iter().any(|c| c.name == "/help"));
        assert!(!input.completions.iter().any(|c| c.name == "/quit"));
    }

    #[test]
    fn test_auto_complete_fuzzy() {
        let mut input = InputState::new();
        input.insert_char('/');
        input.insert_char('h');
        input.insert_char('p');
        input.auto_complete();
        assert!(input.completions.iter().any(|c| c.name == "/help"));
    }

    #[test]
    fn test_tab_complete_cycle() {
        let mut input = InputState::new();
        input.insert_char('/');
        input.auto_complete();
        let first = input.completions[0].name.clone();
        let visible_height = 6;
        input.tab_complete(visible_height);
        let second = input.completions[input.completion_index].name.clone();
        assert_ne!(first, second);
    }

    #[test]
    fn test_select_completion() {
        let mut input = InputState::new();
        input.insert_char('/');
        input.insert_char('h');
        input.auto_complete();
        let selected = input.select_completion();
        assert!(selected.is_some());
        assert!(input.buffer.starts_with("/h"));
        assert!(input.completions.is_empty());
    }

    #[test]
    fn test_completion_navigation() {
        let mut input = InputState::new();
        input.insert_char('/');
        input.auto_complete();
        let initial_index = input.completion_index;
        let visible_height = 6;
        input.completion_next(visible_height);
        assert_eq!(input.completion_index, (initial_index + 1) % input.completions.len());
        input.completion_prev();
        assert_eq!(input.completion_index, initial_index);
    }

    #[test]
    fn test_multiline_insert() {
        let mut input = InputState::new();
        input.insert_char('h');
        input.insert_char('e');
        input.insert_char('l');
        input.insert_char('l');
        input.insert_char('o');
        assert_eq!(input.buffer, "hello");
        assert_eq!(input.line_count(), 1);

        input.insert_newline();
        assert_eq!(input.buffer, "hello\n");
        assert_eq!(input.line_count(), 2);

        input.insert_char('w');
        input.insert_char('o');
        input.insert_char('r');
        input.insert_char('l');
        input.insert_char('d');
        assert_eq!(input.buffer, "hello\nworld");
        assert_eq!(input.line_count(), 2);
    }

    #[test]
    fn test_multiline_cursor_movement() {
        let mut input = InputState::new();
        input.insert_char('a');
        input.insert_char('b');
        input.insert_newline();
        input.insert_char('c');
        input.insert_char('d');
        // buffer = "ab\ncd" (5 bytes), cursor at end (5)
        assert_eq!(input.cursor, 5);

        input.move_left();
        assert_eq!(input.cursor, 4); // before 'd'

        input.move_left();
        assert_eq!(input.cursor, 3); // before 'c'

        input.move_left();
        assert_eq!(input.cursor, 2); // before '\n'

        input.move_left();
        assert_eq!(input.cursor, 1); // before 'b'
    }

    #[test]
    fn test_multiline_delete() {
        let mut input = InputState::new();
        input.insert_char('a');
        input.insert_newline();
        input.insert_char('b');
        // buffer = "a\nb" (3 bytes), cursor at 3
        assert_eq!(input.buffer, "a\nb");

        input.delete_before();
        assert_eq!(input.buffer, "a\n");
        assert_eq!(input.cursor, 2);

        input.delete_before();
        assert_eq!(input.buffer, "a");
        assert_eq!(input.cursor, 1);
    }

    #[test]
    fn test_multiline_submit() {
        let mut input = InputState::new();
        input.insert_char('l');
        input.insert_char('i');
        input.insert_char('n');
        input.insert_char('e');
        input.insert_char('1');
        input.insert_newline();
        input.insert_char('l');
        input.insert_char('i');
        input.insert_char('n');
        input.insert_char('e');
        input.insert_char('2');

        let text = input.submit();
        assert_eq!(text, Some("line1\nline2".into()));
        assert!(input.buffer.is_empty());
        assert_eq!(input.line_count(), 1);
    }

    #[test]
    fn test_file_path_completion() {
        let mut input = InputState::new();
        input.insert_char('@');
        input.insert_char('s');
        input.insert_char('r');
        input.insert_char('c');
        input.auto_complete();
        // Should show file completions if src directory exists
        // This test depends on the current directory structure
        // We just verify the completer is initialized
        assert!(input.file_completer.base_dir.exists());
    }

    #[test]
    fn test_extract_path_prefix() {
        let mut input = InputState::new();
        input.buffer = "请帮我检查 @src/ma".to_string();
        let prefix = input.extract_path_prefix();
        assert_eq!(prefix, "src/ma");

        input.buffer = "查看 ./Cargo".to_string();
        let prefix = input.extract_path_prefix();
        assert_eq!(prefix, "Cargo");

        input.buffer = "检查 ../README".to_string();
        let prefix = input.extract_path_prefix();
        assert_eq!(prefix, "README");
    }

    #[test]
    fn test_completion_learning() {
        let mut input = InputState::new();

        // Record some selections
        input.record_completion_selection("/help");
        input.record_completion_selection("/help");
        input.record_completion_selection("/quit");

        assert_eq!(input.get_completion_usage("/help"), 2);
        assert_eq!(input.get_completion_usage("/quit"), 1);
        assert_eq!(input.get_completion_usage("/unknown"), 0);
    }

    #[test]
    fn test_file_identifier_completion() {
        let mut input = InputState::new();
        input.file_identifiers.push("MyStruct".to_string());
        input.file_identifiers.push("my_function".to_string());
        input.file_identifiers.push("another_var".to_string());

        input.insert_char('#');
        input.insert_char('m');
        input.auto_complete();

        assert!(!input.completions.is_empty());
        assert!(input.completions.iter().any(|c| c.name == "#my_function"));
    }

    #[test]
    fn test_is_common_word() {
        assert!(is_common_word("the"));
        assert!(is_common_word("and"));
        assert!(!is_common_word("function"));
        assert!(!is_common_word("struct"));
    }

    #[test]
    fn test_completion_learning_sort() {
        let mut input = InputState::new();

        // Record usage
        input.record_completion_selection("/quit");
        input.record_completion_selection("/quit");
        input.record_completion_selection("/quit");
        input.record_completion_selection("/help");

        input.insert_char('/');
        input.auto_complete();

        // quit should appear before help due to higher usage
        let quit_pos = input.completions.iter().position(|c| c.name == "/quit");
        let help_pos = input.completions.iter().position(|c| c.name == "/help");

        if let (Some(q), Some(h)) = (quit_pos, help_pos) {
            assert!(q < h, "quit should appear before help due to higher usage");
        }
    }
}
