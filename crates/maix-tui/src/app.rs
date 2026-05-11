use crate::input::InputState;
use crate::ui;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use maix_core::client::MaixClient;
use maix_core::proto::maix::core::v1 as pb;
use ratatui::Terminal;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

// AgentMode values matching pb::AgentMode enum
pub const MODE_AGENT: i32 = 0;
pub const MODE_PLAN: i32 = 1;
pub const MODE_YOLO: i32 = 2;

pub fn mode_name(mode: i32) -> &'static str {
    match mode {
        MODE_PLAN => "PLAN",
        MODE_YOLO => "YOLO",
        _ => "AGENT",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivePanel {
    Memory,
    Tools,
    Stats,
}

#[derive(Debug, Clone)]
pub enum ChatMessage {
    User(String),
    Assistant(String),
    ToolCall { name: String, args: String },
    ToolResult { result: String },
    System(String),
}

#[derive(Debug, Clone)]
pub enum AppEvent {
    TextDelta(String),
    #[allow(dead_code)]
    ReasoningDelta(String),
    ToolCall { name: String, args: String },
    ToolResult { result: String },
    Complete { prompt_tokens: u64, completion_tokens: u64, total_tokens: u64 },
    Error(String),
    #[allow(dead_code)]
    MemoryUpdated,
}

#[derive(Debug, Clone)]
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

pub struct App {
    pub model_name: String,
    pub mode: i32,
    pub messages: Vec<ChatMessage>,
    pub memories: Vec<pb::MemoryEntry>,
    pub tool_defs: Vec<pb::ToolInfo>,
    pub input: InputState,
    pub active_panel: ActivePanel,
    pub is_streaming: Arc<AtomicBool>,
    pub provider_caps: ProviderCaps,
    pub agent_state: Option<String>,
    pub total_tokens: u64,
    pub total_cost: f64,
    pub round_count: u64,
    pub session_id: String,
    #[allow(dead_code)]
    pub server_addr: String,

    client: MaixClient,
    event_tx: tokio::sync::mpsc::UnboundedSender<AppEvent>,
    event_rx: tokio::sync::mpsc::UnboundedReceiver<AppEvent>,
    should_quit: bool,
}

impl App {
    pub async fn new(
        model: String,
        _workdir: std::path::PathBuf,
        mode: i32,
        server_addr: String,
    ) -> Self {
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();

        let client = match MaixClient::connect(&server_addr).await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to connect to maix-server at {}: {e}", server_addr);
                eprintln!("Is maix running? Try: maix --foreground");
                std::process::exit(1);
            }
        };

        let model_name = model.clone();
        let session_id = uuid::Uuid::new_v4().to_string();

        let (tool_defs, memories) = tokio::join!(
            client.list_tools(),
            client.search_memory("", 50),
        );

        let tool_defs = tool_defs.unwrap_or_default();
        let memories = memories.unwrap_or_default();

        App {
            model_name,
            mode,
            messages: vec![ChatMessage::System(format!(
                "Maix-Agent TUI | model: {} | {} mode | server: {}",
                model,
                mode_name(mode),
                server_addr,
            ))],
            memories,
            tool_defs,
            input: InputState::new(),
            active_panel: ActivePanel::Memory,
            is_streaming: Arc::new(AtomicBool::new(false)),
            provider_caps: ProviderCaps::default(),
            agent_state: Some("Idle".into()),
            total_tokens: 0,
            total_cost: 0.0,
            round_count: 0,
            session_id,
            server_addr,
            client,
            event_tx,
            event_rx,
            should_quit: false,
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
        let tick_rate = std::time::Duration::from_millis(50);
        let mut last_tick = tokio::time::Instant::now();

        loop {
            terminal.draw(|f| ui::render(f, self))?;

            if self.should_quit {
                return Ok(());
            }

            let timeout = tick_rate.saturating_sub(last_tick.elapsed());
            let has_event = event::poll(timeout)?;

            if has_event {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.should_quit = true;
                        }
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.should_quit = true;
                        }
                        KeyCode::Tab => {
                            self.active_panel = match self.active_panel {
                                ActivePanel::Memory => ActivePanel::Tools,
                                ActivePanel::Tools => ActivePanel::Stats,
                                ActivePanel::Stats => ActivePanel::Memory,
                            };
                        }
                        KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.messages
                                .push(ChatMessage::System("Session saved.".into()));
                        }
                        _ => {
                            if !self.is_streaming.load(Ordering::SeqCst) {
                                self.handle_input_key(key).await;
                            }
                        }
                    }
                }
            }

            // Drain agent events
            while let Ok(event) = self.event_rx.try_recv() {
                self.handle_app_event(event).await;
            }

            // Refresh memories periodically
            if last_tick.elapsed() > std::time::Duration::from_secs(3) {
                self.refresh_memories().await;
                last_tick = tokio::time::Instant::now();
            }
        }
    }

    async fn handle_input_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.insert_char(c);
            }
            KeyCode::Backspace => self.input.delete_before(),
            KeyCode::Delete => self.input.delete_after(),
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
    }

    async fn handle_slash_command(&mut self, cmd: &str) {
        match cmd {
            "/quit" | "/exit" => self.should_quit = true,
            "/mode plan" => {
                self.mode = MODE_PLAN;
                self.messages
                    .push(ChatMessage::System("Switched to Plan mode".into()));
            }
            "/mode agent" => {
                self.mode = MODE_AGENT;
                self.messages
                    .push(ChatMessage::System("Switched to Agent mode".into()));
            }
            "/mode yolo" => {
                self.mode = MODE_YOLO;
                self.messages
                    .push(ChatMessage::System("Switched to YOLO mode".into()));
            }
            "/memory" => {
                self.active_panel = ActivePanel::Memory;
                self.refresh_memories().await;
            }
            "/tools" => self.active_panel = ActivePanel::Tools,
            "/clear" => {
                self.messages.clear();
                self.messages
                    .push(ChatMessage::System("Cleared.".into()));
            }
            other => {
                self.messages.push(ChatMessage::System(format!(
                    "Unknown command: {other}. Try /quit, /mode, /memory, /clear"
                )));
            }
        }
    }

    async fn send_message(&mut self, text: String) {
        self.messages.push(ChatMessage::User(text.clone()));
        self.is_streaming.store(true, Ordering::SeqCst);
        self.round_count += 1;

        let tx = self.event_tx.clone();
        let client = self.client.clone();
        let streaming_flag = self.is_streaming.clone();

        tokio::spawn(async move {
            let session_id = match client.create_session().await {
                Ok(id) => id,
                Err(e) => {
                    let _ = tx.send(AppEvent::Error(e.to_string()));
                    streaming_flag.store(false, Ordering::SeqCst);
                    return;
                }
            };

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
                                        }
                                    } else {
                                        streaming_flag.store(false, Ordering::SeqCst);
                                        break;
                                    }
                                }
                                pb::chat_output::Output::Status(_s) => {
                                    continue;
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
                if let Some(ChatMessage::Assistant(last)) = self.messages.last_mut() {
                    last.push_str(&text);
                } else {
                    self.messages.push(ChatMessage::Assistant(text));
                }
            }
            AppEvent::ReasoningDelta(_) => {}
            AppEvent::ToolCall { name, args } => {
                self.messages.push(ChatMessage::ToolCall { name, args });
            }
            AppEvent::ToolResult { result } => {
                self.messages.push(ChatMessage::ToolResult { result });
            }
            AppEvent::Complete {
                prompt_tokens,
                completion_tokens,
                total_tokens,
            } => {
                self.total_tokens += total_tokens;
                self.total_cost += prompt_tokens as f64 * 0.5 / 1_000_000.0
                    + completion_tokens as f64 * 2.0 / 1_000_000.0;
                self.agent_state = Some("Idle".into());
            }
            AppEvent::MemoryUpdated => {
                self.refresh_memories().await;
            }
            AppEvent::Error(e) => {
                self.messages
                    .push(ChatMessage::System(format!("Error: {e}")));
                self.agent_state = Some("Errored".into());
            }
        }
    }
}
