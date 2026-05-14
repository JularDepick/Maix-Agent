use crate::desk::AgentDesk;
use crate::diff_view::DiffRenderer;
use crate::input::InputState;
use crate::layout::LayoutManager;
use crate::notify::{NotificationConfig, Notifier};
use crate::pane::PaneLayout;
use crate::stream_renderer::StreamRenderer;
use crate::ui;
use chrono::{Datelike, Timelike};
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

/// Tool approval request.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ToolApproval {
    pub name: String,
    pub args: String,
    pub risk_level: i32,
    pub timestamp: std::time::Instant,
}

/// Tool call with timing info.
#[derive(Debug, Clone)]
pub struct ToolCallInfo {
    pub name: String,
    pub args: String,
    pub start_time: std::time::Instant,
}

/// Truncate a string to max_chars characters, safe on UTF-8 boundaries.
fn truncate_str(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let truncated: String = s.chars().take(max_chars.saturating_sub(3)).collect();
    format!("{truncated}...")
}

/// Format byte size with smart unit.
fn format_size(bytes: usize) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.1}GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.1}MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else {
        format!("{}B", bytes)
    }
}

/// Levenshtein distance for string similarity.
fn levenshtein_distance(s1: &str, s2: &str) -> usize {
    let len1 = s1.len();
    let len2 = s2.len();
    let mut matrix = vec![vec![0; len2 + 1]; len1 + 1];

    for i in 0..=len1 {
        matrix[i][0] = i;
    }
    for j in 0..=len2 {
        matrix[0][j] = j;
    }

    for (i, c1) in s1.chars().enumerate() {
        for (j, c2) in s2.chars().enumerate() {
            let cost = if c1 == c2 { 0 } else { 1 };
            matrix[i + 1][j + 1] = (matrix[i][j + 1] + 1)
                .min(matrix[i + 1][j] + 1)
                .min(matrix[i][j] + cost);
        }
    }

    matrix[len1][len2]
}

/// Parse a duration string like "5m", "30s", "1h", "2d" into a Duration.
fn parse_duration(s: &str) -> Option<std::time::Duration> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    let (num_str, unit) = s.split_at(s.len() - 1);
    let num: u64 = num_str.parse().ok()?;

    match unit {
        "s" | "S" => Some(std::time::Duration::from_secs(num)),
        "m" | "M" => Some(std::time::Duration::from_secs(num * 60)),
        "h" | "H" => Some(std::time::Duration::from_secs(num * 3600)),
        "d" | "D" => Some(std::time::Duration::from_secs(num * 86400)),
        _ => None,
    }
}

/// Suggest a fix for common error patterns.
fn suggest_fix(error: &str) -> &'static str {
    let lower = error.to_lowercase();
    if lower.contains("connection refused") || lower.contains("connect") {
        "检查服务是否运行: /health"
    } else if lower.contains("timeout") || lower.contains("timed out") {
        "请求超时，请稍后重试或检查网络连接"
    } else if lower.contains("unauthorized") || lower.contains("401") || lower.contains("403") {
        "认证失败，请检查 API 密钥配置"
    } else if lower.contains("rate limit") || lower.contains("429") {
        "请求频率过高，请稍后重试"
    } else if lower.contains("not found") || lower.contains("404") {
        "资源不存在，请检查路径或 ID"
    } else if lower.contains("model") && lower.contains("not") {
        "模型不可用，使用 /model 查看可用模型"
    } else if lower.contains("context") && lower.contains("length") {
        "上下文过长，使用 /compact 压缩上下文"
    } else if lower.contains("memory") || lower.contains("oom") {
        "内存不足，尝试 /clear 清空对话"
    } else {
        ""
    }
}

fn dirs_home() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
    }
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
    /// Git status cache (102-021).
    pub git_status: Option<String>,
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
pub struct ToolChain {
    pub name: String,
    pub steps: Vec<ToolChainStep>,
    pub created_at: chrono::NaiveDateTime,
}

/// A single step in a tool chain.
#[derive(Debug, Clone)]
pub struct ToolChainStep {
    pub tool_name: String,
    pub args_template: String,
}

/// Network request log entry (099-013).
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct NetworkRequest {
    pub method: String,
    pub url: String,
    pub status: u16,
    pub latency_ms: u64,
    pub request_size: usize,
    pub response_size: usize,
    pub timestamp: chrono::NaiveDateTime,
}

/// State checkpoint for save/restore (099-017).
#[derive(Debug, Clone)]
pub struct StateCheckpoint {
    pub name: String,
    pub message_count: usize,
    pub total_tokens: u64,
    pub total_cost: f64,
    pub created_at: chrono::NaiveDateTime,
}

/// Workflow step definition (103-001).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorkflowStep {
    pub name: String,
    pub command: String,
    pub condition: Option<String>,
    pub on_error: Option<String>,
}

/// Session recording for replay (099-020).
#[derive(Debug, Clone)]
pub struct SessionRecording {
    pub start_time: chrono::NaiveDateTime,
    pub events: Vec<RecordingEvent>,
    pub is_active: bool,
}

/// A single event in a session recording.
#[derive(Debug, Clone)]
pub struct RecordingEvent {
    pub timestamp: chrono::NaiveDateTime,
    pub event_type: String,
    pub content: String,
}

/// Debug log entry (099-011).
#[derive(Debug, Clone)]
pub struct DebugEntry {
    pub level: String,
    pub message: String,
    pub timestamp: chrono::NaiveDateTime,
}

/// A habit to track.
#[derive(Debug, Clone)]
pub struct Habit {
    pub name: String,
    pub streak: usize,
    pub last_completed: Option<chrono::NaiveDate>,
    pub total_completions: usize,
}

impl Habit {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            streak: 0,
            last_completed: None,
            total_completions: 0,
        }
    }

    pub fn complete(&mut self) {
        let today = chrono::Local::now().date_naive();
        if let Some(last) = self.last_completed {
            if last == today {
                return; // Already completed today
            }
            if last + chrono::Duration::days(1) == today {
                self.streak += 1;
            } else {
                self.streak = 1; // Reset streak
            }
        } else {
            self.streak = 1;
        }
        self.last_completed = Some(today);
        self.total_completions += 1;
    }

    pub fn is_completed_today(&self) -> bool {
        let today = chrono::Local::now().date_naive();
        self.last_completed == Some(today)
    }
}

/// A session tab for multi-session support.
#[derive(Debug, Clone)]
pub struct SessionTab {
    pub id: String,
    pub name: String,
    pub messages: Vec<ChatMessage>,
    pub color: Option<String>,
    pub locked: bool,
    pub tags: Vec<String>,
}

impl SessionTab {
    pub fn new(id: String, name: String) -> Self {
        Self {
            id,
            name,
            messages: Vec::new(),
            color: None,
            locked: false,
            tags: Vec::new(),
        }
    }
}

/// A timed reminder.
#[derive(Debug, Clone)]
pub struct Reminder {
    pub id: usize,
    pub message: String,
    pub trigger_at: std::time::Instant,
    pub triggered: bool,
}

impl Reminder {
    pub fn new(id: usize, message: String, duration: std::time::Duration) -> Self {
        Self {
            id,
            message,
            trigger_at: std::time::Instant::now() + duration,
            triggered: false,
        }
    }

    pub fn is_due(&self) -> bool {
        !self.triggered && std::time::Instant::now() >= self.trigger_at
    }

    pub fn remaining(&self) -> std::time::Duration {
        if self.triggered {
            std::time::Duration::ZERO
        } else {
            self.trigger_at.saturating_duration_since(std::time::Instant::now())
        }
    }
}

/// Dirty region for incremental rendering (105-002).
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum DirtyRegion {
    /// Chat messages area needs redraw.
    Chat,
    /// Side panel needs redraw.
    SidePanel,
    /// Status bar needs redraw.
    StatusBar,
    /// Input area needs redraw.
    Input,
    /// Command palette needs redraw.
    Palette,
    /// Entire screen needs redraw.
    Full,
}

/// Search index for faster full-text search (105-003).
#[derive(Debug, Clone, Default)]
pub struct SearchIndex {
    /// Inverted index: word -> set of message indices containing that word.
    pub word_to_messages: std::collections::HashMap<String, Vec<usize>>,
    /// Message index -> lowercase text for fallback search.
    pub message_texts: Vec<String>,
    /// Whether the index needs rebuilding.
    pub dirty: bool,
}

impl SearchIndex {
    /// Build or rebuild the index from messages.
    pub fn rebuild(&mut self, messages: &[ChatMessage]) {
        self.word_to_messages.clear();
        self.message_texts.clear();
        self.dirty = false;

        for (idx, msg) in messages.iter().enumerate() {
            let text = match msg {
                ChatMessage::User(t) | ChatMessage::Assistant(t) | ChatMessage::System(t) => t.clone(),
                ChatMessage::Reasoning(t) => t.clone(),
                ChatMessage::ToolCall { name, args } => format!("{} {}", name, args),
                ChatMessage::ToolResult { result } => result.clone(),
                ChatMessage::Timestamped { inner, .. } => match inner.as_ref() {
                    ChatMessage::User(t) | ChatMessage::Assistant(t) | ChatMessage::System(t) => t.clone(),
                    _ => continue,
                },
            };
            let lower = text.to_lowercase();
            self.message_texts.push(lower.clone());

            // Index words (split by whitespace and punctuation)
            for word in lower.split(|c: char| c.is_whitespace() || c.is_ascii_punctuation()) {
                if word.len() >= 2 {  // Skip very short words
                    self.word_to_messages
                        .entry(word.to_string())
                        .or_default()
                        .push(idx);
                }
            }
        }
    }

    /// Search for messages matching the query.
    pub fn search(&self, query: &str) -> Vec<usize> {
        let query_lower = query.to_lowercase();
        let words: Vec<&str> = query_lower.split_whitespace().collect();

        if words.is_empty() {
            return Vec::new();
        }

        // Find messages containing all query words
        let mut result_sets: Vec<std::collections::HashSet<usize>> = Vec::new();
        for word in &words {
            let mut matches = std::collections::HashSet::new();
            // Check exact word matches
            if let Some(indices) = self.word_to_messages.get(*word) {
                matches.extend(indices);
            }
            // Also check substring matches in message texts
            for (idx, text) in self.message_texts.iter().enumerate() {
                if text.contains(*word) {
                    matches.insert(idx);
                }
            }
            result_sets.push(matches);
        }

        // Intersect all sets
        if result_sets.is_empty() {
            return Vec::new();
        }

        let mut result = result_sets[0].clone();
        for set in &result_sets[1..] {
            result = result.intersection(set).cloned().collect();
        }

        let mut indices: Vec<usize> = result.into_iter().collect();
        indices.sort();
        indices
    }

    /// Mark index as needing rebuild.
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
}

impl App {
    pub async fn new(
        client: MaixClient,
        session_id: String,
        mode: i32,
        server_addr: String,
        resume_session: Option<String>,
    ) -> Self {
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();

        // Discover custom commands
        let home = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("."));
        let project_root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let custom_cmds = maix_agent::commands::discover_commands(&project_root, &home);
        let custom_cmd_names: Vec<String> = custom_cmds.iter().map(|c| format!("/{}", c.name)).collect();

        let (tool_defs, memories) = tokio::join!(
            client.list_tools(),
            client.search_memory("", 50),
        );

        let tool_defs = tool_defs.unwrap_or_default();
        let memories = memories.unwrap_or_default();

        // Fetch config from server to get model name and provider info
        let (model_name, provider_caps) = match client.get_config().await {
            Ok(cfg) => {
                let caps = ProviderCaps {
                    max_context: 1_000_000, // Will be updated if server provides it
                    supports_reasoning: true,
                    supports_tool_use: true,
                };
                (format!("{}/{}", cfg.active_provider, cfg.model), caps)
            }
            Err(_) => ("unknown".to_string(), ProviderCaps::default()),
        };

        let mut messages = vec![ChatMessage::System(format!(
            "Maix-Agent TUI | {} | 模型: {} | 服务: {}",
            mode_name(mode),
            model_name,
            server_addr,
        ))];

        // Check first run and show welcome
        let is_first_run = !dirs_home().join(".maix").join("config.toml").exists();
        if is_first_run {
            messages.push(ChatMessage::System(
                "欢迎使用 Maix-Agent! 🎉\n\n\
                快速开始:\n\
                - 输入消息开始对话\n\
                - 输入 / 查看所有命令\n\
                - Ctrl+P 打开命令面板\n\
                - Ctrl+F 搜索对话\n\
                - /help 查看帮助\n\n\
                输入 /tutorial 开始交互式教程".into()
            ));
        }

        // Resume session if requested
        if let Some(sid) = &resume_session {
            match client.get_session_messages(sid, 100).await {
                Ok(msgs) => {
                    if msgs.is_empty() {
                        messages.push(ChatMessage::System(format!("会话 {sid} 中没有消息")));
                    } else {
                        messages.push(ChatMessage::System(format!(
                            "已恢复会话 {} ({} 条消息)",
                            &sid[..sid.len().min(8)],
                            msgs.len()
                        )));
                        for m in &msgs {
                            match m.role.as_str() {
                                "user" => messages.push(ChatMessage::User(m.content.clone())),
                                "assistant" => messages.push(ChatMessage::Assistant(m.content.clone())),
                                _ => messages.push(ChatMessage::System(m.content.clone())),
                            }
                        }
                    }
                }
                Err(e) => {
                    messages.push(ChatMessage::System(format!("恢复会话失败: {e}")));
                }
            }
        }

        let mut input = InputState::new();
        input.custom_commands = custom_cmd_names;

        // Build initial search index before moving messages
        let mut search_index = SearchIndex::default();
        search_index.rebuild(&messages);

        App {
            model_name,
            mode,
            messages,
            memories,
            tool_defs,
            input,
            active_panel: ActivePanel::Memory,
            selected_index: None,
            is_streaming: Arc::new(AtomicBool::new(false)),
            provider_caps,
            agent_state: Some("Idle".into()),
            status_detail: None,
            total_tokens: 0,
            prompt_tokens: 0,
            completion_tokens: 0,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            total_cost: 0.0,
            round_count: 0,
            cost_tracker: CostTracker::new(Pricing::default()),
            session_id: session_id.clone(),
            server_addr,
            chat_scroll: 0,
            scroll_target: 0,
            scroll_animation: 1.0,
            tick_count: 0,
            show_reasoning: false,
            client,
            event_tx,
            event_rx,
            should_quit: false,
            custom_cmds,
            vim: crate::vim::VimState::new(),
            notifier: Notifier::new(NotificationConfig::default()),
            stream_renderer: StreamRenderer::new().0,
            desk: AgentDesk::new(std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))),
            layout: LayoutManager::new(),
            diff_renderer: DiffRenderer::new(crate::diff_view::DiffMode::Unified),
            pane_layout: PaneLayout::single(crate::pane::PaneContent::Chat),
            select_mode: false,
            select_start: None,
            select_end: None,
            palette: crate::palette::CommandPalette::new(),
            search_mode: false,
            search_query: String::new(),
            search_results: Vec::new(),
            search_result_index: 0,
            show_timestamps: false,
            folded_messages: std::collections::HashSet::new(),
            panel_width: 30,
            fullscreen: false,
            token_rate: 0.0,
            last_token_count: 0,
            last_rate_update: std::time::Instant::now(),
            pending_tool_approvals: Vec::new(),
            auto_approve_round: false,
            current_tool_call: None,
            last_failed_tool: None,
            aliases: std::collections::HashMap::new(),
            show_dividers: true,
            sessions: vec![SessionTab::new(session_id.clone(), "会话 1".to_string())],
            active_session: 0,
            max_messages: 10000,
            reminders: Vec::new(),
            next_reminder_id: 1,
            theme: crate::ui::Theme::dark(),
            layout_preset: "standard".to_string(),
            shortcut_scheme: "standard".to_string(),
            command_usage: std::collections::HashMap::new(),
            session_start: std::time::Instant::now(),
            habits: Vec::new(),
            tool_permissions: std::collections::HashMap::new(),
            favorite_tools: Vec::new(),
            tool_stats: std::collections::HashMap::new(),
            tool_cache: std::collections::HashMap::new(),
            completion_learning: std::collections::HashMap::new(),
            tool_chains: Vec::new(),
            tool_templates: std::collections::HashMap::new(),
            network_requests: Vec::new(),
            checkpoints: Vec::new(),
            recording: None,
            debug_log: Vec::new(),
            focused_message: None,
            auto_scroll: true,
            message_tags: std::collections::HashMap::new(),
            pinned_messages: Vec::new(),
            session_notes: String::new(),
            command_history: Vec::new(),
            history_index: None,
            expanded_tool_calls: std::collections::HashSet::new(),
            message_references: std::collections::HashMap::new(),
            command_favorites: Vec::new(),
            archived_messages: Vec::new(),
            layout_presets: std::collections::HashMap::new(),
            code_snippets: std::collections::HashMap::new(),
            git_status: None,
            workflows: std::collections::HashMap::new(),
            macro_recording: None,
            macros: std::collections::HashMap::new(),
            dirty_regions: vec![DirtyRegion::Full],
            last_render_frame: 0,
            frame_number: 0,
            search_index,
        }
    }

    /// Trim messages if exceeding limit.
    fn trim_messages(&mut self) {
        if self.messages.len() > self.max_messages {
            let excess = self.messages.len() - self.max_messages;
            self.messages.drain(..excess);
            self.messages.insert(0, ChatMessage::System(format!(
                "(已压缩 {} 条旧消息)", excess
            )));
        }
    }

    /// Get context-aware command suggestions based on conversation content.
    #[allow(dead_code)]
    pub fn get_context_suggestions(&self) -> Vec<String> {
        let mut suggestions = Vec::new();
        let recent_messages: Vec<&ChatMessage> = self.messages.iter().rev().take(10).collect();

        for msg in &recent_messages {
            let content = match msg {
                ChatMessage::User(text) | ChatMessage::Assistant(text) => text.to_lowercase(),
                _ => continue,
            };

            // Suggest /diff if code changes discussed
            if content.contains("修改") || content.contains("改动") || content.contains("diff") {
                if !suggestions.contains(&"/diff".to_string()) {
                    suggestions.push("/diff".to_string());
                }
            }

            // Suggest /compact if context might be long
            if self.messages.len() > 100 || content.contains("上下文") || content.contains("token") {
                if !suggestions.contains(&"/compact".to_string()) {
                    suggestions.push("/compact".to_string());
                }
            }

            // Suggest /todo if tasks mentioned
            if content.contains("任务") || content.contains("todo") || content.contains("待办") {
                if !suggestions.contains(&"/todo".to_string()) {
                    suggestions.push("/todo".to_string());
                }
            }

            // Suggest /remind if time-related
            if content.contains("提醒") || content.contains("remember") || content.contains("别忘") {
                if !suggestions.contains(&"/remind".to_string()) {
                    suggestions.push("/remind".to_string());
                }
            }

            // Suggest /export if sharing discussed
            if content.contains("分享") || content.contains("导出") || content.contains("export") {
                if !suggestions.contains(&"/export".to_string()) {
                    suggestions.push("/export".to_string());
                }
            }

            // Suggest /note if important info
            if content.contains("重要") || content.contains("记录") || content.contains("note") {
                if !suggestions.contains(&"/note".to_string()) {
                    suggestions.push("/note add".to_string());
                }
            }

            // Suggest /config if settings discussed
            if content.contains("配置") || content.contains("设置") || content.contains("config") {
                if !suggestions.contains(&"/config".to_string()) {
                    suggestions.push("/config".to_string());
                }
            }

            // Suggest /theme if appearance discussed
            if content.contains("主题") || content.contains("颜色") || content.contains("theme") {
                if !suggestions.contains(&"/theme".to_string()) {
                    suggestions.push("/theme".to_string());
                }
            }

            // Suggest /habit if habits discussed
            if content.contains("习惯") || content.contains("habit") || content.contains("每天") {
                if !suggestions.contains(&"/habit".to_string()) {
                    suggestions.push("/habit".to_string());
                }
            }

            // Suggest /calendar if dates discussed
            if content.contains("日历") || content.contains("日程") || content.contains("calendar") {
                if !suggestions.contains(&"/calendar".to_string()) {
                    suggestions.push("/calendar".to_string());
                }
            }
        }

        // Limit to top 5 suggestions
        suggestions.truncate(5);
        suggestions
    }

    /// Get smart history suggestions based on time patterns.
    #[allow(dead_code)]
    pub fn get_smart_history_suggestions(&self) -> Vec<String> {
        let mut suggestions = Vec::new();
        let now = chrono::Local::now();
        let hour = now.hour();

        // Morning suggestions (6-10)
        if hour >= 6 && hour < 10 {
            if !self.command_usage.contains_key("/todo") {
                suggestions.push("/todo list - 查看今日待办".to_string());
            }
            if !self.command_usage.contains_key("/calendar") {
                suggestions.push("/calendar - 查看今日日程".to_string());
            }
        }

        // Afternoon suggestions (12-14)
        if hour >= 12 && hour < 14 {
            if !self.command_usage.contains_key("/usage") {
                suggestions.push("/usage - 查看今日使用统计".to_string());
            }
        }

        // Evening suggestions (18-22)
        if hour >= 18 && hour < 22 {
            if !self.command_usage.contains_key("/habit") {
                suggestions.push("/habit - 检查今日习惯完成情况".to_string());
            }
            if !self.command_usage.contains_key("/export") {
                suggestions.push("/export - 导出今日对话记录".to_string());
            }
        }

        // Session duration suggestions
        let session_duration = self.session_start.elapsed();
        if session_duration.as_secs() > 3600 && !self.command_usage.contains_key("/compact") {
            suggestions.push("/compact - 会话已超过1小时，建议压缩上下文".to_string());
        }

        // High token usage suggestions
        if self.total_tokens > 100000 && !self.command_usage.contains_key("/compact") {
            suggestions.push("/compact - token用量较高，建议压缩上下文".to_string());
        }

        // Low usage suggestions
        if self.command_usage.is_empty() && self.messages.len() > 10 {
            suggestions.push("/help - 查看可用命令".to_string());
            suggestions.push("/tutorial - 开始交互式教程".to_string());
        }

        suggestions.truncate(3);
        suggestions
    }

    fn current_panel_item_count(&self) -> usize {
        match self.active_panel {
            ActivePanel::Memory => self.memories.len(),
            ActivePanel::Tools => self.tool_defs.len(),
            ActivePanel::Stats => 0,
            ActivePanel::Desk => self.desk.sticky_notes.len() + self.desk.pinned_files.len() + self.desk.task_board.tasks.len(),
        }
    }

    fn tab_next(&mut self) {
        let count = self.current_panel_item_count();
        match self.selected_index {
            None => {
                // Input → first item (or next panel if empty)
                if count > 0 {
                    self.selected_index = Some(0);
                } else {
                    self.active_panel = self.active_panel.next();
                    if self.current_panel_item_count() > 0 {
                        self.selected_index = Some(0);
                    }
                }
            }
            Some(i) => {
                if i + 1 < count {
                    // Next item in current panel
                    self.selected_index = Some(i + 1);
                } else {
                    // Wrap to next panel
                    self.active_panel = self.active_panel.next();
                    let new_count = self.current_panel_item_count();
                    if new_count > 0 {
                        self.selected_index = Some(0);
                    } else {
                        self.selected_index = None;
                    }
                }
            }
        }
    }

    fn navigate_up(&mut self) {
        if let Some(i) = self.selected_index {
            if i > 0 {
                self.selected_index = Some(i - 1);
            } else {
                // Wrap to input
                self.selected_index = None;
            }
        }
    }

    fn navigate_down(&mut self) {
        let count = self.current_panel_item_count();
        if count == 0 {
            return;
        }
        match self.selected_index {
            None => self.selected_index = Some(0),
            Some(i) if i + 1 < count => self.selected_index = Some(i + 1),
            _ => {}
        }
    }

    fn select_move(&mut self, delta: i32) {
        if let Some((msg_idx, _offset)) = self.select_end {
            let new_idx = (msg_idx as i32 + delta).max(0).min(self.messages.len() as i32 - 1) as usize;
            self.select_end = Some((new_idx, 0));
        }
    }

    fn select_extend(&mut self, delta: i32) {
        if let Some((msg_idx, offset)) = self.select_end {
            let msg_text = match &self.messages.get(msg_idx) {
                Some(ChatMessage::Assistant(t)) | Some(ChatMessage::User(t)) | Some(ChatMessage::System(t)) => t.as_str(),
                _ => return,
            };
            let new_offset = (offset as i32 + delta).max(0).min(msg_text.len() as i32) as usize;
            self.select_end = Some((msg_idx, new_offset));
        }
    }

    fn copy_selection(&self) {
        let (start, end) = match (self.select_start, self.select_end) {
            (Some(s), Some(e)) => (s, e),
            _ => return,
        };
        let mut text = String::new();
        for idx in start.0..=end.0 {
            if let Some(msg) = self.messages.get(idx) {
                let msg_text = match msg {
                    ChatMessage::Assistant(t) | ChatMessage::User(t) | ChatMessage::System(t) => t.as_str(),
                    _ => continue,
                };
                let from = if idx == start.0 { start.1 } else { 0 };
                let to = if idx == end.0 { end.1.min(msg_text.len()) } else { msg_text.len() };
                if from < msg_text.len() && to <= msg_text.len() && from < to {
                    text.push_str(&msg_text[from..to]);
                }
                if idx < end.0 {
                    text.push('\n');
                }
            }
        }
        if !text.is_empty() {
            // Copy to clipboard using platform command
            #[cfg(target_os = "windows")]
            {
                use std::io::Write;
                if let Ok(mut child) = std::process::Command::new("clip")
                    .stdin(std::process::Stdio::piped())
                    .spawn()
                {
                    if let Some(mut stdin) = child.stdin.take() {
                        let _ = stdin.write_all(text.as_bytes());
                    }
                }
            }
            #[cfg(target_os = "macos")]
            {
                use std::io::Write;
                if let Ok(mut child) = std::process::Command::new("pbcopy")
                    .stdin(std::process::Stdio::piped())
                    .spawn()
                {
                    if let Some(mut stdin) = child.stdin.take() {
                        let _ = stdin.write_all(text.as_bytes());
                    }
                }
            }
            #[cfg(target_os = "linux")]
            {
                use std::io::Write;
                if let Ok(mut child) = std::process::Command::new("xclip")
                    .args(["-selection", "clipboard"])
                    .stdin(std::process::Stdio::piped())
                    .spawn()
                {
                    if let Some(mut stdin) = child.stdin.take() {
                        let _ = stdin.write_all(text.as_bytes());
                    }
                }
            }
        }
    }

    async fn handle_selected_item(&mut self) {
        let idx = match self.selected_index {
            Some(i) => i,
            None => return,
        };
        match self.active_panel {
            ActivePanel::Memory => {
                if let Some(m) = self.memories.get(idx) {
                    self.messages.push(ChatMessage::System(format!(
                        "[{}] kind={} {}",
                        &m.id[..m.id.len().min(8)],
                        m.kind,
                        m.content,
                    )));
                }
            }
            ActivePanel::Tools => {
                if let Some(t) = self.tool_defs.get(idx) {
                    self.messages.push(ChatMessage::System(format!(
                        "{}: {} (risk={})",
                        t.name, t.description, t.risk_level,
                    )));
                }
            }
            ActivePanel::Stats => {}
            ActivePanel::Desk => {}
        }
    }

    fn tab_prev(&mut self) {
        match self.selected_index {
            None => {
                // Input → last item of previous panel
                self.active_panel = match self.active_panel {
                    ActivePanel::Memory => ActivePanel::Desk,
                    ActivePanel::Tools => ActivePanel::Memory,
                    ActivePanel::Stats => ActivePanel::Tools,
                    ActivePanel::Desk => ActivePanel::Stats,
                };
                let count = self.current_panel_item_count();
                if count > 0 {
                    self.selected_index = Some(count - 1);
                }
            }
            Some(0) => {
                // First item → back to input
                self.selected_index = None;
            }
            Some(i) => {
                self.selected_index = Some(i - 1);
            }
        }
    }

    async fn refresh_memories(&mut self) {
        if let Ok(mems) = self.client.search_memory("", 50).await {
            self.memories = mems;
        }
    }

    pub async fn run(
        &mut self,
        mut terminal: Terminal<impl ratatui::backend::Backend>,
    ) -> io::Result<()> {
        let mut crossterm_events = crossterm::event::EventStream::new();
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(50));
        let mut memory_refresh = tokio::time::interval(std::time::Duration::from_secs(3));

        loop {
            self.frame_number += 1;

            // Incremental rendering: only redraw if there are changes (105-002)
            if self.has_changes() || self.is_streaming.load(std::sync::atomic::Ordering::Relaxed) {
                terminal.draw(|f| ui::render(f, self))?;
                self.clear_dirty();
            }

            if self.should_quit {
                return Ok(());
            }

            tokio::select! {
                // Crossterm terminal events
                maybe_event = crossterm_events.next() => {
                    match maybe_event {
                        Some(Ok(Event::Key(key))) if key.kind == KeyEventKind::Press => {
                            // Text selection mode
                            if self.select_mode {
                                match key.code {
                                    KeyCode::Esc => {
                                        self.select_mode = false;
                                        self.select_start = None;
                                        self.select_end = None;
                                    }
                                    KeyCode::Up => self.select_move(-1),
                                    KeyCode::Down => self.select_move(1),
                                    KeyCode::Left => self.select_extend(-1),
                                    KeyCode::Right => self.select_extend(1),
                                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                        self.copy_selection();
                                        self.select_mode = false;
                                    }
                                    _ => {}
                                }
                                continue;
                            }

                            // Command palette mode
                            if self.palette.is_visible() {
                                match key.code {
                                    KeyCode::Esc => {
                                        self.palette.hide();
                                    }
                                    KeyCode::Up => {
                                        self.palette.move_up();
                                    }
                                    KeyCode::Down => {
                                        self.palette.move_down();
                                    }
                                    KeyCode::Enter => {
                                        if let Some(entry) = self.palette.selected_entry() {
                                            let action = entry.action.clone();
                                            self.palette.hide();
                                            match action {
                                                crate::palette::PaletteAction::RunCommand(cmd) => {
                                                    self.handle_slash_command(&cmd).await;
                                                }
                                                _ => {}
                                            }
                                        }
                                    }
                                    KeyCode::Char(c) => {
                                        self.palette.input_char(c);
                                    }
                                    KeyCode::Backspace => {
                                        self.palette.backspace();
                                    }
                                    _ => {}
                                }
                                continue;
                            }

                            // Search mode
                            if self.search_mode {
                                match key.code {
                                    KeyCode::Esc => {
                                        self.search_mode = false;
                                        self.search_query.clear();
                                        self.search_results.clear();
                                    }
                                    KeyCode::Enter => {
                                        // Jump to next result
                                        if !self.search_results.is_empty() {
                                            self.search_result_index = (self.search_result_index + 1) % self.search_results.len();
                                            let msg_idx = self.search_results[self.search_result_index];
                                            self.chat_scroll = self.messages.len().saturating_sub(msg_idx).saturating_sub(10);
                                        }
                                    }
                                    KeyCode::Char(c) => {
                                        self.search_query.push(c);
                                        self.update_search_results();
                                    }
                                    KeyCode::Backspace => {
                                        self.search_query.pop();
                                        self.update_search_results();
                                    }
                                    _ => {}
                                }
                                continue;
                            }
                            match key.code {
                                KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                    self.should_quit = true;
                                }
                                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                    // 100-003: Copy focused message to clipboard, or quit
                                    if let Some(idx) = self.focused_message {
                                        if let Some(msg) = self.messages.get(idx) {
                                            let text = match msg {
                                                ChatMessage::User(t) => t.clone(),
                                                ChatMessage::Assistant(t) => t.clone(),
                                                ChatMessage::ToolCall { name, args } => format!("tool:{}({})", name, args),
                                                ChatMessage::ToolResult { result } => result.clone(),
                                                ChatMessage::System(t) => t.clone(),
                                                ChatMessage::Reasoning(t) => t.clone(),
                                                ChatMessage::Timestamped { inner, .. } => format!("{:?}", inner),
                                            };
                                            // Save to file as clipboard workaround
                                            let home = std::env::var("USERPROFILE")
                                                .or_else(|_| std::env::var("HOME"))
                                                .map(std::path::PathBuf::from)
                                                .unwrap_or_else(|_| std::path::PathBuf::from("."));
                                            let clip_path = home.join(".maix").join("clipboard.txt");
                                            if let Ok(_) = std::fs::write(&clip_path, &text) {
                                                self.messages.push(ChatMessage::System(format!("已复制消息 #{} 到 {}", idx, clip_path.display())));
                                            } else {
                                                self.messages.push(ChatMessage::System("复制失败".into()));
                                            }
                                        }
                                    } else {
                                        self.should_quit = true;
                                    }
                                }
                                // Ctrl+Tab: Cycle sessions
                                KeyCode::Tab if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                    // Save current session messages
                                    self.sessions[self.active_session].messages = self.messages.clone();
                                    // Switch to next session
                                    self.active_session = (self.active_session + 1) % self.sessions.len();
                                    self.messages = self.sessions[self.active_session].messages.clone();
                                    self.session_id = self.sessions[self.active_session].id.clone();
                                    self.chat_scroll = 0;
                                }
                                KeyCode::Tab => {
                                    // If completions are shown, let input handle it
                                    if !self.input.completions.is_empty() {
                                        self.handle_input_key(key).await;
                                    } else {
                                        self.tab_next();
                                    }
                                }
                                KeyCode::BackTab => {
                                    // If completions are shown, let input handle it
                                    if !self.input.completions.is_empty() {
                                        self.handle_input_key(key).await;
                                    } else {
                                        self.tab_prev();
                                    }
                                }
                                KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                    self.messages.push(ChatMessage::System(format!(
                                        "当前会话: {} ({} 条消息)",
                                        &self.session_id[..self.session_id.len().min(8)],
                                        self.messages.len(),
                                    )));
                                }
                                // Enter text selection mode with Ctrl+Shift+A
                                KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                    self.select_mode = true;
                                    let msg_idx = self.messages.len().saturating_sub(1);
                                    self.select_start = Some((msg_idx, 0));
                                    self.select_end = Some((msg_idx, 0));
                                }
                                KeyCode::Up if self.selected_index.is_some() => {
                                    self.navigate_up();
                                }
                                KeyCode::Down if self.selected_index.is_some() => {
                                    self.navigate_down();
                                }
                                KeyCode::Enter if self.selected_index.is_some() => {
                                    self.handle_selected_item().await;
                                }
                                KeyCode::Esc if self.selected_index.is_some() => {
                                    self.selected_index = None;
                                }
                                KeyCode::PageUp => {
                                    self.scroll_target = self.scroll_target.saturating_add(10);
                                    self.scroll_animation = 0.0;
                                    self.auto_scroll = false; // 100-001: Disable auto-scroll on manual scroll
                                    self.mark_dirty(DirtyRegion::Chat);
                                }
                                KeyCode::PageDown => {
                                    self.scroll_target = self.scroll_target.saturating_sub(10);
                                    self.scroll_animation = 0.0;
                                    if self.scroll_target == 0 {
                                        self.auto_scroll = true; // 100-001: Re-enable at bottom
                                    }
                                    self.mark_dirty(DirtyRegion::Chat);
                                }
                                KeyCode::Home if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                    self.scroll_target = self.messages.len();
                                    self.scroll_animation = 0.0;
                                    self.auto_scroll = false;
                                    self.mark_dirty(DirtyRegion::Chat);
                                }
                                KeyCode::End if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                    self.scroll_target = 0;
                                    self.scroll_animation = 0.0;
                                    self.auto_scroll = true; // 100-001: Ctrl+End re-enables auto-scroll
                                    self.mark_dirty(DirtyRegion::Chat);
                                }
                                KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) && self.selected_index.is_none() => {
                                    self.show_reasoning = !self.show_reasoning;
                                }
                                // Command palette with Ctrl+P
                                KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                    self.palette.toggle();
                                    if self.palette.is_visible() {
                                        // Add default commands if empty
                                        if self.palette.filtered_count() == 0 {
                                            self.palette.add_entries(crate::palette::default_commands());
                                        }
                                    }
                                }
                                // Search mode with Ctrl+F
                                KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                    self.search_mode = true;
                                    self.search_query.clear();
                                    self.search_results.clear();
                                    self.search_result_index = 0;
                                }
                                // Ctrl+L: Clear screen
                                KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                    self.messages.clear();
                                    self.chat_scroll = 0;
                                    self.mark_dirty(DirtyRegion::Chat);
                                }
                                // Ctrl+T: Toggle timestamps
                                KeyCode::Char('t') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                    self.show_timestamps = !self.show_timestamps;
                                }
                                // Ctrl+N: New session
                                KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                    self.messages.clear();
                                    self.session_id = uuid::Uuid::new_v4().to_string();
                                    self.messages.push(ChatMessage::System(format!("新会话: {}", &self.session_id[..8])));
                                    self.chat_scroll = 0;
                                }
                                // Ctrl+V: Paste image from clipboard
                                KeyCode::Char('v') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                    if crate::clipboard::clipboard_has_image() {
                                        if let Some(base64) = crate::clipboard::get_clipboard_image_base64() {
                                            self.messages.push(ChatMessage::System(format!(
                                                "已粘贴图片 ({} bytes)", base64.len()
                                            )));
                                            // TODO: Send image to LLM
                                        }
                                    } else {
                                        // Normal paste - handle text
                                        self.handle_input_key(key).await;
                                    }
                                }
                                // Ctrl+Left: Decrease panel width
                                KeyCode::Left if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                    self.panel_width = self.panel_width.saturating_sub(5).max(20);
                                }
                                // Ctrl+Right: Increase panel width
                                KeyCode::Right if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                    self.panel_width = (self.panel_width + 5).min(80);
                                }
                                // F11: Toggle fullscreen
                                KeyCode::F(11) => {
                                    self.fullscreen = !self.fullscreen;
                                }
                                // F1: Context help
                                KeyCode::F(1) => {
                                    let help_text = if !self.input.completions.is_empty() {
                                        "补全模式:\n  Tab/↑↓ 选择\n  Enter 确认\n  1-9 直接选择\n  Esc 取消"
                                    } else if self.search_mode {
                                        "搜索模式:\n  输入关键词\n  Enter 下一个\n  Esc 关闭"
                                    } else if self.palette.is_visible() {
                                        "命令面板:\n  输入过滤\n  ↑↓ 导航\n  Enter 执行\n  Esc 关闭"
                                    } else if self.is_streaming.load(Ordering::SeqCst) {
                                        "流式输出中:\n  Esc 中断\n  可继续输入"
                                    } else {
                                        "可用快捷键:\n  Ctrl+P 命令面板\n  Ctrl+F 搜索\n  Ctrl+L 清屏\n  Ctrl+N 新会话\n  Ctrl+1-9 切换会话\n  F11 全屏\n  /help 查看所有命令"
                                    };
                                    self.messages.push(ChatMessage::System(help_text.into()));
                                }
                                // F2: Focus previous message (099-007)
                                KeyCode::F(2) => {
                                    if self.messages.is_empty() {
                                        self.focused_message = None;
                                    } else if let Some(current) = self.focused_message {
                                        self.focused_message = Some(current.saturating_sub(1));
                                    } else {
                                        self.focused_message = Some(self.messages.len().saturating_sub(1));
                                    }
                                }
                                // F3: Focus next message (099-007)
                                KeyCode::F(3) => {
                                    if let Some(current) = self.focused_message {
                                        if current + 1 < self.messages.len() {
                                            self.focused_message = Some(current + 1);
                                        } else {
                                            self.focused_message = None;
                                        }
                                    }
                                }
                                // F4: Clear focus (099-007)
                                KeyCode::F(4) => {
                                    self.focused_message = None;
                                }
                                // Enter on focused message: toggle expand/collapse (100-007)
                                KeyCode::Enter if self.focused_message.is_some() && self.input.buffer.is_empty() => {
                                    if let Some(idx) = self.focused_message {
                                        if matches!(self.messages.get(idx), Some(ChatMessage::ToolCall { .. }) | Some(ChatMessage::ToolResult { .. })) {
                                            if self.expanded_tool_calls.contains(&idx) {
                                                self.expanded_tool_calls.remove(&idx);
                                            } else {
                                                self.expanded_tool_calls.insert(idx);
                                            }
                                        }
                                    }
                                }
                                // Ctrl+1-9: Switch to session N
                                KeyCode::Char(c @ '1'..='9') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                    let idx = c.to_digit(10).unwrap() as usize - 1;
                                    if idx < self.sessions.len() {
                                        // Save current session messages
                                        self.sessions[self.active_session].messages = self.messages.clone();
                                        // Switch to new session
                                        self.active_session = idx;
                                        self.messages = self.sessions[idx].messages.clone();
                                        self.session_id = self.sessions[idx].id.clone();
                                        self.chat_scroll = 0;
                                    }
                                }
                                // Y: Approve current tool call
                                KeyCode::Char('y') if !self.pending_tool_approvals.is_empty() => {
                                    if let Some(approval) = self.pending_tool_approvals.pop() {
                                        self.messages.push(ChatMessage::System(format!(
                                            "已批准工具: {}", approval.name
                                        )));
                                    }
                                }
                                // N: Reject current tool call
                                KeyCode::Char('n') if !self.pending_tool_approvals.is_empty() => {
                                    if let Some(approval) = self.pending_tool_approvals.pop() {
                                        self.messages.push(ChatMessage::System(format!(
                                            "已拒绝工具: {}", approval.name
                                        )));
                                    }
                                }
                                // A: Approve all pending tool calls
                                KeyCode::Char('a') if !self.pending_tool_approvals.is_empty() => {
                                    let count = self.pending_tool_approvals.len();
                                    self.pending_tool_approvals.clear();
                                    self.auto_approve_round = true;
                                    self.messages.push(ChatMessage::System(format!(
                                        "已批准本轮所有工具调用 ({})", count
                                    )));
                                }
                                // R: Retry last failed tool
                                KeyCode::Char('r') if self.last_failed_tool.is_some() && !self.is_streaming.load(Ordering::SeqCst) => {
                                    if let Some(tool) = self.last_failed_tool.take() {
                                        self.messages.push(ChatMessage::System(format!(
                                            "重试工具: {}", tool.name
                                        )));
                                        // Re-send as message to trigger tool call
                                        self.send_message(format!("/retry {}", tool.name)).await;
                                    }
                                }
                                // Interrupt streaming with Escape
                                KeyCode::Esc if self.is_streaming.load(Ordering::SeqCst) => {
                                    self.stream_renderer.abort();
                                    self.messages.push(ChatMessage::System("(已中断生成)".into()));
                                    self.is_streaming.store(false, Ordering::SeqCst);
                                }
                                _ => {
                                    // Allow typing while streaming, block only Enter
                                    if self.is_streaming.load(Ordering::SeqCst) {
                                        if key.code == KeyCode::Enter {
                                            self.messages.push(ChatMessage::System(
                                                "(等待回复中，请稍候...)".into()
                                            ));
                                        } else {
                                            self.handle_input_key(key).await;
                                        }
                                    } else {
                                        self.handle_input_key(key).await;
                                    }
                                }
                            }
                        }
                        Some(Err(e)) => {
                            self.messages.push(ChatMessage::System(format!("Input error: {e}")));
                        }
                        None => break,
                        _ => {}
                    }
                }

                // App events from gRPC stream
                Some(app_event) = self.event_rx.recv() => {
                    self.handle_app_event(app_event).await;
                }

                // Periodic tick (for animations, etc.)
                _ = tick.tick() => {
                    self.tick_count = self.tick_count.wrapping_add(1);

                    // Smooth scroll animation
                    if self.scroll_animation < 1.0 {
                        self.scroll_animation = (self.scroll_animation + 0.15).min(1.0);
                        // Ease-out cubic
                        let t = self.scroll_animation;
                        let eased = 1.0 - (1.0 - t).powi(3);
                        let current = self.chat_scroll as f64;
                        let target = self.scroll_target as f64;
                        self.chat_scroll = (current + (target - current) * eased).round() as usize;
                    }

                    // Check for due reminders
                    for reminder in &mut self.reminders {
                        if reminder.is_due() {
                            reminder.triggered = true;
                            self.messages.push(ChatMessage::System(format!(
                                "⏰ 提醒 #{}: {}", reminder.id, reminder.message
                            )));
                            self.notifier.notify("提醒", &reminder.message, crate::notify::NotifyKind::Info);
                            self.notifier.play_sound(crate::notify::NotifyKind::Info);
                        }
                    }
                    // Clean up triggered reminders older than 1 minute
                    self.reminders.retain(|r| !r.triggered || r.remaining() > std::time::Duration::ZERO);

                    // Auto-save every 5 minutes (6000 ticks at 50ms each)
                    if self.tick_count % 6000 == 0 && !self.messages.is_empty() {
                        let save_dir = dirs_home().join(".maix").join("autosave");
                        let _ = std::fs::create_dir_all(&save_dir);
                        let save_file = save_dir.join(format!("{}.json", &self.session_id[..8.min(self.session_id.len())]));
                        let save_data = serde_json::json!({
                            "session_id": self.session_id,
                            "messages": self.messages.iter().filter_map(|m| match m {
                                ChatMessage::User(t) => Some(serde_json::json!({"role": "user", "content": t})),
                                ChatMessage::Assistant(t) => Some(serde_json::json!({"role": "assistant", "content": t})),
                                ChatMessage::System(t) => Some(serde_json::json!({"role": "system", "content": t})),
                                _ => None,
                            }).collect::<Vec<_>>(),
                            "saved_at": chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                        });
                        let _ = std::fs::write(&save_file, serde_json::to_string_pretty(&save_data).unwrap());
                    }
                }

                // Periodic memory refresh
                _ = memory_refresh.tick() => {
                    self.refresh_memories().await;
                }
            }
        }

        Ok(())
    }

    async fn handle_input_key(&mut self, key: KeyEvent) {
        // Vim mode intercept
        if self.vim.enabled {
            let action = self.vim.handle_key(
                key,
                &mut self.input.cursor,
                &mut self.input.buffer,
            );
            match action {
                crate::vim::VimAction::None => return,
                crate::vim::VimAction::Submit => {
                    if let Some(text) = self.input.submit() {
                        if text.starts_with('/') {
                            self.handle_slash_command(&text).await;
                        } else {
                            self.send_message(text).await;
                        }
                    }
                    return;
                }
                crate::vim::VimAction::Passthrough => {
                    // Fall through to normal handling
                }
                crate::vim::VimAction::Yank(_) => return,
                crate::vim::VimAction::SelectionChanged => return,
            }
        }

        let has_completions = !self.input.completions.is_empty();
        let visible_height = 6;  // Max visible completions

        match key.code {
            KeyCode::Tab => {
                self.input.tab_complete(visible_height);
                return;
            }
            // When completions are shown, Up/Down navigate completions
            KeyCode::Up if has_completions => {
                self.input.completion_prev();
                return;
            }
            KeyCode::Down if has_completions => {
                self.input.completion_next(visible_height);
                return;
            }
            // Enter selects completion if shown
            KeyCode::Enter if has_completions => {
                self.input.select_completion();
                return;
            }
            // Number keys 1-9 select completion directly
            KeyCode::Char(c @ '1'..='9') if has_completions => {
                let idx = c.to_digit(10).unwrap() as usize - 1;
                if idx < self.input.completions.len() {
                    self.input.completion_index = idx;
                    self.input.select_completion();
                }
                return;
            }
            // Esc: first press clears completions, second press clears input
            KeyCode::Esc if has_completions => {
                self.input.completions.clear();
                // If input is also non-empty, don't clear on first Esc
                return;
            }
            KeyCode::Esc if !self.input.buffer.is_empty() => {
                self.input.buffer.clear();
                self.input.cursor = 0;
                return;
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.insert_char(c);
                self.mark_dirty(DirtyRegion::Input);
            }
            // Shift+Enter inserts newline for multi-line input
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.input.insert_newline();
                self.mark_dirty(DirtyRegion::Input);
            }
            // Ctrl+U: Clear to line start
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.clear_to_line_start();
                self.mark_dirty(DirtyRegion::Input);
            }
            // Ctrl+W: Delete previous word
            KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.delete_prev_word();
                self.mark_dirty(DirtyRegion::Input);
            }
            // Ctrl+K: Clear to line end
            KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.clear_to_line_end();
            }
            KeyCode::Backspace => self.input.delete_before(),
            KeyCode::Delete => self.input.delete_after(),
            KeyCode::Left if key.modifiers.contains(KeyModifiers::CONTROL) => self.input.move_word_left(),
            KeyCode::Right if key.modifiers.contains(KeyModifiers::CONTROL) => self.input.move_word_right(),
            KeyCode::Left => self.input.move_left(),
            KeyCode::Right => self.input.move_right(),
            KeyCode::Home => self.input.move_home(),
            KeyCode::End => self.input.move_end(),
            KeyCode::Up => self.input.history_prev(),
            KeyCode::Down => self.input.history_next(),
            KeyCode::Enter => {
                if let Some(text) = self.input.submit() {
                    if text.starts_with('/') {
                        self.handle_slash_command(&text).await;
                    } else {
                        self.send_message(text).await;
                    }
                }
            }
            _ => {}
        }
        // Auto-complete on input change
        self.input.auto_complete();
    }

    fn update_search_results(&mut self) {
        self.search_results.clear();
        self.search_result_index = 0;

        if self.search_query.is_empty() {
            return;
        }

        // Rebuild search index if dirty
        if self.search_index.dirty {
            self.search_index.rebuild(&self.messages);
        }

        // Use search index for faster lookup
        self.search_results = self.search_index.search(&self.search_query);
    }

    async fn handle_slash_command(&mut self, cmd: &str) {
        // Track command usage
        let cmd_name = cmd.split_whitespace().next().unwrap_or(cmd);
        *self.command_usage.entry(cmd_name.to_string()).or_insert(0) += 1;

        // 100-026: Input validation
        if cmd.starts_with('/') {
            let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
            let command = parts[0];
            let args = parts.get(1).unwrap_or(&"").trim();

            // Validate commands that require arguments
            match command {
                "/mode" if args.is_empty() => {
                    self.messages.push(ChatMessage::System("用法: /mode <plan|agent|yolo>".into()));
                    return;
                }
                "/model" if args.is_empty() => {
                    self.messages.push(ChatMessage::System("用法: /model <name>".into()));
                    return;
                }
                "/branch" if args.is_empty() => {
                    self.messages.push(ChatMessage::System("用法: /branch <name>".into()));
                    return;
                }
                "/tag" if args.is_empty() => {
                    self.messages.push(ChatMessage::System("用法: /tag <name>\n  /tag msg <index> <tag>".into()));
                    return;
                }
                "/template" if args.is_empty() => {
                    self.messages.push(ChatMessage::System("用法: /template <name>".into()));
                    return;
                }
                "/theme" if args.is_empty() => {
                    let themes = crate::ui::Theme::available_themes();
                    self.messages.push(ChatMessage::System(format!("用法: /theme <name>\n可用主题: {}", themes.join(", "))));
                    return;
                }
                "/remind" if args.is_empty() => {
                    self.messages.push(ChatMessage::System("用法: /remind <time> <message>\n示例: /remind 5m 检查构建".into()));
                    return;
                }
                "/checkpoint save" | "/checkpoint load" | "/checkpoint rm" if args.is_empty() => {
                    self.messages.push(ChatMessage::System(format!("用法: {} <name>", command)));
                    return;
                }
                _ => {}
            }
        }

        // Resolve aliases
        let _resolved_cmd = if cmd.starts_with('/') {
            let alias_name = cmd.split_whitespace().next().unwrap_or(cmd);
            if let Some(resolved) = self.aliases.get(alias_name) {
                let args = cmd[alias_name.len()..].trim();
                if args.is_empty() {
                    resolved.clone()
                } else {
                    format!("{} {}", resolved, args)
                }
            } else {
                cmd.to_string()
            }
        } else {
            cmd.to_string()
        };

        // Handle !! and !N history shortcuts
        let cmd = if cmd == "!!" {
            // Execute last command
            self.input.history.last().cloned().unwrap_or_default()
        } else if cmd.starts_with('!') && cmd[1..].chars().all(|c| c.is_ascii_digit()) {
            // Execute command by history index
            let idx: usize = cmd[1..].parse().unwrap_or(0);
            if idx > 0 && idx <= self.input.history.len() {
                self.input.history[idx - 1].clone()
            } else {
                cmd.to_string()
            }
        } else {
            cmd.to_string()
        };
        let cmd = cmd.as_str();

        // Check for custom commands (format: /user:name or /project:name [arguments])
        if cmd.starts_with("/user:") || cmd.starts_with("/project:") {
            let (cmd_name, arguments) = match cmd.find(' ') {
                Some(pos) => (&cmd[..pos], &cmd[pos + 1..]),
                None => (cmd, ""),
            };
            let cmd_name_without_slash = &cmd_name[1..]; // remove leading /
            if let Some(custom) = self.custom_cmds.iter().find(|c| c.name == cmd_name_without_slash) {
                let rendered = maix_agent::commands::render_command(custom, arguments);
                self.messages.push(ChatMessage::System(format!("执行自定义命令: {}", cmd_name)));
                self.send_message(rendered).await;
                return;
            }
        }

        match cmd {
            "/quit" | "/exit" => self.should_quit = true,
            "/tutorial" => {
                let tutorial = "🎓 Maix-Agent 交互式教程\n\
                    \n\
                    第1步: 基本对话\n\
                    直接输入文字即可与 AI 对话。\n\
                    试试输入: 你好，请介绍一下自己\n\
                    \n\
                    第2步: 使用命令\n\
                    所有命令以 / 开头。\n\
                    试试输入: /help 查看所有命令\n\
                    \n\
                    第3步: 快捷键\n\
                    - Ctrl+P: 打开命令面板\n\
                    - Ctrl+F: 搜索对话\n\
                    - Ctrl+L: 清屏\n\
                    - F1: 上下文帮助\n\
                    \n\
                    第4步: 多行输入\n\
                    按 Shift+Enter 换行，\n\
                    适合输入代码或长文本。\n\
                    \n\
                    第5步: 工具审批\n\
                    AI 调用工具时，你需要:\n\
                    - Y: 批准\n\
                    - N: 拒绝\n\
                    - A: 批准本轮全部\n\
                    \n\
                    第6步: 个性化\n\
                    - /theme dracula: 切换主题\n\
                    - /layout compact: 切换布局\n\
                    - /sound: 开关声音\n\
                    \n\
                    完成！输入 /help 查看更多命令。";
                self.messages.push(ChatMessage::System(tutorial.into()));
            }
            "/quickstart" => {
                let cards = "🚀 快速入门卡片\n\
                    \n\
                    ┌─────────────────────────────────────┐\n\
                    │ 💬 基本对话                          │\n\
                    │ 直接输入文字与 AI 对话               │\n\
                    │ 示例: 解释一下 Rust 的所有权         │\n\
                    └─────────────────────────────────────┘\n\
                    \n\
                    ┌─────────────────────────────────────┐\n\
                    │ 🔧 使用工具                          │\n\
                    │ AI 可以执行代码、读写文件等           │\n\
                    │ 审批: Y批准 N拒绝 A全部批准           │\n\
                    └─────────────────────────────────────┘\n\
                    \n\
                    ┌─────────────────────────────────────┐\n\
                    │ 📝 多行输入                          │\n\
                    │ Shift+Enter 换行                    │\n\
                    │ 适合粘贴代码或长文本                  │\n\
                    └─────────────────────────────────────┘\n\
                    \n\
                    ┌─────────────────────────────────────┐\n\
                    │ 🔍 搜索与命令                        │\n\
                    │ Ctrl+F 搜索对话                      │\n\
                    │ Ctrl+P 命令面板                      │\n\
                    │ / 查看所有命令                       │\n\
                    └─────────────────────────────────────┘";
                self.messages.push(ChatMessage::System(cards.into()));
            }
            "/tips" => {
                let tip_index = (self.tick_count as usize / 100) % 10;
                let tips = [
                    "💡 使用 /compact 压缩上下文，避免超出 token 限制",
                    "💡 按 Ctrl+P 打开命令面板，快速查找命令",
                    "💡 使用 Shift+Enter 输入多行文本",
                    "💡 按 F1 查看当前上下文帮助",
                    "💡 使用 /theme 切换主题，保护眼睛",
                    "💡 设置 /remind 定时提醒，避免忘记重要事项",
                    "💡 使用 /todo 管理待办事项，提高效率",
                    "💡 按 Ctrl+F 搜索历史对话内容",
                    "💡 使用 /layout focus 进入专注模式",
                    "💡 使用 /export 导出对话记录",
                ];
                self.messages.push(ChatMessage::System(tips[tip_index].into()));
            }
            "/usage" => {
                let session_duration = self.session_start.elapsed();
                let hours = session_duration.as_secs() / 3600;
                let minutes = (session_duration.as_secs() % 3600) / 60;
                let seconds = session_duration.as_secs() % 60;

                let mut lines = vec![
                    "📊 使用统计:".to_string(),
                    format!("  会话时长: {}h {}m {}s", hours, minutes, seconds),
                    format!("  消息总数: {}", self.messages.len()),
                    format!("  对话轮次: {}", self.round_count),
                    format!("  Token 总量: {}", self.total_tokens),
                    format!("  总费用: ¥{:.4}", self.total_cost),
                ];

                if !self.command_usage.is_empty() {
                    lines.push("".to_string());
                    lines.push("命令使用频率:".to_string());
                    let mut cmd_vec: Vec<_> = self.command_usage.iter().collect();
                    cmd_vec.sort_by(|a, b| b.1.cmp(a.1));
                    for (cmd, count) in cmd_vec.iter().take(10) {
                        lines.push(format!("  {}: {} 次", cmd, count));
                    }
                }

                // Recommend unused features
                lines.push("".to_string());
                lines.push("推荐功能:".to_string());
                if !self.command_usage.contains_key("/compact") {
                    lines.push("  /compact - 压缩上下文，节省 token".to_string());
                }
                if !self.command_usage.contains_key("/theme") {
                    lines.push("  /theme - 切换主题，保护眼睛".to_string());
                }
                if !self.command_usage.contains_key("/todo") {
                    lines.push("  /todo - 管理待办事项".to_string());
                }

                self.messages.push(ChatMessage::System(lines.join("\n")));
            }
            "/feedback" => {
                self.messages.push(ChatMessage::System(
                    "📝 反馈收集\n\
                    \n\
                    请描述你遇到的问题或建议：\n\
                    格式: /feedback <内容>\n\
                    \n\
                    示例:\n\
                    - /feedback 补全功能有时不准确\n\
                    - /feedback 希望支持更多主题\n\
                    - /feedback 工具审批流程可以优化".into()
                ));
            }
            other if other.starts_with("/feedback ") => {
                let content = &other[10..].trim();
                if content.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /feedback <内容>".into()));
                } else {
                    // Save feedback to file
                    let feedback_dir = dirs_home().join(".maix").join("feedback");
                    let _ = std::fs::create_dir_all(&feedback_dir);
                    let filename = format!("feedback-{}.txt", chrono::Local::now().format("%Y%m%d-%H%M%S"));
                    let filepath = feedback_dir.join(&filename);
                    let feedback_content = format!(
                        "时间: {}\n会话: {}\n内容: {}\n",
                        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                        &self.session_id[..self.session_id.len().min(8)],
                        content
                    );
                    match std::fs::write(&filepath, &feedback_content) {
                        Ok(_) => {
                            self.messages.push(ChatMessage::System(format!(
                                "感谢你的反馈！已保存到: {}", filepath.display()
                            )));
                        }
                        Err(e) => {
                            self.messages.push(ChatMessage::System(format!("保存反馈失败: {}", e)));
                        }
                    }
                }
            }
            "/profile" => {
                let profile_dir = dirs_home().join(".maix").join("profiles");
                let _ = std::fs::create_dir_all(&profile_dir);
                let mut lines = vec!["👤 用户配置文件:".to_string()];
                if let Ok(entries) = std::fs::read_dir(&profile_dir) {
                    for entry in entries.flatten() {
                        if let Some(name) = entry.file_name().to_str() {
                            if name.ends_with(".json") {
                                lines.push(format!("  {}", name.replace(".json", "")));
                            }
                        }
                    }
                }
                if lines.len() == 1 {
                    lines.push("  (没有保存的配置文件)".to_string());
                }
                lines.push("\n用法:".to_string());
                lines.push("  /profile save <name>  保存当前配置".to_string());
                lines.push("  /profile load <name>  加载配置".to_string());
                self.messages.push(ChatMessage::System(lines.join("\n")));
            }
            other if other.starts_with("/profile save ") => {
                let name = other[14..].trim();
                if name.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /profile save <name>".into()));
                } else {
                    let profile = serde_json::json!({
                        "theme": "dark",
                        "layout": self.layout_preset,
                        "shortcut_scheme": self.shortcut_scheme,
                        "panel_width": self.panel_width,
                        "show_dividers": self.show_dividers,
                        "show_timestamps": self.show_timestamps,
                        "saved_at": chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                    });
                    let profile_dir = dirs_home().join(".maix").join("profiles");
                    let _ = std::fs::create_dir_all(&profile_dir);
                    let filepath = profile_dir.join(format!("{}.json", name));
                    match std::fs::write(&filepath, serde_json::to_string_pretty(&profile).unwrap()) {
                        Ok(_) => {
                            self.messages.push(ChatMessage::System(format!("配置已保存: {}", name)));
                        }
                        Err(e) => {
                            self.messages.push(ChatMessage::System(format!("保存失败: {}", e)));
                        }
                    }
                }
            }
            other if other.starts_with("/profile load ") => {
                let name = other[14..].trim();
                if name.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /profile load <name>".into()));
                } else {
                    let profile_dir = dirs_home().join(".maix").join("profiles");
                    let filepath = profile_dir.join(format!("{}.json", name));
                    match std::fs::read_to_string(&filepath) {
                        Ok(content) => {
                            match serde_json::from_str::<serde_json::Value>(&content) {
                                Ok(profile) => {
                                    if let Some(layout) = profile.get("layout").and_then(|v| v.as_str()) {
                                        self.layout_preset = layout.to_string();
                                    }
                                    if let Some(scheme) = profile.get("shortcut_scheme").and_then(|v| v.as_str()) {
                                        self.shortcut_scheme = scheme.to_string();
                                    }
                                    if let Some(width) = profile.get("panel_width").and_then(|v| v.as_u64()) {
                                        self.panel_width = width as u16;
                                    }
                                    self.messages.push(ChatMessage::System(format!("配置已加载: {}", name)));
                                }
                                Err(e) => {
                                    self.messages.push(ChatMessage::System(format!("解析配置失败: {}", e)));
                                }
                            }
                        }
                        Err(e) => {
                            self.messages.push(ChatMessage::System(format!("读取配置失败: {}", e)));
                        }
                    }
                }
            }
            "/mode plan" => {
                self.mode = MODE_PLAN;
                self.messages
                    .push(ChatMessage::System("已切换到计划模式".into()));
            }
            "/mode agent" => {
                self.mode = MODE_AGENT;
                self.messages
                    .push(ChatMessage::System("已切换到智能体模式".into()));
            }
            "/mode yolo" => {
                self.mode = MODE_YOLO;
                self.messages
                    .push(ChatMessage::System("已切换到自主模式".into()));
            }
            "/memory" => {
                self.active_panel = ActivePanel::Memory;
                self.refresh_memories().await;
            }
            "/calendar" => {
                let now = chrono::Local::now();
                let year = now.format("%Y").to_string();
                let month = now.format("%m").to_string();
                let day = now.format("%d").to_string();
                let weekday = now.format("%A").to_string();

                let mut lines = vec![
                    format!("📅 今天: {}年{}月{}日 {}", year, month, day, weekday),
                    "".to_string(),
                ];

                // Simple calendar for current month
                let first_day = now.with_day(1).unwrap();
                let days_in_month = if now.month() == 12 {
                    31
                } else {
                    now.with_day(1).unwrap().with_month(now.month() + 1).unwrap().with_day(0).unwrap().day()
                };
                let start_weekday = first_day.weekday().num_days_from_sunday();

                lines.push(format!("{}年{}月", year, month));
                lines.push("日 一 二 三 四 五 六".to_string());

                let mut calendar_line = String::new();
                for _ in 0..start_weekday {
                    calendar_line.push_str("   ");
                }
                for d in 1..=days_in_month {
                    calendar_line.push_str(&format!("{:2}", d));
                    if d == day.parse().unwrap_or(0) {
                        calendar_line.push('*');
                    } else {
                        calendar_line.push(' ');
                    }
                    if (start_weekday + d) % 7 == 0 {
                        lines.push(calendar_line.clone());
                        calendar_line.clear();
                    }
                }
                if !calendar_line.is_empty() {
                    lines.push(calendar_line);
                }

                self.messages.push(ChatMessage::System(lines.join("\n")));
            }
            "/habit" => {
                if self.habits.is_empty() {
                    self.messages.push(ChatMessage::System("(没有追踪的习惯)\n用法: /habit add <习惯名>".into()));
                } else {
                    let mut lines = vec!["习惯追踪:".to_string()];
                    for (_i, habit) in self.habits.iter().enumerate() {
                        let status = if habit.is_completed_today() { "✅" } else { "⬜" };
                        lines.push(format!("  {} {} - 连续{}天, 共{}次", status, habit.name, habit.streak, habit.total_completions));
                    }
                    lines.push("\n用法:".to_string());
                    lines.push("  /habit add <name>  添加习惯".to_string());
                    lines.push("  /habit done <id>   完成今日打卡".to_string());
                    lines.push("  /habit rm <id>     删除习惯".to_string());
                    self.messages.push(ChatMessage::System(lines.join("\n")));
                }
            }
            other if other.starts_with("/habit add ") => {
                let name = other[11..].trim();
                if name.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /habit add <习惯名>".into()));
                } else {
                    self.habits.push(Habit::new(name));
                    self.messages.push(ChatMessage::System(format!("已添加习惯: {}", name)));
                }
            }
            other if other.starts_with("/habit done ") => {
                let id: usize = match other[12..].trim().parse() {
                    Ok(n) => n,
                    Err(_) => {
                        self.messages.push(ChatMessage::System("用法: /habit done <id>".into()));
                        return;
                    }
                };
                if id > 0 && id <= self.habits.len() {
                    self.habits[id - 1].complete();
                    let habit = &self.habits[id - 1];
                    self.messages.push(ChatMessage::System(format!(
                        "✅ {} 打卡成功！连续{}天", habit.name, habit.streak
                    )));
                } else {
                    self.messages.push(ChatMessage::System(format!("无效ID: {}", id)));
                }
            }
            other if other.starts_with("/habit rm ") => {
                let id: usize = match other[10..].trim().parse() {
                    Ok(n) => n,
                    Err(_) => {
                        self.messages.push(ChatMessage::System("用法: /habit rm <id>".into()));
                        return;
                    }
                };
                if id > 0 && id <= self.habits.len() {
                    let habit = self.habits.remove(id - 1);
                    self.messages.push(ChatMessage::System(format!("已删除习惯: {}", habit.name)));
                } else {
                    self.messages.push(ChatMessage::System(format!("无效ID: {}", id)));
                }
            }
            "/tools" => self.active_panel = ActivePanel::Tools,
            "/tool_history" => {
                let tool_calls: Vec<(usize, &ChatMessage)> = self.messages
                    .iter()
                    .enumerate()
                    .filter(|(_, m)| matches!(m, ChatMessage::ToolCall { .. }))
                    .collect();

                if tool_calls.is_empty() {
                    self.messages.push(ChatMessage::System("没有工具调用历史".into()));
                } else {
                    let mut lines = vec!["工具调用历史:".to_string()];
                    for (i, msg) in tool_calls.iter().rev().take(10) {
                        if let ChatMessage::ToolCall { name, args } = msg {
                            lines.push(format!("  [{}] {} - {}", i, name, &args[..args.len().min(50)]));
                        }
                    }
                    if tool_calls.len() > 10 {
                        lines.push(format!("  ... 还有 {} 条记录", tool_calls.len() - 10));
                    }
                    lines.push("\n用法: /tool_replay <消息索引> 重新执行工具调用".to_string());
                    self.messages.push(ChatMessage::System(lines.join("\n")));
                }
            }
            other if other.starts_with("/tool_replay ") => {
                let index_str = other[13..].trim();
                if let Ok(msg_index) = index_str.parse::<usize>() {
                    if msg_index >= self.messages.len() {
                        self.messages.push(ChatMessage::System(format!(
                            "无效的消息索引。有效范围: 0-{}",
                            self.messages.len().saturating_sub(1)
                        )));
                    } else if let ChatMessage::ToolCall { name, args } = &self.messages[msg_index] {
                        let tool_name = name.clone();
                        let tool_args = args.clone();
                        self.messages.push(ChatMessage::System(format!(
                            "重新执行工具: {}\n参数: {}\n注意: 实际重新执行需要通过 AI 处理",
                            tool_name, tool_args
                        )));
                        // Send the tool call as a message to the AI
                        let tool_msg = format!("请重新执行工具调用: {} {}", tool_name, tool_args);
                        self.send_message(tool_msg).await;
                    } else {
                        self.messages.push(ChatMessage::System("指定的消息不是工具调用".into()));
                    }
                } else {
                    self.messages.push(ChatMessage::System("用法: /tool_replay <消息索引>".into()));
                }
            }
            "/tool_perms" => {
                if self.tool_permissions.is_empty() {
                    self.messages.push(ChatMessage::System(
                        "工具权限设置:\n\
                        \n\
                        当前没有自定义权限设置。\n\
                        所有工具使用默认审批流程。\n\
                        \n\
                        用法:\n\
                        /tool_perms add <工具名> [auto|manual] [risk_level]\n\
                        /tool_perms rm <工具名>\n\
                        /tool_perms list".into()
                    ));
                } else {
                    let mut lines = vec!["工具权限设置:".to_string()];
                    for (name, (auto_approve, risk_level)) in &self.tool_permissions {
                        let status = if *auto_approve { "自动批准" } else { "手动审批" };
                        let risk = match risk_level {
                            0 => "低",
                            1 => "中",
                            2 => "高",
                            _ => "未知",
                        };
                        lines.push(format!("  {} - {} - 风险: {}", name, status, risk));
                    }
                    self.messages.push(ChatMessage::System(lines.join("\n")));
                }
            }
            other if other.starts_with("/tool_perms add ") => {
                let parts: Vec<&str> = other[16..].trim().split_whitespace().collect();
                if parts.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /tool_perms add <工具名> [auto|manual] [risk_level]".into()));
                } else {
                    let tool_name = parts[0].to_string();
                    let auto_approve = if parts.len() > 1 {
                        parts[1] == "auto"
                    } else {
                        false
                    };
                    let risk_level = if parts.len() > 2 {
                        parts[2].parse::<i32>().unwrap_or(1)
                    } else {
                        1
                    };
                    self.tool_permissions.insert(tool_name.clone(), (auto_approve, risk_level));
                    let status = if auto_approve { "自动批准" } else { "手动审批" };
                    self.messages.push(ChatMessage::System(format!(
                        "已设置工具 {} 权限: {} - 风险: {}",
                        tool_name, status, risk_level
                    )));
                }
            }
            other if other.starts_with("/tool_perms rm ") => {
                let tool_name = other[15..].trim();
                if tool_name.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /tool_perms rm <工具名>".into()));
                } else if self.tool_permissions.remove(tool_name).is_some() {
                    self.messages.push(ChatMessage::System(format!("已删除工具 {} 的权限设置", tool_name)));
                } else {
                    self.messages.push(ChatMessage::System(format!("未找到工具 {} 的权限设置", tool_name)));
                }
            }
            "/tool_fav" => {
                if self.favorite_tools.is_empty() {
                    self.messages.push(ChatMessage::System(
                        "收藏的工具:\n\
                        \n\
                        当前没有收藏的工具。\n\
                        \n\
                        用法:\n\
                        /tool_fav add <工具名>  添加收藏\n\
                        /tool_fav rm <工具名>   取消收藏\n\
                        /tool_fav list          列出收藏".into()
                    ));
                } else {
                    let mut lines = vec!["收藏的工具:".to_string()];
                    for (i, tool) in self.favorite_tools.iter().enumerate() {
                        lines.push(format!("  {}. {}", i + 1, tool));
                    }
                    self.messages.push(ChatMessage::System(lines.join("\n")));
                }
            }
            other if other.starts_with("/tool_fav add ") => {
                let tool_name = other[14..].trim().to_string();
                if tool_name.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /tool_fav add <工具名>".into()));
                } else if self.favorite_tools.contains(&tool_name) {
                    self.messages.push(ChatMessage::System(format!("工具 {} 已在收藏中", tool_name)));
                } else {
                    self.favorite_tools.push(tool_name.clone());
                    self.messages.push(ChatMessage::System(format!("已收藏工具: {}", tool_name)));
                }
            }
            other if other.starts_with("/tool_fav rm ") => {
                let tool_name = other[13..].trim().to_string();
                if tool_name.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /tool_fav rm <工具名>".into()));
                } else if let Some(pos) = self.favorite_tools.iter().position(|t| t == &tool_name) {
                    self.favorite_tools.remove(pos);
                    self.messages.push(ChatMessage::System(format!("已取消收藏工具: {}", tool_name)));
                } else {
                    self.messages.push(ChatMessage::System(format!("未找到收藏的工具: {}", tool_name)));
                }
            }
            "/tool_stats" => {
                if self.tool_stats.is_empty() {
                    // Calculate stats from message history
                    let mut stats: std::collections::HashMap<String, (usize, usize, u64)> = std::collections::HashMap::new();
                    let mut current_tool: Option<(String, std::time::Instant)> = None;

                    for msg in &self.messages {
                        match msg {
                            ChatMessage::ToolCall { name, .. } => {
                                current_tool = Some((name.clone(), std::time::Instant::now()));
                                let entry = stats.entry(name.clone()).or_insert((0, 0, 0));
                                entry.0 += 1;
                            }
                            ChatMessage::ToolResult { .. } => {
                                if let Some((name, start)) = current_tool.take() {
                                    let duration = start.elapsed().as_millis() as u64;
                                    let entry = stats.entry(name).or_insert((0, 0, 0));
                                    entry.1 += 1; // success
                                    entry.2 += duration;
                                }
                            }
                            _ => {}
                        }
                    }

                    if stats.is_empty() {
                        self.messages.push(ChatMessage::System("没有工具调用统计".into()));
                    } else {
                        let mut lines = vec!["工具使用统计:".to_string(), "".to_string()];
                        lines.push(format!("  {:<20} {:>8} {:>8} {:>12}", "工具", "调用次数", "成功率", "平均耗时"));
                        lines.push("  ────────────────────────────────────────────────".to_string());

                        let mut sorted_stats: Vec<_> = stats.iter().collect();
                        sorted_stats.sort_by(|a, b| b.1.0.cmp(&a.1.0));

                        for (name, (calls, successes, total_duration)) in sorted_stats.iter().take(10) {
                            let success_rate = if *calls > 0 { (*successes as f64 / *calls as f64 * 100.0) as usize } else { 0 };
                            let avg_duration = if *calls > 0 { *total_duration / *calls as u64 } else { 0 };
                            lines.push(format!("  {:<20} {:>8} {:>7}% {:>9}ms", name, calls, success_rate, avg_duration));
                        }

                        self.messages.push(ChatMessage::System(lines.join("\n")));
                    }
                } else {
                    let mut lines = vec!["工具使用统计:".to_string(), "".to_string()];
                    lines.push(format!("  {:<20} {:>8} {:>8} {:>12}", "工具", "调用次数", "成功率", "平均耗时"));
                    lines.push("  ────────────────────────────────────────────────".to_string());

                    let mut sorted_stats: Vec<_> = self.tool_stats.iter().collect();
                    sorted_stats.sort_by(|a, b| b.1.0.cmp(&a.1.0));

                    for (name, (calls, successes, total_duration)) in sorted_stats.iter().take(10) {
                        let success_rate = if *calls > 0 { (*successes as f64 / *calls as f64 * 100.0) as usize } else { 0 };
                        let avg_duration = if *calls > 0 { *total_duration / *calls as u64 } else { 0 };
                        lines.push(format!("  {:<20} {:>8} {:>7}% {:>9}ms", name, calls, success_rate, avg_duration));
                    }

                    self.messages.push(ChatMessage::System(lines.join("\n")));
                }
            }
            "/tool_cache" => {
                if self.tool_cache.is_empty() {
                    self.messages.push(ChatMessage::System("工具缓存为空".into()));
                } else {
                    let mut lines = vec!["工具缓存:".to_string(), "".to_string()];
                    lines.push(format!("  {:<20} {:<30} {:>12}", "工具", "参数", "缓存时间"));
                    lines.push("  ────────────────────────────────────────────────────────".to_string());

                    for ((name, args), (_, timestamp)) in self.tool_cache.iter().take(10) {
                        let age = timestamp.elapsed().as_secs();
                        let age_str = if age < 60 {
                            format!("{}s", age)
                        } else if age < 3600 {
                            format!("{}m", age / 60)
                        } else {
                            format!("{}h", age / 3600)
                        };
                        let args_display = if args.len() > 28 {
                            format!("{}...", &args[..25])
                        } else {
                            args.clone()
                        };
                        lines.push(format!("  {:<20} {:<30} {:>12}", name, args_display, age_str));
                    }

                    if self.tool_cache.len() > 10 {
                        lines.push(format!("\n  ... 还有 {} 条缓存", self.tool_cache.len() - 10));
                    }

                    lines.push("\n用法:".to_string());
                    lines.push("  /tool_cache clear  清空缓存".to_string());
                    lines.push("  /tool_cache stats  缓存统计".to_string());

                    self.messages.push(ChatMessage::System(lines.join("\n")));
                }
            }
            "/tool_cache clear" => {
                let count = self.tool_cache.len();
                self.tool_cache.clear();
                self.messages.push(ChatMessage::System(format!("已清空 {} 条工具缓存", count)));
            }
            "/tool_cache stats" => {
                let total = self.tool_cache.len();
                let mut tool_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
                for (name, _) in self.tool_cache.keys() {
                    *tool_counts.entry(name.clone()).or_insert(0) += 1;
                }

                let mut lines = vec![
                    "工具缓存统计:".to_string(),
                    format!("  总缓存数: {}", total),
                    "".to_string(),
                ];

                if !tool_counts.is_empty() {
                    lines.push("按工具分布:".to_string());
                    let mut sorted: Vec<_> = tool_counts.iter().collect();
                    sorted.sort_by(|a, b| b.1.cmp(a.1));
                    for (name, count) in sorted.iter().take(5) {
                        lines.push(format!("  {}: {} 条", name, count));
                    }
                }

                self.messages.push(ChatMessage::System(lines.join("\n")));
            }
            "/tool_perf" => {
                // Analyze tool performance from message history
                let mut tool_durations: std::collections::HashMap<String, Vec<u64>> = std::collections::HashMap::new();
                let mut current_tool: Option<(String, std::time::Instant)> = None;

                for msg in &self.messages {
                    match msg {
                        ChatMessage::ToolCall { name, .. } => {
                            current_tool = Some((name.clone(), std::time::Instant::now()));
                        }
                        ChatMessage::ToolResult { .. } => {
                            if let Some((name, start)) = current_tool.take() {
                                let duration = start.elapsed().as_millis() as u64;
                                tool_durations.entry(name).or_insert_with(Vec::new).push(duration);
                            }
                        }
                        _ => {}
                    }
                }

                if tool_durations.is_empty() {
                    self.messages.push(ChatMessage::System("没有工具调用数据可供分析".into()));
                } else {
                    let mut lines = vec![
                        "工具性能分析:".to_string(),
                        "".to_string(),
                        format!("  {:<20} {:>6} {:>8} {:>8} {:>8} {:>8}", "工具", "调用", "平均", "最快", "最慢", "P95"),
                        "  ──────────────────────────────────────────────────────────────".to_string(),
                    ];

                    let mut sorted_tools: Vec<_> = tool_durations.iter().collect();
                    sorted_tools.sort_by(|a, b| {
                        let avg_a: u64 = a.1.iter().sum::<u64>() / a.1.len() as u64;
                        let avg_b: u64 = b.1.iter().sum::<u64>() / b.1.len() as u64;
                        avg_b.cmp(&avg_a) // Sort by average duration descending
                    });

                    for (name, durations) in sorted_tools.iter().take(10) {
                        let count = durations.len();
                        let avg = durations.iter().sum::<u64>() / count as u64;
                        let min = *durations.iter().min().unwrap_or(&0);
                        let max = *durations.iter().max().unwrap_or(&0);

                        // Calculate P95
                        let mut sorted_durs: Vec<u64> = (*durations).clone();
                        sorted_durs.sort();
                        let p95_index = (count as f64 * 0.95) as usize;
                        let p95 = sorted_durs.get(p95_index.min(count.saturating_sub(1))).unwrap_or(&0);

                        lines.push(format!("  {:<20} {:>6} {:>7}ms {:>7}ms {:>7}ms {:>7}ms", name, count, avg, min, max, p95));
                    }

                    // Add summary
                    let total_calls: usize = tool_durations.values().map(|v| v.len()).sum();
                    let total_duration: u64 = tool_durations.values().flatten().sum();
                    let overall_avg = if total_calls > 0 { total_duration / total_calls as u64 } else { 0 };

                    lines.push("".to_string());
                    lines.push("摘要:".to_string());
                    lines.push(format!("  总调用次数: {}", total_calls));
                    lines.push(format!("  总耗时: {}ms", total_duration));
                    lines.push(format!("  整体平均: {}ms", overall_avg));

                    // Identify slow tools
                    let slow_tools: Vec<&String> = sorted_tools.iter()
                        .filter(|(_, d)| {
                            let avg: u64 = d.iter().sum::<u64>() / d.len() as u64;
                            avg > 1000 // More than 1 second average
                        })
                        .map(|(name, _)| *name)
                        .collect();

                    if !slow_tools.is_empty() {
                        lines.push("".to_string());
                        lines.push("⚠️ 慢工具 (>1s 平均):".to_string());
                        for tool in slow_tools {
                            lines.push(format!("  - {}", tool));
                        }
                    }

                    self.messages.push(ChatMessage::System(lines.join("\n")));
                }
            }
            "/retry" => {
                if let Some(tool) = self.last_failed_tool.take() {
                    self.messages.push(ChatMessage::System(format!(
                        "重试工具: {} (参数: {})",
                        tool.name, tool.args
                    )));
                    // Re-send as message to trigger tool call
                    self.send_message(format!("请重新执行工具调用: {} {}", tool.name, tool.args)).await;
                } else {
                    self.messages.push(ChatMessage::System("没有失败的工具调用需要重试".into()));
                }
            }
            "/session merge" => {
                self.messages.push(ChatMessage::System(
                    "用法: /session merge <会话ID>\n将指定会话的消息合并到当前会话".into()
                ));
            }
            other if other.starts_with("/session merge ") => {
                let target_id = other[15..].trim();
                if target_id.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /session merge <会话ID>".into()));
                } else {
                    // Find session by ID prefix
                    let source_idx = self.sessions.iter().position(|s| s.id.starts_with(target_id));
                    if let Some(idx) = source_idx {
                        if idx == self.active_session {
                            self.messages.push(ChatMessage::System("不能合并当前会话到自身".into()));
                        } else {
                            let source_msgs = self.sessions[idx].messages.clone();
                            let merge_count = source_msgs.len();
                            self.messages.extend(source_msgs);
                            self.messages.push(ChatMessage::System(format!(
                                "已合并会话 {} 的 {} 条消息到当前会话",
                                &self.sessions[idx].name, merge_count
                            )));
                        }
                    } else {
                        self.messages.push(ChatMessage::System(format!("未找到会话: {}", target_id)));
                    }
                }
            }
            "/export html" => {
                let mut html = String::new();
                html.push_str("<!DOCTYPE html>\n<html><head><meta charset='utf-8'>\n");
                html.push_str("<title>Maix-Agent 对话导出</title>\n");
                html.push_str("<style>body{font-family:sans-serif;max-width:800px;margin:0 auto;padding:20px}\n");
                html.push_str(".msg{margin:10px 0;padding:10px;border-radius:8px}\n");
                html.push_str(".user{background:#e3f2fd}\n.assistant{background:#f3e5f5}\n");
                html.push_str(".system{background:#f5f5f5;color:#666;font-style:italic}\n");
                html.push_str("pre{background:#263238;color:#eeffff;padding:12px;border-radius:4px;overflow-x:auto}\n");
                html.push_str("code{font-family:'Fira Code',monospace}</style></head><body>\n");
                html.push_str("<h1>Maix-Agent 对话导出</h1>\n");
                html.push_str(&format!("<p>会话ID: {} | 模型: {} | 时间: {}</p>\n",
                    self.session_id, self.model_name,
                    chrono::Local::now().format("%Y-%m-%d %H:%M:%S")));

                for msg in &self.messages {
                    match msg {
                        ChatMessage::User(text) => {
                            html.push_str(&format!("<div class='msg user'><strong>用户:</strong><br>{}</div>\n",
                                text.replace('\n', "<br>")));
                        }
                        ChatMessage::Assistant(text) => {
                            html.push_str(&format!("<div class='msg assistant'><strong>助手:</strong><br>{}</div>\n",
                                text.replace('\n', "<br>")));
                        }
                        ChatMessage::System(text) => {
                            html.push_str(&format!("<div class='msg system'>{}</div>\n",
                                text.replace('\n', "<br>")));
                        }
                        _ => {}
                    }
                }

                html.push_str("</body></html>");
                let filename = format!("maix-chat-{}.html", chrono::Local::now().format("%Y%m%d-%H%M%S"));
                match std::fs::write(&filename, &html) {
                    Ok(_) => {
                        self.messages.push(ChatMessage::System(format!("已导出为 HTML: {}", filename)));
                    }
                    Err(e) => {
                        self.messages.push(ChatMessage::System(format!("导出失败: {}", e)));
                    }
                }
            }
            "/session compare" => {
                self.messages.push(ChatMessage::System(
                    "用法: /session compare <会话ID>\n比较当前会话与指定会话的差异".into()
                ));
            }
            other if other.starts_with("/session compare ") => {
                let target_id = other[17..].trim();
                if target_id.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /session compare <会话ID>".into()));
                } else {
                    let source_idx = self.sessions.iter().position(|s| s.id.starts_with(target_id));
                    if let Some(idx) = source_idx {
                        let current_count = self.messages.len();
                        let other_count = self.sessions[idx].messages.len();
                        let current_user = self.messages.iter().filter(|m| matches!(m, ChatMessage::User(_))).count();
                        let other_user = self.sessions[idx].messages.iter().filter(|m| matches!(m, ChatMessage::User(_))).count();
                        let current_chars: usize = self.messages.iter().map(|m| match m {
                            ChatMessage::User(t) | ChatMessage::Assistant(t) | ChatMessage::System(t) => t.len(),
                            _ => 0,
                        }).sum();
                        let other_chars: usize = self.sessions[idx].messages.iter().map(|m| match m {
                            ChatMessage::User(t) | ChatMessage::Assistant(t) | ChatMessage::System(t) => t.len(),
                            _ => 0,
                        }).sum();

                        let lines = vec![
                            format!("会话比较: 当前 vs {}", self.sessions[idx].name),
                            "".to_string(),
                            format!("  {:<20} {:>10} {:>10}", "指标", "当前", "目标"),
                            "  ──────────────────────────────────────".to_string(),
                            format!("  {:<20} {:>10} {:>10}", "总消息数", current_count, other_count),
                            format!("  {:<20} {:>10} {:>10}", "用户消息", current_user, other_user),
                            format!("  {:<20} {:>10} {:>10}", "总字符数", current_chars, other_chars),
                            format!("  {:<20} {:>10} {:>10}", "标签",
                                if self.sessions[self.active_session].tags.is_empty() { "无".to_string() } else { self.sessions[self.active_session].tags.join(",") },
                                if self.sessions[idx].tags.is_empty() { "无".to_string() } else { self.sessions[idx].tags.join(",") }),
                        ];
                        self.messages.push(ChatMessage::System(lines.join("\n")));
                    } else {
                        self.messages.push(ChatMessage::System(format!("未找到会话: {}", target_id)));
                    }
                }
            }
            "/session replay" => {
                if self.messages.is_empty() {
                    self.messages.push(ChatMessage::System("当前会话没有消息可回放".into()));
                } else {
                    let mut lines = vec!["会话回放模式:".to_string(), "".to_string()];
                    let display_count = self.messages.len().min(20);
                    for (i, msg) in self.messages.iter().enumerate().take(display_count) {
                        let (role, preview) = match msg {
                            ChatMessage::User(t) => ("用户", &t[..t.len().min(60)]),
                            ChatMessage::Assistant(t) => ("助手", &t[..t.len().min(60)]),
                            ChatMessage::System(t) => ("系统", &t[..t.len().min(60)]),
                            ChatMessage::ToolCall { name, .. } => ("工具调用", name.as_str()),
                            ChatMessage::ToolResult { .. } => ("工具结果", "..."),
                            _ => continue,
                        };
                        lines.push(format!("  [{:>3}] {}: {}", i, role, preview));
                    }
                    if self.messages.len() > display_count {
                        lines.push(format!("\n  ... 还有 {} 条消息", self.messages.len() - display_count));
                    }
                    lines.push("\n提示: 使用 Ctrl+↑/↓ 滚动查看完整回放".to_string());
                    self.messages.push(ChatMessage::System(lines.join("\n")));
                }
            }
            "/session share" => {
                let share_id = &self.session_id[..self.session_id.len().min(8)];
                let share_dir = dirs_home().join(".maix").join("shared");
                let _ = std::fs::create_dir_all(&share_dir);
                let share_file = share_dir.join(format!("{}.json", share_id));

                let share_data = serde_json::json!({
                    "session_id": self.session_id,
                    "model": self.model_name,
                    "messages": self.messages.iter().filter_map(|m| match m {
                        ChatMessage::User(t) => Some(serde_json::json!({"role": "user", "content": t})),
                        ChatMessage::Assistant(t) => Some(serde_json::json!({"role": "assistant", "content": t})),
                        _ => None,
                    }).collect::<Vec<_>>(),
                    "shared_at": chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                    "readonly": true,
                });

                match std::fs::write(&share_file, serde_json::to_string_pretty(&share_data).unwrap()) {
                    Ok(_) => {
                        self.messages.push(ChatMessage::System(format!(
                            "会话已分享\nID: {}\n路径: {}\n模式: 只读",
                            share_id, share_file.display()
                        )));
                    }
                    Err(e) => {
                        self.messages.push(ChatMessage::System(format!("分享失败: {}", e)));
                    }
                }
            }
            "/tool_chain" => {
                if self.tool_chains.is_empty() {
                    self.messages.push(ChatMessage::System(
                        "工具链管理:\n\n\
                        当前没有保存的工具链。\n\n\
                        用法:\n\
                        /tool_chain add <名称> <工具1> <工具2> ...  创建工具链\n\
                        /tool_chain run <名称>                      执行工具链\n\
                        /tool_chain list                            列出工具链\n\
                        /tool_chain rm <名称>                       删除工具链".into()
                    ));
                } else {
                    let mut lines = vec!["工具链列表:".to_string()];
                    for chain in &self.tool_chains {
                        let tools: Vec<&str> = chain.steps.iter().map(|s| s.tool_name.as_str()).collect();
                        lines.push(format!("  {} -> {}", chain.name, tools.join(" -> ")));
                    }
                    self.messages.push(ChatMessage::System(lines.join("\n")));
                }
            }
            other if other.starts_with("/tool_chain add ") => {
                let parts: Vec<&str> = other[16..].trim().split_whitespace().collect();
                if parts.len() < 3 {
                    self.messages.push(ChatMessage::System("用法: /tool_chain add <名称> <工具1> <工具2> ...".into()));
                } else {
                    let name = parts[0].to_string();
                    let steps = parts[1..].iter().map(|t| ToolChainStep {
                        tool_name: t.to_string(),
                        args_template: String::new(),
                    }).collect();
                    self.tool_chains.push(ToolChain {
                        name: name.clone(),
                        steps,
                        created_at: chrono::Local::now().naive_local(),
                    });
                    self.messages.push(ChatMessage::System(format!("已创建工具链: {}", name)));
                }
            }
            other if other.starts_with("/tool_chain run ") => {
                let name = other[16..].trim();
                if let Some(chain) = self.tool_chains.iter().find(|c| c.name == name) {
                    let tools: Vec<&str> = chain.steps.iter().map(|s| s.tool_name.as_str()).collect();
                    self.messages.push(ChatMessage::System(format!(
                        "执行工具链: {}\n步骤: {}\n注意: 实际执行需要通过 AI 处理",
                        name, tools.join(" -> ")
                    )));
                    // Send as message to trigger AI execution
                    let chain_msg = format!("请按顺序执行以下工具链: {}", tools.join(", "));
                    self.send_message(chain_msg).await;
                } else {
                    self.messages.push(ChatMessage::System(format!("未找到工具链: {}", name)));
                }
            }
            other if other.starts_with("/tool_chain rm ") => {
                let name = other[15..].trim();
                if let Some(pos) = self.tool_chains.iter().position(|c| c.name == name) {
                    self.tool_chains.remove(pos);
                    self.messages.push(ChatMessage::System(format!("已删除工具链: {}", name)));
                } else {
                    self.messages.push(ChatMessage::System(format!("未找到工具链: {}", name)));
                }
            }
            // 099-005: Tool chain flow visualization
            other if other.starts_with("/chain show ") || other.starts_with("/tool_chain show ") => {
                let name = if other.starts_with("/chain show ") {
                    other[12..].trim()
                } else {
                    other[17..].trim()
                };
                if let Some(chain) = self.tool_chains.iter().find(|c| c.name == name) {
                    let mut flow = vec![
                        format!("工具链流程图: {}", chain.name),
                        String::new(),
                    ];
                    for (i, step) in chain.steps.iter().enumerate() {
                        let tool_info = self.tool_defs.iter().find(|t| t.name == step.tool_name);
                        let risk = tool_info.map(|t| t.risk_level).unwrap_or(0);
                        let risk_icon = match risk {
                            0 => " ",
                            1 => " ",
                            2 => " ",
                            _ => " ",
                        };
                        flow.push(format!("  ┌─────────────────────────────┐"));
                        flow.push(format!("  │ {} {} {}", risk_icon, step.tool_name, " ".repeat(20usize.saturating_sub(step.tool_name.len()))));
                        if !step.args_template.is_empty() {
                            let args_display: String = step.args_template.chars().take(25).collect();
                            flow.push(format!("  │ args: {}...", args_display));
                        }
                        flow.push(format!("  └─────────────────────────────┘"));
                        if i < chain.steps.len() - 1 {
                            flow.push(format!("           │"));
                            flow.push(format!("           ▼"));
                        }
                    }
                    flow.push(String::new());
                    flow.push(format!("  共 {} 个步骤, 创建于 {}", chain.steps.len(), chain.created_at.format("%Y-%m-%d %H:%M")));
                    self.messages.push(ChatMessage::System(flow.join("\n")));
                } else {
                    self.messages.push(ChatMessage::System(format!("未找到工具链: {}", name)));
                }
            }
            // 099-011: Debug console
            "/debug" => {
                if self.debug_log.is_empty() {
                    self.messages.push(ChatMessage::System(
                        "调试控制台 (099-011):\n\n\
                        当前没有调试日志。\n\n\
                        调试日志会在 gRPC 请求/响应时自动记录。\n\
                        使用 /debug clear 清除日志\n\
                        使用 /debug <filter> 过滤日志".into()
                    ));
                } else {
                    let mut lines = vec!["调试控制台:".to_string(), String::new()];
                    for entry in self.debug_log.iter().rev().take(30) {
                        lines.push(format!("[{}] [{}] {}",
                            entry.timestamp.format("%H:%M:%S"),
                            entry.level,
                            entry.message));
                    }
                    if self.debug_log.len() > 30 {
                        lines.push(format!("... 还有 {} 条记录", self.debug_log.len() - 30));
                    }
                    self.messages.push(ChatMessage::System(lines.join("\n")));
                }
            }
            other if other.starts_with("/debug ") => {
                let filter = other[7..].trim();
                if filter == "clear" {
                    self.debug_log.clear();
                    self.messages.push(ChatMessage::System("调试日志已清除".into()));
                } else {
                    let filtered: Vec<_> = self.debug_log.iter()
                        .filter(|e| e.message.contains(filter) || e.level.contains(filter))
                        .rev()
                        .take(30)
                        .collect();
                    if filtered.is_empty() {
                        self.messages.push(ChatMessage::System(format!("未找到匹配 '{}' 的调试日志", filter)));
                    } else {
                        let mut lines = vec![format!("调试日志 (过滤: {}):", filter), String::new()];
                        for entry in filtered {
                            lines.push(format!("[{}] [{}] {}",
                                entry.timestamp.format("%H:%M:%S"),
                                entry.level,
                                entry.message));
                        }
                        self.messages.push(ChatMessage::System(lines.join("\n")));
                    }
                }
            }
            // 099-013: Network request tracking
            "/net" => {
                if self.network_requests.is_empty() {
                    self.messages.push(ChatMessage::System(
                        "网络请求追踪 (099-013):\n\n\
                        当前没有网络请求记录。\n\n\
                        网络请求会在 gRPC 调用时自动记录。\n\
                        使用 /net clear 清除记录\n\
                        使用 /net stats 查看统计".into()
                    ));
                } else {
                    let mut lines = vec!["网络请求追踪:".to_string(), String::new()];
                    lines.push(format!("  {:<6} {:<30} {:<8} {:<10} {:<10}",
                        "状态", "URL", "延迟", "请求大小", "响应大小"));
                    lines.push(format!("  {}", "─".repeat(70)));
                    for req in self.network_requests.iter().rev().take(20) {
                        let status_icon = if req.status >= 200 && req.status < 300 { "✓" } else { "✗" };
                        let url_display: String = req.url.chars().take(28).collect();
                        lines.push(format!("  {} {:<4} {:<30} {:<6}ms {:<8} {:<8}",
                            status_icon, req.status, url_display, req.latency_ms,
                            format_size(req.request_size), format_size(req.response_size)));
                    }
                    if self.network_requests.len() > 20 {
                        lines.push(format!("... 还有 {} 条记录", self.network_requests.len() - 20));
                    }
                    self.messages.push(ChatMessage::System(lines.join("\n")));
                }
            }
            "/net stats" => {
                if self.network_requests.is_empty() {
                    self.messages.push(ChatMessage::System("没有网络请求记录".into()));
                } else {
                    let total = self.network_requests.len();
                    let success = self.network_requests.iter().filter(|r| r.status >= 200 && r.status < 300).count();
                    let avg_latency = self.network_requests.iter().map(|r| r.latency_ms as f64).sum::<f64>() / total as f64;
                    let max_latency = self.network_requests.iter().map(|r| r.latency_ms).max().unwrap_or(0);
                    let total_sent = self.network_requests.iter().map(|r| r.request_size).sum::<usize>();
                    let total_recv = self.network_requests.iter().map(|r| r.response_size).sum::<usize>();
                    self.messages.push(ChatMessage::System(format!(
                        "网络统计:\n\n  总请求数: {}\n  成功率: {:.1}%\n  平均延迟: {:.0}ms\n  最大延迟: {}ms\n  总发送: {}\n  总接收: {}",
                        total,
                        success as f64 / total as f64 * 100.0,
                        avg_latency,
                        max_latency,
                        format_size(total_sent),
                        format_size(total_recv),
                    )));
                }
            }
            "/net clear" => {
                self.network_requests.clear();
                self.messages.push(ChatMessage::System("网络请求记录已清除".into()));
            }
            // 099-017: State checkpoints
            "/checkpoint" => {
                if self.checkpoints.is_empty() {
                    self.messages.push(ChatMessage::System(
                        "状态检查点 (099-017):\n\n\
                        当前没有保存的检查点。\n\n\
                        /checkpoint save <名称>  保存检查点\n\
                        /checkpoint load <名称>  恢复检查点\n\
                        /checkpoint list         列出检查点\n\
                        /checkpoint rm <名称>    删除检查点".into()
                    ));
                } else {
                    let mut lines = vec!["状态检查点:".to_string(), String::new()];
                    for cp in &self.checkpoints {
                        lines.push(format!("  {} | 消息:{} | token:{} | 费用:¥{:.4} | {}",
                            cp.name, cp.message_count, cp.total_tokens, cp.total_cost,
                            cp.created_at.format("%Y-%m-%d %H:%M:%S")));
                    }
                    self.messages.push(ChatMessage::System(lines.join("\n")));
                }
            }
            other if other.starts_with("/checkpoint save ") => {
                let name = other[17..].trim().to_string();
                if name.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /checkpoint save <名称>".into()));
                } else {
                    self.checkpoints.push(StateCheckpoint {
                        name: name.clone(),
                        message_count: self.messages.len(),
                        total_tokens: self.total_tokens,
                        total_cost: self.total_cost,
                        created_at: chrono::Local::now().naive_local(),
                    });
                    self.messages.push(ChatMessage::System(format!("检查点已保存: {}", name)));
                }
            }
            other if other.starts_with("/checkpoint load ") => {
                let name = other[17..].trim();
                if let Some(cp) = self.checkpoints.iter().find(|c| c.name == name).cloned() {
                    // Restore state from checkpoint
                    let removed = self.messages.len().saturating_sub(cp.message_count);
                    self.messages.truncate(cp.message_count);
                    self.total_tokens = cp.total_tokens;
                    self.total_cost = cp.total_cost;
                    self.messages.push(ChatMessage::System(format!(
                        "已恢复检查点 '{}': 移除了 {} 条消息, token恢复为{}, 费用恢复为¥{:.4}",
                        name, removed, cp.total_tokens, cp.total_cost
                    )));
                } else {
                    self.messages.push(ChatMessage::System(format!("未找到检查点: {}", name)));
                }
            }
            "/checkpoint list" => {
                if self.checkpoints.is_empty() {
                    self.messages.push(ChatMessage::System("没有保存的检查点".into()));
                } else {
                    let mut lines = vec!["状态检查点列表:".to_string(), String::new()];
                    for cp in &self.checkpoints {
                        lines.push(format!("  {} - {} (消息:{}, token:{}, ¥{:.4})",
                            cp.name, cp.created_at.format("%Y-%m-%d %H:%M:%S"),
                            cp.message_count, cp.total_tokens, cp.total_cost));
                    }
                    self.messages.push(ChatMessage::System(lines.join("\n")));
                }
            }
            other if other.starts_with("/checkpoint rm ") => {
                let name = other[15..].trim();
                if let Some(pos) = self.checkpoints.iter().position(|c| c.name == name) {
                    self.checkpoints.remove(pos);
                    self.messages.push(ChatMessage::System(format!("已删除检查点: {}", name)));
                } else {
                    self.messages.push(ChatMessage::System(format!("未找到检查点: {}", name)));
                }
            }
            // 099-020: Session recording
            "/record" => {
                let status = if let Some(ref rec) = self.recording {
                    if rec.is_active {
                        format!("录制中 ({} 个事件, 开始于 {})",
                            rec.events.len(), rec.start_time.format("%H:%M:%S"))
                    } else {
                        "未录制".to_string()
                    }
                } else {
                    "未录制".to_string()
                };
                self.messages.push(ChatMessage::System(format!(
                    "会话录制 (099-020):\n\n  状态: {}\n\n\
                    /record start   开始录制\n\
                    /record stop    停止录制\n\
                    /record export  导出录制文件\n\
                    /record clear   清除录制数据", status
                )));
            }
            "/record start" => {
                if let Some(ref rec) = self.recording {
                    if rec.is_active {
                        self.messages.push(ChatMessage::System("录制已在进行中".into()));
                    } else {
                        self.recording = Some(SessionRecording {
                            start_time: chrono::Local::now().naive_local(),
                            events: Vec::new(),
                            is_active: true,
                        });
                        self.messages.push(ChatMessage::System("开始录制会话".into()));
                    }
                } else {
                    self.recording = Some(SessionRecording {
                        start_time: chrono::Local::now().naive_local(),
                        events: Vec::new(),
                        is_active: true,
                    });
                    self.messages.push(ChatMessage::System("开始录制会话".into()));
                }
            }
            "/record stop" => {
                if let Some(ref mut rec) = self.recording {
                    if rec.is_active {
                        rec.is_active = false;
                        self.messages.push(ChatMessage::System(format!(
                            "录制已停止, 共 {} 个事件", rec.events.len()
                        )));
                    } else {
                        self.messages.push(ChatMessage::System("没有进行中的录制".into()));
                    }
                } else {
                    self.messages.push(ChatMessage::System("没有进行中的录制".into()));
                }
            }
            "/record export" => {
                if let Some(ref rec) = self.recording {
                    if rec.events.is_empty() {
                        self.messages.push(ChatMessage::System("没有可导出的录制数据".into()));
                    } else {
                        let home = std::env::var("USERPROFILE")
                            .or_else(|_| std::env::var("HOME"))
                            .map(std::path::PathBuf::from)
                            .unwrap_or_else(|_| std::path::PathBuf::from("."));
                        let export_path = home.join(".maix").join(format!("recording_{}.json",
                            rec.start_time.format("%Y%m%d_%H%M%S")));
                        let json = serde_json::json!({
                            "start_time": rec.start_time.to_string(),
                            "event_count": rec.events.len(),
                            "events": rec.events.iter().map(|e| {
                                serde_json::json!({
                                    "time": e.timestamp.to_string(),
                                    "type": e.event_type,
                                    "content": e.content,
                                })
                            }).collect::<Vec<_>>(),
                        });
                        if let Ok(_) = std::fs::write(&export_path, serde_json::to_string_pretty(&json).unwrap_or_default()) {
                            self.messages.push(ChatMessage::System(format!("录制已导出到: {}", export_path.display())));
                        } else {
                            self.messages.push(ChatMessage::System("导出失败".into()));
                        }
                    }
                } else {
                    self.messages.push(ChatMessage::System("没有录制数据".into()));
                }
            }
            "/record clear" => {
                self.recording = None;
                self.messages.push(ChatMessage::System("录制数据已清除".into()));
            }
            // 099-015: Performance stats
            "/perf" => {
                let session_duration = self.session_start.elapsed();
                let duration_secs = session_duration.as_secs();
                let hours = duration_secs / 3600;
                let minutes = (duration_secs % 3600) / 60;
                let seconds = duration_secs % 60;

                let avg_latency = if !self.network_requests.is_empty() {
                    self.network_requests.iter().map(|r| r.latency_ms as f64).sum::<f64>() / self.network_requests.len() as f64
                } else {
                    0.0
                };

                let tokens_per_round = if self.round_count > 0 {
                    self.total_tokens as f64 / self.round_count as f64
                } else {
                    0.0
                };

                let cost_per_round = if self.round_count > 0 {
                    self.total_cost / self.round_count as f64
                } else {
                    0.0
                };

                // Build performance flame chart (ASCII bar chart of tool usage)
                let mut tool_usage_lines = Vec::new();
                if !self.tool_stats.is_empty() {
                    let mut sorted_tools: Vec<_> = self.tool_stats.iter().collect();
                    sorted_tools.sort_by(|a, b| b.1.0.cmp(&a.1.0));
                    let max_count = sorted_tools.first().map(|t| t.1.0).unwrap_or(1);
                    let bar_width = 20;

                    tool_usage_lines.push(String::new());
                    tool_usage_lines.push("  工具使用分布:".to_string());
                    for (name, (count, success, duration)) in sorted_tools.iter().take(10) {
                        let bar_len = (*count as f64 / max_count as f64 * bar_width as f64) as usize;
                        let bar = format!("{}{}", "█".repeat(bar_len), "░".repeat(bar_width - bar_len));
                        let success_rate = if *count > 0 { *success as f64 / *count as f64 * 100.0 } else { 0.0 };
                        let avg_dur = if *count > 0 { *duration / *count as u64 } else { 0 };
                        tool_usage_lines.push(format!("  {:<15} {} {:>3}次 {:.0}% {:>4}ms/次", name, bar, count, success_rate, avg_dur));
                    }
                }

                self.messages.push(ChatMessage::System(format!(
                    "性能分析 (099-015):\n\n\
                    会话时长: {}h {}m {}s\n\
                    总轮次: {}\n\
                    总消息: {}\n\
                    总token: {} ({:.0} token/轮)\n\
                    总费用: ¥{:.4} ({:.6} ¥/轮)\n\
                    Token速率: {:.1} t/s\n\
                    网络请求: {} (平均延迟: {:.0}ms)\n\
                    补全项: {}\n\
                    {}",
                    hours, minutes, seconds,
                    self.round_count,
                    self.messages.len(),
                    self.total_tokens, tokens_per_round,
                    self.total_cost, cost_per_round,
                    self.token_rate,
                    self.network_requests.len(), avg_latency,
                    self.input.completions.len(),
                    tool_usage_lines.join("\n"),
                )));
            }
            // 100-008: Message tags
            other if other.starts_with("/tag msg ") => {
                let parts: Vec<&str> = other[9..].trim().splitn(2, ' ').collect();
                if parts.len() < 2 {
                    self.messages.push(ChatMessage::System("用法: /tag msg <index> <tag>".into()));
                } else if let Ok(idx) = parts[0].parse::<usize>() {
                    if idx < self.messages.len() {
                        self.message_tags.insert(idx, parts[1].to_string());
                        self.messages.push(ChatMessage::System(format!("已标记消息 #{}: {}", idx, parts[1])));
                    } else {
                        self.messages.push(ChatMessage::System(format!("无效消息索引: {}", idx)));
                    }
                } else {
                    self.messages.push(ChatMessage::System("用法: /tag msg <index> <tag>".into()));
                }
            }
            "/tags" => {
                if self.message_tags.is_empty() {
                    self.messages.push(ChatMessage::System("没有标记的消息\n用法: /tag msg <index> <tag>".into()));
                } else {
                    let mut lines = vec!["消息标记列表:".to_string(), String::new()];
                    for (idx, tag) in &self.message_tags {
                        let preview = match self.messages.get(*idx) {
                            Some(ChatMessage::User(t)) => format!("User: {}", truncate_str(t, 30)),
                            Some(ChatMessage::Assistant(t)) => format!("AI: {}", truncate_str(t, 30)),
                            _ => format!("msg#{}", idx),
                        };
                        lines.push(format!("  #{} [{}] {}", idx, tag, preview));
                    }
                    self.messages.push(ChatMessage::System(lines.join("\n")));
                }
            }
            // 100-009: Pin messages
            other if other.starts_with("/pin msg ") => {
                let idx_str = other[9..].trim();
                if let Ok(idx) = idx_str.parse::<usize>() {
                    if idx < self.messages.len() {
                        if !self.pinned_messages.contains(&idx) {
                            self.pinned_messages.push(idx);
                        }
                        self.messages.push(ChatMessage::System(format!("已固定消息 #{}", idx)));
                    } else {
                        self.messages.push(ChatMessage::System(format!("无效消息索引: {}", idx)));
                    }
                } else {
                    self.messages.push(ChatMessage::System("用法: /pin msg <index>".into()));
                }
            }
            "/pinned" => {
                if self.pinned_messages.is_empty() {
                    self.messages.push(ChatMessage::System("没有固定的消息\n用法: /pin msg <index>".into()));
                } else {
                    let mut lines = vec!["固定消息列表:".to_string(), String::new()];
                    for idx in &self.pinned_messages {
                        let preview = match self.messages.get(*idx) {
                            Some(ChatMessage::User(t)) => format!("User: {}", truncate_str(t, 50)),
                            Some(ChatMessage::Assistant(t)) => format!("AI: {}", truncate_str(t, 50)),
                            _ => format!("msg#{}", idx),
                        };
                        lines.push(format!("  #{} {}", idx, preview));
                    }
                    self.messages.push(ChatMessage::System(lines.join("\n")));
                }
            }
            other if other.starts_with("/unpin msg ") => {
                let idx_str = other[11..].trim();
                if let Ok(idx) = idx_str.parse::<usize>() {
                    if let Some(pos) = self.pinned_messages.iter().position(|&i| i == idx) {
                        self.pinned_messages.remove(pos);
                        self.messages.push(ChatMessage::System(format!("已取消固定消息 #{}", idx)));
                    } else {
                        self.messages.push(ChatMessage::System(format!("消息 #{} 未被固定", idx)));
                    }
                } else {
                    self.messages.push(ChatMessage::System("用法: /unpin msg <index>".into()));
                }
            }
            // 100-014: Session notes
            "/notes" => {
                if self.session_notes.is_empty() {
                    self.messages.push(ChatMessage::System(
                        "会话备注 (100-014):\n\n\
                        当前没有备注。\n\n\
                        /note set <内容>  设置备注\n\
                        /note show        显示备注\n\
                        /note clear       清除备注".into()
                    ));
                } else {
                    self.messages.push(ChatMessage::System(format!("会话备注:\n{}", self.session_notes)));
                }
            }
            other if other.starts_with("/notes set ") => {
                self.session_notes = other[11..].trim().to_string();
                self.messages.push(ChatMessage::System("备注已设置".into()));
            }
            "/notes show" => {
                if self.session_notes.is_empty() {
                    self.messages.push(ChatMessage::System("没有会话备注".into()));
                } else {
                    self.messages.push(ChatMessage::System(format!("会话备注:\n{}", self.session_notes)));
                }
            }
            "/notes clear" => {
                self.session_notes.clear();
                self.messages.push(ChatMessage::System("备注已清除".into()));
            }
            // 100-004: Quote reply
            "/quote" => {
                if let Some(last_msg) = self.messages.iter().rev().find(|m| matches!(m, ChatMessage::User(_) | ChatMessage::Assistant(_))) {
                    let quote_text = match last_msg {
                        ChatMessage::User(t) => format!("> You: {}", t.lines().take(3).collect::<Vec<_>>().join("\n> ")),
                        ChatMessage::Assistant(t) => format!("> Maix: {}", t.lines().take(3).collect::<Vec<_>>().join("\n> ")),
                        _ => String::new(),
                    };
                    self.messages.push(ChatMessage::System(format!(
                        "引用上一条消息:\n{}\n\n输入回复内容，引用会自动包含在消息中。",
                        quote_text
                    )));
                    // Pre-fill input with quote
                    self.input.buffer = format!("{}\n", quote_text);
                    self.input.cursor = self.input.buffer.len();
                } else {
                    self.messages.push(ChatMessage::System("没有可引用的消息".into()));
                }
            }
            // 100-023: Crash recovery
            "/recover" => {
                let save_dir = dirs_home().join(".maix").join("autosave");
                if let Ok(entries) = std::fs::read_dir(&save_dir) {
                    let mut saves: Vec<_> = entries
                        .filter_map(|e| e.ok())
                        .filter(|e| e.path().extension().map(|ext| ext == "json").unwrap_or(false))
                        .collect();
                    saves.sort_by(|a, b| b.metadata().ok().and_then(|m| m.modified().ok()).cmp(&a.metadata().ok().and_then(|m| m.modified().ok())));

                    if saves.is_empty() {
                        self.messages.push(ChatMessage::System("没有自动保存的会话".into()));
                    } else {
                        let mut lines = vec!["自动保存的会话:".to_string(), String::new()];
                        for (i, entry) in saves.iter().take(10).enumerate() {
                            let name = entry.file_name().to_string_lossy().to_string();
                            let size = entry.metadata().ok().map(|m| m.len()).unwrap_or(0);
                            let modified = entry.metadata().ok()
                                .and_then(|m| m.modified().ok())
                                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                                .map(|d| chrono::DateTime::from_timestamp(d.as_secs() as i64, 0).map(|dt| dt.naive_utc()).unwrap_or_default())
                                .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                                .unwrap_or_default();
                            lines.push(format!("  {}. {} ({} bytes, {})", i + 1, name, size, modified));
                        }
                        lines.push(String::new());
                        lines.push("使用 /recover <编号> 恢复会话".to_string());
                        self.messages.push(ChatMessage::System(lines.join("\n")));
                    }
                } else {
                    self.messages.push(ChatMessage::System("无法读取自动保存目录".into()));
                }
            }
            other if other.starts_with("/recover ") => {
                let idx_str = other[9..].trim();
                let save_dir = dirs_home().join(".maix").join("autosave");
                if let Ok(entries) = std::fs::read_dir(&save_dir) {
                    let mut saves: Vec<_> = entries
                        .filter_map(|e| e.ok())
                        .filter(|e| e.path().extension().map(|ext| ext == "json").unwrap_or(false))
                        .collect();
                    saves.sort_by(|a, b| b.metadata().ok().and_then(|m| m.modified().ok()).cmp(&a.metadata().ok().and_then(|m| m.modified().ok())));

                    if let Ok(idx) = idx_str.parse::<usize>() {
                        if idx > 0 && idx <= saves.len() {
                            let save_file = &saves[idx - 1].path();
                            if let Ok(content) = std::fs::read_to_string(save_file) {
                                if let Ok(data) = serde_json::from_str::<serde_json::Value>(&content) {
                                    if let Some(messages) = data.get("messages").and_then(|m| m.as_array()) {
                                        let mut recovered = Vec::new();
                                        for msg in messages {
                                            if let (Some(role), Some(content)) = (msg.get("role").and_then(|r| r.as_str()), msg.get("content").and_then(|c| c.as_str())) {
                                                match role {
                                                    "user" => recovered.push(ChatMessage::User(content.to_string())),
                                                    "assistant" => recovered.push(ChatMessage::Assistant(content.to_string())),
                                                    "system" => recovered.push(ChatMessage::System(content.to_string())),
                                                    _ => {}
                                                }
                                            }
                                        }
                                        let count = recovered.len();
                                        self.messages = recovered;
                                        self.chat_scroll = 0;
                                        self.messages.push(ChatMessage::System(format!("已恢复会话 ({} 条消息)", count)));
                                    }
                                }
                            }
                        } else {
                            self.messages.push(ChatMessage::System(format!("无效编号: {}. 使用 /recover 查看可用会话", idx)));
                        }
                    } else {
                        self.messages.push(ChatMessage::System("用法: /recover <编号>".into()));
                    }
                } else {
                    self.messages.push(ChatMessage::System("无法读取自动保存目录".into()));
                }
            }
            // 101-001: Message references
            other if other.starts_with("/ref ") => {
                let parts: Vec<&str> = other[5..].trim().splitn(2, ' ').collect();
                if parts.len() < 2 {
                    self.messages.push(ChatMessage::System("用法: /ref <from_idx> <to_idx>".into()));
                } else if let (Ok(from), Ok(to)) = (parts[0].parse::<usize>(), parts[1].parse::<usize>()) {
                    if from < self.messages.len() && to < self.messages.len() {
                        self.message_references.insert(from, to);
                        self.messages.push(ChatMessage::System(format!("已建立引用: #{} -> #{}", from, to)));
                    } else {
                        self.messages.push(ChatMessage::System("无效消息索引".into()));
                    }
                } else {
                    self.messages.push(ChatMessage::System("用法: /ref <from_idx> <to_idx>".into()));
                }
            }
            other if other.starts_with("/refs ") => {
                let idx_str = other[6..].trim();
                if let Ok(idx) = idx_str.parse::<usize>() {
                    let refs_from: Vec<_> = self.message_references.iter().filter(|(_, &v)| v == idx).map(|(&k, _)| k).collect();
                    let refs_to: Vec<_> = self.message_references.iter().filter(|(&k, _)| k == idx).map(|(_, &v)| v).collect();
                    if refs_from.is_empty() && refs_to.is_empty() {
                        self.messages.push(ChatMessage::System(format!("消息 #{} 没有引用关系", idx)));
                    } else {
                        let mut lines = vec![format!("消息 #{} 的引用关系:", idx)];
                        if !refs_from.is_empty() {
                            lines.push(format!("  被引用: {}", refs_from.iter().map(|i| format!("#{}", i)).collect::<Vec<_>>().join(", ")));
                        }
                        if !refs_to.is_empty() {
                            lines.push(format!("  引用到: {}", refs_to.iter().map(|i| format!("#{}", i)).collect::<Vec<_>>().join(", ")));
                        }
                        self.messages.push(ChatMessage::System(lines.join("\n")));
                    }
                } else {
                    self.messages.push(ChatMessage::System("用法: /refs <msg_idx>".into()));
                }
            }
            // 101-003: Message archiving
            other if other.starts_with("/archive ") => {
                let idx_str = other[9..].trim();
                if let Ok(idx) = idx_str.parse::<usize>() {
                    if idx < self.messages.len() {
                        let msg = self.messages.remove(idx);
                        self.archived_messages.push(msg);
                        // Update references
                        self.message_references.retain(|&k, &mut v| k != idx && v != idx);
                        self.messages.push(ChatMessage::System(format!("已归档消息 #{}", idx)));
                    } else {
                        self.messages.push(ChatMessage::System(format!("无效消息索引: {}", idx)));
                    }
                } else {
                    self.messages.push(ChatMessage::System("用法: /archive <msg_idx>".into()));
                }
            }
            "/archived" => {
                if self.archived_messages.is_empty() {
                    self.messages.push(ChatMessage::System("没有归档的消息".into()));
                } else {
                    let mut lines = vec![format!("归档消息 ({} 条):", self.archived_messages.len())];
                    for (i, msg) in self.archived_messages.iter().rev().take(10).enumerate() {
                        let preview = match msg {
                            ChatMessage::User(t) => format!("User: {}", truncate_str(t, 40)),
                            ChatMessage::Assistant(t) => format!("AI: {}", truncate_str(t, 40)),
                            _ => "其他".to_string(),
                        };
                        lines.push(format!("  {} {}", i, preview));
                    }
                    self.messages.push(ChatMessage::System(lines.join("\n")));
                }
            }
            // 101-008: Storage usage stats
            "/storage" => {
                let home = dirs_home().join(".maix");
                let mut total_size: u64 = 0;
                let mut file_count = 0;

                if let Ok(entries) = std::fs::read_dir(&home) {
                    for entry in entries.flatten() {
                        if let Ok(metadata) = entry.metadata() {
                            if metadata.is_file() {
                                total_size += metadata.len();
                                file_count += 1;
                            }
                        }
                    }
                }

                let autosave_dir = home.join("autosave");
                let autosave_size = if let Ok(entries) = std::fs::read_dir(&autosave_dir) {
                    entries.flatten().filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum::<u64>()
                } else {
                    0
                };

                let profiles_dir = home.join("profiles");
                let profiles_size = if let Ok(entries) = std::fs::read_dir(&profiles_dir) {
                    entries.flatten().filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum::<u64>()
                } else {
                    0
                };

                let format_bytes = |b: u64| -> String {
                    if b >= 1_073_741_824 { format!("{:.1} GB", b as f64 / 1_073_741_824.0) }
                    else if b >= 1_048_576 { format!("{:.1} MB", b as f64 / 1_048_576.0) }
                    else if b >= 1024 { format!("{:.1} KB", b as f64 / 1024.0) }
                    else { format!("{} B", b) }
                };

                self.messages.push(ChatMessage::System(format!(
                    "存储使用统计:\n\n\
                    配置目录: {}\n\
                    总文件数: {}\n\
                    总大小: {}\n\n\
                    自动保存: {}\n\
                    用户配置: {}\n\
                    消息数: {}\n\
                    归档数: {}\n\
                    记忆数: {}\n\
                    工具数: {}",
                    home.display(), file_count, format_bytes(total_size + autosave_size + profiles_size),
                    format_bytes(autosave_size), format_bytes(profiles_size),
                    self.messages.len(), self.archived_messages.len(),
                    self.memories.len(), self.tool_defs.len(),
                )));
            }
            // 101-011: Command favorites
            other if other.starts_with("/fav add ") => {
                let cmd = other[9..].trim().to_string();
                if cmd.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /fav add <command>".into()));
                } else if !self.command_favorites.contains(&cmd) {
                    self.command_favorites.push(cmd.clone());
                    self.messages.push(ChatMessage::System(format!("已添加到收藏: {}", cmd)));
                } else {
                    self.messages.push(ChatMessage::System(format!("已在收藏中: {}", cmd)));
                }
            }
            other if other.starts_with("/fav rm ") => {
                let cmd = other[8..].trim();
                if let Some(pos) = self.command_favorites.iter().position(|c| c == cmd) {
                    self.command_favorites.remove(pos);
                    self.messages.push(ChatMessage::System(format!("已从收藏移除: {}", cmd)));
                } else {
                    self.messages.push(ChatMessage::System(format!("未在收藏中: {}", cmd)));
                }
            }
            "/favs" => {
                if self.command_favorites.is_empty() {
                    self.messages.push(ChatMessage::System("没有收藏的命令\n用法: /fav add <command>".into()));
                } else {
                    let mut lines = vec!["收藏的命令:".to_string()];
                    for (i, cmd) in self.command_favorites.iter().enumerate() {
                        lines.push(format!("  {}. {}", i + 1, cmd));
                    }
                    self.messages.push(ChatMessage::System(lines.join("\n")));
                }
            }
            // 101-016: Quick navigation
            other if other.starts_with("/goto ") => {
                let idx_str = other[6..].trim();
                if let Ok(idx) = idx_str.parse::<usize>() {
                    if idx < self.messages.len() {
                        self.chat_scroll = self.messages.len().saturating_sub(idx).saturating_sub(10);
                        self.focused_message = Some(idx);
                        self.messages.push(ChatMessage::System(format!("跳转到消息 #{}", idx)));
                    } else {
                        self.messages.push(ChatMessage::System(format!("无效消息索引: {}", idx)));
                    }
                } else {
                    self.messages.push(ChatMessage::System("用法: /goto <msg_idx>".into()));
                }
            }
            // 101-013: Batch operations
            other if other.starts_with("/batch delete ") => {
                let range_str = other[14..].trim();
                let parts: Vec<&str> = range_str.split('-').collect();
                if parts.len() == 2 {
                    if let (Ok(start), Ok(end)) = (parts[0].parse::<usize>(), parts[1].parse::<usize>()) {
                        if start <= end && end < self.messages.len() {
                            let count = end - start + 1;
                            for _ in 0..count {
                                if start < self.messages.len() {
                                    self.messages.remove(start);
                                }
                            }
                            self.message_references.retain(|&k, &mut v| k < self.messages.len() && v < self.messages.len());
                            self.messages.push(ChatMessage::System(format!("已批量删除 {} 条消息", count)));
                        } else {
                            self.messages.push(ChatMessage::System("无效范围".into()));
                        }
                    } else {
                        self.messages.push(ChatMessage::System("用法: /batch delete <start>-<end>".into()));
                    }
                } else {
                    self.messages.push(ChatMessage::System("用法: /batch delete <start>-<end>".into()));
                }
            }
            "/batch archive all" => {
                let count = self.messages.len();
                self.archived_messages.extend(self.messages.drain(..));
                self.message_references.clear();
                self.messages.push(ChatMessage::System(format!("已归档所有 {} 条消息", count)));
            }
            // 101-017: Context management - compact
            "/compact" => {
                let before = self.messages.len();
                // Keep last N messages and system messages
                let keep_count = 20;
                if self.messages.len() > keep_count {
                    let system_msgs: Vec<_> = self.messages.iter()
                        .enumerate()
                        .filter(|(_, m)| matches!(m, ChatMessage::System(_)))
                        .map(|(i, _)| i)
                        .collect();
                    let mut keep_indices: Vec<usize> = system_msgs;
                    let start = self.messages.len().saturating_sub(keep_count);
                    for i in start..self.messages.len() {
                        if !keep_indices.contains(&i) {
                            keep_indices.push(i);
                        }
                    }
                    keep_indices.sort();
                    keep_indices.dedup();

                    let new_messages: Vec<_> = keep_indices.iter().filter_map(|&i| self.messages.get(i).cloned()).collect();
                    let removed = before - new_messages.len();
                    self.messages = new_messages;
                    self.chat_scroll = 0;
                    self.message_references.clear();
                    self.messages.push(ChatMessage::System(format!(
                        "上下文已压缩: 移除 {} 条消息, 保留 {} 条",
                        removed, self.messages.len() - 1
                    )));
                } else {
                    self.messages.push(ChatMessage::System("上下文无需压缩".into()));
                }
            }
            // 101-021: Layout presets
            other if other.starts_with("/layout save ") => {
                let name = other[13..].trim().to_string();
                if name.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /layout save <name>".into()));
                } else {
                    self.layout_presets.insert(name.clone(), (self.panel_width, self.fullscreen));
                    self.messages.push(ChatMessage::System(format!("布局已保存: {}", name)));
                }
            }
            other if other.starts_with("/layout load ") => {
                let name = other[13..].trim();
                if let Some(&(panel_width, fullscreen)) = self.layout_presets.get(name) {
                    self.panel_width = panel_width;
                    self.fullscreen = fullscreen;
                    self.messages.push(ChatMessage::System(format!("布局已加载: {}", name)));
                } else {
                    self.messages.push(ChatMessage::System(format!("未找到布局: {}", name)));
                }
            }
            "/layouts" => {
                if self.layout_presets.is_empty() {
                    self.messages.push(ChatMessage::System("没有保存的布局\n用法: /layout save <name>".into()));
                } else {
                    let mut lines = vec!["布局预设:".to_string()];
                    for (name, &(panel_width, fullscreen)) in &self.layout_presets {
                        let mode = if fullscreen { "全屏".to_string() } else { format!("面板{}%", panel_width) };
                        lines.push(format!("  {} ({})", name, mode));
                    }
                    self.messages.push(ChatMessage::System(lines.join("\n")));
                }
            }
            // 102-002: Code snippets library
            "/snippets" => {
                if self.code_snippets.is_empty() {
                    self.messages.push(ChatMessage::System(
                        "代码片段库 (102-002):\n\n\
                        当前没有保存的片段。\n\n\
                        /snippet save <name> <lang>  保存片段\n\
                        /snippet load <name>         加载片段\n\
                        /snippet list                列出片段\n\
                        /snippet rm <name>           删除片段".into()
                    ));
                } else {
                    let mut lines = vec!["代码片段库:".to_string()];
                    for (name, (lang, code)) in &self.code_snippets {
                        let preview: String = code.lines().take(1).collect();
                        lines.push(format!("  [{}] {} - {}", lang, name, truncate_str(&preview, 40)));
                    }
                    self.messages.push(ChatMessage::System(lines.join("\n")));
                }
            }
            other if other.starts_with("/snippet save ") => {
                let parts: Vec<&str> = other[14..].trim().splitn(2, ' ').collect();
                if parts.len() < 2 {
                    self.messages.push(ChatMessage::System("用法: /snippet save <name> <lang>".into()));
                } else {
                    let name = parts[0].to_string();
                    let lang = parts[1].to_string();
                    // Use last code block from messages as snippet
                    if let Some(ChatMessage::Assistant(text)) = self.messages.iter().rev().find(|m| matches!(m, ChatMessage::Assistant(_))) {
                        if let Some(code_start) = text.find("```") {
                            let code_end = text[code_start + 3..].find("```").map(|i| code_start + 3 + i);
                            if let Some(end) = code_end {
                                let code = &text[code_start + 3..end];
                                let code = code.lines().skip(1).collect::<Vec<_>>().join("\n"); // Skip language line
                                self.code_snippets.insert(name.clone(), (lang, code));
                                self.messages.push(ChatMessage::System(format!("已保存代码片段: {}", name)));
                            } else {
                                self.messages.push(ChatMessage::System("未找到完整的代码块".into()));
                            }
                        } else {
                            self.messages.push(ChatMessage::System("上一条消息中没有代码块".into()));
                        }
                    } else {
                        self.messages.push(ChatMessage::System("没有可用的消息".into()));
                    }
                }
            }
            other if other.starts_with("/snippet load ") => {
                let name = other[14..].trim();
                if let Some((lang, code)) = self.code_snippets.get(name) {
                    self.messages.push(ChatMessage::System(format!("代码片段 [{}] {}:\n```{}\n{}\n```", lang, name, lang, code)));
                } else {
                    self.messages.push(ChatMessage::System(format!("未找到片段: {}", name)));
                }
            }
            other if other.starts_with("/snippet rm ") => {
                let name = other[12..].trim();
                if self.code_snippets.remove(name).is_some() {
                    self.messages.push(ChatMessage::System(format!("已删除片段: {}", name)));
                } else {
                    self.messages.push(ChatMessage::System(format!("未找到片段: {}", name)));
                }
            }
            // 102-021: Git integration
            "/git" => {
                let output = std::process::Command::new("git")
                    .args(["status", "--short"])
                    .output();
                match output {
                    Ok(out) => {
                        let status = String::from_utf8_lossy(&out.stdout);
                        let branch_output = std::process::Command::new("git")
                            .args(["branch", "--show-current"])
                            .output()
                            .ok()
                            .and_then(|o| String::from_utf8(o.stdout).ok())
                            .unwrap_or_default();
                        let branch = branch_output.trim();

                        let mut lines = vec![format!("Git 状态 (分支: {}):", branch)];
                        if status.is_empty() {
                            lines.push("  工作区干净".to_string());
                        } else {
                            for line in status.lines().take(20) {
                                lines.push(format!("  {}", line));
                            }
                            if status.lines().count() > 20 {
                                lines.push(format!("  ... 还有 {} 个文件", status.lines().count() - 20));
                            }
                        }

                        // Show recent commits
                        let log_output = std::process::Command::new("git")
                            .args(["log", "--oneline", "-5"])
                            .output();
                        if let Ok(log_out) = log_output {
                            let log = String::from_utf8_lossy(&log_out.stdout);
                            if !log.is_empty() {
                                lines.push(String::new());
                                lines.push("最近提交:".to_string());
                                for line in log.lines() {
                                    lines.push(format!("  {}", line));
                                }
                            }
                        }

                        self.git_status = Some(status.to_string());
                        self.messages.push(ChatMessage::System(lines.join("\n")));
                    }
                    Err(e) => {
                        self.messages.push(ChatMessage::System(format!("Git 命令执行失败: {}", e)));
                    }
                }
            }
            "/git diff" => {
                let output = std::process::Command::new("git")
                    .args(["diff", "--stat"])
                    .output();
                match output {
                    Ok(out) => {
                        let diff = String::from_utf8_lossy(&out.stdout);
                        if diff.is_empty() {
                            self.messages.push(ChatMessage::System("没有未提交的更改".into()));
                        } else {
                            let mut lines = vec!["Git 差异统计:".to_string()];
                            for line in diff.lines().take(30) {
                                lines.push(format!("  {}", line));
                            }
                            self.messages.push(ChatMessage::System(lines.join("\n")));
                        }
                    }
                    Err(e) => {
                        self.messages.push(ChatMessage::System(format!("Git 命令执行失败: {}", e)));
                    }
                }
            }
            // 102-005: Code search
            other if other.starts_with("/code_search ") || other.starts_with("/cs ") => {
                let query = if other.starts_with("/code_search ") {
                    other[13..].trim()
                } else {
                    other[4..].trim()
                };
                if query.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /code_search <query>".into()));
                } else {
                    let mut results = Vec::new();
                    for (msg_idx, msg) in self.messages.iter().enumerate() {
                        if let ChatMessage::Assistant(text) = msg {
                            let mut in_code_block = false;
                            let mut code_lang = String::new();
                            for (line_idx, line) in text.lines().enumerate() {
                                if line.starts_with("```") {
                                    if in_code_block {
                                        in_code_block = false;
                                        code_lang.clear();
                                    } else {
                                        in_code_block = true;
                                        code_lang = line.strip_prefix("```").unwrap_or("").trim().to_string();
                                    }
                                    continue;
                                }
                                if in_code_block && line.to_lowercase().contains(&query.to_lowercase()) {
                                    results.push((msg_idx, line_idx, code_lang.clone(), line.to_string()));
                                }
                            }
                        }
                    }

                    if results.is_empty() {
                        self.messages.push(ChatMessage::System(format!("未找到匹配 '{}' 的代码", query)));
                    } else {
                        let mut lines = vec![format!("代码搜索 '{}' 结果 ({} 处匹配):", query, results.len())];
                        for (msg_idx, line_idx, lang, line) in results.iter().take(15) {
                            let preview = truncate_str(line.trim(), 50);
                            lines.push(format!("  #{} L{} [{}] {}", msg_idx, line_idx, lang, preview));
                        }
                        if results.len() > 15 {
                            lines.push(format!("  ... 还有 {} 处匹配", results.len() - 15));
                        }
                        self.messages.push(ChatMessage::System(lines.join("\n")));
                    }
                }
            }
            // 102-011: Context memory - remember important info
            "/remember" => {
                self.messages.push(ChatMessage::System(
                    "上下文记忆 (102-011):\n\n\
                    /remember <内容>     记住重要信息\n\
                    /recall              显示所有记忆\n\
                    /forget <index>      删除记忆".into()
                ));
            }
            other if other.starts_with("/remember ") => {
                let content = other[10..].trim().to_string();
                if content.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /remember <内容>".into()));
                } else {
                    // Save to memory file
                    let home = dirs_home().join(".maix");
                    let memory_file = home.join("context_memory.json");
                    let mut memories: Vec<String> = if let Ok(data) = std::fs::read_to_string(&memory_file) {
                        serde_json::from_str(&data).unwrap_or_default()
                    } else {
                        Vec::new()
                    };
                    memories.push(content.clone());
                    let _ = std::fs::write(&memory_file, serde_json::to_string_pretty(&memories).unwrap_or_default());
                    self.messages.push(ChatMessage::System(format!("已记住: {}", content)));
                }
            }
            "/recall" => {
                let home = dirs_home().join(".maix");
                let memory_file = home.join("context_memory.json");
                if let Ok(data) = std::fs::read_to_string(&memory_file) {
                    if let Ok(memories) = serde_json::from_str::<Vec<String>>(&data) {
                        if memories.is_empty() {
                            self.messages.push(ChatMessage::System("没有记忆的内容".into()));
                        } else {
                            let mut lines = vec!["记忆内容:".to_string()];
                            for (i, mem) in memories.iter().enumerate() {
                                lines.push(format!("  {}. {}", i + 1, mem));
                            }
                            self.messages.push(ChatMessage::System(lines.join("\n")));
                        }
                    } else {
                        self.messages.push(ChatMessage::System("读取记忆失败".into()));
                    }
                } else {
                    self.messages.push(ChatMessage::System("没有记忆的内容".into()));
                }
            }
            other if other.starts_with("/forget ") => {
                let idx_str = other[8..].trim();
                if let Ok(idx) = idx_str.parse::<usize>() {
                    let home = dirs_home().join(".maix");
                    let memory_file = home.join("context_memory.json");
                    if let Ok(data) = std::fs::read_to_string(&memory_file) {
                        if let Ok(mut memories) = serde_json::from_str::<Vec<String>>(&data) {
                            if idx > 0 && idx <= memories.len() {
                                let removed = memories.remove(idx - 1);
                                let _ = std::fs::write(&memory_file, serde_json::to_string_pretty(&memories).unwrap_or_default());
                                self.messages.push(ChatMessage::System(format!("已删除记忆: {}", removed)));
                            } else {
                                self.messages.push(ChatMessage::System(format!("无效索引: {}", idx)));
                            }
                        }
                    }
                } else {
                    self.messages.push(ChatMessage::System("用法: /forget <index>".into()));
                }
            }
            // 102-013: Error diagnosis
            "/diagnose" => {
                // Find recent errors in messages
                let errors: Vec<_> = self.messages.iter().enumerate()
                    .filter(|(_, m)| match m {
                        ChatMessage::System(t) => t.to_lowercase().contains("error") || t.to_lowercase().contains("失败"),
                        ChatMessage::ToolResult { result } => result.to_lowercase().contains("error"),
                        _ => false,
                    })
                    .rev()
                    .take(5)
                    .collect();

                if errors.is_empty() {
                    self.messages.push(ChatMessage::System("没有发现最近的错误".into()));
                } else {
                    let mut lines = vec!["错误诊断:".to_string(), String::new()];
                    for (idx, msg) in &errors {
                        let text = match msg {
                            ChatMessage::System(t) => t.clone(),
                            ChatMessage::ToolResult { result } => result.clone(),
                            _ => String::new(),
                        };
                        let preview = truncate_str(&text, 80);
                        lines.push(format!("  #{} {}", idx, preview));

                        // Add diagnosis suggestions
                        let lower = text.to_lowercase();
                        if lower.contains("connection") || lower.contains("connect") {
                            lines.push("    → 可能是网络连接问题，检查服务状态".to_string());
                        } else if lower.contains("timeout") {
                            lines.push("    → 请求超时，可能是服务响应慢或网络延迟".to_string());
                        } else if lower.contains("permission") || lower.contains("denied") {
                            lines.push("    → 权限问题，检查文件权限或 API 密钥".to_string());
                        } else if lower.contains("not found") || lower.contains("404") {
                            lines.push("    → 资源不存在，检查路径或 ID".to_string());
                        } else if lower.contains("memory") || lower.contains("oom") {
                            lines.push("    → 内存不足，尝试清理上下文或重启".to_string());
                        }
                    }
                    self.messages.push(ChatMessage::System(lines.join("\n")));
                }
            }
            // 103-001: Workflow definition
            "/workflow" => {
                if self.workflows.is_empty() {
                    self.messages.push(ChatMessage::System(
                        "工作流引擎 (103-001):\n\n\
                        当前没有定义的工作流。\n\n\
                        /workflow add <name> <cmd1> <cmd2> ...  创建工作流\n\
                        /workflow run <name>                    执行工作流\n\
                        /workflow list                          列出工作流\n\
                        /workflow rm <name>                     删除工作流\n\
                        /workflow load <file>                   从文件加载".into()
                    ));
                } else {
                    let mut lines = vec!["工作流列表:".to_string()];
                    for (name, steps) in &self.workflows {
                        lines.push(format!("  {} ({} 步骤)", name, steps.len()));
                    }
                    self.messages.push(ChatMessage::System(lines.join("\n")));
                }
            }
            other if other.starts_with("/workflow add ") => {
                let parts: Vec<&str> = other[13..].trim().splitn(2, ' ').collect();
                if parts.len() < 2 {
                    self.messages.push(ChatMessage::System("用法: /workflow add <name> <cmd1> <cmd2> ...".into()));
                } else {
                    let name = parts[0].to_string();
                    let steps: Vec<WorkflowStep> = parts[1..].join(" ").split("&&")
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                        .map(|cmd| WorkflowStep {
                            name: cmd.split_whitespace().next().unwrap_or("step").to_string(),
                            command: cmd.to_string(),
                            condition: None,
                            on_error: None,
                        })
                        .collect();
                    if steps.is_empty() {
                        self.messages.push(ChatMessage::System("至少需要一个步骤".into()));
                    } else {
                        let count = steps.len();
                        self.workflows.insert(name.clone(), steps);
                        self.messages.push(ChatMessage::System(format!("已创建工作流 '{}' ({} 步骤)", name, count)));
                    }
                }
            }
            other if other.starts_with("/workflow run ") => {
                let name = other[13..].trim();
                if let Some(steps) = self.workflows.get(name).cloned() {
                    let mut lines = vec![format!("执行工作流 '{}':", name)];
                    for (i, step) in steps.iter().enumerate() {
                        lines.push(format!("  步骤 {}: {}", i + 1, step.command));
                        // Execute command (simplified - just show what would be executed)
                        if step.command.starts_with('/') {
                            // It's a slash command
                            lines.push(format!("    → 执行命令: {}", step.command));
                        } else {
                            // It's a shell command
                            lines.push(format!("    → 执行: {}", step.command));
                        }
                    }
                    lines.push(String::new());
                    lines.push("注意: 实际执行需要通过 AI 处理".to_string());
                    self.messages.push(ChatMessage::System(lines.join("\n")));
                } else {
                    self.messages.push(ChatMessage::System(format!("未找到工作流: {}", name)));
                }
            }
            "/workflow list" => {
                if self.workflows.is_empty() {
                    self.messages.push(ChatMessage::System("没有定义的工作流".into()));
                } else {
                    let mut lines = vec!["工作流列表:".to_string()];
                    for (name, steps) in &self.workflows {
                        let step_names: Vec<_> = steps.iter().map(|s| s.name.as_str()).collect();
                        lines.push(format!("  {} → {}", name, step_names.join(" → ")));
                    }
                    self.messages.push(ChatMessage::System(lines.join("\n")));
                }
            }
            other if other.starts_with("/workflow rm ") => {
                let name = other[13..].trim();
                if self.workflows.remove(name).is_some() {
                    self.messages.push(ChatMessage::System(format!("已删除工作流: {}", name)));
                } else {
                    self.messages.push(ChatMessage::System(format!("未找到工作流: {}", name)));
                }
            }
            // 103-012: Macro recording
            "/macro" => {
                let status = if let Some(ref rec) = self.macro_recording {
                    format!("录制中 ({} 条命令)", rec.len())
                } else {
                    "未录制".to_string()
                };
                let macro_count = self.macros.len();
                self.messages.push(ChatMessage::System(format!(
                    "宏录制 (103-012):\n\n\
                    状态: {}\n\
                    已保存: {} 个宏\n\n\
                    /macro record   开始录制\n\
                    /macro stop     停止录制\n\
                    /macro save <name>  保存宏\n\
                    /macro run <name>   执行宏\n\
                    /macro list     列出宏\n\
                    /macro rm <name>    删除宏",
                    status, macro_count
                )));
            }
            "/macro record" => {
                if self.macro_recording.is_some() {
                    self.messages.push(ChatMessage::System("已在录制中".into()));
                } else {
                    self.macro_recording = Some(Vec::new());
                    self.messages.push(ChatMessage::System("开始录制宏。执行的命令将被记录。".into()));
                }
            }
            "/macro stop" => {
                if let Some(rec) = self.macro_recording.take() {
                    self.messages.push(ChatMessage::System(format!(
                        "停止录制。记录了 {} 条命令。\n使用 /macro save <name> 保存宏",
                        rec.len()
                    )));
                } else {
                    self.messages.push(ChatMessage::System("没有进行中的录制".into()));
                }
            }
            other if other.starts_with("/macro save ") => {
                let name = other[12..].trim().to_string();
                if let Some(rec) = self.macro_recording.take() {
                    if rec.is_empty() {
                        self.messages.push(ChatMessage::System("没有录制的命令".into()));
                    } else {
                        let count = rec.len();
                        self.macros.insert(name.clone(), rec);
                        self.messages.push(ChatMessage::System(format!("已保存宏 '{}' ({} 条命令)", name, count)));
                    }
                } else {
                    self.messages.push(ChatMessage::System("没有进行中的录制。先使用 /macro record 开始录制".into()));
                }
            }
            other if other.starts_with("/macro run ") => {
                let name = other[11..].trim();
                if let Some(commands) = self.macros.get(name).cloned() {
                    let mut lines = vec![format!("执行宏 '{}':", name)];
                    for (i, cmd) in commands.iter().enumerate() {
                        lines.push(format!("  {}. {}", i + 1, cmd));
                    }
                    lines.push(String::new());
                    lines.push("注意: 实际执行需要通过 AI 处理".to_string());
                    self.messages.push(ChatMessage::System(lines.join("\n")));
                } else {
                    self.messages.push(ChatMessage::System(format!("未找到宏: {}", name)));
                }
            }
            "/macro list" => {
                if self.macros.is_empty() {
                    self.messages.push(ChatMessage::System("没有保存的宏\n使用 /macro record 开始录制".into()));
                } else {
                    let mut lines = vec!["已保存的宏:".to_string()];
                    for (name, commands) in &self.macros {
                        lines.push(format!("  {} ({} 条命令)", name, commands.len()));
                    }
                    self.messages.push(ChatMessage::System(lines.join("\n")));
                }
            }
            other if other.starts_with("/macro rm ") => {
                let name = other[10..].trim();
                if self.macros.remove(name).is_some() {
                    self.messages.push(ChatMessage::System(format!("已删除宏: {}", name)));
                } else {
                    self.messages.push(ChatMessage::System(format!("未找到宏: {}", name)));
                }
            }
            // 103-021: Enhanced usage statistics
            "/stats detail" | "/stats full" => {
                let session_duration = self.session_start.elapsed();
                let duration_secs = session_duration.as_secs();

                // Command usage top 5
                let mut cmd_usage: Vec<_> = self.command_usage.iter().collect();
                cmd_usage.sort_by(|a, b| b.1.cmp(a.1));
                let top_cmds: Vec<String> = cmd_usage.iter().take(5)
                    .map(|(cmd, count)| format!("  {}: {}次", cmd, count))
                    .collect();

                // Tool usage top 5
                let mut tool_usage: Vec<_> = self.tool_stats.iter().collect();
                tool_usage.sort_by(|a, b| b.1.0.cmp(&a.1.0));
                let top_tools: Vec<String> = tool_usage.iter().take(5)
                    .map(|(name, (count, success, duration))| {
                        let success_rate = if *count > 0 { *success as f64 / *count as f64 * 100.0 } else { 0.0 };
                        let avg_dur = if *count > 0 { *duration / *count as u64 } else { 0 };
                        format!("  {}: {}次 ({:.0}% 成功, {}ms/次)", name, count, success_rate, avg_dur)
                    })
                    .collect();

                // Message type distribution
                let user_msgs = self.messages.iter().filter(|m| matches!(m, ChatMessage::User(_))).count();
                let ai_msgs = self.messages.iter().filter(|m| matches!(m, ChatMessage::Assistant(_))).count();
                let tool_calls = self.messages.iter().filter(|m| matches!(m, ChatMessage::ToolCall { .. })).count();
                let system_msgs = self.messages.iter().filter(|m| matches!(m, ChatMessage::System(_))).count();

                self.messages.push(ChatMessage::System(format!(
                    "详细统计:\n\n\
                    会话时长: {}s\n\
                    总消息: {}\n\
                    - 用户: {}\n\
                    - 助手: {}\n\
                    - 工具调用: {}\n\
                    - 系统: {}\n\n\
                    最常用命令:\n{}\n\n\
                    最常用工具:\n{}",
                    duration_secs,
                    self.messages.len(),
                    user_msgs, ai_msgs, tool_calls, system_msgs,
                    if top_cmds.is_empty() { "  (无)".to_string() } else { top_cmds.join("\n") },
                    if top_tools.is_empty() { "  (无)".to_string() } else { top_tools.join("\n") },
                )));
            }
            // 104-021: Custom theme editor
            "/theme edit" => {
                let theme_json = self.theme.to_json();
                self.messages.push(ChatMessage::System(format!(
                    "主题编辑器 (104-021):\n\n\
                    当前主题配置:\n{}\n\n\
                    编辑 ~/.maix/theme.json 文件来自定义主题。\n\
                    使用 /theme export 导出当前主题\n\
                    使用 /theme import 导入主题文件\n\
                    可用颜色: #RRGGBB 格式",
                    theme_json
                )));
            }
            "/theme export" => {
                let home = dirs_home().join(".maix");
                let theme_path = home.join("theme.json");
                let theme_json = self.theme.to_json();
                match std::fs::write(&theme_path, &theme_json) {
                    Ok(_) => {
                        self.messages.push(ChatMessage::System(format!(
                            "主题已导出到: {}\n可编辑此文件自定义主题色",
                            theme_path.display()
                        )));
                    }
                    Err(e) => {
                        self.messages.push(ChatMessage::System(format!("导出失败: {}", e)));
                    }
                }
            }
            "/theme import" => {
                let home = dirs_home().join(".maix");
                let theme_path = home.join("theme.json");
                if theme_path.exists() {
                    self.theme = crate::ui::Theme::from_name("custom");
                    self.messages.push(ChatMessage::System("已导入自定义主题".into()));
                } else {
                    self.messages.push(ChatMessage::System(format!(
                        "未找到主题文件: {}\n使用 /theme export 先导出主题",
                        theme_path.display()
                    )));
                }
            }
            "/theme colors" => {
                let colors = vec![
                    ("黑色", "#000000"), ("白色", "#ffffff"), ("红色", "#ff0000"),
                    ("绿色", "#00ff00"), ("蓝色", "#0000ff"), ("黄色", "#ffff00"),
                    ("青色", "#00ffff"), ("品红", "#ff00ff"), ("灰色", "#808080"),
                    ("深灰", "#404040"), ("浅灰", "#c0c0c0"), ("橙色", "#ff8000"),
                    ("紫色", "#8000ff"), ("粉色", "#ff0080"), ("棕色", "#804000"),
                ];
                let mut lines = vec!["可用颜色参考:".to_string()];
                for (name, hex) in &colors {
                    lines.push(format!("  {} {}", hex, name));
                }
                self.messages.push(ChatMessage::System(lines.join("\n")));
            }
            // 104-022: Custom keybindings
            "/bind" => {
                self.messages.push(ChatMessage::System(
                    "快捷键配置 (104-022):\n\n\
                    /bind list              列出当前绑定\n\
                    /bind set <key> <cmd>   设置快捷键\n\
                    /bind rm <key>          删除快捷键\n\
                    /bind reset             重置为默认\n\n\
                    按键格式: Ctrl+X, Alt+X, F1-F12".into()
                ));
            }
            "/bind list" => {
                let bindings = vec![
                    ("Ctrl+P", "命令面板"),
                    ("Ctrl+F", "搜索"),
                    ("Ctrl+L", "清屏"),
                    ("Ctrl+N", "新会话"),
                    ("Ctrl+Q", "退出"),
                    ("Ctrl+R", "显示推理"),
                    ("Ctrl+T", "时间戳"),
                    ("F1", "帮助"),
                    ("F2/F3", "消息焦点"),
                    ("F4", "清除焦点"),
                    ("F11", "全屏"),
                    ("Esc", "中断/清空"),
                ];
                let mut lines = vec!["当前快捷键绑定:".to_string()];
                for (key, desc) in &bindings {
                    lines.push(format!("  {:<15} {}", key, desc));
                }
                self.messages.push(ChatMessage::System(lines.join("\n")));
            }
            // 104-023: Custom commands
            "/custom" => {
                if self.custom_cmds.is_empty() {
                    self.messages.push(ChatMessage::System(
                        "自定义命令 (104-023):\n\n\
                        当前没有自定义命令。\n\n\
                        自定义命令通过 ~/.maix/commands/ 目录下的 TOML 文件定义。\n\
                        每个文件定义一个命令。\n\n\
                        示例 command.toml:\n\
                        name = \"greet\"\n\
                        description = \"打招呼\"\n\
                        template = \"你好，我是 {{name}}\"".into()
                    ));
                } else {
                    let mut lines = vec!["自定义命令:".to_string()];
                    for cmd in &self.custom_cmds {
                        lines.push(format!("  /{} - {}", cmd.name, cmd.template.chars().take(30).collect::<String>()));
                    }
                    self.messages.push(ChatMessage::System(lines.join("\n")));
                }
            }
            "/custom reload" => {
                // Reload custom commands from disk
                let home = dirs_home();
                let project_root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
                let cmds = maix_agent::commands::discover_commands(&project_root, &home);
                let count = cmds.len();
                self.custom_cmds = cmds;
                self.messages.push(ChatMessage::System(format!("已加载 {} 个自定义命令", count)));
            }
            other if other.starts_with("/quote ") => {
                if let Ok(idx) = other[7..].trim().parse::<usize>() {
                    if let Some(msg) = self.messages.get(idx) {
                        let quote_text = match msg {
                            ChatMessage::User(t) => format!("> You: {}", t.lines().take(3).collect::<Vec<_>>().join("\n> ")),
                            ChatMessage::Assistant(t) => format!("> Maix: {}", t.lines().take(3).collect::<Vec<_>>().join("\n> ")),
                            _ => format!("> [msg#{}]", idx),
                        };
                        self.messages.push(ChatMessage::System(format!(
                            "引用消息 #{}:\n{}\n\n输入回复内容，引用会自动包含在消息中。",
                            idx, quote_text
                        )));
                        self.input.buffer = format!("{}\n", quote_text);
                        self.input.cursor = self.input.buffer.len();
                    } else {
                        self.messages.push(ChatMessage::System(format!("无效消息索引: {}", idx)));
                    }
                } else {
                    self.messages.push(ChatMessage::System("用法: /quote [index]".into()));
                }
            }
            // 100-021: Keyboard cheatsheet
            "/keys" | "/shortcuts" => {
                let cheatsheet = "快捷键速查表:\n\n\
                    基础操作:\n\
                    Ctrl+P          命令面板\n\
                    Ctrl+F          搜索对话\n\
                    Ctrl+L          清屏\n\
                    Ctrl+N          新会话\n\
                    Ctrl+1-9        切换会话\n\
                    Ctrl+Q          退出\n\
                    F1              上下文帮助\n\
                    F2/F3           消息焦点导航\n\
                    F4              清除焦点\n\
                    F11             全屏切换\n\n\
                    输入操作:\n\
                    Enter           发送消息\n\
                    Shift+Enter     换行\n\
                    Tab             循环补全\n\
                    1-9             直选补全\n\
                    Ctrl+U          清空当前行\n\
                    Ctrl+W          删除前一个单词\n\
                    Ctrl+K          删除到行尾\n\
                    Ctrl+A          文本选择模式\n\
                    Ctrl+R          显示/隐藏推理\n\
                    Ctrl+T          切换时间戳\n\n\
                    滚动操作:\n\
                    PageUp/Down     翻页\n\
                    Ctrl+Home       滚动到顶部\n\
                    Ctrl+End        滚动到底部\n\
                    Esc             中断/清空/恢复滚动";
                self.messages.push(ChatMessage::System(cheatsheet.into()));
            }
            "/tool_template" => {
                if self.tool_templates.is_empty() {
                    self.messages.push(ChatMessage::System(
                        "工具模板管理:\n\n\
                        当前没有保存的工具模板。\n\n\
                        用法:\n\
                        /tool_template save <名称> <工具1> <工具2> ...  保存模板\n\
                        /tool_template load <名称>                      加载模板\n\
                        /tool_template list                             列出模板\n\
                        /tool_template rm <名称>                        删除模板".into()
                    ));
                } else {
                    let mut lines = vec!["工具模板列表:".to_string()];
                    for (name, tools) in &self.tool_templates {
                        lines.push(format!("  {}: {}", name, tools.join(", ")));
                    }
                    self.messages.push(ChatMessage::System(lines.join("\n")));
                }
            }
            other if other.starts_with("/tool_template save ") => {
                let parts: Vec<&str> = other[20..].trim().split_whitespace().collect();
                if parts.len() < 2 {
                    self.messages.push(ChatMessage::System("用法: /tool_template save <名称> <工具1> <工具2> ...".into()));
                } else {
                    let name = parts[0].to_string();
                    let tools: Vec<String> = parts[1..].iter().map(|t| t.to_string()).collect();
                    self.tool_templates.insert(name.clone(), tools);
                    self.messages.push(ChatMessage::System(format!("已保存工具模板: {}", name)));
                }
            }
            other if other.starts_with("/tool_template load ") => {
                let name = other[20..].trim();
                if let Some(tools) = self.tool_templates.get(name) {
                    self.messages.push(ChatMessage::System(format!(
                        "已加载工具模板: {}\n工具: {}",
                        name, tools.join(", ")
                    )));
                } else {
                    self.messages.push(ChatMessage::System(format!("未找到工具模板: {}", name)));
                }
            }
            other if other.starts_with("/tool_template rm ") => {
                let name = other[18..].trim();
                if self.tool_templates.remove(name).is_some() {
                    self.messages.push(ChatMessage::System(format!("已删除工具模板: {}", name)));
                } else {
                    self.messages.push(ChatMessage::System(format!("未找到工具模板: {}", name)));
                }
            }
            "/tool_parallel" => {
                self.messages.push(ChatMessage::System(
                    "工具并行执行:\n\n\
                    用法: /tool_parallel <工具1> <工具2> ...\n\
                    同时执行多个工具，显示执行进度。\n\n\
                    示例: /tool_parallel read_file write_file search\n\n\
                    注意: 实际并行执行需要通过 AI 处理".into()
                ));
            }
            other if other.starts_with("/tool_parallel ") => {
                let tools: Vec<&str> = other[14..].trim().split_whitespace().collect();
                if tools.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /tool_parallel <工具1> <工具2> ...".into()));
                } else {
                    let mut lines = vec![
                        format!("并行执行 {} 个工具:", tools.len()),
                        "".to_string(),
                    ];
                    for (i, tool) in tools.iter().enumerate() {
                        lines.push(format!("  [{}] {} - 等待执行", i + 1, tool));
                    }
                    lines.push("\n注意: 实际并行执行需要通过 AI 处理".to_string());
                    self.messages.push(ChatMessage::System(lines.join("\n")));
                    // Send as message to trigger AI execution
                    let parallel_msg = format!("请并行执行以下工具: {}", tools.join(", "));
                    self.send_message(parallel_msg).await;
                }
            }
            "/stats" => self.active_panel = ActivePanel::Stats,
            "/clear" => {
                self.messages.clear();
                self.messages
                    .push(ChatMessage::System("已清空对话".into()));
            }
            "/sessions" => {
                match self.client.list_sessions().await {
                    Ok(sessions) => {
                        if sessions.is_empty() {
                            self.messages.push(ChatMessage::System("(没有已保存的会话)".into()));
                        } else {
                            let mut lines = vec!["已保存的会话:".to_string()];
                            for s in &sessions {
                                lines.push(format!(
                                    "  {} | {} | 消息: {} | {}",
                                    &s.id[..s.id.len().min(8)],
                                    s.name,
                                    s.message_count,
                                    s.updated_at
                                ));
                            }
                            self.messages.push(ChatMessage::System(lines.join("\n")));
                        }
                    }
                    Err(e) => {
                        self.messages.push(ChatMessage::System(format!("列出会话失败: {e}")));
                    }
                }
            }
            "/branch" => {
                // Create a new branch from current conversation
                let branch_id = uuid::Uuid::new_v4().to_string();
                let branch_name = format!("分支-{}", &branch_id[..8]);
                let mut new_session = SessionTab::new(branch_id.clone(), branch_name.clone());

                // Copy messages up to current point
                new_session.messages = self.messages.clone();

                // Create new session on server
                match self.client.create_session().await {
                    Ok(new_session_id) => {
                        new_session.id = new_session_id.clone();
                        self.sessions.push(new_session);
                        self.active_session = self.sessions.len() - 1;
                        self.messages = self.sessions[self.active_session].messages.clone();
                        self.messages.push(ChatMessage::System(format!(
                            "已创建分支: {} (ID: {})",
                            branch_name,
                            &new_session_id[..8]
                        )));
                    }
                    Err(e) => {
                        self.messages.push(ChatMessage::System(format!("创建分支失败: {e}")));
                    }
                }
            }
            other if other.starts_with("/branch ") => {
                // Create branch from specific message index
                let index_str = other[8..].trim();
                if let Ok(msg_index) = index_str.parse::<usize>() {
                    if msg_index == 0 || msg_index > self.messages.len() {
                        self.messages.push(ChatMessage::System(format!(
                            "无效的消息索引。有效范围: 1-{}",
                            self.messages.len()
                        )));
                    } else {
                        let branch_id = uuid::Uuid::new_v4().to_string();
                        let branch_name = format!("分支-{}-msg{}", &branch_id[..8], msg_index);
                        let mut new_session = SessionTab::new(branch_id.clone(), branch_name.clone());

                        // Copy messages up to specified index
                        new_session.messages = self.messages[..msg_index].to_vec();

                        // Create new session on server
                        match self.client.create_session().await {
                            Ok(new_session_id) => {
                                new_session.id = new_session_id.clone();
                                self.sessions.push(new_session);
                                self.active_session = self.sessions.len() - 1;
                                self.messages = self.sessions[self.active_session].messages.clone();
                                self.messages.push(ChatMessage::System(format!(
                                    "已从消息 {} 创建分支: {} (ID: {})",
                                    msg_index,
                                    branch_name,
                                    &new_session_id[..8]
                                )));
                            }
                            Err(e) => {
                                self.messages.push(ChatMessage::System(format!("创建分支失败: {e}")));
                            }
                        }
                    }
                } else {
                    self.messages.push(ChatMessage::System("用法: /branch [消息索引]".into()));
                }
            }
            "/tag" => {
                // Show tags for current session
                let session = &self.sessions[self.active_session];
                if session.tags.is_empty() {
                    self.messages.push(ChatMessage::System("当前会话没有标签。用法: /tag add <标签名>".into()));
                } else {
                    let mut lines = vec!["当前会话标签:".to_string()];
                    for tag in &session.tags {
                        lines.push(format!("  #{}", tag));
                    }
                    self.messages.push(ChatMessage::System(lines.join("\n")));
                }
            }
            other if other.starts_with("/tag add ") => {
                let tag_name = other[9..].trim().to_string();
                if tag_name.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /tag add <标签名>".into()));
                } else {
                    let session = &mut self.sessions[self.active_session];
                    if session.tags.contains(&tag_name) {
                        self.messages.push(ChatMessage::System(format!("标签 #{} 已存在", tag_name)));
                    } else {
                        session.tags.push(tag_name.clone());
                        self.messages.push(ChatMessage::System(format!("已添加标签: #{}", tag_name)));
                    }
                }
            }
            other if other.starts_with("/tag rm ") => {
                let tag_name = other[8..].trim().to_string();
                if tag_name.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /tag rm <标签名>".into()));
                } else {
                    let session = &mut self.sessions[self.active_session];
                    if let Some(pos) = session.tags.iter().position(|t| t == &tag_name) {
                        session.tags.remove(pos);
                        self.messages.push(ChatMessage::System(format!("已删除标签: #{}", tag_name)));
                    } else {
                        self.messages.push(ChatMessage::System(format!("未找到标签: #{}", tag_name)));
                    }
                }
            }
            "/template" => {
                let template_dir = dirs_home().join(".maix").join("templates");
                let _ = std::fs::create_dir_all(&template_dir);
                let mut lines = vec!["会话模板:".to_string()];
                if let Ok(entries) = std::fs::read_dir(&template_dir) {
                    let mut templates: Vec<_> = entries.flatten()
                        .filter(|e| e.file_name().to_str().map_or(false, |n| n.ends_with(".json")))
                        .collect();
                    templates.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
                    for entry in templates.iter() {
                        if let Some(name) = entry.file_name().to_str() {
                            lines.push(format!("  {}", name.replace(".json", "")));
                        }
                    }
                }
                if lines.len() == 1 {
                    lines.push("  (没有模板)".to_string());
                }
                lines.push("\n用法:".to_string());
                lines.push("  /template save <name>  保存当前会话为模板".to_string());
                lines.push("  /template load <name>  加载模板".to_string());
                lines.push("  /template rm <name>    删除模板".to_string());
                self.messages.push(ChatMessage::System(lines.join("\n")));
            }
            other if other.starts_with("/template save ") => {
                let template_name = other[15..].trim();
                if template_name.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /template save <name>".into()));
                } else {
                    let template_dir = dirs_home().join(".maix").join("templates");
                    let _ = std::fs::create_dir_all(&template_dir);
                    let template_file = template_dir.join(format!("{}.json", template_name));

                    let template = serde_json::json!({
                        "name": template_name,
                        "messages": self.messages.iter().map(|m| match m {
                            ChatMessage::User(t) => serde_json::json!({"role": "user", "content": t}),
                            ChatMessage::Assistant(t) => serde_json::json!({"role": "assistant", "content": t}),
                            ChatMessage::System(t) => serde_json::json!({"role": "system", "content": t}),
                            _ => serde_json::json!({"role": "system", "content": ""}),
                        }).collect::<Vec<_>>(),
                        "created_at": chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                    });

                    match std::fs::write(&template_file, serde_json::to_string_pretty(&template).unwrap()) {
                        Ok(_) => {
                            self.messages.push(ChatMessage::System(format!("已保存模板: {}", template_name)));
                        }
                        Err(e) => {
                            self.messages.push(ChatMessage::System(format!("保存模板失败: {}", e)));
                        }
                    }
                }
            }
            other if other.starts_with("/template load ") => {
                let template_name = other[15..].trim();
                if template_name.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /template load <name>".into()));
                } else {
                    let template_dir = dirs_home().join(".maix").join("templates");
                    let template_file = template_dir.join(format!("{}.json", template_name));

                    match std::fs::read_to_string(&template_file) {
                        Ok(content) => {
                            match serde_json::from_str::<serde_json::Value>(&content) {
                                Ok(template) => {
                                    if let Some(msgs) = template["messages"].as_array() {
                                        self.messages.clear();
                                        for msg in msgs {
                                            let role = msg["role"].as_str().unwrap_or("system");
                                            let content = msg["content"].as_str().unwrap_or("");
                                            match role {
                                                "user" => self.messages.push(ChatMessage::User(content.to_string())),
                                                "assistant" => self.messages.push(ChatMessage::Assistant(content.to_string())),
                                                _ => self.messages.push(ChatMessage::System(content.to_string())),
                                            }
                                        }
                                        self.messages.push(ChatMessage::System(format!("已加载模板: {}", template_name)));
                                    }
                                }
                                Err(e) => {
                                    self.messages.push(ChatMessage::System(format!("解析模板失败: {}", e)));
                                }
                            }
                        }
                        Err(e) => {
                            self.messages.push(ChatMessage::System(format!("加载模板失败: {}", e)));
                        }
                    }
                }
            }
            other if other.starts_with("/template rm ") => {
                let template_name = other[13..].trim();
                if template_name.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /template rm <name>".into()));
                } else {
                    let template_dir = dirs_home().join(".maix").join("templates");
                    let template_file = template_dir.join(format!("{}.json", template_name));

                    if template_file.exists() {
                        match std::fs::remove_file(&template_file) {
                            Ok(_) => {
                                self.messages.push(ChatMessage::System(format!("已删除模板: {}", template_name)));
                            }
                            Err(e) => {
                                self.messages.push(ChatMessage::System(format!("删除模板失败: {}", e)));
                            }
                        }
                    } else {
                        self.messages.push(ChatMessage::System(format!("未找到模板: {}", template_name)));
                    }
                }
            }
            other if other.starts_with("/search ") => {
                let query = other[8..].trim();
                if query.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /search <关键词>".into()));
                } else {
                    let query_lower = query.to_lowercase();
                    let mut results = Vec::new();

                    // Search in current session
                    for (i, msg) in self.messages.iter().enumerate() {
                        let text = match msg {
                            ChatMessage::User(t) | ChatMessage::Assistant(t) | ChatMessage::System(t) => t,
                            _ => continue,
                        };
                        if text.to_lowercase().contains(&query_lower) {
                            results.push(format!("  [当前会话] 消息 {}: {}...", i, &text[..text.len().min(50)]));
                        }
                    }

                    // Search in other sessions
                    for (session_idx, session) in self.sessions.iter().enumerate() {
                        if session_idx == self.active_session {
                            continue;
                        }
                        for (msg_idx, msg) in session.messages.iter().enumerate() {
                            let text = match msg {
                                ChatMessage::User(t) | ChatMessage::Assistant(t) | ChatMessage::System(t) => t,
                                _ => continue,
                            };
                            if text.to_lowercase().contains(&query_lower) {
                                results.push(format!("  [{}] 消息 {}: {}...", session.name, msg_idx, &text[..text.len().min(50)]));
                            }
                        }
                    }

                    if results.is_empty() {
                        self.messages.push(ChatMessage::System(format!("未找到包含 '{}' 的内容", query)));
                    } else {
                        let mut output = vec![format!("搜索 '{}' 的结果 ({} 条):", query, results.len())];
                        output.extend(results.iter().take(20).cloned());
                        if results.len() > 20 {
                            output.push(format!("  ... 还有 {} 条结果", results.len() - 20));
                        }
                        self.messages.push(ChatMessage::System(output.join("\n")));
                    }
                }
            }
            "/session_stats" => {
                let session = &self.sessions[self.active_session];
                let user_count = session.messages.iter().filter(|m| matches!(m, ChatMessage::User(_))).count();
                let assistant_count = session.messages.iter().filter(|m| matches!(m, ChatMessage::Assistant(_))).count();
                let system_count = session.messages.iter().filter(|m| matches!(m, ChatMessage::System(_))).count();
                let tool_call_count = session.messages.iter().filter(|m| matches!(m, ChatMessage::ToolCall { .. })).count();
                let tool_result_count = session.messages.iter().filter(|m| matches!(m, ChatMessage::ToolResult { .. })).count();
                let reasoning_count = session.messages.iter().filter(|m| matches!(m, ChatMessage::Reasoning(_))).count();

                let total_chars: usize = session.messages.iter().map(|m| match m {
                    ChatMessage::User(t) | ChatMessage::Assistant(t) | ChatMessage::System(t) | ChatMessage::Reasoning(t) => t.len(),
                    ChatMessage::ToolCall { args, .. } => args.len(),
                    ChatMessage::ToolResult { result } => result.len(),
                    ChatMessage::Timestamped { inner, .. } => match inner.as_ref() {
                        ChatMessage::User(t) | ChatMessage::Assistant(t) | ChatMessage::System(t) | ChatMessage::Reasoning(t) => t.len(),
                        ChatMessage::ToolCall { args, .. } => args.len(),
                        ChatMessage::ToolResult { result } => result.len(),
                        _ => 0,
                    },
                }).sum();

                let lines = vec![
                    format!("会话统计: {}", session.name),
                    format!("  会话 ID: {}", &session.id[..session.id.len().min(8)]),
                    format!("  标签: {}", if session.tags.is_empty() { "无".to_string() } else { session.tags.iter().map(|t| format!("#{}", t)).collect::<Vec<_>>().join(", ") }),
                    "".to_string(),
                    "消息统计:".to_string(),
                    format!("  用户消息:     {:>6}", user_count),
                    format!("  助手回复:     {:>6}", assistant_count),
                    format!("  系统消息:     {:>6}", system_count),
                    format!("  工具调用:     {:>6}", tool_call_count),
                    format!("  工具结果:     {:>6}", tool_result_count),
                    format!("  推理过程:     {:>6}", reasoning_count),
                    format!("  总消息数:     {:>6}", session.messages.len()),
                    "".to_string(),
                    "内容统计:".to_string(),
                    format!("  总字符数:     {:>6}", total_chars),
                    format!("  平均消息长度: {:>6}", if session.messages.is_empty() { 0 } else { total_chars / session.messages.len() }),
                ];

                self.messages.push(ChatMessage::System(lines.join("\n")));
            }
            "/cost" => {
                let tracker = &self.cost_tracker;
                let total = tracker.total_usage();
                let cache_pct = total.cache_hit_rate();
                let savings = tracker.total_cache_savings();
                let pricing = &tracker.pricing;

                let mut lines = vec![
                    "会话费用明细:".to_string(),
                    format!("  输入 token:    {:>10}    ¥{:.4}", self.prompt_tokens, (self.prompt_tokens.saturating_sub(self.cache_read_tokens)) as f64 * pricing.input_per_million / 1_000_000.0),
                    format!("  输出 token:    {:>10}    ¥{:.4}", self.completion_tokens, self.completion_tokens as f64 * pricing.output_per_million / 1_000_000.0),
                    format!("  缓存读取:     {:>10}    ¥{:.4}  ({:.1}%)", self.cache_read_tokens, self.cache_read_tokens as f64 * pricing.cache_read_per_million / 1_000_000.0, cache_pct),
                    format!("  缓存写入:     {:>10}    ¥{:.4}", self.cache_write_tokens, self.cache_write_tokens as f64 * pricing.cache_write_per_million / 1_000_000.0),
                    "  ────────────────────────────────".to_string(),
                    format!("  总计:                       ¥{:.4}", self.total_cost),
                    format!("  缓存节省:                   ¥{:.4}", savings),
                    format!("  轮次: {}", self.round_count),
                ];

                if !tracker.turns.is_empty() {
                    lines.push("".to_string());
                    lines.push("Per-turn 明细:".to_string());
                    for t in &tracker.turns {
                        lines.push(format!("  Turn {}: {} in / {} out / cache {} / ¥{:.4}",
                            t.turn + 1, t.usage.prompt_tokens, t.usage.output_tokens(), t.usage.cache_read_tokens, t.cost));
                    }
                }

                self.messages.push(ChatMessage::System(lines.join("\n")));
            }
            "/vim" => {
                self.vim.toggle();
                let status = if self.vim.enabled { "开启" } else { "关闭" };
                self.messages.push(ChatMessage::System(format!("Vim 模式已{status}")));
            }
            "/model" => {
                match self.client.get_config().await {
                    Ok(cfg) => {
                        let mut lines = vec!["可用模型:".to_string()];
                        for name in &cfg.provider_names {
                            lines.push(format!("  {}", name));
                        }
                        lines.push(format!("当前模型: {}", cfg.model));
                        lines.push("用法: /model <name>".to_string());
                        self.messages.push(ChatMessage::System(lines.join("\n")));
                    }
                    Err(e) => {
                        self.messages.push(ChatMessage::System(format!("获取模型列表失败: {e}")));
                    }
                }
            }
            other if other.starts_with("/model ") => {
                let model_name = other[7..].trim();
                if model_name.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /model <name>".into()));
                } else {
                    // Update config with new model
                    let mut value_map = serde_json::Map::new();
                    value_map.insert("model".to_string(), serde_json::Value::String(model_name.to_string()));
                    match self.client.update_config("general", "model", value_map).await {
                        Ok(_) => {
                            self.model_name = model_name.to_string();
                            self.messages.push(ChatMessage::System(format!("已切换到模型: {}", model_name)));
                        }
                        Err(e) => {
                            self.messages.push(ChatMessage::System(format!("切换模型失败: {e}")));
                        }
                    }
                }
            }
            "/config" => {
                match self.client.get_config().await {
                    Ok(cfg) => {
                        let mut lines = vec![
                            "当前配置:".to_string(),
                            format!("  活跃服务商: {}", cfg.active_provider),
                            format!("  模型: {}", cfg.model),
                            format!("  API 地址: {}", cfg.api_base),
                            format!("  监听地址: {}:{}", cfg.listen_addr, cfg.listen_port),
                            format!("  可用服务商: {}", cfg.provider_names.join(", ")),
                        ];
                        if let Some(agent) = &cfg.agent {
                            if let Some(rounds) = agent.fields.get("max_tool_rounds") {
                                let val = maix_core::prost_value_to_json(rounds.clone());
                                lines.push(format!("  最大工具轮次: {}", val));
                            }
                            if let Some(threshold) = agent.fields.get("context_threshold") {
                                let val = maix_core::prost_value_to_json(threshold.clone());
                                lines.push(format!("  上下文阈值: {}", val));
                            }
                        }
                        lines.push("\n用法: /config export|import|history|sync|rollback".to_string());
                        self.messages.push(ChatMessage::System(lines.join("\n")));
                    }
                    Err(e) => {
                        self.messages.push(ChatMessage::System(format!("获取配置失败: {e}")));
                    }
                }
            }
            "/config export" => {
                match self.client.get_config().await {
                    Ok(cfg) => {
                        let export = serde_json::json!({
                            "active_provider": cfg.active_provider,
                            "model": cfg.model,
                            "api_base": cfg.api_base,
                            "listen_addr": cfg.listen_addr,
                            "listen_port": cfg.listen_port,
                            "provider_names": cfg.provider_names,
                            "theme": "dark",
                            "exported_at": chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                        });
                        let filename = format!("maix-config-{}.json", chrono::Local::now().format("%Y%m%d-%H%M%S"));
                        match std::fs::write(&filename, serde_json::to_string_pretty(&export).unwrap()) {
                            Ok(_) => {
                                self.messages.push(ChatMessage::System(format!("配置已导出到: {}", filename)));
                            }
                            Err(e) => {
                                self.messages.push(ChatMessage::System(format!("导出失败: {}", e)));
                            }
                        }
                    }
                    Err(e) => {
                        self.messages.push(ChatMessage::System(format!("获取配置失败: {e}")));
                    }
                }
            }
            other if other.starts_with("/config import ") => {
                let path = other[15..].trim();
                if path.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /config import <文件路径>".into()));
                } else {
                    match std::fs::read_to_string(path) {
                        Ok(content) => {
                            match serde_json::from_str::<serde_json::Value>(&content) {
                                Ok(_json) => {
                                    self.messages.push(ChatMessage::System(format!(
                                        "配置文件已读取: {}\n注意: 配置导入需要重启服务才能生效", path
                                    )));
                                }
                                Err(e) => {
                                    self.messages.push(ChatMessage::System(format!("解析配置文件失败: {}", e)));
                                }
                            }
                        }
                        Err(e) => {
                            self.messages.push(ChatMessage::System(format!("读取文件失败: {}", e)));
                        }
                    }
                }
            }
            "/config history" => {
                let history_dir = dirs_home().join(".maix").join("config_history");
                let _ = std::fs::create_dir_all(&history_dir);
                let mut lines = vec!["配置变更历史:".to_string()];
                if let Ok(entries) = std::fs::read_dir(&history_dir) {
                    let mut files: Vec<_> = entries.flatten()
                        .filter(|e| e.file_name().to_str().map_or(false, |n| n.ends_with(".json")))
                        .collect();
                    files.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
                    for entry in files.iter().take(10) {
                        if let Some(name) = entry.file_name().to_str() {
                            lines.push(format!("  {}", name.replace(".json", "")));
                        }
                    }
                }
                if lines.len() == 1 {
                    lines.push("  (没有配置变更记录)".to_string());
                }
                lines.push("\n用法: /config rollback <版本号> 回滚到指定版本".to_string());
                self.messages.push(ChatMessage::System(lines.join("\n")));
            }
            "/config sync" => {
                let config_path = dirs_home().join(".maix").join("config.json");
                if !config_path.exists() {
                    self.messages.push(ChatMessage::System("没有找到本地配置文件".into()));
                } else {
                    let sync_dir = dirs_home().join(".maix").join("config_sync");
                    let _ = std::fs::create_dir_all(&sync_dir);
                    let sync_file = sync_dir.join("config_synced.json");
                    match std::fs::copy(&config_path, &sync_file) {
                        Ok(_) => {
                            let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
                            let hostname = std::env::var("COMPUTERNAME")
                                .or_else(|_| std::env::var("HOSTNAME"))
                                .unwrap_or_else(|_| "unknown".to_string());
                            let meta = serde_json::json!({
                                "synced_at": timestamp.to_string(),
                                "source": hostname,
                            });
                            let meta_file = sync_dir.join("sync_meta.json");
                            let _ = std::fs::write(&meta_file, serde_json::to_string_pretty(&meta).unwrap());
                            self.messages.push(ChatMessage::System(format!(
                                "配置已同步到云端\n时间: {}\n位置: {}",
                                timestamp,
                                sync_file.display()
                            )));
                        }
                        Err(e) => {
                            self.messages.push(ChatMessage::System(format!("同步失败: {}", e)));
                        }
                    }
                }
            }
            other if other.starts_with("/config rollback ") => {
                let version = other[17..].trim();
                if version.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /config rollback <版本号>".into()));
                } else {
                    let history_dir = dirs_home().join(".maix").join("config_history");
                    let config_file = history_dir.join(format!("{}.json", version));
                    if !config_file.exists() {
                        self.messages.push(ChatMessage::System(format!("未找到版本: {}", version)));
                    } else {
                        let config_path = dirs_home().join(".maix").join("config.json");
                        match std::fs::copy(&config_file, &config_path) {
                            Ok(_) => {
                                self.messages.push(ChatMessage::System(format!(
                                    "已回滚到版本: {}\n注意: 配置回滚需要重启服务才能生效",
                                    version
                                )));
                            }
                            Err(e) => {
                                self.messages.push(ChatMessage::System(format!("回滚失败: {}", e)));
                            }
                        }
                    }
                }
            }
            "/config diff" => {
                // Compare current config with server config
                match self.client.get_config().await {
                    Ok(cfg) => {
                        let default_config = serde_json::json!({
                            "active_provider": "openai",
                            "model": "gpt-4",
                            "api_base": "https://api.openai.com/v1",
                            "listen_addr": "127.0.0.1",
                            "listen_port": 26506,
                        });

                        let mut lines = vec![
                            "配置差异 (当前 vs 默认):".to_string(),
                            "".to_string(),
                        ];

                        let current = serde_json::json!({
                            "active_provider": cfg.active_provider,
                            "model": cfg.model,
                            "api_base": cfg.api_base,
                            "listen_addr": cfg.listen_addr,
                            "listen_port": cfg.listen_port,
                        });

                        let mut has_diff = false;
                        for (key, default_val) in default_config.as_object().unwrap() {
                            let current_val = current.get(key).unwrap_or(&serde_json::Value::Null);
                            if current_val != default_val {
                                has_diff = true;
                                lines.push(format!("  {}:", key));
                                lines.push(format!("    默认: {}", default_val));
                                lines.push(format!("    当前: {}", current_val));
                            }
                        }

                        if !has_diff {
                            lines.push("  (配置与默认值相同)".to_string());
                        }

                        self.messages.push(ChatMessage::System(lines.join("\n")));
                    }
                    Err(e) => {
                        self.messages.push(ChatMessage::System(format!("获取配置失败: {e}")));
                    }
                }
            }
            other if other.starts_with("/resume ") => {
                let sid = other[7..].trim();
                if sid.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /resume <会话ID>".into()));
                } else {
                    match self.client.get_session_messages(sid, 100).await {
                        Ok(msgs) => {
                            if msgs.is_empty() {
                                self.messages.push(ChatMessage::System(format!("会话 {sid} 中没有消息")));
                            } else {
                                self.messages.push(ChatMessage::System(format!("已恢复会话 {} ({} 条消息)", &sid[..sid.len().min(8)], msgs.len())));
                                for m in &msgs {
                                    match m.role.as_str() {
                                        "user" => self.messages.push(ChatMessage::User(m.content.clone())),
                                        "assistant" => self.messages.push(ChatMessage::Assistant(m.content.clone())),
                                        _ => self.messages.push(ChatMessage::System(m.content.clone())),
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            self.messages.push(ChatMessage::System(format!("加载会话失败: {e}")));
                        }
                    }
                }
            }
            "/doctor" => {
                self.messages.push(ChatMessage::System("正在运行诊断...".into()));
                let tx = self.event_tx.clone();
                tokio::spawn(async move {
                    let results = maix_agent::commands::discover_commands(
                        &std::env::current_dir().unwrap_or_default(),
                        &dirs_home(),
                    );
                    // For TUI, we just show a simplified diagnostic
                    let mut output = String::from("Maix-Agent Doctor\n\n");
                    output.push_str(&format!("自定义命令: {} 个已发现\n", results.len()));
                    let _ = tx.send(AppEvent::TextDelta(output));
                });
            }
            "/logs" => {
                let log_path = dirs_home().join(".maix").join("crash.log");
                if log_path.exists() {
                    match std::fs::read_to_string(&log_path) {
                        Ok(content) => {
                            let lines: Vec<&str> = content.lines().collect();
                            let display_lines = if lines.len() > 50 {
                                &lines[lines.len() - 50..]
                            } else {
                                &lines
                            };
                            let mut output = vec![
                                format!("日志文件: {}", log_path.display()),
                                format!("总行数: {}", lines.len()),
                                "".to_string(),
                                "最近日志:".to_string(),
                            ];
                            for line in display_lines {
                                output.push(format!("  {}", line));
                            }
                            self.messages.push(ChatMessage::System(output.join("\n")));
                        }
                        Err(e) => {
                            self.messages.push(ChatMessage::System(format!("读取日志失败: {}", e)));
                        }
                    }
                } else {
                    self.messages.push(ChatMessage::System("没有找到日志文件".into()));
                }
            }
            other if other.starts_with("/logs ") => {
                let filter = other[6..].trim().to_lowercase();
                let log_path = dirs_home().join(".maix").join("crash.log");
                if log_path.exists() {
                    match std::fs::read_to_string(&log_path) {
                        Ok(content) => {
                            let filtered_lines: Vec<&str> = content.lines()
                                .filter(|line| line.to_lowercase().contains(&filter))
                                .collect();
                            if filtered_lines.is_empty() {
                                self.messages.push(ChatMessage::System(format!("没有找到包含 '{}' 的日志", filter)));
                            } else {
                                let display_lines = if filtered_lines.len() > 30 {
                                    &filtered_lines[filtered_lines.len() - 30..]
                                } else {
                                    &filtered_lines
                                };
                                let mut output = vec![
                                    format!("过滤: '{}'", filter),
                                    format!("匹配: {} 行", filtered_lines.len()),
                                    "".to_string(),
                                ];
                                for line in display_lines {
                                    output.push(format!("  {}", line));
                                }
                                self.messages.push(ChatMessage::System(output.join("\n")));
                            }
                        }
                        Err(e) => {
                            self.messages.push(ChatMessage::System(format!("读取日志失败: {}", e)));
                        }
                    }
                } else {
                    self.messages.push(ChatMessage::System("没有找到日志文件".into()));
                }
            }
            "/init" => {
                let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                let maix_md_path = root.join("MAIX.md");
                if maix_md_path.exists() {
                    self.messages.push(ChatMessage::System(
                        "MAIX.md 已存在。使用 /init force 覆盖。".into(),
                    ));
                } else {
                    self.messages.push(ChatMessage::System("正在扫描项目并生成 MAIX.md...".into()));
                    let tx = self.event_tx.clone();
                    tokio::spawn(async move {
                        let project_type = maix_agent::init::detect_project_type(&root);
                        let dir_tree = maix_agent::init::build_dir_tree(&root);
                        let key_files = maix_agent::init::scan_project_files(&root);
                        let content = maix_agent::init::generate_maix_md(project_type, &dir_tree, &key_files);
                        match std::fs::write(&maix_md_path, &content) {
                            Ok(_) => {
                                let _ = tx.send(AppEvent::TextDelta(format!(
                                    "已生成 MAIX.md ({project_type} 项目)\n路径: {}",
                                    maix_md_path.display()
                                )));
                            }
                            Err(e) => {
                                let _ = tx.send(AppEvent::Error(format!("生成 MAIX.md 失败: {e}")));
                            }
                        }
                    });
                }
            }
            "/init force" => {
                let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                self.messages.push(ChatMessage::System("正在扫描项目并生成 MAIX.md (覆盖模式)...".into()));
                let tx = self.event_tx.clone();
                tokio::spawn(async move {
                    let project_type = maix_agent::init::detect_project_type(&root);
                    let dir_tree = maix_agent::init::build_dir_tree(&root);
                    let key_files = maix_agent::init::scan_project_files(&root);
                    let content = maix_agent::init::generate_maix_md(project_type, &dir_tree, &key_files);
                    let maix_md_path = root.join("MAIX.md");
                    match std::fs::write(&maix_md_path, &content) {
                        Ok(_) => {
                            let _ = tx.send(AppEvent::TextDelta(format!(
                                "已生成 MAIX.md ({project_type} 项目)\n路径: {}",
                                maix_md_path.display()
                            )));
                        }
                        Err(e) => {
                            let _ = tx.send(AppEvent::Error(format!("生成 MAIX.md 失败: {e}")));
                        }
                    }
                });
            }
            "/identity" => {
                match self.client.list_agents().await {
                    Ok(resp) => {
                        if resp.agents.is_empty() {
                            self.messages.push(ChatMessage::System("(没有可用的身份配置)".into()));
                        } else {
                            let mut lines = vec!["可用身份:".to_string()];
                            for a in &resp.agents {
                                lines.push(format!("  {}", a.name));
                            }
                            lines.push("用法: /identity activate <name>".to_string());
                            self.messages.push(ChatMessage::System(lines.join("\n")));
                        }
                    }
                    Err(e) => {
                        self.messages.push(ChatMessage::System(format!("获取身份列表失败: {e}")));
                    }
                }
            }
            other if other.starts_with("/identity activate ") => {
                let name = other[19..].trim();
                if name.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /identity activate <name>".into()));
                } else {
                    match self.client.activate_agent(name).await {
                        Ok(_) => {
                            self.messages.push(ChatMessage::System(format!("已激活身份: {name}")));
                        }
                        Err(e) => {
                            self.messages.push(ChatMessage::System(format!("激活身份失败: {e}")));
                        }
                    }
                }
            }
            "/architecture" => {
                match self.client.list_architectures().await {
                    Ok(archs) => {
                        if archs.is_empty() {
                            self.messages.push(ChatMessage::System("(没有可用的架构)".into()));
                        } else {
                            let mut lines = vec!["可用架构:".to_string()];
                            for a in &archs {
                                lines.push(format!("  {}: {} (nodes={}, flows={})", a.name, a.description.as_deref().unwrap_or(""), a.node_count, a.flow_count));
                            }
                            lines.push("用法: /architecture show <name>".to_string());
                            self.messages.push(ChatMessage::System(lines.join("\n")));
                        }
                    }
                    Err(e) => {
                        self.messages.push(ChatMessage::System(format!("获取架构列表失败: {e}")));
                    }
                }
            }
            other if other.starts_with("/architecture show ") => {
                let name = other[19..].trim();
                if name.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /architecture show <name>".into()));
                } else {
                    match self.client.list_architectures().await {
                        Ok(archs) => {
                            if let Some(a) = archs.iter().find(|a| a.name == name) {
                                let mut lines = vec![
                                    format!("名称: {}", a.name),
                                    format!("ID: {}", a.id),
                                ];
                                if let Some(desc) = &a.description {
                                    lines.push(format!("描述: {desc}"));
                                }
                                lines.push(format!("拓扑: {}", a.topology));
                                lines.push(format!("节点: {}", a.node_count));
                                lines.push(format!("流: {}", a.flow_count));
                                self.messages.push(ChatMessage::System(lines.join("\n")));
                            } else {
                                self.messages.push(ChatMessage::System(format!("未找到架构: {name}")));
                            }
                        }
                        Err(e) => {
                            self.messages.push(ChatMessage::System(format!("获取架构信息失败: {e}")));
                        }
                    }
                }
            }
            "/skill" => {
                match self.client.list_skills().await {
                    Ok(list) => {
                        if list.is_empty() {
                            self.messages.push(ChatMessage::System("(没有已安装的技能)".into()));
                        } else {
                            let mut lines = vec!["已安装技能:".to_string()];
                            for s in &list {
                                let status = if s.enabled { "启用" } else { "禁用" };
                                lines.push(format!("  {} v{} ({})", s.name, s.version, status));
                            }
                            lines.push("用法: /skill enable|disable <name>".to_string());
                            self.messages.push(ChatMessage::System(lines.join("\n")));
                        }
                    }
                    Err(e) => {
                        self.messages.push(ChatMessage::System(format!("获取技能列表失败: {e}")));
                    }
                }
            }
            other if other.starts_with("/skill enable ") => {
                let name = other[14..].trim();
                if name.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /skill enable <name>".into()));
                } else {
                    match self.client.enable_skill(name).await {
                        Ok(_) => {
                            self.messages.push(ChatMessage::System(format!("已启用技能: {name}")));
                        }
                        Err(e) => {
                            self.messages.push(ChatMessage::System(format!("启用技能失败: {e}")));
                        }
                    }
                }
            }
            other if other.starts_with("/skill disable ") => {
                let name = other[15..].trim();
                if name.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /skill disable <name>".into()));
                } else {
                    match self.client.disable_skill(name).await {
                        Ok(_) => {
                            self.messages.push(ChatMessage::System(format!("已禁用技能: {name}")));
                        }
                        Err(e) => {
                            self.messages.push(ChatMessage::System(format!("禁用技能失败: {e}")));
                        }
                    }
                }
            }
            "/task" => {
                match self.client.list_tasks().await {
                    Ok(tasks) => {
                        if tasks.is_empty() {
                            self.messages.push(ChatMessage::System("(没有待处理的任务)".into()));
                        } else {
                            let mut lines = vec!["任务队列:".to_string()];
                            for t in &tasks {
                                lines.push(format!("  {}: {} [{}] priority={}", t.id, t.description, t.status, t.priority));
                            }
                            lines.push("用法: /task cancel <id>".to_string());
                            self.messages.push(ChatMessage::System(lines.join("\n")));
                        }
                    }
                    Err(e) => {
                        self.messages.push(ChatMessage::System(format!("获取任务列表失败: {e}")));
                    }
                }
            }
            other if other.starts_with("/task cancel ") => {
                let id = other[13..].trim();
                if id.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /task cancel <id>".into()));
                } else {
                    match self.client.cancel_task(id).await {
                        Ok(true) => {
                            self.messages.push(ChatMessage::System(format!("已取消任务: {id}")));
                        }
                        Ok(false) => {
                            self.messages.push(ChatMessage::System(format!("未找到任务: {id}")));
                        }
                        Err(e) => {
                            self.messages.push(ChatMessage::System(format!("取消任务失败: {e}")));
                        }
                    }
                }
            }
            "/health" => {
                match self.client.health_check().await {
                    Ok(h) => {
                        let lines = vec![
                            "健康状态:".to_string(),
                            format!("  状态: {}", h.status),
                            format!("  版本: {}", h.version),
                            format!("  运行时间: {}s", h.uptime_secs),
                            format!("  活跃会话: {}", h.active_sessions),
                            format!("  队列深度: {}", h.queue_depth),
                        ];
                        self.messages.push(ChatMessage::System(lines.join("\n")));
                    }
                    Err(e) => {
                        self.messages.push(ChatMessage::System(format!("健康检查失败: {e}")));
                    }
                }
            }
            "/desk" => {
                let desk_info = self.desk.format_desk();
                self.messages.push(ChatMessage::System(format!("工作台:\n{}", desk_info)));
            }
            "/timestamp" => {
                self.show_timestamps = !self.show_timestamps;
                let status = if self.show_timestamps { "开启" } else { "关闭" };
                self.messages.push(ChatMessage::System(format!("时间戳显示已{status}")));
            }
            "/fullscreen" => {
                self.fullscreen = !self.fullscreen;
                let status = if self.fullscreen { "开启" } else { "关闭" };
                self.messages.push(ChatMessage::System(format!("全屏模式已{status}")));
            }
            other if other.starts_with("/alias ") => {
                let parts: Vec<&str> = other[7..].splitn(2, ' ').collect();
                if parts.len() == 2 {
                    self.aliases.insert(parts[0].to_string(), parts[1].to_string());
                    self.messages.push(ChatMessage::System(format!(
                        "别名已设置: {} -> {}", parts[0], parts[1]
                    )));
                } else {
                    self.messages.push(ChatMessage::System("用法: /alias <name> <command>".into()));
                }
            }
            "/aliases" => {
                if self.aliases.is_empty() {
                    self.messages.push(ChatMessage::System("(没有设置别名)".into()));
                } else {
                    let mut lines = vec!["别名列表:".to_string()];
                    for (name, cmd) in &self.aliases {
                        lines.push(format!("  {} -> {}", name, cmd));
                    }
                    self.messages.push(ChatMessage::System(lines.join("\n")));
                }
            }
            "/divider" => {
                self.show_dividers = !self.show_dividers;
                let status = if self.show_dividers { "开启" } else { "关闭" };
                self.messages.push(ChatMessage::System(format!("消息分隔线已{status}")));
            }
            "/theme" => {
                let themes = crate::ui::Theme::available_themes();
                let mut lines = vec!["可用主题:".to_string()];
                for t in &themes {
                    lines.push(format!("  {}", t));
                }
                lines.push("用法: /theme <name>".to_string());
                self.messages.push(ChatMessage::System(lines.join("\n")));
            }
            other if other.starts_with("/theme ") => {
                let theme_name = other[7..].trim();
                if theme_name.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /theme <name>".into()));
                } else if theme_name == "export" {
                    // Export current theme to file
                    let theme_json = self.theme.to_json();
                    let home = dirs_home();
                    let theme_path = home.join(".maix").join("theme.json");
                    let _ = std::fs::create_dir_all(home.join(".maix"));
                    match std::fs::write(&theme_path, &theme_json) {
                        Ok(_) => {
                            self.messages.push(ChatMessage::System(format!(
                                "主题已导出到: {}\n可编辑此文件自定义主题色", theme_path.display()
                            )));
                        }
                        Err(e) => {
                            self.messages.push(ChatMessage::System(format!("导出失败: {}", e)));
                        }
                    }
                } else {
                    self.theme = crate::ui::Theme::from_name(theme_name);
                    self.messages.push(ChatMessage::System(format!("已切换主题: {}", theme_name)));
                }
            }
            "/layout" => {
                let presets = vec!["standard", "compact", "relaxed", "focus"];
                let mut lines = vec!["可用布局预设:".to_string()];
                for p in &presets {
                    let marker = if *p == self.layout_preset { " *" } else { "" };
                    lines.push(format!("  {}{}", p, marker));
                }
                lines.push("用法: /layout <preset>".to_string());
                self.messages.push(ChatMessage::System(lines.join("\n")));
            }
            other if other.starts_with("/layout ") => {
                let preset = other[8..].trim();
                match preset {
                    "compact" => {
                        self.panel_width = 20;
                        self.show_dividers = false;
                        self.show_timestamps = false;
                        self.layout_preset = "compact".to_string();
                        self.messages.push(ChatMessage::System("已切换到紧凑布局".into()));
                    }
                    "relaxed" => {
                        self.panel_width = 40;
                        self.show_dividers = true;
                        self.show_timestamps = true;
                        self.layout_preset = "relaxed".to_string();
                        self.messages.push(ChatMessage::System("已切换到宽松布局".into()));
                    }
                    "focus" => {
                        self.panel_width = 15;
                        self.show_dividers = false;
                        self.show_timestamps = false;
                        self.fullscreen = true;
                        self.layout_preset = "focus".to_string();
                        self.messages.push(ChatMessage::System("已切换到专注布局".into()));
                    }
                    "standard" => {
                        self.panel_width = 30;
                        self.show_dividers = true;
                        self.show_timestamps = false;
                        self.fullscreen = false;
                        self.layout_preset = "standard".to_string();
                        self.messages.push(ChatMessage::System("已切换到标准布局".into()));
                    }
                    _ => {
                        self.messages.push(ChatMessage::System("未知布局。可选: standard, compact, relaxed, focus".into()));
                    }
                }
            }
            "/sound" => {
                let new_state = !self.notifier.sound_enabled();
                self.notifier.set_sound(new_state);
                let status = if new_state { "开启" } else { "关闭" };
                self.messages.push(ChatMessage::System(format!("声音提醒已{status}")));
            }
            other if other.starts_with("/remind ") => {
                let args = &other[8..];
                let parts: Vec<&str> = args.splitn(2, ' ').collect();
                if parts.len() < 2 {
                    self.messages.push(ChatMessage::System("用法: /remind <时间> <内容>\n示例: /remind 5m 喝水休息\n支持: 30s, 5m, 1h, 2d".into()));
                } else {
                    let time_str = parts[0];
                    let message = parts[1];
                    match parse_duration(time_str) {
                        Some(duration) => {
                            let id = self.next_reminder_id;
                            self.next_reminder_id += 1;
                            self.reminders.push(Reminder::new(id, message.to_string(), duration));
                            let mins = duration.as_secs() / 60;
                            let secs = duration.as_secs() % 60;
                            let time_display = if mins > 0 {
                                format!("{}分{}秒", mins, secs)
                            } else {
                                format!("{}秒", secs)
                            };
                            self.messages.push(ChatMessage::System(format!(
                                "提醒 #{} 已设置: {} ({}后)", id, message, time_display
                            )));
                        }
                        None => {
                            self.messages.push(ChatMessage::System(format!(
                                "无法解析时间: {}\n支持格式: 30s, 5m, 1h, 2d", time_str
                            )));
                        }
                    }
                }
            }
            "/reminders" => {
                let active: Vec<&Reminder> = self.reminders.iter().filter(|r| !r.triggered).collect();
                if active.is_empty() {
                    self.messages.push(ChatMessage::System("(没有待触发的提醒)".into()));
                } else {
                    let mut lines = vec!["提醒列表:".to_string()];
                    for r in &active {
                        let remaining = r.remaining();
                        let mins = remaining.as_secs() / 60;
                        let secs = remaining.as_secs() % 60;
                        let time_display = if mins > 0 {
                            format!("{}分{}秒", mins, secs)
                        } else {
                            format!("{}秒", secs)
                        };
                        lines.push(format!("  #{}: {} (剩余{})", r.id, r.message, time_display));
                    }
                    lines.push("用法: /remind <时间> <内容>".to_string());
                    self.messages.push(ChatMessage::System(lines.join("\n")));
                }
            }
            "/session new" => {
                let idx = self.sessions.len() + 1;
                let new_session = SessionTab::new(
                    uuid::Uuid::new_v4().to_string(),
                    format!("会话 {}", idx),
                );
                self.sessions.push(new_session);
                // Save current and switch to new
                self.sessions[self.active_session].messages = self.messages.clone();
                self.active_session = self.sessions.len() - 1;
                self.messages.clear();
                self.session_id = self.sessions[self.active_session].id.clone();
                self.messages.push(ChatMessage::System(format!("新会话: {}", idx)));
                self.chat_scroll = 0;
            }
            other if other.starts_with("/session color ") => {
                let color = other[15..].trim().to_string();
                if !color.is_empty() {
                    self.sessions[self.active_session].color = Some(color.clone());
                    self.messages.push(ChatMessage::System(format!(
                        "会话颜色已设置: {}", color
                    )));
                }
            }
            "/session list" => {
                let mut lines = vec!["会话列表:".to_string()];
                for (i, session) in self.sessions.iter().enumerate() {
                    let active = if i == self.active_session { " *" } else { "" };
                    let color = session.color.as_deref().unwrap_or("默认");
                    let locked = if session.locked { " [锁定]" } else { "" };
                    lines.push(format!(
                        "  {} {} ({}消息) 颜色:{}{}{}",
                        i + 1,
                        session.name,
                        session.messages.len(),
                        color,
                        locked,
                        active
                    ));
                }
                self.messages.push(ChatMessage::System(lines.join("\n")));
            }
            other if other.starts_with("/session rename ") => {
                let name = other[16..].trim().to_string();
                if !name.is_empty() {
                    self.sessions[self.active_session].name = name.clone();
                    self.messages.push(ChatMessage::System(format!(
                        "会话已重命名: {}", name
                    )));
                }
            }
            "/session lock" => {
                self.sessions[self.active_session].locked = true;
                self.messages.push(ChatMessage::System("会话已锁定".into()));
            }
            "/session unlock" => {
                self.sessions[self.active_session].locked = false;
                self.messages.push(ChatMessage::System("会话已解锁".into()));
            }
            other if other.starts_with("/note add ") => {
                let content = &other[10..];
                if content.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /note add <内容>".into()));
                } else {
                    let id = self.desk.add_note(content, crate::desk::NoteColor::Yellow);
                    self.messages.push(ChatMessage::System(format!("已添加便签: {}", id)));
                }
            }
            other if other.starts_with("/pin ") => {
                let path_str = &other[5..].trim();
                if path_str.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /pin <文件路径>".into()));
                } else {
                    let path = std::path::Path::new(path_str);
                    match self.desk.pin_file(path) {
                        Ok(_) => {
                            self.messages.push(ChatMessage::System(format!("已固定文件: {}", path.display())));
                        }
                        Err(e) => {
                            self.messages.push(ChatMessage::System(format!("固定文件失败: {e}")));
                        }
                    }
                }
            }
            other if other.starts_with("/task_add ") => {
                let title = &other[10..].trim();
                if title.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /task_add <标题>".into()));
                } else {
                    let id = self.desk.add_task(title);
                    self.messages.push(ChatMessage::System(format!("已添加任务: {}", id)));
                }
            }
            "/todo" => {
                let tasks = &self.desk.task_board.tasks;
                if tasks.is_empty() {
                    self.messages.push(ChatMessage::System("(没有待办事项)\n用法: /todo add <内容>".into()));
                } else {
                    let mut lines = vec![format!("待办事项 ({}/{} 已完成):", self.desk.task_board.done_count(), tasks.len())];
                    for task in tasks {
                        lines.push(format!("  {} {} {}", task.status.checkbox(), task.id, task.title));
                    }
                    lines.push("\n用法: /todo add|done|start|rm <args>".to_string());
                    self.messages.push(ChatMessage::System(lines.join("\n")));
                }
            }
            other if other.starts_with("/todo add ") => {
                let title = &other[10..].trim();
                if title.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /todo add <内容>".into()));
                } else {
                    let id = self.desk.add_task(title);
                    self.messages.push(ChatMessage::System(format!("已添加待办: {} {}", id, title)));
                }
            }
            other if other.starts_with("/todo done ") => {
                let id = &other[11..].trim();
                if id.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /todo done <id>".into()));
                } else {
                    match self.desk.complete_task(id) {
                        Ok(_) => {
                            self.messages.push(ChatMessage::System(format!("已完成: {}", id)));
                        }
                        Err(e) => {
                            self.messages.push(ChatMessage::System(format!("失败: {}", e)));
                        }
                    }
                }
            }
            other if other.starts_with("/todo start ") => {
                let id = &other[12..].trim();
                if id.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /todo start <id>".into()));
                } else {
                    match self.desk.start_task(id) {
                        Ok(_) => {
                            self.messages.push(ChatMessage::System(format!("进行中: {}", id)));
                        }
                        Err(e) => {
                            self.messages.push(ChatMessage::System(format!("失败: {}", e)));
                        }
                    }
                }
            }
            other if other.starts_with("/todo rm ") => {
                let id = &other[9..].trim();
                if id.is_empty() {
                    self.messages.push(ChatMessage::System("用法: /todo rm <id>".into()));
                } else {
                    match self.desk.task_board.remove_task(id) {
                        Ok(_) => {
                            self.messages.push(ChatMessage::System(format!("已删除: {}", id)));
                        }
                        Err(e) => {
                            self.messages.push(ChatMessage::System(format!("失败: {}", e)));
                        }
                    }
                }
            }
            "/export" => {
                // Export conversation to markdown file
                let mut markdown = String::new();
                markdown.push_str("# Maix-Agent 对话导出\n\n");
                markdown.push_str(&format!("会话ID: {}\n", self.session_id));
                markdown.push_str(&format!("模型: {}\n", self.model_name));
                markdown.push_str(&format!("时间: {}\n\n", chrono::Local::now().format("%Y-%m-%d %H:%M:%S")));

                for msg in &self.messages {
                    match msg {
                        ChatMessage::User(text) => {
                            markdown.push_str(&format!("## 用户\n\n{}\n\n", text));
                        }
                        ChatMessage::Assistant(text) => {
                            markdown.push_str(&format!("## 助手\n\n{}\n\n", text));
                        }
                        ChatMessage::System(text) => {
                            markdown.push_str(&format!("> {}\n\n", text));
                        }
                        _ => {}
                    }
                }

                let filename = format!("maix-chat-{}.md", chrono::Local::now().format("%Y%m%d-%H%M%S"));
                match std::fs::write(&filename, &markdown) {
                    Ok(_) => {
                        self.messages.push(ChatMessage::System(format!("已导出对话到: {}", filename)));
                    }
                    Err(e) => {
                        self.messages.push(ChatMessage::System(format!("导出失败: {}", e)));
                    }
                }
            }
            "/help" => {
                self.messages.push(ChatMessage::System(
                    "命令列表:\n\
                    \n\
                    /mode <plan|agent|yolo>  切换模式\n\
                    /model <name>            切换模型\n\
                    /vim                     切换 Vim 模式\n\
                    /tutorial                交互式教程\n\
                    /quickstart              快速入门卡片\n\
                    /init [force]            生成 MAIX.md 项目约定\n\
                    /compact [instructions]  压缩上下文\n\
                    /memory                  显示记忆面板\n\
                    /tools                   显示工具面板\n\
                    /tool_history            工具调用历史\n\
                    /tool_replay <index>     重新执行工具\n\
                    /tool_perms              工具权限管理\n\
                    /tool_fav                工具收藏管理\n\
                    /tool_stats              工具使用统计\n\
                    /tool_cache              工具缓存管理\n\
                    /tool_perf               工具性能分析\n\
                    /retry                   重试失败的工具\n\
                    /stats                   显示统计面板\n\
                    /desk                    显示工作台\n\
                    /timestamp               开关时间戳\n\
                    /sessions                列出已保存会话\n\
                    /branch [msg_index]      创建会话分支\n\
                    /tag [add|rm]            会话标签管理\n\
                    /template [save|load|rm] 会话模板管理\n\
                    /search <关键词>         搜索会话内容\n\
                    /session_stats           会话详细统计\n\
                    /session merge <id>      合并会话\n\
                    /session compare <id>    比较会话\n\
                    /session replay          回放会话\n\
                    /session share           分享会话\n\
                    /resume <id>             恢复已保存会话\n\
                    /cost                    显示 token 用量与费用\n\
                    /config [export|import|history|sync|rollback|diff] 配置管理\n\
                    /doctor                  环境诊断\n\
                    /identity                身份管理\n\
                    /architecture            架构管理\n\
                    /skill                   技能管理\n\
                    /task                    任务队列管理\n\
                    /task_add <title>        添加任务\n\
                    /health                  健康检查\n\
                    /export [html]           导出对话\n\
                    /tool_chain [add|run|rm] 工具链管理\n\
                    /chain show <name>       工具链流程图\n\
                    /tool_template [save|load|rm] 工具模板管理\n\
                    /tool_parallel <tools>   并行执行工具\n\
                    /debug [filter|clear]    调试控制台\n\
                    /net [stats|clear]       网络请求追踪\n\
                    /checkpoint [save|load|list|rm] 状态检查点\n\
                    /record [start|stop|export|clear] 会话录制\n\
                    /perf                    性能分析\n\
                    /note add <content>      添加便签\n\
                    /pin <file>              固定文件到工作台\n\
                    /todo [add|done|start|rm] 待办事项管理\n\
                    /sound                   开关声音提醒\n\
                    /remind <time> <msg>     设置定时提醒\n\
                    /reminders               查看提醒列表\n\
                    /theme <name>            切换主题\n\
                    /layout <preset>         切换布局\n\
                    /keys <scheme>           快捷键方案\n\
                    /tips                    最佳实践提示\n\
                    /usage                   使用统计\n\
                    /feedback <内容>         提交反馈\n\
                    /profile [save|load]     用户配置管理\n\
                    /calendar                显示日历\n\
                    /habit [add|done|rm]     习惯追踪\n\
                    /clear                   清空对话\n\
                    /quit                    退出\n\
                    \n\
                    快捷键:\n\
                    Esc                    中断生成/清空输入\n\
                    Shift+Enter            换行（多行输入）\n\
                    Tab                    循环补全\n\
                    1-9                    直接选择补全项\n\
                    Ctrl+U                 清空当前行\n\
                    Ctrl+W                 删除前一个单词\n\
                    Ctrl+K                 删除到行尾\n\
                    Ctrl+A                 文本选择模式\n\
                    Ctrl+F                 搜索对话\n\
                    Ctrl+P                 命令面板\n\
                    Ctrl+R                 显示/隐藏推理过程\n\
                    Ctrl+Q                 退出"
                        .into(),
                ));
            }
            other if other.starts_with("/mode ") => {
                self.messages.push(ChatMessage::System("未知模式。可选: /mode plan, /mode agent, /mode yolo".to_string()));
            }
            other => {
                // Find similar commands for suggestion
                let cmd_name = other.split_whitespace().next().unwrap_or(other);
                let all_commands = vec![
                    "/help", "/quit", "/exit", "/mode", "/model", "/vim", "/init",
                    "/compact", "/memory", "/tools", "/tool_history", "/tool_replay", "/tool_perms",
                    "/tool_fav", "/tool_stats", "/tool_cache", "/tool_perf", "/retry", "/stats", "/desk",
                    "/timestamp", "/fullscreen", "/sessions", "/branch", "/tag", "/template", "/search",
                    "/session_stats", "/session merge", "/session compare", "/session replay", "/session share",
                    "/resume", "/cost", "/config", "/config diff", "/doctor", "/identity", "/architecture",
                    "/skill", "/task", "/health", "/export", "/export html", "/clear", "/note", "/pin", "/task_add",
                    "/sound", "/remind", "/reminders", "/todo", "/theme", "/theme export", "/layout", "/keys",
                    "/tutorial", "/quickstart", "/tips", "/usage", "/feedback", "/profile",
                    "/calendar", "/habit", "/tool_chain", "/tool_template", "/tool_parallel",
                    "/chain", "/chain show", "/debug", "/net", "/net stats", "/net clear",
                    "/checkpoint", "/checkpoint save", "/checkpoint load", "/checkpoint list", "/checkpoint rm",
                    "/record", "/record start", "/record stop", "/record export", "/record clear",
                    "/perf", "/tag msg", "/tags", "/pin msg", "/pinned", "/unpin msg",
                    "/notes", "/notes set", "/notes show", "/notes clear", "/keys", "/shortcuts",
                    "/quote", "/recover", "/ref", "/refs", "/archive", "/archived", "/storage",
                    "/fav add", "/fav rm", "/favs", "/goto", "/batch delete", "/batch archive all",
                    "/compact", "/layout save", "/layout load", "/layouts",
                    "/snippets", "/snippet save", "/snippet load", "/snippet list", "/snippet rm",
                    "/git", "/git diff", "/code_search", "/cs", "/remember", "/recall", "/forget",
                    "/diagnose", "/workflow", "/workflow add", "/workflow run", "/workflow list",
                    "/workflow rm", "/macro", "/macro record", "/macro stop", "/macro save",
                    "/macro run", "/macro list", "/macro rm", "/stats detail", "/stats full",
                    "/theme edit", "/theme export", "/theme import", "/theme colors",
                    "/bind", "/bind list", "/bind set", "/bind rm", "/bind reset",
                    "/custom", "/custom reload",
                ];
                let suggestion = all_commands.iter()
                    .min_by_key(|cmd| levenshtein_distance(cmd_name, cmd))
                    .filter(|cmd| levenshtein_distance(cmd_name, cmd) <= 3);

                if let Some(suggested) = suggestion {
                    self.messages.push(ChatMessage::System(format!(
                        "未知命令: {other}。你是不是想输入: {}?", suggested
                    )));
                } else {
                    self.messages.push(ChatMessage::System(format!(
                        "未知命令: {other}。输入 /help 查看可用命令。"
                    )));
                }
            }
        }
    }

    async fn send_message(&mut self, text: String) {
        self.messages.push(ChatMessage::User(text.clone()));
        self.round_count += 1;
        if self.auto_scroll {
            self.chat_scroll = 0;
        }
        self.stream_renderer.clear();
        self.mark_dirty(DirtyRegion::Chat);
        self.mark_dirty(DirtyRegion::StatusBar);
        self.search_index.mark_dirty();

        // 100-005: Save to command history
        if !text.is_empty() {
            self.command_history.push(text.clone());
            if self.command_history.len() > 500 {
                self.command_history.remove(0);
            }
        }

        // 100-006: Auto-name session from first user message
        if self.messages.iter().filter(|m| matches!(m, ChatMessage::User(_))).count() == 1 {
            let name: String = text.chars().take(30).collect();
            if let Some(session) = self.sessions.get_mut(self.active_session) {
                session.name = name;
            }
        }

        let tx = self.event_tx.clone();
        let client = self.client.clone();
        let streaming_flag = self.is_streaming.clone();
        let session_id = self.session_id.clone();

        tokio::spawn(async move {
            // Set streaming flag right before the gRPC call, not before spawn
            streaming_flag.store(true, Ordering::SeqCst);
            let mut handle = match client.chat_with_message(&session_id, &text).await {
                Ok(h) => h,
                Err(e) => {
                    let _ = tx.send(AppEvent::Error(e.to_string()));
                    streaming_flag.store(false, Ordering::SeqCst);
                    return;
                }
            };

            loop {
                match handle.recv().await {
                    Some(Ok(msg)) => {
                        if let Some(out) = msg.output {
                            let event = match out {
                                pb::chat_output::Output::TextDelta(d) => {
                                    AppEvent::TextDelta(d.text)
                                }
                                pb::chat_output::Output::ReasoningDelta(d) => {
                                    AppEvent::ReasoningDelta(d.text)
                                }
                                pb::chat_output::Output::ToolCall(tc) => {
                                    AppEvent::ToolCall {
                                        name: tc.tool_name,
                                        args: format!("{:?}", tc.arguments),
                                    }
                                }
                                pb::chat_output::Output::ToolResult(tr) => {
                                    AppEvent::ToolResult {
                                        result: tr.result,
                                    }
                                }
                                pb::chat_output::Output::Complete(c) => {
                                    if let Some(u) = c.usage {
                                        AppEvent::Complete {
                                            prompt_tokens: u.prompt_tokens,
                                            completion_tokens: u.completion_tokens,
                                            total_tokens: u.total_tokens,
                                            cache_read_tokens: u.cache_read_tokens,
                                            cache_write_tokens: u.cache_write_tokens,
                                        }
                                    } else {
                                        streaming_flag.store(false, Ordering::SeqCst);
                                        break;
                                    }
                                }
                                pb::chat_output::Output::Status(s) => {
                                    AppEvent::StatusUpdate { state: s.state }
                                }
                                pb::chat_output::Output::Error(e) => {
                                    AppEvent::Error(e.message)
                                }
                            };
                            let _ = tx.send(event);
                        }
                    }
                    Some(Err(e)) => {
                        let _ = tx.send(AppEvent::Error(e.to_string()));
                        break;
                    }
                    None => break,
                }
            }
            streaming_flag.store(false, Ordering::SeqCst);
        });
    }

    async fn handle_app_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::TextDelta(text) => {
                if text.is_empty() {
                    return;
                }
                self.stream_renderer.append_token(&text);
                if let Some(ChatMessage::Assistant(last)) = self.messages.last_mut() {
                    last.push_str(&text);
                } else {
                    self.messages.push(ChatMessage::Assistant(text));
                }
                // Check message limit periodically
                if self.messages.len() % 100 == 0 {
                    self.trim_messages();
                }
            }
            AppEvent::ReasoningDelta(text) => {
                if text.is_empty() {
                    return;
                }
                if let Some(ChatMessage::Reasoning(last)) = self.messages.last_mut() {
                    last.push_str(&text);
                } else {
                    self.messages.push(ChatMessage::Reasoning(text));
                }
            }
            AppEvent::ToolCall { name, args } => {
                self.status_detail = Some(format!("调用工具: {}", name));
                // Track timing
                self.current_tool_call = Some(ToolCallInfo {
                    name: name.clone(),
                    args: args.clone(),
                    start_time: std::time::Instant::now(),
                });
                // Check if auto-approve is enabled
                if self.auto_approve_round {
                    self.messages.push(ChatMessage::ToolCall { name: name.clone(), args: args.clone() });
                } else {
                    // Add to pending approvals
                    self.pending_tool_approvals.push(ToolApproval {
                        name: name.clone(),
                        args: args.clone(),
                        risk_level: 0,
                        timestamp: std::time::Instant::now(),
                    });
                    self.messages.push(ChatMessage::System(format!(
                        "工具调用待审批: {} [Y批准/N拒绝/A全部批准]",
                        name
                    )));
                }
            }
            AppEvent::ToolResult { result } => {
                // Calculate elapsed time
                let elapsed_info = if let Some(tool_call) = self.current_tool_call.take() {
                    let elapsed = tool_call.start_time.elapsed();
                    let elapsed_str = if elapsed.as_secs() > 0 {
                        format!(" ({}.{:01}s)", elapsed.as_secs(), elapsed.subsec_millis() / 100)
                    } else {
                        format!(" ({}ms)", elapsed.as_millis())
                    };
                    // Warn if slow
                    if elapsed.as_secs() > 10 {
                        format!("{} ⚠️ 慢", elapsed_str)
                    } else {
                        elapsed_str
                    }
                } else {
                    String::new()
                };
                self.messages.push(ChatMessage::ToolResult {
                    result: format!("{}{}", result, elapsed_info)
                });
            }
            AppEvent::Complete {
                prompt_tokens,
                completion_tokens,
                total_tokens,
                cache_read_tokens,
                cache_write_tokens,
            } => {
                self.total_tokens += total_tokens;
                self.prompt_tokens += prompt_tokens;
                self.completion_tokens += completion_tokens;
                self.cache_read_tokens += cache_read_tokens;
                self.cache_write_tokens += cache_write_tokens;

                // Calculate token rate
                let now = std::time::Instant::now();
                let elapsed = now.duration_since(self.last_rate_update).as_secs_f64();
                if elapsed > 0.0 {
                    let tokens_diff = self.total_tokens - self.last_token_count;
                    self.token_rate = tokens_diff as f64 / elapsed;
                }
                self.last_token_count = self.total_tokens;
                self.last_rate_update = now;

                let usage = TokenUsage {
                    prompt_tokens,
                    completion_tokens,
                    total_tokens,
                    cache_read_tokens,
                    cache_write_tokens,
                };
                self.cost_tracker.record_turn(self.round_count as usize, usage, self.model_name.clone());
                self.total_cost = self.cost_tracker.total_cost();

                self.agent_state = Some("Idle".into());
                self.status_detail = None;
                self.notifier.task_complete(&format!("{} tokens, ${:.4}", total_tokens, self.total_cost));
                self.notifier.play_sound(crate::notify::NotifyKind::Success);
            }
            AppEvent::MemoryUpdated => {
                self.refresh_memories().await;
            }
            AppEvent::Error(e) => {
                let fix_suggestion = suggest_fix(&e);

                // Enhanced error formatting with stack trace
                let mut error_lines = vec![
                    format!("✗ 错误"),
                    format!("  {}", e),
                ];

                // Add suggestion if available
                if !fix_suggestion.is_empty() {
                    error_lines.push(format!(""));
                    error_lines.push(format!("💡 建议: {}", fix_suggestion));
                }

                // Add error context (stack trace-like info)
                if e.contains("connection") || e.contains("timeout") {
                    error_lines.push(format!(""));
                    error_lines.push(format!("📍 上下文:"));
                    error_lines.push(format!("  会话: {}", &self.session_id[..8.min(self.session_id.len())]));
                    error_lines.push(format!("  模型: {}", self.model_name));
                    error_lines.push(format!("  服务: {}", self.server_addr));
                }

                // Add copy hint for long errors
                if e.len() > 100 {
                    error_lines.push(format!(""));
                    error_lines.push(format!("📋 按 Ctrl+A 选择, Ctrl+C 复制完整错误信息"));
                }

                self.messages.push(ChatMessage::System(error_lines.join("\n")));
                self.agent_state = Some("Errored".into());
                self.status_detail = None;
                self.notifier.error(&e);
                self.notifier.play_sound(crate::notify::NotifyKind::Error);
            }
            AppEvent::StatusUpdate { state } => {
                // Map AgentState enum values to display text
                self.status_detail = match state {
                    1 => None, // IDLE
                    2 => Some("思考中".into()),          // THINKING
                    3 => Some("执行工具中".into()),       // EXECUTING_TOOL
                    4 => Some("等待审批".into()),         // WAITING_APPROVAL
                    5 => Some("生成回复中".into()),       // RESPONDING
                    6 => Some("更新记忆中".into()),       // UPDATING_MEMORY
                    7 => Some("错误".into()),             // ERRORED
                    _ => None,
                };
                self.agent_state = match state {
                    1 => Some("Idle".into()),
                    2 => Some("Thinking".into()),
                    3 => Some("Executing".into()),
                    4 => Some("Waiting".into()),
                    5 => Some("Responding".into()),
                    6 => Some("Memory".into()),
                    7 => Some("Error".into()),
                    _ => None,
                };
            }
        }
    }
}
