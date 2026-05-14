//! Agent core loop and state machine (Phase 1).
//!
//! Also provides the `Maix` facade — the central orchestrator that ties together
//! config, providers, models, memory, tools, skills, and identities.
//! All entry points (CLI, TUI, Gateway) drive the system through `Maix`.

use maix_core::{MaixResult, Message, MessageContent, Role, ToolCall, TokenUsage};
use maix_core::model_router::ModelRouter;
use maix_memory::{MemoryEntry, MemoryKind, MemoryStore};
use maix_provider::{ChatRequest, ChatStream, LLMProvider};
use maix_tools::{RiskLevel, ToolCtx, ToolRegistry};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

pub mod bookmarks;
pub mod branching;
pub mod chain;
pub mod commands;
pub mod compaction;
pub mod context;
pub mod export;
pub mod hooks;
pub mod init;
pub mod maix_md;
pub mod orchestrator;
pub mod planner;
pub mod reasoning;
pub mod recovery;
pub mod reflection;
pub mod router;
pub mod runtime;
pub mod session;
pub mod tool_selector;
pub use session::Session;

// ---------------------------------------------------------------------------
// Maix facade — central orchestrator for all entry points
// ---------------------------------------------------------------------------

/// Central orchestrator that owns the full Agent runtime.
/// CLI, TUI, and Gateway are thin wrappers around this struct.
pub struct Maix {
    pub agent: Agent,
    pub config: maix_core::Config,
    pub router: maix_core::ModelRouter,
    pub identities: maix_core::IdentityManager,
    pub skills: maix_skills::SkillRegistry,
    pub skills_dir: std::path::PathBuf,
    pub default_provider_name: String,
    pub default_model: String,
}

impl Maix {
    /// Build a fully initialized Maix instance from config.
    pub fn new(
        default_provider: &str,
        default_model: &str,
        workdir: std::path::PathBuf,
    ) -> MaixResult<Self> {
        let cfg = maix_core::Config::load().unwrap_or_else(|e| {
            tracing::warn!("Failed to load config: {e}, using defaults");
            maix_core::Config::minimal()
        });

        let router = Self::build_router(&cfg, default_provider);
        let default_route = router.default_route().clone();
        let provider = Self::build_provider(&cfg, &default_route.provider)?;

        let memory_dir = if cfg.memory.dir.as_os_str().is_empty() {
            maix_core::config::default_memory_dir()
        } else {
            cfg.memory.dir.clone()
        };
        let memory = maix_memory::FileMemoryStore::new(memory_dir)?;

        let tools = Arc::new(ToolRegistry::with_builtins());
        let agent_config = AgentConfig {
            mode: AgentMode::Agent,
            ..Default::default()
        };

        let agent = Agent::new(
            agent_config,
            provider,
            tools,
            Box::new(memory),
            uuid::Uuid::new_v4().to_string(),
            workdir,
        )
        .with_hooks(&cfg.hooks)
        .with_auto_router(router.clone());

        let skills_dir = maix_core::config::default_memory_dir()
            .parent()
            .unwrap_or(&std::path::PathBuf::from("."))
            .join("skills");
        let skills = maix_skills::SkillRegistry::new(skills_dir.clone());

        let identities = maix_core::IdentityManager::new().with_defaults();

        Ok(Self {
            agent,
            config: cfg,
            router,
            identities,
            skills,
            skills_dir,
            default_provider_name: default_route.provider,
            default_model: default_model.into(),
        })
    }

    /// Build a model router from config.
    pub fn build_router(cfg: &maix_core::Config, _default_provider: &str) -> maix_core::ModelRouter {
        let model = cfg.model.clone();
        let provider = cfg.provider.clone();
        maix_core::ModelRouter::new(provider, model)
            .with_auto_mode(cfg.agent.auto_mode.clone())
    }

    /// Build an LLM provider from config.
    pub fn build_provider(
        cfg: &maix_core::Config,
        _name: &str,
    ) -> MaixResult<Arc<dyn LLMProvider>> {
        let api_key = cfg.api_key.clone();
        if api_key.is_empty() {
            return Err(maix_core::MaixError::Provider(
                "API key not configured. Set in ~/.maix/settings.json or MAIX_API_KEY env var".into()
            ));
        }
        let provider = maix_provider::OpenAICompatProvider::new(
            cfg.api_base.clone(),
            api_key,
            cfg.model.clone(),
        )
        .with_context_window(1_000_000);
        Ok(Arc::new(provider))
    }

    /// Rebuild provider for a different model route.
    pub fn switch_provider(&mut self, name: &str) -> MaixResult<()> {
        let provider = Self::build_provider(&self.config, name)?;
        self.agent.provider = provider;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Agent State & Mode
// ---------------------------------------------------------------------------

pub use maix_core::AgentState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentMode {
    Plan,
    Agent,
    Yolo,
}

// ---------------------------------------------------------------------------
// Agent Event — returned by each step of the loop
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum AgentEvent {
    Thinking,
    TextDelta(String),
    ReasoningDelta(String),
    ToolCallRequested {
        id: String,
        name: String,
        args: Value,
        needs_approval: bool,
    },
    WaitingApproval,
    ToolCallApproved,
    ToolCallDenied(String),
    ToolResult {
        id: String,
        result: String,
    },
    ResponseComplete {
        text: String,
        usage: TokenUsage,
    },
    MemoryUpdated {
        summary: String,
    },
    Error(String),
}

// ---------------------------------------------------------------------------
// Agent configuration
// ---------------------------------------------------------------------------

pub struct AgentConfig {
    pub max_tool_rounds: usize,
    pub context_threshold: f32,
    pub system_prompt_template: String,
    pub mode: AgentMode,
    pub auto_memory_summary: bool,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_tool_rounds: 16,
            context_threshold: 0.9,
            system_prompt_template: DEFAULT_SYSTEM_PROMPT.into(),
            mode: AgentMode::Agent,
            auto_memory_summary: true,
        }
    }
}

pub const DEFAULT_SYSTEM_PROMPT: &str = include_str!("../assets/system_prompt.md");

// ---------------------------------------------------------------------------
// Agent struct
// ---------------------------------------------------------------------------

pub struct Agent {
    pub config: AgentConfig,
    pub provider: Arc<dyn LLMProvider>,
    pub tools: Arc<ToolRegistry>,
    pub memory: Box<dyn MemoryStore>,
    pub session: Session,
    state: AgentState,
    tool_ctx: ToolCtx,
    pub permissions: maix_core::permissions::PermissionChecker,
    pub hooks: hooks::HookRunner,
    /// Auto-mode router for per-turn model selection.
    auto_router: Option<ModelRouter>,
}

impl Agent {
    pub fn new(
        config: AgentConfig,
        provider: Arc<dyn LLMProvider>,
        tools: Arc<ToolRegistry>,
        memory: Box<dyn MemoryStore>,
        session_id: String,
        working_dir: std::path::PathBuf,
    ) -> Self {
        Self {
            config,
            provider,
            tools,
            memory,
            session: Session::new(session_id.clone()),
            state: AgentState::Idle,
            tool_ctx: ToolCtx {
                session_id,
                working_dir,
                ask_user_tx: None,
            },
            permissions: maix_core::permissions::PermissionChecker::empty(),
            hooks: hooks::HookRunner::empty(),
            auto_router: None,
        }
    }

    /// Set auto-mode router for per-turn model selection.
    pub fn with_auto_router(mut self, router: ModelRouter) -> Self {
        self.auto_router = Some(router);
        self
    }

    /// Set hooks from config.
    pub fn with_hooks(mut self, hooks_config: &std::collections::HashMap<String, Vec<maix_core::config::HookEntry>>) -> Self {
        // Convert HookEntry to hooks::HookConfig
        let converted: std::collections::HashMap<String, Vec<hooks::HookConfig>> = hooks_config
            .iter()
            .map(|(k, v)| {
                let configs: Vec<hooks::HookConfig> = v
                    .iter()
                    .map(|h| hooks::HookConfig {
                        matcher: h.matcher.clone(),
                        command: h.command.clone(),
                        timeout_ms: h.timeout_ms,
                    })
                    .collect();
                (k.clone(), configs)
            })
            .collect();
        self.hooks = hooks::HookRunner::from_config(&converted);
        self
    }

    pub fn state(&self) -> AgentState {
        self.state
    }

    /// Build the system prompt from template.
    fn build_system_prompt(&self, memory_context: &str) -> String {
        // Load MAIX.md hierarchy
        let maix_md_content = {
            let project_root = self.tool_ctx.working_dir.clone();
            let home = std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOME"))
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("."));
            let loader = maix_md::MaixMdLoader::new(project_root, home);
            loader.load_all(None)
        };

        let combined_memory = if maix_md_content.is_empty() {
            memory_context.to_string()
        } else if memory_context.is_empty() {
            maix_md_content
        } else {
            format!("{}\n\n{}", maix_md_content, memory_context)
        };

        let tools_section = if self.config.mode == AgentMode::Plan {
            "You are in Plan mode: read-only tools only. Do NOT modify files or run commands."
                .to_string()
        } else {
            let defs = self.tools.get_defs();
            let tool_list: Vec<String> = defs
                .iter()
                .map(|d| format!("- {}: {}", d.name, d.description))
                .collect();
            format!(
                "Available tools:\n{}",
                tool_list.join("\n")
            )
        };

        let git_context = self.build_git_context();
        let platform = if cfg!(windows) { "Windows" } else if cfg!(target_os = "macos") { "macOS" } else { "Linux" };
        let working_dir = self.tool_ctx.working_dir.display().to_string();

        self.config
            .system_prompt_template
            .replace("{tools_section}", &tools_section)
            .replace("{memory_context}", &combined_memory)
            .replace("{current_date}", &chrono::Utc::now().format("%Y-%m-%d").to_string())
            .replace("{platform}", platform)
            .replace("{working_dir}", &working_dir)
            .replace("{git_context}", &git_context)
    }

    /// Auto-detect git repo and inject branch/status into system prompt.
    fn build_git_context(&self) -> String {
        let cwd = &self.tool_ctx.working_dir;

        // Check if we're in a git repo
        let is_repo = std::process::Command::new("git")
            .args(["rev-parse", "--is-inside-work-tree"])
            .current_dir(cwd)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if !is_repo {
            return String::new();
        }

        let branch = std::process::Command::new("git")
            .args(["branch", "--show-current"])
            .current_dir(cwd)
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "detached HEAD".into());

        let status_summary = std::process::Command::new("git")
            .args(["status", "--porcelain=v1"])
            .current_dir(cwd)
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .unwrap_or_default();

        let changed = status_summary.lines().count();
        let status_str = if changed == 0 {
            "clean".to_string()
        } else {
            format!("{changed} file(s) changed")
        };

        format!("\n\n## Git Context\nBranch: {branch}\nWorking tree: {status_str}")
    }

    /// Assemble messages for the LLM call.
    async fn assemble_messages(&mut self) -> MaixResult<Vec<Message>> {
        let memory_ctx = self
            .memory
            .get_context_for_session(&self.session.id, 4000)
            .await
            .unwrap_or_default();

        let system_prompt = self.build_system_prompt(&memory_ctx);
        let mut messages = vec![Message {
            role: Role::System,
            content: MessageContent::Text(system_prompt),
            name: None,
            tool_call_id: None,
            tool_calls: None,
            reasoning_content: None,
        }];

        // Safety: truncate if messages exceed limit, avoiding orphan tool results
        let max_messages = 100;
        let history = if self.session.messages.len() > max_messages {
            let truncated = &self.session.messages[self.session.messages.len() - max_messages..];
            // Skip leading Tool messages that lack their parent Assistant message
            let mut start = 0;
            while start < truncated.len() && truncated[start].role == Role::Tool {
                start += 1;
            }
            &truncated[start..]
        } else {
            &self.session.messages
        };
        messages.extend(history.iter().cloned());
        Ok(messages)
    }

    /// Check if context exceeds threshold and auto-compact if needed.
    /// Uses multiple trigger conditions for smarter compaction timing.
    async fn maybe_compact_context(&mut self) -> MaixResult<()> {
        let context_window = self.provider.context_window() as f64;
        let threshold = self.config.context_threshold as f64;
        let current = self.session.total_tokens as f64;

        // Trigger 1: Token threshold exceeded
        let token_trigger = current > context_window * threshold;

        // Trigger 2: Too many messages
        let msg_trigger = self.session.messages.len() > 200;

        // Trigger 3: Tool output accumulation (> 25% of context)
        let tool_output_tokens: u64 = self.session.messages.iter()
            .filter(|m| m.role == Role::Tool)
            .map(|m| m.content.text().unwrap_or("").len() as u64 / 4)
            .sum();
        let tool_trigger = tool_output_tokens as f64 > context_window * 0.25;

        if (token_trigger || msg_trigger || tool_trigger) && self.session.messages.len() > 8 {
            let reason = if token_trigger {
                format!("{}% context used", (current / context_window * 100.0) as u32)
            } else if msg_trigger {
                format!("{} messages", self.session.messages.len())
            } else {
                format!("{}K tool output tokens", tool_output_tokens / 1000)
            };

            tracing::info!("Context compaction triggered: {}", reason);
            self.compact_context().await?;
        }
        Ok(())
    }

    /// Public entry point: compact context with optional user instructions.
    pub async fn compact(&mut self, instructions: Option<&str>) -> MaixResult<()> {
        self.compact_context_internal(instructions).await
    }

    /// Compact context by summarizing older messages and keeping recent ones.
    /// Preserves system messages and tool call/result pairs.
    async fn compact_context(&mut self) -> MaixResult<()> {
        // Pre-compaction: apply lightweight strategies first
        self.pre_compact();

        self.compact_context_internal(None).await
    }

    /// Lightweight pre-compaction: truncate long tool outputs, remove duplicates.
    /// Returns the number of tokens saved.
    fn pre_compact(&mut self) -> u64 {
        let mut tokens_saved = 0u64;

        // Strategy 1: Truncate long tool outputs (> 500 chars)
        for msg in &mut self.session.messages {
            if msg.role == Role::Tool {
                if let Some(text) = msg.content.text() {
                    if text.len() > 500 {
                        let truncated = format!(
                            "{}\n... [truncated, {} chars total]",
                            &text[..300.min(text.len())],
                            text.len()
                        );
                        let old_tokens = text.len() as u64 / 4;
                        let new_tokens = truncated.len() as u64 / 4;
                        tokens_saved += old_tokens.saturating_sub(new_tokens);
                        msg.content = MessageContent::Text(truncated);
                    }
                }
            }
        }

        // Strategy 2: Remove duplicate tool calls (same name + same args, keep last result)
        let mut seen_tool_calls: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        let mut to_remove = Vec::new();

        for (i, msg) in self.session.messages.iter().enumerate() {
            if msg.role == Role::Tool {
                if let Some(text) = msg.content.text() {
                    // Create a key from the tool call context
                    let key = format!("tool:{}", text.len());
                    if let Some(&prev_idx) = seen_tool_calls.get(&key) {
                        // Mark the older duplicate for removal
                        to_remove.push(prev_idx);
                    }
                    seen_tool_calls.insert(key, i);
                }
            }
        }

        // Remove duplicates (in reverse order to preserve indices)
        to_remove.sort_unstable();
        to_remove.dedup();
        for &idx in to_remove.iter().rev() {
            if idx < self.session.messages.len() {
                let removed = self.session.messages.remove(idx);
                tokens_saved += removed.content.text().unwrap_or("").len() as u64 / 4;
            }
        }

        if tokens_saved > 0 {
            tracing::info!("Pre-compaction saved ~{} tokens", tokens_saved);
        }

        tokens_saved
    }

    async fn compact_context_internal(&mut self, instructions: Option<&str>) -> MaixResult<()> {
        // Find a safe split point: don't split tool call/result pairs
        let total = self.session.messages.len();
        let mut split_point = total.saturating_sub(6);

        // Ensure we don't split in the middle of a tool call sequence
        // If the message at split_point is a Tool result, move back to include the assistant message with tool_calls
        while split_point > 0 && self.session.messages[split_point].role == Role::Tool {
            split_point -= 1;
        }
        // If the message at split_point is an Assistant with tool_calls, include the next Tool results
        if split_point < total {
            if let Some(ref tool_calls) = self.session.messages[split_point].tool_calls {
                if !tool_calls.is_empty() {
                    // Move forward past all tool results for these calls
                    let expected_ids: Vec<String> = tool_calls.iter().map(|tc| tc.id.clone()).collect();
                    let mut end = split_point + 1;
                    while end < total && self.session.messages[end].role == Role::Tool {
                        if let Some(ref id) = self.session.messages[end].tool_call_id {
                            if expected_ids.contains(id) {
                                end += 1;
                                continue;
                            }
                        }
                        break;
                    }
                    split_point = end;
                }
            }
        }

        // Collect messages to summarize (exclude system messages)
        let mut old_messages = Vec::new();
        let mut system_prefix = Vec::new();
        for (i, msg) in self.session.messages[..split_point].iter().enumerate() {
            if i == 0 && msg.role == Role::System {
                system_prefix.push(msg.clone());
            } else if msg.role == Role::System {
                // Keep system messages but don't summarize them
                system_prefix.push(msg.clone());
            } else {
                old_messages.push(msg.clone());
            }
        }

        let recent: Vec<Message> = self.session.messages[split_point..].to_vec();

        let summary_text = if old_messages.is_empty() {
            String::new()
        } else {
            self.summarize_messages(&old_messages, instructions).await?
        };

        let mut new_messages = system_prefix;
        if !summary_text.is_empty() {
            new_messages.push(Message {
                role: Role::System,
                content: MessageContent::Text(format!(
                    "[Previous conversation summary]\n{}",
                    summary_text
                )),
                name: None,
                tool_call_id: None,
                tool_calls: None,
                reasoning_content: None,
            });
        }
        new_messages.extend(recent);

        self.session.messages = new_messages;

        // Recalculate token estimate
        self.session.total_tokens = self
            .session
            .messages
            .iter()
            .map(|m| {
                m.content
                    .text()
                    .unwrap_or("")
                    .len() as u64
                    / 4
            })
            .sum();

        tracing::info!(
            "Context compacted: summarized {} old messages, kept {} recent",
            split_point,
            self.session.messages.len() - 1,
        );
        Ok(())
    }

    /// Use LLM to summarize a set of messages.
    async fn summarize_messages(&mut self, messages: &[Message], instructions: Option<&str>) -> MaixResult<String> {
        let conversation: Vec<String> = messages
            .iter()
            .filter_map(|m| {
                let text = m.content.text()?;
                if text.is_empty() {
                    return None;
                }
                let role = match m.role {
                    Role::User => "User",
                    Role::Assistant => "Assistant",
                    Role::Tool => "Tool",
                    Role::System => "System",
                };
                Some(format!("{role}: {text}"))
            })
            .collect();

        let instruction_text = instructions
            .map(|i| format!(" User focus instructions: {i}\n"))
            .unwrap_or_default();
        let prompt = format!(
            "{instruction_text}Summarize the following conversation concisely. Preserve key facts, decisions, code changes, and context that would be needed to continue the conversation:\n\n{}",
            conversation.join("\n\n")
        );

        let req = ChatRequest {
            messages: vec![
                Message {
                    role: Role::System,
                    content: MessageContent::Text(
                        "You are a conversation summarizer. Be concise but comprehensive."
                            .into(),
                    ),
                    name: None,
                    tool_call_id: None,
                    tool_calls: None,
                    reasoning_content: None,
                },
                Message {
                    role: Role::User,
                    content: MessageContent::Text(prompt),
                    name: None,
                    tool_call_id: None,
                    tool_calls: None,
                    reasoning_content: None,
                },
            ],
            tools: None,
            tool_choice: None,
            temperature: Some(0.3),
            max_tokens: Some(1024),
            model_override: None,
        };

        let stream = self.provider.chat_stream(req).await?;
        let (text, _usage, _tool_calls, _reasoning) =
            self.process_stream(stream, None).await?;
        Ok(text)
    }

    /// Stream chunks, yielding events and collecting the final message.
    async fn process_stream(
        &mut self,
        stream: ChatStream,
        tx: Option<&tokio::sync::mpsc::UnboundedSender<AgentEvent>>,
    ) -> MaixResult<(String, TokenUsage, Vec<ToolCall>, String)> {
        let mut text = String::new();
        let mut reasoning = String::new();
        let mut tool_calls_map: HashMap<u32, (Option<String>, Option<String>, String)> =
            HashMap::new();
        let mut usage = TokenUsage::default();

        tokio::pin!(stream);
        use futures::StreamExt;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            if let Some(u) = chunk.usage {
                usage = u;
            }
            for choice in chunk.choices {
                if let Some(delta) = choice.delta {
                    if let Some(c) = delta.content {
                        text.push_str(&c);
                        if let Some(tx) = &tx {
                            let _ = tx.send(AgentEvent::TextDelta(c));
                        }
                    }
                    if let Some(r) = delta.reasoning_content {
                        reasoning.push_str(&r);
                        if let Some(tx) = &tx {
                            let _ = tx.send(AgentEvent::ReasoningDelta(r));
                        }
                    }
                    if let Some(tcs) = delta.tool_calls {
                        for tc in tcs {
                            let entry = tool_calls_map
                                .entry(tc.index)
                                .or_insert_with(|| (tc.id.clone(), None, String::new()));
                            if let Some(ref f) = tc.function {
                                if let Some(name) = &f.name {
                                    entry.1 = Some(name.clone());
                                }
                                if let Some(args) = &f.arguments {
                                    entry.2.push_str(args);
                                }
                            }
                        }
                    }
                }
            }
        }

        let tool_calls: Vec<ToolCall> = {
            let mut sorted: Vec<_> = tool_calls_map.into_iter().collect();
            sorted.sort_by_key(|(k, _)| *k);
            sorted
                .into_iter()
                .filter_map(|(_, (id, name, args))| {
                    if let (Some(id), Some(name)) = (id, name) {
                        Some(ToolCall {
                            id,
                            call_type: "function".into(),
                            function: maix_core::FunctionCall {
                                name,
                                arguments: args,
                            },
                        })
                    } else {
                        None
                    }
                })
                .collect()
        };

        Ok((text, usage, tool_calls, reasoning))
    }

    /// Main run loop — call for each user input.
    /// Returns AgentEvents via the optional channel.
    /// When `approval_rx` is Some, tool calls with `needs_approval==true` will
    /// pause and wait for a (tool_call_id, approved) message. When None, all
    /// tools auto-execute.
    pub async fn run(
        &mut self,
        user_input: &str,
        tx: Option<tokio::sync::mpsc::UnboundedSender<AgentEvent>>,
        mut approval_rx: Option<tokio::sync::mpsc::UnboundedReceiver<(String, bool)>>,
    ) -> MaixResult<String> {
        self.state = AgentState::Thinking;
        if let Some(ref tx) = tx {
            let _ = tx.send(AgentEvent::Thinking);
        }

        // Add user message
        self.session
            .add_message(Role::User, user_input, 1 + user_input.len() as u64 / 4);

        let mut final_text = String::new();
        let mut final_usage = TokenUsage::default();
        let tool_defs: Vec<maix_core::ToolDef> = self
            .tools
            .get_defs()
            .iter()
            .filter(|d| self.config.mode != AgentMode::Plan || d.risk_level == RiskLevel::ReadOnly)
            .map(|d| d.to_openai())
            .collect();

        // Tool calling loop
        for _round in 0..self.config.max_tool_rounds {
            // Check context threshold and auto-compact if needed
            self.maybe_compact_context().await?;

            let messages = self.assemble_messages().await?;

            // Auto-mode routing: decide which model to use this turn
            let model_override = if let Some(ref router) = self.auto_router {
                let decision = router.decide(
                    user_input,
                    self.session.total_tokens,
                    self.provider.context_window() as u64,
                );
                tracing::info!(
                    "Auto-mode routing: {} → {} ({})",
                    decision.reason,
                    decision.route.model,
                    decision.thinking_level as u8,
                );
                Some(decision.route.model.clone())
            } else {
                None
            };

            let req = ChatRequest {
                messages,
                tools: if tool_defs.is_empty() {
                    None
                } else {
                    Some(tool_defs.clone())
                },
                tool_choice: None,
                temperature: Some(0.7),
                max_tokens: Some(4096),
                model_override,
            };

            let stream = self.provider.chat_stream(req).await?;
            let (text, usage, tool_calls, reasoning) =
                self.process_stream(stream, tx.as_ref()).await?;
            final_usage.prompt_tokens += usage.prompt_tokens;
            final_usage.completion_tokens += usage.completion_tokens;
            final_usage.total_tokens += usage.total_tokens;
            final_usage.cache_read_tokens += usage.cache_read_tokens;
            final_usage.cache_write_tokens += usage.cache_write_tokens;
            // Write real token usage back to session (replaces heuristic)
            if final_usage.total_tokens > 0 {
                self.session.total_tokens = final_usage.total_tokens;
            }

            if !tool_calls.is_empty() {
                // Add assistant message with tool_calls
                let tool_reasoning = if reasoning.is_empty() { None } else { Some(reasoning.clone()) };
                self.session.add_assistant_tool_calls(&tool_calls, tool_reasoning);

                self.state = AgentState::ExecutingTool;

                // Categorize tool calls: denied, auto-approved, needs-approval
                let mut denied: Vec<(&ToolCall, String)> = Vec::new();
                let mut auto_approved: Vec<&ToolCall> = Vec::new();
                let mut needs_approval: Vec<&ToolCall> = Vec::new();

                for tc in &tool_calls {
                    let args: Value =
                        serde_json::from_str(&tc.function.arguments).unwrap_or(Value::Null);

                    // Permission check: deny rules reject, allow rules auto-approve,
                    // fallback to RiskLevel default
                    let is_read_only = self.tools.get(&tc.function.name)
                        .map(|t| t.def().risk_level == RiskLevel::ReadOnly)
                        .unwrap_or(false);
                    let risk_fallback = if is_read_only {
                        maix_core::permissions::PermissionDecision::Allowed
                    } else {
                        maix_core::permissions::PermissionDecision::AskUser
                    };
                    let perm_decision = self.permissions.check(
                        &tc.function.name,
                        &tc.function.arguments,
                        risk_fallback,
                    );

                    let needs = self.config.mode == AgentMode::Agent
                        && perm_decision == maix_core::permissions::PermissionDecision::AskUser;

                    if let Some(ref tx) = tx {
                        let _ = tx.send(AgentEvent::ToolCallRequested {
                            id: tc.id.clone(),
                            name: tc.function.name.clone(),
                            args: args.clone(),
                            needs_approval: needs,
                        });
                    }

                    match perm_decision {
                        maix_core::permissions::PermissionDecision::Denied(reason) => {
                            denied.push((tc, reason));
                        }
                        maix_core::permissions::PermissionDecision::Allowed => {
                            auto_approved.push(tc);
                        }
                        maix_core::permissions::PermissionDecision::AskUser => {
                            if needs {
                                needs_approval.push(tc);
                            } else {
                                auto_approved.push(tc);
                            }
                        }
                    }
                }

                // Batch approval: show all pending tools, get approval for each
                let mut approved_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
                if !needs_approval.is_empty() {
                    if let Some(ref mut rx) = approval_rx {
                        if let Some(ref tx) = tx {
                            let _ = tx.send(AgentEvent::WaitingApproval);
                        }
                        // Wait for approval for each tool that needs it
                        for tc in &needs_approval {
                            match rx.recv().await {
                                Some((id, appr)) if id == tc.id => {
                                    if let Some(ref tx) = tx {
                                        if appr {
                                            let _ = tx.send(AgentEvent::ToolCallApproved);
                                            approved_ids.insert(tc.id.clone());
                                        } else {
                                            let _ = tx.send(AgentEvent::ToolCallDenied("denied by user".into()));
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    } else {
                        // Auto-approve when no approval channel
                        for tc in &needs_approval {
                            approved_ids.insert(tc.id.clone());
                        }
                    }
                }

                // All auto-approved tools are always approved
                for tc in &auto_approved {
                    approved_ids.insert(tc.id.clone());
                }

                // Execute ALL approved tools in parallel, with PreToolUse/PostToolUse hooks
                let ctx = &self.tool_ctx;
                let tools = &self.tools;
                let hooks = &self.hooks;
                let working_dir = &self.tool_ctx.working_dir;
                let execution_futures: Vec<_> = tool_calls.iter()
                    .filter(|tc| approved_ids.contains(&tc.id))
                    .map(|tc| {
                        let args: Value =
                            serde_json::from_str(&tc.function.arguments).unwrap_or(Value::Null);
                        let name = tc.function.name.clone();
                        let id = tc.id.clone();
                        let args_str = tc.function.arguments.clone();
                        async move {
                            // PreToolUse hook — can block execution
                            if let Err(block) = hooks.run_pre_tool(&name, &args_str, working_dir).await {
                                return (id, format!("Tool blocked by hook: {}", block.reason));
                            }

                            let result = match tools.get(&name) {
                                Some(tool) => match tool.execute(ctx, args).await {
                                    Ok(r) => r,
                                    Err(e) => format!("Tool error: {e}"),
                                },
                                None => format!("Unknown tool: {name}"),
                            };

                            // PostToolUse hook — fire and forget
                            hooks.run_post_tool(&name, &args_str, &result, working_dir).await;

                            (id, result)
                        }
                    })
                    .collect();

                let results = futures::future::join_all(execution_futures).await;

                // Handle denied tools (by permission rules or user rejection)
                for (tc, reason) in &denied {
                    if let Some(ref tx) = tx {
                        let _ = tx.send(AgentEvent::ToolResult {
                            id: tc.id.clone(),
                            result: format!("Tool denied: {reason}"),
                        });
                    }
                    self.session.add_tool_result(&tc.id, &format!("Tool denied: {reason}"));
                }
                // Also handle user-rejected tools from approval flow
                for tc in &tool_calls {
                    if !approved_ids.contains(&tc.id) && !denied.iter().any(|(d, _)| d.id == tc.id) {
                        if let Some(ref tx) = tx {
                            let _ = tx.send(AgentEvent::ToolResult {
                                id: tc.id.clone(),
                                result: "Tool execution denied by user".to_string(),
                            });
                        }
                        self.session.add_tool_result(&tc.id, "Tool execution denied by user");
                    }
                }

                // Emit results and add to session
                for (id, result) in results {
                    if let Some(ref tx) = tx {
                        let _ = tx.send(AgentEvent::ToolResult {
                            id: id.clone(),
                            result: result.clone(),
                        });
                    }
                    self.session.add_tool_result(&id, &result);
                }

                // Continue loop for next LLM call
                continue;
            }

            // No tool calls — this is the final response
            final_text = text;

            // Add assistant message with reasoning
            let reasoning_str = if reasoning.is_empty() { None } else { Some(reasoning) };
            self.session.add_message_with_reasoning(
                Role::Assistant,
                &final_text,
                reasoning_str,
                final_text.len() as u64 / 4,
            );

            if let Some(ref tx) = tx {
                let _ = tx.send(AgentEvent::ResponseComplete {
                    text: final_text.clone(),
                    usage: final_usage.clone(),
                });
            }

            self.state = AgentState::Responding;
            break;
        }

        // If loop exhausted without a final response, warn the user
        if final_text.is_empty() {
            final_text = format!(
                "Reached maximum tool calling rounds ({}). The task may be incomplete.",
                self.config.max_tool_rounds
            );
            tracing::warn!("Agent loop exhausted {} rounds", self.config.max_tool_rounds);
            if let Some(ref tx) = tx {
                let _ = tx.send(AgentEvent::ResponseComplete {
                    text: final_text.clone(),
                    usage: final_usage.clone(),
                });
            }
        }

        // Update memory after response
        self.state = AgentState::UpdatingMemory;
        if self.config.auto_memory_summary && !final_text.is_empty() {
            let summary = MemoryEntry {
                id: format!("ep_{}", uuid::Uuid::new_v4()),
                content: format!(
                    "User: {}\nAssistant: {}",
                    user_input,
                    &final_text[..final_text.len().min(500)]
                ),
                kind: MemoryKind::Episodic,
                importance: 0.7,
                created_at: chrono::Utc::now(),
                metadata: {
                    let mut m = HashMap::new();
                    m.insert("session_id".to_string(), self.session.id.clone());
                    m
                },
            };
            let _ = self.memory.save(summary).await;
            if let Some(ref tx) = tx {
                let _ = tx.send(AgentEvent::MemoryUpdated {
                    summary: "Conversation recorded".into(),
                });
            }
        }

        // Run Stop hooks
        self.hooks.run_stop(&self.tool_ctx.working_dir).await;

        self.state = AgentState::Idle;
        Ok(final_text)
    }
}
