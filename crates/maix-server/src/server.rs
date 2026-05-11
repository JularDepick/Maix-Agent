use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use futures::Stream;
use tokio::sync::{mpsc, RwLock, Semaphore};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;
use tonic::{Request, Response, Status, Streaming};

use maix_agent::{Agent, AgentConfig, AgentEvent};
use maix_core::proto::maix::core::v1::core_service_server::CoreService;
use maix_core::proto::maix::core::v1 as pb;
use maix_core::{json_to_prost_struct, prost_struct_to_json, Architecture, Config, IdentityManager};
use maix_memory::{FileMemoryStore, MemoryStore, SharedMemoryProxy};
use maix_monitor::{EventBus, Monitor};
use maix_provider::LLMProvider;
use maix_skills::SkillRegistry;
use maix_task_queue::{InsertAt, Task, TaskQueue};
use maix_tools::{RiskLevel, ToolRegistry};

use crate::chat_stream::{self, agent_event_to_chat_output};
use crate::session_manager::{SessionHandle, SessionMeta, SessionStore};

const MAX_CONCURRENT_REQUESTS: usize = 256;

// ---------------------------------------------------------------------------
// ServerCore
// ---------------------------------------------------------------------------

pub struct ServerCore {
    #[allow(dead_code)]
    pub config: Config,
    pub provider: Arc<dyn LLMProvider>,
    pub event_bus: Arc<EventBus>,
    pub monitor: Arc<RwLock<Monitor>>,
    pub memory: Arc<RwLock<Box<dyn MemoryStore>>>,
    pub tools: Arc<ToolRegistry>,
    pub queue: Arc<RwLock<TaskQueue>>,
    pub skills: Arc<RwLock<SkillRegistry>>,
    pub identities: RwLock<IdentityManager>,
    pub architectures: RwLock<Vec<Architecture>>,
    pub sessions: SessionStore,
    pub cancel_root: CancellationToken,
    pub start_time: Instant,
    pub semaphore: Semaphore,
    pub shutdown_flag: AtomicBool,
}

impl ServerCore {
    pub async fn from_config(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        let provider: Arc<dyn LLMProvider> = {
            let default = config
                .providers
                .iter()
                .find(|(_, pc)| !pc.api_key.is_empty())
                .map(|(k, _)| k.clone())
                .or_else(|| config.providers.keys().next().cloned())
                .unwrap_or_else(|| "deepseek".into());
            let pc = config.providers.get(&default);
            let api_key = pc.map(|p| p.api_key.clone()).unwrap_or_default();
            let api_base = pc
                .map(|p| p.api_base.clone())
                .unwrap_or_else(|| "https://api.deepseek.com".into());
            let model = pc
                .and_then(|p| p.model.clone())
                .unwrap_or_else(|| default.clone());
            let mut p = maix_provider::OpenAICompatProvider::new(api_base, api_key, model)
                .with_context_window(1_000_000);
            if default.contains("deepseek") {
                p = p.with_reasoning();
            }
            Arc::new(p)
        };

        let memory_dir = if config.memory.dir.as_os_str().is_empty() {
            maix_core::config::default_memory_dir()
        } else {
            config.memory.dir.clone()
        };
        let memory: Box<dyn MemoryStore> = Box::new(FileMemoryStore::new(memory_dir)?);
        let memory = Arc::new(RwLock::new(memory));

        let event_bus = Arc::new(EventBus::new(256));
        let monitor = Arc::new(RwLock::new(Monitor::new(event_bus.clone())));

        // Background subscriber: feed event_bus events into the monitor
        {
            let mut bus_rx = event_bus.subscribe();
            let mon = monitor.clone();
            tokio::spawn(async move {
                loop {
                    match bus_rx.recv().await {
                        Ok(event) => {
                            mon.write().await.handle_event(&event);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!("monitor event bus lagged by {n}");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            });
        }

        let skills_dir = maix_core::config::default_memory_dir()
            .parent()
            .unwrap_or(&std::path::PathBuf::from("."))
            .join("skills");
        let skills = Arc::new(RwLock::new(SkillRegistry::new(skills_dir)));

        Ok(Self {
            config,
            provider,
            event_bus,
            monitor,
            memory,
            tools: Arc::new(ToolRegistry::with_builtins()),
            queue: Arc::new(RwLock::new(TaskQueue::new())),
            skills,
            identities: RwLock::new(IdentityManager::new().with_defaults()),
            architectures: RwLock::new(vec![
                Architecture::sequential("sequential"),
                Architecture::debate("debate", 2),
                Architecture::router("router"),
            ]),
            sessions: SessionStore::new(),
            cancel_root: CancellationToken::new(),
            start_time: Instant::now(),
            semaphore: Semaphore::new(MAX_CONCURRENT_REQUESTS),
            shutdown_flag: AtomicBool::new(false),
        })
    }

    pub async fn build_agent(&self, session_id: &str, workdir: std::path::PathBuf) -> Agent {
        let memory_proxy = SharedMemoryProxy::new(self.memory.clone());
        Agent::new(
            AgentConfig::default(),
            self.provider.clone(),
            self.tools.clone(),
            Box::new(memory_proxy),
            session_id.to_string(),
            workdir,
        )
    }
}

// ---------------------------------------------------------------------------
// Newtype wrapper to satisfy orphan rules for tonic's CoreService trait
// ---------------------------------------------------------------------------

pub struct MaixCoreService(pub Arc<ServerCore>);

#[tonic::async_trait]
impl CoreService for MaixCoreService {
    type ChatStream =
        Pin<Box<dyn Stream<Item = Result<pb::ChatOutput, Status>> + Send + 'static>>;

    async fn chat(
        &self,
        request: Request<Streaming<pb::ChatInput>>,
    ) -> Result<Response<Self::ChatStream>, Status> {
        let _permit = self
            .0
            .semaphore
            .acquire()
            .await
            .map_err(|_| Status::resource_exhausted("too many concurrent requests"))?;

        let mut in_stream = request.into_inner();
        let (session_id, text, _identity) =
            chat_stream::read_first_user_message(&mut in_stream).await?;

        // Get or create session
        let handle = match self.0.sessions.get(&session_id).await {
            Some(h) => h,
            None => {
                let working_dir = std::env::current_dir().unwrap_or_default();
                let agent = self.0.build_agent(&session_id, working_dir).await;
                let now = chrono::Utc::now().to_rfc3339();
                let meta = SessionMeta {
                    id: session_id.clone(),
                    name: session_id.clone(),
                    created_at: now.clone(),
                    updated_at: now,
                    message_count: 0,
                };
                let h = SessionHandle {
                    agent: Arc::new(tokio::sync::Mutex::new(Some(agent))),
                    meta,
                    cancel: CancellationToken::new(),
                };
                self.0.sessions.insert(session_id.clone(), h.clone()).await;
                h
            }
        };

        // Increment message count for the first user message
        self.0.sessions.increment_message_count(&session_id).await;

        // Take agent from slot
        let agent = {
            let mut lock = handle.agent.lock().await;
            lock.take()
                .ok_or_else(|| Status::failed_precondition("session is busy"))?
        };
        let agent = Arc::new(tokio::sync::Mutex::new(agent));

        // Event bridge
        let (event_tx, mut event_rx) = mpsc::unbounded_channel::<AgentEvent>();
        let (out_tx, out_rx) = mpsc::channel::<Result<pb::ChatOutput, Status>>(128);
        let (approval_tx, approval_rx) = mpsc::unbounded_channel::<(String, bool)>();

        let sid = session_id.clone();
        let cancel = self.0.cancel_root.child_token();

        // Spawn agent run
        let run_agent = agent.clone();
        let run_cancel = cancel.clone();
        let run_handle = tokio::spawn(async move {
            let mut agent_guard = run_agent.lock().await;
            tokio::select! {
                res = agent_guard.run(&text, Some(event_tx), Some(approval_rx)) => {
                    res
                }
                _ = run_cancel.cancelled() => {
                    Err(maix_core::MaixError::Cancelled)
                }
            }
        });

        // Spawn outbound bridge
        let out_tx_clone = out_tx.clone();
        let out_sid = sid.clone();
        let out_cancel = cancel.clone();
        let out_bus = self.0.event_bus.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    event = event_rx.recv() => {
                        match event {
                            Some(e) => {
                                // Emit TokenUsed to event bus for monitor tracking
                                if let AgentEvent::ResponseComplete { ref usage, .. } = &e {
                                    let _ = out_bus.sender().send(maix_monitor::AgentEvent::TokenUsed {
                                        agent_id: out_sid.clone(),
                                        prompt_tokens: usage.prompt_tokens,
                                        completion_tokens: usage.completion_tokens,
                                        cost_estimate: 0.0,
                                    });
                                }
                                let msg = agent_event_to_chat_output(&out_sid, e);
                                if out_tx_clone.send(Ok(msg)).await.is_err() {
                                    break;
                                }
                            }
                            None => break,
                        }
                    }
                    _ = out_cancel.cancelled() => {
                        let _ = out_tx_clone.send(Err(Status::cancelled("request cancelled"))).await;
                        break;
                    }
                }
            }
        });

        // Spawn inbound forwarder
        let inb_cancel = cancel.clone();
        let core = self.0.clone();
        let inb_sid = sid.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    event = chat_stream::read_next_inbound(&mut in_stream) => {
                        match event {
                            Ok(Some(chat_stream::InboundEvent::ToolApproval { tool_call_id, approved })) => {
                                let _ = approval_tx.send((tool_call_id, approved));
                            }
                            Ok(Some(chat_stream::InboundEvent::Cancel)) => {
                                inb_cancel.cancel();
                                break;
                            }
                            Ok(Some(chat_stream::InboundEvent::SetMode(_mode))) => {
                                // Applied after run completes
                            }
                            Ok(Some(chat_stream::InboundEvent::UserMessage { .. })) => {
                                core.sessions.increment_message_count(&inb_sid).await;
                            }
                            Ok(None) | Err(_) => break,
                        }
                    }
                    _ = inb_cancel.cancelled() => break,
                }
            }
        });

        // Wait for agent run
        match run_handle.await {
            Ok(Ok(_text)) => {}
            Ok(Err(e)) => {
                let _ = out_tx
                    .send(Ok(agent_event_to_chat_output(
                        &sid,
                        AgentEvent::Error(e.to_string()),
                    )))
                    .await;
            }
            Err(join_err) => {
                tracing::error!("agent task panicked: {join_err}");
            }
        }

        // Put agent back into session slot
        let inner_agent = match Arc::try_unwrap(agent) {
            Ok(m) => m.into_inner(),
            Err(_) => {
                tracing::warn!("agent had outstanding references; creating fresh agent for session");
                self.0.build_agent(&sid, std::env::current_dir().unwrap_or_default()).await
            }
        };

        if let Some(h) = self.0.sessions.get(&sid).await {
            let mut lock = h.agent.lock().await;
            *lock = Some(inner_agent);
        }

        drop(out_tx);
        Ok(Response::new(Box::pin(ReceiverStream::new(out_rx))))
    }

    // ---- Lifecycle ----

    async fn health_check(
        &self,
        _request: Request<pb::HealthCheckRequest>,
    ) -> Result<Response<pb::HealthCheckResponse>, Status> {
        let uptime = self.0.start_time.elapsed().as_secs();
        Ok(Response::new(pb::HealthCheckResponse {
            status: "ok".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            uptime_secs: uptime,
            active_sessions: self.0.sessions.count().await as u32,
            queue_depth: self.0.queue.read().await.list().len() as u32,
        }))
    }

    async fn shutdown(
        &self,
        request: Request<pb::ShutdownRequest>,
    ) -> Result<Response<pb::ShutdownResponse>, Status> {
        let req = request.into_inner();
        if self.0.shutdown_flag.load(Ordering::SeqCst) {
            return Ok(Response::new(pb::ShutdownResponse {
                accepted: false,
                message: "shutdown already in progress".into(),
            }));
        }
        self.0.shutdown_flag.store(true, Ordering::SeqCst);
        if req.force {
            self.0.cancel_root.cancel();
        }
        tokio::spawn({
            let cancel = self.0.cancel_root.clone();
            async move {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                cancel.cancel();
            }
        });
        Ok(Response::new(pb::ShutdownResponse {
            accepted: true,
            message: "shutting down".into(),
        }))
    }

    // ---- Sessions ----

    async fn create_session(
        &self,
        request: Request<pb::CreateSessionRequest>,
    ) -> Result<Response<pb::CreateSessionResponse>, Status> {
        let req = request.into_inner();
        let session_id = uuid::Uuid::new_v4().to_string();
        let working_dir = std::env::current_dir().unwrap_or_default();
        let agent = self.0.build_agent(&session_id, working_dir).await;
        let name = req.name.unwrap_or_else(|| session_id.clone());
        let now = chrono::Utc::now().to_rfc3339();
        let meta = SessionMeta {
            id: session_id.clone(),
            name,
            created_at: now.clone(),
            updated_at: now,
            message_count: 0,
        };
        let handle = SessionHandle {
            agent: Arc::new(tokio::sync::Mutex::new(Some(agent))),
            meta,
            cancel: CancellationToken::new(),
        };
        self.0.sessions.insert(session_id.clone(), handle).await;
        Ok(Response::new(pb::CreateSessionResponse { session_id }))
    }

    async fn list_sessions(
        &self,
        _request: Request<pb::ListSessionsRequest>,
    ) -> Result<Response<pb::ListSessionsResponse>, Status> {
        let sessions = self
            .0
            .sessions
            .list_meta()
            .await
            .into_iter()
            .map(|m| pb::SessionInfo {
                id: m.id,
                name: m.name,
                created_at: m.created_at,
                updated_at: m.updated_at,
                message_count: m.message_count as u32,
            })
            .collect();
        Ok(Response::new(pb::ListSessionsResponse { sessions }))
    }

    async fn delete_session(
        &self,
        request: Request<pb::DeleteSessionRequest>,
    ) -> Result<Response<pb::DeleteSessionResponse>, Status> {
        let req = request.into_inner();
        let deleted = self.0.sessions.remove(&req.session_id).await.is_some();
        Ok(Response::new(pb::DeleteSessionResponse { deleted }))
    }

    // ---- Agent management ----

    async fn list_agents(
        &self,
        _request: Request<pb::ListAgentsRequest>,
    ) -> Result<Response<pb::ListAgentsResponse>, Status> {
        let ids = self.0.identities.read().await;
        let active = ids.active_name().map(|s| s.to_string());
        let agents = ids
            .list()
            .into_iter()
            .map(|id| pb::AgentInfo {
                name: id.name.clone(),
                description: id.description.clone(),
                tone: id.tone.clone(),
                traits: id.personality_traits.clone(),
                domains: id.knowledge_domains.clone(),
            })
            .collect();
        Ok(Response::new(pb::ListAgentsResponse { agents, active }))
    }

    async fn activate_agent(
        &self,
        request: Request<pb::ActivateAgentRequest>,
    ) -> Result<Response<pb::ActivateAgentResponse>, Status> {
        let req = request.into_inner();
        let mut ids = self.0.identities.write().await;
        match ids.activate(&req.name) {
            Ok(()) => {
                let prompt = ids
                    .active()
                    .map(|id| id.build_prompt("", ""))
                    .unwrap_or_default();
                Ok(Response::new(pb::ActivateAgentResponse {
                    activated: true,
                    system_prompt: prompt,
                }))
            }
            Err(e) => Ok(Response::new(pb::ActivateAgentResponse {
                activated: false,
                system_prompt: e,
            })),
        }
    }

    // ---- Tools ----

    async fn list_tools(
        &self,
        _request: Request<pb::ListToolsRequest>,
    ) -> Result<Response<pb::ListToolsResponse>, Status> {
        let tools = self
            .0
            .tools
            .get_defs()
            .into_iter()
            .map(|d| pb::ToolInfo {
                name: d.name,
                description: d.description,
                parameters: Some(json_to_prost_struct(d.parameters)),
                risk_level: risk_to_pb(d.risk_level),
            })
            .collect();
        Ok(Response::new(pb::ListToolsResponse { tools }))
    }

    async fn call_tool(
        &self,
        request: Request<pb::CallToolRequest>,
    ) -> Result<Response<pb::CallToolResponse>, Status> {
        let req = request.into_inner();
        let args = req
            .arguments
            .map(prost_struct_to_json)
            .unwrap_or(serde_json::Value::Null);
        let ctx = maix_tools::ToolCtx {
            session_id: req.session_id.clone(),
            working_dir: if req.working_dir.is_empty() {
                std::env::current_dir().unwrap_or_default()
            } else {
                std::path::PathBuf::from(req.working_dir)
            },
        };
        let start = std::time::Instant::now();
        match self.0.tools.get(&req.tool_name) {
            Some(tool) => match tool.execute(&ctx, args).await {
                Ok(result) => {
                    let duration_ms = start.elapsed().as_millis() as u64;
                    Ok(Response::new(pb::CallToolResponse {
                        result,
                        error: None,
                        duration_ms,
                    }))
                }
                Err(e) => Ok(Response::new(pb::CallToolResponse {
                    result: String::new(),
                    error: Some(e.to_string()),
                    duration_ms: start.elapsed().as_millis() as u64,
                })),
            },
            None => Ok(Response::new(pb::CallToolResponse {
                result: String::new(),
                error: Some(format!("unknown tool: {}", req.tool_name)),
                duration_ms: 0,
            })),
        }
    }

    // ---- Memory ----

    async fn search_memory(
        &self,
        request: Request<pb::SearchMemoryRequest>,
    ) -> Result<Response<pb::SearchMemoryResponse>, Status> {
        let req = request.into_inner();
        let limit = if req.limit == 0 { 10 } else { req.limit as usize };
        let entries = self
            .0
            .memory
            .read()
            .await
            .search(&req.query, limit)
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .into_iter()
            .map(|e| pb::MemoryEntry {
                id: e.id,
                content: e.content,
                kind: memory_kind_to_pb(e.kind),
                importance: e.importance,
                created_at: e.created_at.to_rfc3339(),
            })
            .collect();
        Ok(Response::new(pb::SearchMemoryResponse { entries }))
    }

    async fn save_memory(
        &self,
        request: Request<pb::SaveMemoryRequest>,
    ) -> Result<Response<pb::SaveMemoryResponse>, Status> {
        let req = request.into_inner();
        let kind = pb_memory_kind(req.kind);
        let entry = maix_memory::MemoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            content: req.content,
            kind,
            importance: req.importance.unwrap_or(0.7),
            created_at: chrono::Utc::now(),
            metadata: {
                let mut m = std::collections::HashMap::new();
                if let Some(sid) = req.session_id {
                    m.insert("session_id".into(), sid);
                }
                m
            },
        };
        let id = self
            .0
            .memory
            .write()
            .await
            .save(entry)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(pb::SaveMemoryResponse { memory_id: id }))
    }

    async fn forget_memory(
        &self,
        request: Request<pb::ForgetMemoryRequest>,
    ) -> Result<Response<pb::ForgetMemoryResponse>, Status> {
        let req = request.into_inner();
        let deleted = self
            .0
            .memory
            .write()
            .await
            .forget(&req.memory_id)
            .await
            .is_ok();
        Ok(Response::new(pb::ForgetMemoryResponse { deleted }))
    }

    // ---- Task queue ----

    async fn submit_task(
        &self,
        request: Request<pb::SubmitTaskRequest>,
    ) -> Result<Response<pb::SubmitTaskResponse>, Status> {
        let req = request.into_inner();
        let task = Task {
            id: uuid::Uuid::new_v4().to_string(),
            description: req.description.clone(),
            input: req.input.clone(),
            priority: req.priority as u8,
            depends_on: req.depends_on.clone(),
            deadline: None,
            retry: maix_task_queue::RetryPolicy {
                max_retries: 3,
                retries: 0,
            },
            created_at: std::time::Instant::now(),
        };
        let task_id = task.id.clone();
        self.0.queue.write().await.enqueue(task);
        Ok(Response::new(pb::SubmitTaskResponse { task_id }))
    }

    async fn list_tasks(
        &self,
        request: Request<pb::ListTasksRequest>,
    ) -> Result<Response<pb::ListTasksResponse>, Status> {
        let req = request.into_inner();
        let tasks = self
            .0
            .queue
            .read()
            .await
            .list()
            .into_iter()
            .filter(|t| {
                req.status_filter
                    .as_ref()
                    .map(|f| format!("{:?}", t.status) == *f)
                    .unwrap_or(true)
            })
            .map(|t| pb::TaskInfo {
                id: t.task.id.clone(),
                description: t.task.description.clone(),
                priority: t.task.priority as u32,
                status: format!("{:?}", t.status),
                assigned: t.assigned.clone(),
                retries: t.task.retry.retries,
                max_retries: t.task.retry.max_retries,
            })
            .collect();
        Ok(Response::new(pb::ListTasksResponse { tasks }))
    }

    async fn cancel_task(
        &self,
        request: Request<pb::CancelTaskRequest>,
    ) -> Result<Response<pb::CancelTaskResponse>, Status> {
        let req = request.into_inner();
        let cancelled = self.0.queue.write().await.cancel(&req.task_id).is_some();
        Ok(Response::new(pb::CancelTaskResponse { cancelled }))
    }

    async fn reprioritize_task(
        &self,
        request: Request<pb::ReprioritizeTaskRequest>,
    ) -> Result<Response<pb::ReprioritizeTaskResponse>, Status> {
        let req = request.into_inner();
        let updated = self
            .0
            .queue
            .write()
            .await
            .reprioritize(&req.task_id, req.priority as u8)
            .is_ok();
        Ok(Response::new(pb::ReprioritizeTaskResponse { updated }))
    }

    async fn suspend_task(
        &self,
        request: Request<pb::SuspendTaskRequest>,
    ) -> Result<Response<pb::SuspendTaskResponse>, Status> {
        let req = request.into_inner();
        let suspended = self.0.queue.write().await.suspend(&req.task_id).is_ok();
        Ok(Response::new(pb::SuspendTaskResponse { suspended }))
    }

    async fn resume_task(
        &self,
        request: Request<pb::ResumeTaskRequest>,
    ) -> Result<Response<pb::ResumeTaskResponse>, Status> {
        let req = request.into_inner();
        let position = req
            .position
            .map(|p| parse_position(&p))
            .unwrap_or(InsertAt::Tail);
        let resumed = self
            .0
            .queue
            .write()
            .await
            .resume(&req.task_id, position)
            .is_ok();
        Ok(Response::new(pb::ResumeTaskResponse { resumed }))
    }

    // ---- Skills ----

    async fn install_skill(
        &self,
        request: Request<pb::InstallSkillRequest>,
    ) -> Result<Response<pb::InstallSkillResponse>, Status> {
        let req = request.into_inner();
        let source = std::path::PathBuf::from(req.source_path);
        match self.0.skills.write().await.install(&source) {
            Ok(name) => Ok(Response::new(pb::InstallSkillResponse {
                name: name.clone(),
                version: "0.1.0".into(),
            })),
            Err(e) => Err(Status::internal(format!("install failed: {e}"))),
        }
    }

    async fn list_skills(
        &self,
        _request: Request<pb::ListSkillsRequest>,
    ) -> Result<Response<pb::ListSkillsResponse>, Status> {
        let skills = self
            .0
            .skills
            .read()
            .await
            .enabled()
            .into_iter()
            .map(|s| pb::SkillInfo {
                name: s.manifest.skill.name.clone(),
                version: s.manifest.skill.version.clone(),
                runtime: s.manifest.skill.runtime.clone(),
                enabled: s.enabled,
                installed_at: s.loaded_at.to_rfc3339(),
            })
            .collect();
        Ok(Response::new(pb::ListSkillsResponse { skills }))
    }

    async fn remove_skill(
        &self,
        request: Request<pb::RemoveSkillRequest>,
    ) -> Result<Response<pb::RemoveSkillResponse>, Status> {
        let req = request.into_inner();
        let removed = self.0.skills.write().await.remove(&req.name).is_ok();
        Ok(Response::new(pb::RemoveSkillResponse { removed }))
    }

    async fn enable_skill(
        &self,
        request: Request<pb::EnableSkillRequest>,
    ) -> Result<Response<pb::EnableSkillResponse>, Status> {
        let req = request.into_inner();
        let enabled = self.0.skills.write().await.enable(&req.name).is_ok();
        Ok(Response::new(pb::EnableSkillResponse { enabled }))
    }

    async fn disable_skill(
        &self,
        request: Request<pb::DisableSkillRequest>,
    ) -> Result<Response<pb::DisableSkillResponse>, Status> {
        let req = request.into_inner();
        let disabled = self.0.skills.write().await.disable(&req.name).is_ok();
        Ok(Response::new(pb::DisableSkillResponse { disabled }))
    }

    // ---- Architectures ----

    async fn list_architectures(
        &self,
        _request: Request<pb::ListArchitecturesRequest>,
    ) -> Result<Response<pb::ListArchitecturesResponse>, Status> {
        let architectures = self
            .0
            .architectures
            .read()
            .await
            .iter()
            .map(|a| pb::ArchitectureInfo {
                id: a.id.clone(),
                name: a.name.clone(),
                description: a.description.clone(),
                topology: format!("{:?}", a.detect_topology()),
                node_count: a.nodes.len() as u32,
                flow_count: a.flows.len() as u32,
            })
            .collect();
        Ok(Response::new(pb::ListArchitecturesResponse { architectures }))
    }

    type RunArchitectureStream =
        Pin<Box<dyn Stream<Item = Result<pb::RunArchitectureOutput, Status>> + Send + 'static>>;

    async fn run_architecture(
        &self,
        request: Request<pb::RunArchitectureRequest>,
    ) -> Result<Response<Self::RunArchitectureStream>, Status> {
        let req = request.into_inner();
        let architectures = self.0.architectures.read().await;
        let arch = architectures.iter().find(|a| a.name == req.name).cloned();
        drop(architectures);

        let (tx, rx) = mpsc::channel(16);
        match arch {
            Some(a) => {
                if let Err(e) = a.validate() {
                    let _ = tx
                        .send(Ok(pb::RunArchitectureOutput {
                            node_id: "system".into(),
                            role: "orchestrator".into(),
                            output: Some(pb::run_architecture_output::Output::Error(format!(
                                "{e:?}"
                            ))),
                        }))
                        .await;
                } else {
                    let _ = tx
                        .send(Ok(pb::RunArchitectureOutput {
                            node_id: "system".into(),
                            role: "orchestrator".into(),
                            output: Some(pb::run_architecture_output::Output::Complete(
                                format!(
                                    "Architecture '{}' accepted; execution deferred",
                                    a.name
                                ),
                            )),
                        }))
                        .await;
                }
            }
            None => {
                let _ = tx
                    .send(Ok(pb::RunArchitectureOutput {
                        node_id: "system".into(),
                        role: "orchestrator".into(),
                        output: Some(pb::run_architecture_output::Output::Error(format!(
                            "unknown architecture: {}",
                            req.name
                        ))),
                    }))
                    .await;
            }
        }
        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }

    // ---- Events ----

    type SubscribeEventsStream =
        Pin<Box<dyn Stream<Item = Result<pb::Event, Status>> + Send + 'static>>;

    async fn subscribe_events(
        &self,
        _request: Request<pb::SubscribeEventsRequest>,
    ) -> Result<Response<Self::SubscribeEventsStream>, Status> {
        let mut sub = self.0.event_bus.subscribe();
        let (tx, rx) = mpsc::channel(64);
        tokio::spawn(async move {
            loop {
                match sub.recv().await {
                    Ok(event) => {
                        use maix_monitor::AgentEvent as MonEvent;
                        let (event_type, agent_id) = match &event {
                            MonEvent::StateChanged { agent_id, .. } => ("StateChanged", agent_id.clone()),
                            MonEvent::TaskDone { agent_id, .. } => ("TaskDone", agent_id.clone()),
                            MonEvent::TaskFailed { agent_id, .. } => ("TaskFailed", agent_id.clone()),
                            MonEvent::TokenUsed { agent_id, .. } => ("TokenUsed", agent_id.clone()),
                            MonEvent::QueueChanged { .. } => ("QueueChanged", String::new()),
                            _ => ("Unknown", String::new()),
                        };
                        let timestamp = chrono::Utc::now().to_rfc3339();
                        let payload = serde_json::to_value(&event)
                            .ok()
                            .map(json_to_prost_struct);
                        let msg = pb::Event {
                            r#type: event_type.to_string(),
                            agent_id,
                            timestamp,
                            payload,
                        };
                        if tx.send(Ok(msg)).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("event bus lagged by {n} messages");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }

    async fn get_work_status(
        &self,
        _request: Request<pb::GetWorkStatusRequest>,
    ) -> Result<Response<pb::WorkStatusSnapshot>, Status> {
        let (agents, metrics) = self.0.monitor.read().await.snapshot();
        let agents: Vec<pb::AgentSnapshot> = agents
            .into_iter()
            .map(|a| pb::AgentSnapshot {
                agent_id: a.agent_id,
                role: a.role,
                state: format!("{:?}", a.state),
                current_task: a.current_task,
                total_tokens: a.stats.total_tokens,
                tool_calls: a.stats.tool_calls as u32,
                avg_latency_ms: a.stats.avg_latency_ms,
            })
            .collect();
        let snapshot = pb::WorkStatusSnapshot {
            total_agents: metrics.total_agents as u32,
            active_agents: metrics.active_agents as u32,
            idle_agents: metrics.idle_agents as u32,
            queue_depth: metrics.queue_depth as u32,
            tasks_completed: metrics.tasks_completed,
            tasks_failed: metrics.tasks_failed,
            total_tokens: metrics.total_tokens,
            total_cost: metrics.total_cost,
            uptime_secs: self.0.start_time.elapsed().as_secs(),
            agents,
        };
        Ok(Response::new(snapshot))
    }

    type WatchWorkStatusStream =
        Pin<Box<dyn Stream<Item = Result<pb::WorkStatusSnapshot, Status>> + Send + 'static>>;

    async fn watch_work_status(
        &self,
        request: Request<pb::WatchWorkStatusRequest>,
    ) -> Result<Response<Self::WatchWorkStatusStream>, Status> {
        let interval_secs = request.into_inner().interval_secs.max(1);
        let (tx, rx) = mpsc::channel(16);
        let core = self.0.clone();
        let start_time = self.0.start_time;
        tokio::spawn(async move {
            loop {
                let (agents, metrics) = core.monitor.read().await.snapshot();
                let agents = agents
                    .into_iter()
                    .map(|a| pb::AgentSnapshot {
                        agent_id: a.agent_id,
                        role: a.role,
                        state: format!("{:?}", a.state),
                        current_task: a.current_task,
                        total_tokens: a.stats.total_tokens,
                        tool_calls: a.stats.tool_calls as u32,
                        avg_latency_ms: a.stats.avg_latency_ms,
                    })
                    .collect();
                let snapshot = pb::WorkStatusSnapshot {
                    total_agents: metrics.total_agents as u32,
                    active_agents: metrics.active_agents as u32,
                    idle_agents: metrics.idle_agents as u32,
                    queue_depth: metrics.queue_depth as u32,
                    tasks_completed: metrics.tasks_completed,
                    tasks_failed: metrics.tasks_failed,
                    total_tokens: metrics.total_tokens,
                    total_cost: metrics.total_cost,
                    uptime_secs: start_time.elapsed().as_secs(),
                    agents,
                };
                if tx.send(Ok(snapshot)).await.is_err() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_secs(interval_secs as u64)).await;
            }
        });
        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn risk_to_pb(r: RiskLevel) -> i32 {
    match r {
        RiskLevel::ReadOnly => 1,
        RiskLevel::Write => 2,
        RiskLevel::Network => 3,
        RiskLevel::Shell => 4,
    }
}

fn memory_kind_to_pb(k: maix_memory::MemoryKind) -> i32 {
    match k {
        maix_memory::MemoryKind::Episodic => 1,
        maix_memory::MemoryKind::Semantic => 2,
        maix_memory::MemoryKind::Working => 3,
    }
}

fn pb_memory_kind(v: i32) -> maix_memory::MemoryKind {
    match v {
        1 => maix_memory::MemoryKind::Episodic,
        2 => maix_memory::MemoryKind::Semantic,
        3 => maix_memory::MemoryKind::Working,
        _ => maix_memory::MemoryKind::Semantic,
    }
}

fn parse_position(s: &str) -> InsertAt {
    match s {
        "head" => InsertAt::Head,
        "tail" => InsertAt::Tail,
        s if s.starts_with("after:") => InsertAt::After(s[6..].to_string()),
        s if s.starts_with("before:") => InsertAt::Before(s[7..].to_string()),
        _ => InsertAt::Tail,
    }
}
