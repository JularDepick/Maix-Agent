#![allow(dead_code)]
//! Command palette — fuzzy search for commands, files, and symbols.
//!
//! Activated with Ctrl+P, provides a popup overlay with search-as-you-type.

/// Command category for grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandCategory {
    Command,
    File,
    Symbol,
    Recent,
}

impl CommandCategory {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Command => "Commands",
            Self::File => "Files",
            Self::Symbol => "Symbols",
            Self::Recent => "Recent",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            Self::Command => ">",
            Self::File => "f",
            Self::Symbol => "#",
            Self::Recent => "~",
        }
    }
}

/// A palette entry.
#[derive(Debug, Clone)]
pub struct PaletteEntry {
    pub label: String,
    pub description: String,
    pub category: CommandCategory,
    pub action: PaletteAction,
    pub score: f32,
}

/// Action to execute when an entry is selected.
#[derive(Debug, Clone)]
pub enum PaletteAction {
    RunCommand(String),
    OpenFile(String),
    GoToSymbol { file: String, symbol: String },
    Custom(String),
}

/// Command palette state.
pub struct CommandPalette {
    entries: Vec<PaletteEntry>,
    filtered: Vec<usize>,
    query: String,
    selected: usize,
    visible: bool,
}

impl CommandPalette {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            filtered: Vec::new(),
            query: String::new(),
            selected: 0,
            visible: false,
        }
    }

    pub fn show(&mut self) {
        self.visible = true;
        self.query.clear();
        self.selected = 0;
        self.update_filter();
    }

    pub fn hide(&mut self) {
        self.visible = false;
        self.query.clear();
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn toggle(&mut self) {
        if self.visible {
            self.hide();
        } else {
            self.show();
        }
    }

    pub fn add_entry(&mut self, entry: PaletteEntry) {
        self.entries.push(entry);
    }

    pub fn add_entries(&mut self, entries: Vec<PaletteEntry>) {
        self.entries.extend(entries);
    }

    pub fn set_query(&mut self, query: &str) {
        self.query = query.to_string();
        self.selected = 0;
        self.update_filter();
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn input_char(&mut self, c: char) {
        self.query.push(c);
        self.selected = 0;
        self.update_filter();
    }

    pub fn backspace(&mut self) {
        self.query.pop();
        self.selected = 0;
        self.update_filter();
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.filtered.len() {
            self.selected += 1;
        }
    }

    pub fn selected_entry(&self) -> Option<&PaletteEntry> {
        self.filtered.get(self.selected).map(|&i| &self.entries[i])
    }

    pub fn filtered_entries(&self) -> Vec<&PaletteEntry> {
        self.filtered.iter().map(|&i| &self.entries[i]).collect()
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    pub fn filtered_count(&self) -> usize {
        self.filtered.len()
    }

    fn update_filter(&mut self) {
        if self.query.is_empty() {
            self.filtered = (0..self.entries.len()).collect();
        } else {
            let query_lower = self.query.to_lowercase();
            self.filtered = self.entries
                .iter()
                .enumerate()
                .filter_map(|(i, entry)| {
                    let score = fuzzy_score(&query_lower, &entry.label.to_lowercase());
                    if score > 0.0 {
                        Some((i, score))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .into_iter()
                .collect::<Vec<_>>()
                .into_iter()
                .map(|(i, _)| i)
                .collect();

            // Sort by score (highest first)
            self.filtered.sort_by(|&a, &b| {
                let sa = fuzzy_score(&query_lower, &self.entries[a].label.to_lowercase());
                let sb = fuzzy_score(&query_lower, &self.entries[b].label.to_lowercase());
                sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
            });
        }
    }

    /// Format the palette for display.
    pub fn format_display(&self, max_visible: usize) -> Vec<String> {
        let mut lines = vec![format!("> {}_", self.query)];
        lines.push("─".repeat(40));

        let start = if self.selected >= max_visible {
            self.selected - max_visible + 1
        } else {
            0
        };

        for (display_idx, &entry_idx) in self.filtered.iter().skip(start).take(max_visible).enumerate() {
            let entry = &self.entries[entry_idx];
            let marker = if start + display_idx == self.selected { "→" } else { " " };
            lines.push(format!(
                "{} {} {} {}",
                marker,
                entry.category.icon(),
                entry.label,
                entry.description
            ));
        }

        lines
    }
}

impl Default for CommandPalette {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple fuzzy matching score. Returns 0.0 for no match.
fn fuzzy_score(query: &str, target: &str) -> f32 {
    if query.is_empty() {
        return 1.0;
    }
    if target.is_empty() {
        return 0.0;
    }

    // Exact match
    if target == query {
        return 100.0;
    }

    // Starts with
    if target.starts_with(query) {
        return 90.0;
    }

    // Contains
    if target.contains(query) {
        return 70.0;
    }

    // Subsequence match
    let query_chars: Vec<char> = query.chars().collect();
    let target_chars: Vec<char> = target.chars().collect();
    let mut qi = 0;
    let mut consecutive = 0;
    let mut score = 0.0;

    for tc in &target_chars {
        if qi < query_chars.len() && *tc == query_chars[qi] {
            score += 10.0 + consecutive as f32 * 5.0;
            consecutive += 1;
            qi += 1;
        } else {
            consecutive = 0;
        }
    }

    if qi == query_chars.len() {
        score
    } else {
        0.0
    }
}

/// Default commands for the palette.
pub fn default_commands() -> Vec<PaletteEntry> {
    vec![
        PaletteEntry {
            label: "/help".to_string(),
            description: "Show help".to_string(),
            category: CommandCategory::Command,
            action: PaletteAction::RunCommand("/help".to_string()),
            score: 0.0,
        },
        PaletteEntry {
            label: "/compact".to_string(),
            description: "Compact context".to_string(),
            category: CommandCategory::Command,
            action: PaletteAction::RunCommand("/compact".to_string()),
            score: 0.0,
        },
        PaletteEntry {
            label: "/clear".to_string(),
            description: "Clear conversation".to_string(),
            category: CommandCategory::Command,
            action: PaletteAction::RunCommand("/clear".to_string()),
            score: 0.0,
        },
        PaletteEntry {
            label: "/mode".to_string(),
            description: "Switch mode".to_string(),
            category: CommandCategory::Command,
            action: PaletteAction::RunCommand("/mode".to_string()),
            score: 0.0,
        },
        PaletteEntry {
            label: "/git status".to_string(),
            description: "Show git status".to_string(),
            category: CommandCategory::Command,
            action: PaletteAction::RunCommand("/git status".to_string()),
            score: 0.0,
        },
        PaletteEntry {
            label: "/undo".to_string(),
            description: "Undo last edit".to_string(),
            category: CommandCategory::Command,
            action: PaletteAction::RunCommand("/undo".to_string()),
            score: 0.0,
        },
        PaletteEntry {
            label: "/task list".to_string(),
            description: "List tasks".to_string(),
            category: CommandCategory::Command,
            action: PaletteAction::RunCommand("/task list".to_string()),
            score: 0.0,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_palette_show_hide() {
        let mut p = CommandPalette::new();
        assert!(!p.is_visible());
        p.show();
        assert!(p.is_visible());
        p.hide();
        assert!(!p.is_visible());
    }

    #[test]
    fn test_palette_toggle() {
        let mut p = CommandPalette::new();
        p.toggle();
        assert!(p.is_visible());
        p.toggle();
        assert!(!p.is_visible());
    }

    #[test]
    fn test_palette_input() {
        let mut p = CommandPalette::new();
        p.show();
        p.input_char('h');
        p.input_char('e');
        assert_eq!(p.query(), "he");
    }

    #[test]
    fn test_palette_backspace() {
        let mut p = CommandPalette::new();
        p.show();
        p.input_char('a');
        p.input_char('b');
        p.backspace();
        assert_eq!(p.query(), "a");
    }

    #[test]
    fn test_palette_filter() {
        let mut p = CommandPalette::new();
        p.add_entries(default_commands());
        p.show();
        p.set_query("help");
        assert_eq!(p.filtered_count(), 1);
        assert_eq!(p.selected_entry().unwrap().label, "/help");
    }

    #[test]
    fn test_palette_navigation() {
        let mut p = CommandPalette::new();
        p.add_entries(default_commands());
        p.show();
        p.move_down();
        p.move_down();
        assert_eq!(p.selected_index(), 2);
        p.move_up();
        assert_eq!(p.selected_index(), 1);
    }

    #[test]
    fn test_palette_empty_query_shows_all() {
        let mut p = CommandPalette::new();
        p.add_entries(default_commands());
        p.show();
        assert_eq!(p.filtered_count(), default_commands().len());
    }

    #[test]
    fn test_fuzzy_score_exact() {
        assert!(fuzzy_score("help", "help") > 90.0);
    }

    #[test]
    fn test_fuzzy_score_starts_with() {
        assert!(fuzzy_score("hel", "help") > 80.0);
    }

    #[test]
    fn test_fuzzy_score_contains() {
        assert!(fuzzy_score("lp", "help") > 0.0);
    }

    #[test]
    fn test_fuzzy_score_no_match() {
        assert_eq!(fuzzy_score("xyz", "help"), 0.0);
    }

    #[test]
    fn test_fuzzy_score_empty_query() {
        assert_eq!(fuzzy_score("", "help"), 1.0);
    }

    #[test]
    fn test_format_display() {
        let mut p = CommandPalette::new();
        p.add_entries(default_commands());
        p.show();
        let display = p.format_display(5);
        assert!(display[0].starts_with("> "));
        assert!(display.len() <= 7); // header + separator + 5 entries
    }

    #[test]
    fn test_category_label() {
        assert_eq!(CommandCategory::Command.label(), "Commands");
        assert_eq!(CommandCategory::File.label(), "Files");
    }

    #[test]
    fn test_category_icon() {
        assert_eq!(CommandCategory::Command.icon(), ">");
        assert_eq!(CommandCategory::File.icon(), "f");
    }
}
