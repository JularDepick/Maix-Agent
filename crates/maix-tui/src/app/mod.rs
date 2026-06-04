//! App module - Main application state and logic.

mod helpers;
mod init;
mod navigation;
mod state;
mod streaming;

pub(crate) use helpers::*;

use crate::desk::AgentDesk;
use crate::diff_view::DiffRenderer;
use crate::input::InputState;
use crate::layout::LayoutManager;
use crate::notify::{NotificationConfig, Notifier};
use crate::pane::PaneLayout;
use crate::stream_renderer::StreamRenderer;
use chrono::Timelike;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures::StreamExt;
use maix_core::client::MaixClient;
use maix_core::proto::maix::core::v1 as pb;
use maix_core::types::{CostTracker, Pricing, TokenUsage};
use ratatui::Terminal;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Tool approval request pending user confirmation.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ToolApproval {
    pub name: String,
    pub args: String,
    pub risk_level: i32,
    pub timestamp: std::time::Instant,
}

/// Tool call with timing info for performance tracking.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ToolCallInfo {
    pub name: String,
    pub args: String,
    pub start_time: std::time::Instant,
}

// AgentMode values matching pb::AgentMode enum
pub const MODE_AGENT: i32 = 0;
pub const MODE_PLAN: i32 = 1;
pub const MODE_YOLO: i32 = 2;

pub fn mode_name(mode: i32) -> &'static str {
    match mode {
        MODE_PLAN => "计划模式",
        MODE_YOLO => "自主模式",
        _ => "智能体",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivePanel {
    Memory,
    Tools,
    Stats,
    Desk,
}

impl ActivePanel {
    pub fn next(self) -> Self {
        match self {
            ActivePanel::Memory => ActivePanel::Tools,
            ActivePanel::Tools => ActivePanel::Stats,
            ActivePanel::Stats => ActivePanel::Desk,
            ActivePanel::Desk => ActivePanel::Memory,
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ChatMessage {
    User(String),
    Assistant(String),
    Reasoning(String),
    ToolCall { name: String, args: String },
    ToolResult { result: String },
    System(String),
    Timestamped { time: String, inner: Box<ChatMessage> },
}

#[derive(Debug, Clone)]
pub enum AppEvent {
    TextDelta(String),
    #[allow(dead_code)]
    ReasoningDelta(String),
    ToolCall { name: String, args: String },
    ToolResult { result: String },
    Complete { prompt_tokens: u64, completion_tokens: u64, total_tokens: u64, cache_read_tokens: u64, cache_write_tokens: u64 },
    Error(String),
    #[allow(dead_code)]
    MemoryUpdated,
    StatusUpdate { state: i32 },
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ProviderCaps {
    pub max_context: u64,
    pub supports_reasoning: bool,
    pub supports_tool_use: bool,
}

impl Default for ProviderCaps {
    fn default() -> Self {
        Self {
            max_context: 1_000_000,
            supports_reasoning: true,
            supports_tool_use: true,
        }
    }
}

#[allow(dead_code)]
pub struct App {
    pub model_name: String,
    pub mode: i32,
    pub messages: Vec<ChatMessage>,
    pub memories: Vec<pb::MemoryEntry>,
    pub tool_defs: Vec<pb::ToolInfo>,
    pub input: InputState,
    pub active_panel: ActivePanel,
    pub selected_index: Option<usize>,
    pub is_streaming: Arc<AtomicBool>,
    pub provider_caps: ProviderCaps,
    pub agent_state: Option<String>,
    pub status_detail: Option<String>,
    pub total_tokens: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub total_cost: f64,
    pub round_count: u64,
    pub cost_tracker: CostTracker,
    pub session_id: String,
    pub server_addr: String,
    pub chat_scroll: usize,
    /// Target scroll position for smooth scrolling.
    pub scroll_target: usize,
    /// Scroll animation progress (0.0 to 1.0).
    pub scroll_animation: f64,
    /// Maximum messages to keep in memory.
    max_messages: usize,
    pub tick_count: u64,
    pub show_reasoning: bool,

    client: MaixClient,
    event_tx: tokio::sync::mpsc::UnboundedSender<AppEvent>,
    event_rx: tokio::sync::mpsc::UnboundedReceiver<AppEvent>,
    should_quit: bool,
    custom_cmds: Vec<maix_agent::commands::CustomCommand>,
    pub vim: crate::vim::VimState,
    pub notifier: Notifier,
    pub stream_renderer: StreamRenderer,
    pub desk: AgentDesk,
    /// Text selection mode active.
    pub select_mode: bool,
    /// Selection cursor position (message index, char offset).
    pub select_start: Option<(usize, usize)>,
    pub select_end: Option<(usize, usize)>,
    pub layout: LayoutManager,
    pub diff_renderer: DiffRenderer,
    pub pane_layout: PaneLayout,
    pub palette: crate::palette::CommandPalette,
    /// Search mode active.
    pub search_mode: bool,
    /// Search query.
    pub search_query: String,
    /// Search results (message indices).
    pub search_results: Vec<usize>,
    /// Current search result index.
    pub search_result_index: usize,
    /// Show timestamps on messages.
    pub show_timestamps: bool,
    /// Folded message indices (collapsed long messages).
    pub folded_messages: std::collections::HashSet<usize>,
    /// Side panel width percentage (20-80).
    pub panel_width: u16,
    /// Fullscreen mode.
    pub fullscreen: bool,
    /// Token rate tracking (tokens per second).
    pub token_rate: f64,
    /// Last token count for rate calculation.
    pub last_token_count: u64,
    /// Last rate update time.
    pub last_rate_update: std::time::Instant,
    /// Pending tool approval queue.
    pub pending_tool_approvals: Vec<ToolApproval>,
    /// Auto-approve all tools in current round.
    pub auto_approve_round: bool,
    /// Current tool call timing.
    pub current_tool_call: Option<ToolCallInfo>,
    /// Last failed tool call for retry.
    pub last_failed_tool: Option<ToolCallInfo>,
    /// Command aliases.
    pub aliases: std::collections::HashMap<String, String>,
    /// Show message dividers.
    pub show_dividers: bool,
    /// Multiple sessions support.
    pub sessions: Vec<SessionTab>,
    /// Active session index.
    pub active_session: usize,
    /// Timed reminders.
    pub reminders: Vec<Reminder>,
    /// Next reminder ID.
    pub next_reminder_id: usize,
    /// Current UI theme.
    pub theme: crate::ui::Theme,
    /// Current layout preset name.
    pub layout_preset: String,
    /// Current shortcut scheme: "standard", "vim", "emacs".
    pub shortcut_scheme: String,
    /// Command usage statistics.
    pub command_usage: std::collections::HashMap<String, usize>,
    /// Session start time.
    pub session_start: std::time::Instant,
    /// Habit tracking.
    pub habits: Vec<Habit>,
    /// Tool permissions: tool_name -> (auto_approve, risk_level)
    pub tool_permissions: std::collections::HashMap<String, (bool, i32)>,
    /// Favorite tools list.
    pub favorite_tools: Vec<String>,
    /// Tool usage statistics: tool_name -> (call_count, success_count, total_duration_ms)
    pub tool_stats: std::collections::HashMap<String, (usize, usize, u64)>,
    /// Tool result cache: (tool_name, args_hash) -> (result, timestamp)
    pub tool_cache: std::collections::HashMap<(String, String), (String, std::time::Instant)>,
    /// Completion learning: track selection frequency for ranking
    pub completion_learning: std::collections::HashMap<String, usize>,
    /// Tool chains: list of tool chains
    pub tool_chains: Vec<ToolChain>,
    /// Tool templates: name -> list of tool names
    pub tool_templates: std::collections::HashMap<String, Vec<String>>,
    /// Network request log for debugging (099-013).
    pub network_requests: Vec<NetworkRequest>,
    /// State checkpoints for save/restore (099-017).
    pub checkpoints: Vec<StateCheckpoint>,
    /// Session recording state (099-020).
    pub recording: Option<SessionRecording>,
    /// Debug log entries (099-011).
    pub debug_log: Vec<DebugEntry>,
    /// Currently focused message index for highlighting (099-007).
    pub focused_message: Option<usize>,
    /// Auto-scroll enabled (100-001).
    pub auto_scroll: bool,
    /// Message tags/markers (100-008): msg_idx -> tag
    pub message_tags: std::collections::HashMap<usize, String>,
    /// Pinned messages (100-009).
    pub pinned_messages: Vec<usize>,
    /// Session notes (100-014).
    pub session_notes: String,
    /// Input command history (100-005).
    pub command_history: Vec<String>,
    /// Command history navigation index.
    pub history_index: Option<usize>,
    /// Tool call expanded state (100-007): msg_idx -> expanded
    pub expanded_tool_calls: std::collections::HashSet<usize>,
    /// Message references (101-001): msg_idx -> referenced_msg_idx
    pub message_references: std::collections::HashMap<usize, usize>,
    /// Command favorites (101-011).
    pub command_favorites: Vec<String>,
    /// Archived messages (101-003).
    pub archived_messages: Vec<ChatMessage>,
    /// Layout presets (101-021).
    pub layout_presets: std::collections::HashMap<String, (u16, bool)>,
    /// Code snippets library (102-002): name -> (language, code)
    pub code_snippets: std::collections::HashMap<String, (String, String)>,
    /// Git repository status (102-021).
    pub git_status: Option<crate::git_status::GitStatus>,
    /// Workflow definitions (103-001): name -> workflow steps
    pub workflows: std::collections::HashMap<String, Vec<WorkflowStep>>,
    /// Macro recording (103-012): recorded commands
    pub macro_recording: Option<Vec<String>>,
    /// Macro library (103-012): name -> commands
    pub macros: std::collections::HashMap<String, Vec<String>>,
    /// Dirty regions for incremental rendering (105-002).
    pub dirty_regions: Vec<DirtyRegion>,
    /// Last render frame number for change detection.
    pub last_render_frame: u64,
    /// Current frame number.
    pub frame_number: u64,
    /// Search index for faster full-text search (105-003).
    pub search_index: SearchIndex,
}

/// A chain of tool calls to execute sequentially.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ToolChain {
    pub name: String,
    pub steps: Vec<ToolChainStep>,
    pub created_at: chrono::NaiveDateTime,
}

/// A step in a tool chain.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ToolChainStep {
    pub tool_name: String,
    pub args_template: String,
    pub condition: Option<String>,
}

/// A workflow step with conditional logic.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct WorkflowStep {
    pub tool_name: String,
    pub args: String,
    pub condition: Option<String>,
    pub on_success: Option<String>,
    pub on_failure: Option<String>,
}

/// A session tab for multiple conversations.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SessionTab {
    pub id: String,
    pub name: String,
    pub created_at: chrono::NaiveDateTime,
    pub message_count: usize,
    pub last_message: Option<String>,
}

impl SessionTab {
    pub fn new(id: String, name: String) -> Self {
        Self {
            id,
            name,
            created_at: chrono::Local::now().naive_local(),
            message_count: 0,
            last_message: None,
        }
    }
}

/// A habit for tracking user patterns.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Habit {
    pub name: String,
    pub pattern: String,
    pub count: usize,
    pub last_seen: chrono::NaiveDateTime,
}

impl Habit {
    #[allow(dead_code)]
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            pattern: String::new(),
            count: 0,
            last_seen: chrono::Local::now().naive_local(),
        }
    }

    #[allow(dead_code)]
    pub fn complete(&mut self) {
        self.count += 1;
        self.last_seen = chrono::Local::now().naive_local();
    }

    #[allow(dead_code)]
    pub fn is_completed_today(&self) -> bool {
        let today = chrono::Local::now().naive_local().date();
        self.last_seen.date() == today
    }
}

/// A timed reminder.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Reminder {
    pub id: usize,
    pub message: String,
    pub due_time: chrono::NaiveDateTime,
    pub created_at: chrono::NaiveDateTime,
    pub recurring: bool,
    pub interval: Option<std::time::Duration>,
    pub triggered: bool,
}

impl Reminder {
    #[allow(dead_code)]
    pub fn new(id: usize, message: String, duration: std::time::Duration) -> Self {
        Self {
            id,
            message,
            due_time: chrono::Local::now().naive_local() + chrono::Duration::from_std(duration).unwrap_or(chrono::Duration::hours(1)),
            created_at: chrono::Local::now().naive_local(),
            recurring: false,
            interval: None,
            triggered: false,
        }
    }

    pub fn is_due(&self) -> bool {
        chrono::Local::now().naive_local() >= self.due_time
    }

    #[allow(dead_code)]
    pub fn remaining(&self) -> std::time::Duration {
        let now = chrono::Local::now().naive_local();
        if now >= self.due_time {
            std::time::Duration::ZERO
        } else {
            (self.due_time - now).to_std().unwrap_or(std::time::Duration::ZERO)
        }
    }
}

/// Network request log entry.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct NetworkRequest {
    pub timestamp: chrono::NaiveDateTime,
    pub method: String,
    pub url: String,
    pub status: u16,
    pub duration_ms: u64,
    pub size_bytes: usize,
}

/// State checkpoint for save/restore.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct StateCheckpoint {
    pub name: String,
    pub timestamp: chrono::NaiveDateTime,
    pub messages: Vec<ChatMessage>,
    pub session_id: String,
}

/// Session recording state.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SessionRecording {
    pub start_time: chrono::NaiveDateTime,
    pub events: Vec<RecordedEvent>,
    pub is_recording: bool,
}

/// A recorded event in session recording.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RecordedEvent {
    pub timestamp: chrono::NaiveDateTime,
    pub event_type: String,
    pub data: String,
}

/// Debug log entry.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DebugEntry {
    pub timestamp: chrono::NaiveDateTime,
    pub level: String,
    pub message: String,
    pub context: Option<String>,
}

/// Dirty region for incremental rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum DirtyRegion {
    Full,
    Chat,
    Sidebar,
    Input,
    StatusBar,
    Header,
}

/// Search index for full-text search.
#[derive(Debug, Clone, Default)]
pub struct SearchIndex {
    pub entries: Vec<SearchEntry>,
    pub dirty: bool,
}

/// A search index entry.
#[derive(Debug, Clone)]
pub struct SearchEntry {
    pub message_index: usize,
    pub content: String,
    pub tokens: Vec<String>,
}

impl SearchIndex {
    pub fn rebuild(&mut self, messages: &[ChatMessage]) {
        self.entries.clear();
        self.dirty = false;
        for (i, msg) in messages.iter().enumerate() {
            let content = match msg {
                ChatMessage::User(text) | ChatMessage::Assistant(text) | ChatMessage::System(text) => text.clone(),
                ChatMessage::ToolCall { name, args } => format!("{name} {args}"),
                ChatMessage::ToolResult { result } => result.clone(),
                ChatMessage::Reasoning(text) => text.clone(),
                ChatMessage::Timestamped { inner, .. } => {
                    match inner.as_ref() {
                        ChatMessage::User(text) | ChatMessage::Assistant(text) | ChatMessage::System(text) => text.clone(),
                        _ => continue,
                    }
                }
            };

            let tokens: Vec<String> = content
                .split_whitespace()
                .map(|s| s.to_lowercase())
                .collect();

            self.entries.push(SearchEntry {
                message_index: i,
                content,
                tokens,
            });
        }
    }

    pub fn search(&self, query: &str) -> Vec<usize> {
        let query_lower = query.to_lowercase();
        let query_tokens: Vec<&str> = query_lower.split_whitespace().collect();

        self.entries
            .iter()
            .filter(|entry| {
                entry.content.to_lowercase().contains(&query_lower)
                    || query_tokens.iter().any(|t| entry.tokens.contains(&t.to_string()))
            })
            .map(|entry| entry.message_index)
            .collect()
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }
}

impl App {
    /// Mark a region as dirty for incremental rendering.
    pub fn mark_dirty(&mut self, region: DirtyRegion) {
        if !self.dirty_regions.contains(&region) {
            self.dirty_regions.push(region);
        }
    }

    /// Clear all dirty regions after render.
    pub fn clear_dirty(&mut self) {
        self.dirty_regions.clear();
        self.last_render_frame = self.frame_number;
    }

    /// Check if a specific region needs redraw.
    #[allow(dead_code)]
    pub fn is_dirty(&self, region: &DirtyRegion) -> bool {
        self.dirty_regions.contains(region) || self.dirty_regions.contains(&DirtyRegion::Full)
    }

    /// Check if any region needs redraw.
    pub fn has_changes(&self) -> bool {
        !self.dirty_regions.is_empty()
    }

    /// Truncate old messages to prevent memory issues in long-running sessions.
    /// Keeps the most recent `max_messages` messages.
    pub fn truncate_messages(&mut self) {
        let max = self.max_messages;
        if self.messages.len() > max {
            let excess = self.messages.len() - max;
            // Keep a summary of removed messages
            let removed_count = excess;
            self.messages.drain(..excess);
            // Insert a summary at the beginning
            self.messages.insert(0, ChatMessage::System(format!(
                "... ({removed_count} older messages truncated for memory)"
            )));
        }
    }

    /// Get current memory usage estimate (approximate).
    #[allow(dead_code)]
    pub fn memory_estimate(&self) -> usize {
        let msg_size: usize = self.messages.iter().map(|m| match m {
            ChatMessage::User(s) | ChatMessage::Assistant(s) | ChatMessage::System(s) | ChatMessage::Reasoning(s) => s.len(),
            ChatMessage::ToolCall { name, args } => name.len() + args.len(),
            ChatMessage::ToolResult { result } => result.len(),
            ChatMessage::Timestamped { inner, .. } => match inner.as_ref() {
                ChatMessage::User(s) | ChatMessage::Assistant(s) | ChatMessage::System(s) => s.len(),
                _ => 0,
            },
        }).sum();
        msg_size + self.memories.len() * 256 + self.tool_defs.len() * 512
    }
}
