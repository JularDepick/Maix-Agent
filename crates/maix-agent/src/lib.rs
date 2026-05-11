//! Agent core loop and state machine (Phase 1).
//!
//! Also provides the `Maix` facade — the central orchestrator that ties together
//! config, providers, models, memory, tools, skills, and identities.
//! All entry points (CLI, TUI, Gateway) drive the system through `Maix`.

use maix_core::{MaixResult, Message, MessageContent, Role, ToolCall, TokenUsage};
use maix_memory::{MemoryEntry, MemoryKind, MemoryStore};
use maix_provider::{ChatRequest, ChatStream, LLMProvider};
use maix_tools::{RiskLevel, ToolCtx, ToolRegistry};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

pub mod orchestrator;
pub mod router;
pub mod runtime;
pub mod session;
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
        );

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
    pub fn build_router(cfg: &maix_core::Config, default_provider: &str) -> maix_core::ModelRouter {
        let mut router = maix_core::ModelRouter::new(default_provider, default_provider);
        for (name, pc) in &cfg.providers {
            if pc.api_key.is_empty() {
                continue;
            }
            let model = pc.model.clone().unwrap_or_else(|| name.clone());
            if name.contains("deepseek") || model.contains("deepseek") {
                if model.contains("pro") || model.contains("v4") || model.contains("v3") {
                    router = router.with_route(maix_core::TaskCategory::Reasoning, name, model.clone());
                    router = router.with_route(maix_core::TaskCategory::Research, name, model);
                } else {
                    router = router.with_route(maix_core::TaskCategory::Coding, name, model.clone());
                    router = router.with_route(maix_core::TaskCategory::FastReply, name, model);
                }
            }
        }
        router
    }

    /// Build an LLM provider from config.
    pub fn build_provider(
        cfg: &maix_core::Config,
        name: &str,
    ) -> MaixResult<Arc<dyn LLMProvider>> {
        if let Some(pc) = cfg.providers.get(name) {
            let api_key = if pc.api_key.is_empty() {
                std::env::var(format!("MAIX_PROVIDERS_{}_API_KEY", name.to_uppercase()))
                    .unwrap_or_default()
            } else {
                pc.api_key.clone()
            };
            if api_key.is_empty() {
                return Err(maix_core::MaixError::Provider(format!(
                    "No API key for provider '{name}'. Set via config or env var."
                )));
            }
            let model = pc.model.clone().unwrap_or_else(|| name.to_string());
            let mut provider = maix_provider::OpenAICompatProvider::new(
                pc.api_base.clone(),
                api_key,
                model,
            )
            .with_context_window(1_000_000);
            if name.contains("deepseek") {
                provider = provider.with_reasoning();
                provider = provider.with_body_field(
                    "reasoning_effort",
                    serde_json::json!("medium"),
                );
            }
            return Ok(Arc::new(provider));
        }
        // Fallback
        let env_key = format!("MAIX_PROVIDERS_{}_API_KEY", name.to_uppercase());
        let api_key = std::env::var(&env_key).unwrap_or_default();
        if api_key.is_empty() {
            return Err(maix_core::MaixError::Provider(format!(
                "No provider config for '{name}' and {env_key} not set"
            )));
        }
        Ok(Arc::new(
            maix_provider::OpenAICompatProvider::new(
                "https://api.deepseek.com".into(),
                api_key,
                name.to_string(),
            )
            .with_context_window(1_000_000),
        ))
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentState {
    Idle,
    Thinking,
    ExecutingTool,
    WaitingApproval,
    Responding,
    UpdatingMemory,
    Errored,
    Paused,
}

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

pub const DEFAULT_SYSTEM_PROMPT: &str = "\
You are Maix, an intelligent AI agent with persistent memory. \
You can use tools to read/write files, run shell commands, and fetch web content. \
\n\n{m tools_section}\
\n\n## Memory Context\n{memory_context}\
\n\nCurrent date: {current_date}\
\n\nBe concise and helpful. When you need information, use tools. \
After important discussions, note key facts so they can be remembered.";

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
            },
        }
    }

    pub fn state(&self) -> AgentState {
        self.state
    }

    /// Build the system prompt from template.
    fn build_system_prompt(&self, memory_context: &str) -> String {
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
                "Available tools:\n{}\n\nUse tool_calls to invoke them. Multiple tools may be called in one response.",
                tool_list.join("\n")
            )
        };

        self.config
            .system_prompt_template
            .replace("{tools_section}", &tools_section)
            .replace("{memory_context}", memory_context)
            .replace("{current_date}", &chrono::Utc::now().format("%Y-%m-%d").to_string())
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

        // Append conversation history
        messages.extend(self.session.messages.iter().cloned());
        Ok(messages)
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
            let messages = self.assemble_messages().await?;

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
            };

            let stream = self.provider.chat_stream(req).await?;
            let (text, usage, tool_calls, reasoning) =
                self.process_stream(stream, tx.as_ref()).await?;
            final_usage.prompt_tokens += usage.prompt_tokens;
            final_usage.completion_tokens += usage.completion_tokens;
            final_usage.total_tokens += usage.total_tokens;

            if !tool_calls.is_empty() {
                // Add assistant message with tool_calls
                let mut metadata = HashMap::new();
                metadata.insert("session_id".to_string(), self.session.id.clone());

                let tool_reasoning = if reasoning.is_empty() { None } else { Some(reasoning.clone()) };
                self.session.add_assistant_tool_calls(&tool_calls, tool_reasoning);

                for tc in &tool_calls {
                    self.state = AgentState::ExecutingTool;
                    let args: Value =
                        serde_json::from_str(&tc.function.arguments).unwrap_or(Value::Null);

                    let needs_approval = self.config.mode == AgentMode::Agent
                        && self
                            .tools
                            .get(&tc.function.name)
                            .map(|t| t.def().risk_level.needs_approval(true))
                            .unwrap_or(false);

                    if let Some(ref tx) = tx {
                        let _ = tx.send(AgentEvent::ToolCallRequested {
                            id: tc.id.clone(),
                            name: tc.function.name.clone(),
                            args: args.clone(),
                            needs_approval,
                        });
                    }

                    // Approval gate (gRPC / interactive mode)
                    let approved = if needs_approval {
                        if let Some(ref mut rx) = approval_rx {
                            if let Some(ref tx) = tx {
                                let _ = tx.send(AgentEvent::WaitingApproval);
                            }
                            match rx.recv().await {
                                Some((id, appr)) if id == tc.id => {
                                    if let Some(ref tx) = tx {
                                        if appr {
                                            let _ = tx.send(AgentEvent::ToolCallApproved);
                                        } else {
                                            let _ = tx.send(AgentEvent::ToolCallDenied("denied by user".into()));
                                        }
                                    }
                                    appr
                                }
                                _ => false,
                            }
                        } else {
                            true // auto-approve when no approval channel
                        }
                    } else {
                        true
                    };

                    let result = if approved {
                        match self.tools.get(&tc.function.name) {
                            Some(tool) => tool.execute(&self.tool_ctx, args).await?,
                            None => format!("Unknown tool: {}", tc.function.name),
                        }
                    } else {
                        "Tool execution denied by user".to_string()
                    };

                    if let Some(ref tx) = tx {
                        let _ = tx.send(AgentEvent::ToolResult {
                            id: tc.id.clone(),
                            result: result.clone(),
                        });
                    }

                    // Add tool result message with tool_call_id
                    self.session.add_tool_result(&tc.id, &result);
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

        self.state = AgentState::Idle;
        Ok(final_text)
    }
}
