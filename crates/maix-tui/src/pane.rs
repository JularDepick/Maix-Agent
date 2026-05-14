#![allow(dead_code)]
//! Split pane management — horizontal/vertical splits with focus switching.

/// Content type for a pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaneContent {
    Chat,
    Code { file_path: String },
    FileTree,
    Preview { title: String },
    Terminal,
}

/// Split direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

/// A single pane with content and area.
#[derive(Debug, Clone)]
pub struct Pane {
    pub content: PaneContent,
    pub width: u16,
    pub height: u16,
    pub focused: bool,
}

impl Pane {
    pub fn new(content: PaneContent) -> Self {
        Self {
            content,
            width: 80,
            height: 24,
            focused: true,
        }
    }
}

/// Pane layout manager.
pub struct PaneLayout {
    pub panes: Vec<Pane>,
    pub split: SplitDirection,
    pub focus_index: usize,
    pub split_ratio: f32,
}

impl PaneLayout {
    pub fn single(content: PaneContent) -> Self {
        Self {
            panes: vec![Pane::new(content)],
            split: SplitDirection::Vertical,
            focus_index: 0,
            split_ratio: 0.5,
        }
    }

    pub fn split(&mut self, direction: SplitDirection, content: PaneContent) {
        self.split = direction;
        let mut new_pane = Pane::new(content);
        new_pane.focused = false;
        self.panes.push(new_pane);
        self.recalculate_areas(80, 24);
    }

    pub fn close_pane(&mut self, index: usize) {
        if self.panes.len() <= 1 || index >= self.panes.len() {
            return;
        }
        self.panes.remove(index);
        if self.focus_index >= self.panes.len() {
            self.focus_index = self.panes.len() - 1;
        }
        self.panes[self.focus_index].focused = true;
    }

    pub fn resize(&mut self, delta: f32) {
        self.split_ratio = (self.split_ratio + delta).clamp(0.1, 0.9);
        self.recalculate_areas(80, 24);
    }

    pub fn focus_next(&mut self) {
        if self.panes.is_empty() {
            return;
        }
        self.panes[self.focus_index].focused = false;
        self.focus_index = (self.focus_index + 1) % self.panes.len();
        self.panes[self.focus_index].focused = true;
    }

    pub fn focus_prev(&mut self) {
        if self.panes.is_empty() {
            return;
        }
        self.panes[self.focus_index].focused = false;
        if self.focus_index == 0 {
            self.focus_index = self.panes.len() - 1;
        } else {
            self.focus_index -= 1;
        }
        self.panes[self.focus_index].focused = true;
    }

    pub fn focused_pane(&self) -> Option<&Pane> {
        self.panes.get(self.focus_index)
    }

    pub fn focused_pane_mut(&mut self) -> Option<&mut Pane> {
        self.panes.get_mut(self.focus_index)
    }

    pub fn pane_count(&self) -> usize {
        self.panes.len()
    }

    pub fn recalculate_areas(&mut self, total_width: u16, total_height: u16) {
        let n = self.panes.len();
        if n == 0 {
            return;
        }
        match self.split {
            SplitDirection::Vertical => {
                if n == 2 {
                    let first_width = (total_width as f32 * self.split_ratio) as u16;
                    let second_width = total_width - first_width;
                    self.panes[0].width = first_width;
                    self.panes[0].height = total_height;
                    self.panes[1].width = second_width;
                    self.panes[1].height = total_height;
                } else {
                    let pane_width = total_width / n as u16;
                    for pane in &mut self.panes {
                        pane.width = pane_width;
                        pane.height = total_height;
                    }
                }
            }
            SplitDirection::Horizontal => {
                if n == 2 {
                    let first_height = (total_height as f32 * self.split_ratio) as u16;
                    let second_height = total_height - first_height;
                    self.panes[0].width = total_width;
                    self.panes[0].height = first_height;
                    self.panes[1].width = total_width;
                    self.panes[1].height = second_height;
                } else {
                    let pane_height = total_height / n as u16;
                    for pane in &mut self.panes {
                        pane.width = total_width;
                        pane.height = pane_height;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_pane() {
        let layout = PaneLayout::single(PaneContent::Chat);
        assert_eq!(layout.pane_count(), 1);
        assert_eq!(layout.focus_index, 0);
    }

    #[test]
    fn test_split_vertical() {
        let mut layout = PaneLayout::single(PaneContent::Chat);
        layout.split(SplitDirection::Vertical, PaneContent::FileTree);
        assert_eq!(layout.pane_count(), 2);
    }

    #[test]
    fn test_split_horizontal() {
        let mut layout = PaneLayout::single(PaneContent::Chat);
        layout.split(SplitDirection::Horizontal, PaneContent::Code {
            file_path: "main.rs".into(),
        });
        assert_eq!(layout.pane_count(), 2);
    }

    #[test]
    fn test_focus_next() {
        let mut layout = PaneLayout::single(PaneContent::Chat);
        layout.split(SplitDirection::Vertical, PaneContent::FileTree);
        assert_eq!(layout.focus_index, 0);
        layout.focus_next();
        assert_eq!(layout.focus_index, 1);
        layout.focus_next();
        assert_eq!(layout.focus_index, 0); // wraps
    }

    #[test]
    fn test_focus_prev() {
        let mut layout = PaneLayout::single(PaneContent::Chat);
        layout.split(SplitDirection::Vertical, PaneContent::FileTree);
        layout.focus_prev();
        assert_eq!(layout.focus_index, 1); // wraps backward
    }

    #[test]
    fn test_close_pane() {
        let mut layout = PaneLayout::single(PaneContent::Chat);
        layout.split(SplitDirection::Vertical, PaneContent::FileTree);
        layout.close_pane(1);
        assert_eq!(layout.pane_count(), 1);
    }

    #[test]
    fn test_close_last_pane_noop() {
        let mut layout = PaneLayout::single(PaneContent::Chat);
        layout.close_pane(0);
        assert_eq!(layout.pane_count(), 1); // can't close last
    }

    #[test]
    fn test_resize() {
        let mut layout = PaneLayout::single(PaneContent::Chat);
        assert!((layout.split_ratio - 0.5).abs() < 0.01);
        layout.resize(0.1);
        assert!((layout.split_ratio - 0.6).abs() < 0.01);
        layout.resize(-0.5);
        assert!((layout.split_ratio - 0.1).abs() < 0.01); // clamped
    }

    #[test]
    fn test_recalculate_vertical() {
        let mut layout = PaneLayout::single(PaneContent::Chat);
        layout.split(SplitDirection::Vertical, PaneContent::FileTree);
        layout.recalculate_areas(100, 40);
        assert_eq!(layout.panes[0].width, 50);
        assert_eq!(layout.panes[0].height, 40);
    }

    #[test]
    fn test_recalculate_horizontal() {
        let mut layout = PaneLayout::single(PaneContent::Chat);
        layout.split(SplitDirection::Horizontal, PaneContent::FileTree);
        layout.recalculate_areas(100, 40);
        assert_eq!(layout.panes[0].width, 100);
        assert_eq!(layout.panes[0].height, 20);
    }

    #[test]
    fn test_focused_pane() {
        let mut layout = PaneLayout::single(PaneContent::Chat);
        layout.split(SplitDirection::Vertical, PaneContent::Terminal);
        let focused = layout.focused_pane().unwrap();
        assert!(focused.focused);
        assert_eq!(focused.content, PaneContent::Chat);
        layout.focus_next();
        let focused = layout.focused_pane().unwrap();
        assert_eq!(focused.content, PaneContent::Terminal);
    }
}
