//! TUI layout management — panels, tabs, and split layouts.

use serde::{Deserialize, Serialize};

/// Type of panel content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PanelType {
    /// Main conversation panel.
    Chat,
    /// Terminal output panel.
    Terminal,
    /// Code preview panel.
    CodePreview,
    /// File explorer panel.
    FileExplorer,
    /// Symbol search panel.
    SymbolSearch,
    /// Git status panel.
    GitStatus,
    /// Help panel.
    Help,
}

impl std::fmt::Display for PanelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Chat => write!(f, "Chat"),
            Self::Terminal => write!(f, "Terminal"),
            Self::CodePreview => write!(f, "Code Preview"),
            Self::FileExplorer => write!(f, "File Explorer"),
            Self::SymbolSearch => write!(f, "Symbol Search"),
            Self::GitStatus => write!(f, "Git Status"),
            Self::Help => write!(f, "Help"),
        }
    }
}

/// Layout split direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SplitDirection {
    /// Split horizontally (side by side).
    Horizontal,
    /// Split vertically (top and bottom).
    Vertical,
}

/// A single panel in the layout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Panel {
    pub id: String,
    pub panel_type: PanelType,
    pub visible: bool,
    pub title: String,
}

impl Panel {
    pub fn new(id: &str, panel_type: PanelType) -> Self {
        let title = panel_type.to_string();
        Self {
            id: id.to_string(),
            panel_type,
            visible: true,
            title,
        }
    }
}

/// A layout node — either a leaf panel or a split container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LayoutNode {
    /// A leaf panel.
    Leaf(Panel),
    /// A split container with two children and a ratio.
    Split {
        direction: SplitDirection,
        ratio: f32,
        first: Box<LayoutNode>,
        second: Box<LayoutNode>,
    },
}

#[allow(dead_code)]
impl LayoutNode {
    /// Create a leaf node.
    pub fn leaf(panel: Panel) -> Self {
        Self::Leaf(panel)
    }

    /// Split this node horizontally.
    pub fn split_h(self, other: LayoutNode, ratio: f32) -> Self {
        Self::Split {
            direction: SplitDirection::Horizontal,
            ratio,
            first: Box::new(self),
            second: Box::new(other),
        }
    }

    /// Split this node vertically.
    pub fn split_v(self, other: LayoutNode, ratio: f32) -> Self {
        Self::Split {
            direction: SplitDirection::Vertical,
            ratio,
            first: Box::new(self),
            second: Box::new(other),
        }
    }

    /// Get all panel IDs in this layout.
    pub fn panel_ids(&self) -> Vec<String> {
        match self {
            Self::Leaf(panel) => vec![panel.id.clone()],
            Self::Split { first, second, .. } => {
                let mut ids = first.panel_ids();
                ids.extend(second.panel_ids());
                ids
            }
        }
    }

    /// Find a panel by ID.
    pub fn find_panel(&self, id: &str) -> Option<&Panel> {
        match self {
            Self::Leaf(panel) => {
                if panel.id == id { Some(panel) } else { None }
            }
            Self::Split { first, second, .. } => {
                first.find_panel(id).or_else(|| second.find_panel(id))
            }
        }
    }

    /// Find a mutable panel by ID.
    pub fn find_panel_mut(&mut self, id: &str) -> Option<&mut Panel> {
        match self {
            Self::Leaf(panel) => {
                if panel.id == id { Some(panel) } else { None }
            }
            Self::Split { first, second, .. } => {
                first.find_panel_mut(id).or_else(|| second.find_panel_mut(id))
            }
        }
    }
}

/// A named tab containing a layout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tab {
    pub id: String,
    pub name: String,
    pub layout: LayoutNode,
    pub session_id: Option<String>,
}

impl Tab {
    pub fn new(id: &str, name: &str, layout: LayoutNode) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            layout,
            session_id: None,
        }
    }
}

/// Layout manager — manages panels, tabs, and splits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutManager {
    tabs: Vec<Tab>,
    active_tab: usize,
}

#[allow(dead_code)]
impl LayoutManager {
    pub fn new() -> Self {
        let default_layout = LayoutNode::leaf(Panel::new("main", PanelType::Chat));
        let default_tab = Tab::new("tab-1", "Main", default_layout);

        Self {
            tabs: vec![default_tab],
            active_tab: 0,
        }
    }

    /// Get the active tab.
    pub fn active_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active_tab)
    }

    /// Get the active tab mutable.
    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.active_tab)
    }

    /// Get the active tab index.
    pub fn active_tab_index(&self) -> usize {
        self.active_tab
    }

    /// List all tabs.
    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    /// Create a new tab.
    pub fn create_tab(&mut self, name: &str) -> String {
        let id = format!("tab-{}", self.tabs.len() + 1);
        let layout = LayoutNode::leaf(Panel::new("main", PanelType::Chat));
        self.tabs.push(Tab::new(&id, name, layout));
        id
    }

    /// Switch to a tab by index.
    pub fn switch_tab(&mut self, index: usize) -> Result<(), String> {
        if index < self.tabs.len() {
            self.active_tab = index;
            Ok(())
        } else {
            Err(format!("tab index {} out of range (have {})", index, self.tabs.len()))
        }
    }

    /// Close a tab by index.
    pub fn close_tab(&mut self, index: usize) -> Result<(), String> {
        if self.tabs.len() <= 1 {
            return Err("cannot close the last tab".to_string());
        }
        if index >= self.tabs.len() {
            return Err(format!("tab index {} out of range", index));
        }

        self.tabs.remove(index);
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
        Ok(())
    }

    /// Rename a tab.
    pub fn rename_tab(&mut self, index: usize, name: &str) -> Result<(), String> {
        if let Some(tab) = self.tabs.get_mut(index) {
            tab.name = name.to_string();
            Ok(())
        } else {
            Err(format!("tab index {} out of range", index))
        }
    }

    /// Split the active panel horizontally.
    pub fn split_horizontal(&mut self, panel_type: PanelType) -> Result<String, String> {
        let tab = self.tabs.get_mut(self.active_tab)
            .ok_or_else(|| "no active tab".to_string())?;

        let new_id = format!("panel-{}", tab.layout.panel_ids().len() + 1);
        let new_panel = Panel::new(&new_id, panel_type);

        // Take the current layout and wrap it in a split
        let current = std::mem::replace(
            &mut tab.layout,
            LayoutNode::leaf(Panel::new("temp", PanelType::Chat)),
        );
        tab.layout = current.split_h(LayoutNode::leaf(new_panel), 0.7);

        Ok(new_id)
    }

    /// Split the active panel vertically.
    pub fn split_vertical(&mut self, panel_type: PanelType) -> Result<String, String> {
        let tab = self.tabs.get_mut(self.active_tab)
            .ok_or_else(|| "no active tab".to_string())?;

        let new_id = format!("panel-{}", tab.layout.panel_ids().len() + 1);
        let new_panel = Panel::new(&new_id, panel_type);

        let current = std::mem::replace(
            &mut tab.layout,
            LayoutNode::leaf(Panel::new("temp", PanelType::Chat)),
        );
        tab.layout = current.split_v(LayoutNode::leaf(new_panel), 0.7);

        Ok(new_id)
    }

    /// Toggle panel visibility.
    pub fn toggle_panel(&mut self, panel_id: &str) -> Result<bool, String> {
        let tab = self.tabs.get_mut(self.active_tab)
            .ok_or_else(|| "no active tab".to_string())?;

        if let Some(panel) = tab.layout.find_panel_mut(panel_id) {
            panel.visible = !panel.visible;
            Ok(panel.visible)
        } else {
            Err(format!("panel '{}' not found", panel_id))
        }
    }

    /// Format layout state for display.
    pub fn format_state(&self) -> String {
        let mut lines = vec![
            format!("Tabs ({}/{}):", self.active_tab + 1, self.tabs.len()),
        ];

        for (i, tab) in self.tabs.iter().enumerate() {
            let marker = if i == self.active_tab { "→" } else { " " };
            let panel_ids = tab.layout.panel_ids();
            lines.push(format!("{} {} [{}] ({} panels)", marker, tab.name, tab.id, panel_ids.len()));
        }

        if let Some(tab) = self.active_tab() {
            lines.push(String::new());
            lines.push(format!("Active tab: {}", tab.name));
            lines.push(format!("Panels: {}", tab.layout.panel_ids().join(", ")));
        }

        lines.join("\n")
    }
}

impl Default for LayoutManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_layout() {
        let mgr = LayoutManager::new();
        assert_eq!(mgr.tabs().len(), 1);
        assert_eq!(mgr.active_tab_index(), 0);
        assert_eq!(mgr.active_tab().unwrap().name, "Main");
    }

    #[test]
    fn test_create_tab() {
        let mut mgr = LayoutManager::new();
        let id = mgr.create_tab("Terminal");
        assert_eq!(mgr.tabs().len(), 2);
        assert!(id.starts_with("tab-"));
    }

    #[test]
    fn test_switch_tab() {
        let mut mgr = LayoutManager::new();
        mgr.create_tab("Tab 2");
        assert!(mgr.switch_tab(1).is_ok());
        assert_eq!(mgr.active_tab_index(), 1);
    }

    #[test]
    fn test_close_tab() {
        let mut mgr = LayoutManager::new();
        mgr.create_tab("Tab 2");
        assert!(mgr.close_tab(1).is_ok());
        assert_eq!(mgr.tabs().len(), 1);
    }

    #[test]
    fn test_cannot_close_last_tab() {
        let mut mgr = LayoutManager::new();
        assert!(mgr.close_tab(0).is_err());
    }

    #[test]
    fn test_rename_tab() {
        let mut mgr = LayoutManager::new();
        assert!(mgr.rename_tab(0, "Renamed").is_ok());
        assert_eq!(mgr.active_tab().unwrap().name, "Renamed");
    }

    #[test]
    fn test_split_horizontal() {
        let mut mgr = LayoutManager::new();
        let panel_id = mgr.split_horizontal(PanelType::Terminal).unwrap();
        assert!(panel_id.starts_with("panel-"));

        let tab = mgr.active_tab().unwrap();
        let ids = tab.layout.panel_ids();
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn test_split_vertical() {
        let mut mgr = LayoutManager::new();
        let panel_id = mgr.split_vertical(PanelType::GitStatus).unwrap();
        assert!(panel_id.starts_with("panel-"));
    }

    #[test]
    fn test_toggle_panel() {
        let mut mgr = LayoutManager::new();
        let panel_id = mgr.split_horizontal(PanelType::Terminal).unwrap();

        let visible = mgr.toggle_panel(&panel_id).unwrap();
        assert!(!visible);

        let visible = mgr.toggle_panel(&panel_id).unwrap();
        assert!(visible);
    }

    #[test]
    fn test_panel_type_display() {
        assert_eq!(PanelType::Chat.to_string(), "Chat");
        assert_eq!(PanelType::Terminal.to_string(), "Terminal");
    }

    #[test]
    fn test_layout_node_leaf() {
        let node = LayoutNode::leaf(Panel::new("test", PanelType::Chat));
        assert_eq!(node.panel_ids(), vec!["test"]);
        assert!(node.find_panel("test").is_some());
        assert!(node.find_panel("other").is_none());
    }

    #[test]
    fn test_layout_node_split() {
        let node = LayoutNode::leaf(Panel::new("a", PanelType::Chat))
            .split_h(LayoutNode::leaf(Panel::new("b", PanelType::Terminal)), 0.7);

        let ids = node.panel_ids();
        assert_eq!(ids.len(), 2);
        assert!(node.find_panel("a").is_some());
        assert!(node.find_panel("b").is_some());
    }

    #[test]
    fn test_format_state() {
        let mut mgr = LayoutManager::new();
        mgr.split_horizontal(PanelType::Terminal).unwrap();
        let state = mgr.format_state();
        assert!(state.contains("Main"));
        assert!(state.contains("panel"));
    }
}
