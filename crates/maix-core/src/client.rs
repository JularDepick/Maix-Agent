//! gRPC client wrapper for the Maix-Agent core service.
//! All clients (CLI, TUI, Gateway) use this to communicate with maix.exe.

use std::sync::Arc;

use tokio::sync::{mpsc, Mutex};
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Channel;
use tonic::{Request, Status, Streaming};

use crate::proto::maix::core::v1::core_service_client::CoreServiceClient;
use crate::proto::maix::core::v1 as pb;

// ---------------------------------------------------------------------------
// MaixClient
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct MaixClient {
    inner: Arc<Mutex<CoreServiceClient<Channel>>>,
    server_addr: String,
}

impl MaixClient {
    pub async fn connect(addr: &str) -> Result<Self, tonic::transport::Error> {
        // Auto-launch maix.exe if not running
        crate::auto_launch::ensure_server_running(addr).await;

        let client = CoreServiceClient::connect(format!("http://{addr}")).await?;
        Ok(Self {
            inner: Arc::new(Mutex::new(client)),
            server_addr: addr.to_string(),
        })
    }

    pub fn server_addr(&self) -> &str {
        &self.server_addr
    }

    // -- Lifecycle --

    pub async fn health_check(&self) -> Result<pb::HealthCheckResponse, Status> {
        self.inner
            .lock()
            .await
            .health_check(Request::new(pb::HealthCheckRequest {}))
            .await
            .map(|r| r.into_inner())
    }

    pub async fn shutdown(&self, force: bool) -> Result<pb::ShutdownResponse, Status> {
        self.inner
            .lock()
            .await
            .shutdown(Request::new(pb::ShutdownRequest { force }))
            .await
            .map(|r| r.into_inner())
    }

    // -- Sessions --

    pub async fn create_session(&self) -> Result<String, Status> {
        self.inner
            .lock()
            .await
            .create_session(Request::new(pb::CreateSessionRequest { name: None }))
            .await
            .map(|r| r.into_inner().session_id)
    }

    pub async fn list_sessions(&self) -> Result<Vec<pb::SessionInfo>, Status> {
        self.inner
            .lock()
            .await
            .list_sessions(Request::new(pb::ListSessionsRequest {}))
            .await
            .map(|r| r.into_inner().sessions)
    }

    pub async fn delete_session(&self, session_id: &str) -> Result<bool, Status> {
        self.inner
            .lock()
            .await
            .delete_session(Request::new(pb::DeleteSessionRequest {
                session_id: session_id.into(),
            }))
            .await
            .map(|r| r.into_inner().deleted)
    }

    // -- Chat --

    pub async fn chat(
        &self,
    ) -> Result<
        (
            mpsc::Sender<pb::ChatInput>,
            tonic::Response<Streaming<pb::ChatOutput>>,
        ),
        Status,
    > {
        let (tx, rx) = mpsc::channel::<pb::ChatInput>(32);
        let stream = ReceiverStream::new(rx);
        let mut client = self.inner.lock().await;
        let response = client.chat(Request::new(stream)).await?;
        Ok((tx, response))
    }

    pub async fn chat_with_message(
        &self,
        session_id: &str,
        text: &str,
    ) -> Result<ChatHandle, Status> {
        let (tx, rx) = mpsc::channel::<pb::ChatInput>(32);

        let first_msg = pb::ChatInput {
            input: Some(pb::chat_input::Input::UserMessage(pb::UserMessage {
                session_id: session_id.to_string(),
                text: text.to_string(),
                identity: None,
                architecture: None,
            })),
        };
        tx.send(first_msg)
            .await
            .map_err(|_| Status::internal("chat stream closed"))?;

        let stream = ReceiverStream::new(rx);
        let mut client = self.inner.lock().await;
        let response = client.chat(Request::new(stream)).await?;
        drop(client);

        Ok(ChatHandle::new(session_id.to_string(), tx, response))
    }

    // -- Agents --

    pub async fn list_agents(&self) -> Result<pb::ListAgentsResponse, Status> {
        self.inner
            .lock()
            .await
            .list_agents(Request::new(pb::ListAgentsRequest {}))
            .await
            .map(|r| r.into_inner())
    }

    pub async fn activate_agent(&self, name: &str) -> Result<pb::ActivateAgentResponse, Status> {
        self.inner
            .lock()
            .await
            .activate_agent(Request::new(pb::ActivateAgentRequest {
                name: name.into(),
            }))
            .await
            .map(|r| r.into_inner())
    }

    // -- Tools --

    pub async fn list_tools(&self) -> Result<Vec<pb::ToolInfo>, Status> {
        self.inner
            .lock()
            .await
            .list_tools(Request::new(pb::ListToolsRequest {}))
            .await
            .map(|r| r.into_inner().tools)
    }

    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: Option<prost_types::Struct>,
        session_id: &str,
        working_dir: &str,
    ) -> Result<pb::CallToolResponse, Status> {
        self.inner
            .lock()
            .await
            .call_tool(Request::new(pb::CallToolRequest {
                tool_name: tool_name.into(),
                arguments,
                session_id: session_id.into(),
                working_dir: working_dir.into(),
            }))
            .await
            .map(|r| r.into_inner())
    }

    // -- Memory --

    pub async fn search_memory(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<pb::MemoryEntry>, Status> {
        self.inner
            .lock()
            .await
            .search_memory(Request::new(pb::SearchMemoryRequest {
                query: query.into(),
                limit,
                session_id: None,
            }))
            .await
            .map(|r| r.into_inner().entries)
    }

    pub async fn save_memory(
        &self,
        content: &str,
        importance: f32,
        session_id: Option<&str>,
    ) -> Result<String, Status> {
        self.inner
            .lock()
            .await
            .save_memory(Request::new(pb::SaveMemoryRequest {
                content: content.into(),
                importance: Some(importance),
                session_id: session_id.map(|s| s.into()),
                kind: pb::MemoryKind::Semantic.into(),
            }))
            .await
            .map(|r| r.into_inner().memory_id)
    }

    pub async fn forget_memory(&self, memory_id: &str) -> Result<bool, Status> {
        self.inner
            .lock()
            .await
            .forget_memory(Request::new(pb::ForgetMemoryRequest {
                memory_id: memory_id.into(),
            }))
            .await
            .map(|r| r.into_inner().deleted)
    }

    // -- Tasks --

    pub async fn submit_task(
        &self,
        description: &str,
        input: &str,
        priority: u32,
    ) -> Result<String, Status> {
        self.inner
            .lock()
            .await
            .submit_task(Request::new(pb::SubmitTaskRequest {
                description: description.into(),
                input: input.into(),
                priority,
                depends_on: vec![],
                position: None,
            }))
            .await
            .map(|r| r.into_inner().task_id)
    }

    pub async fn list_tasks(&self) -> Result<Vec<pb::TaskInfo>, Status> {
        self.inner
            .lock()
            .await
            .list_tasks(Request::new(pb::ListTasksRequest {
                status_filter: None,
            }))
            .await
            .map(|r| r.into_inner().tasks)
    }

    pub async fn cancel_task(&self, task_id: &str) -> Result<bool, Status> {
        self.inner
            .lock()
            .await
            .cancel_task(Request::new(pb::CancelTaskRequest {
                task_id: task_id.into(),
            }))
            .await
            .map(|r| r.into_inner().cancelled)
    }

    pub async fn reprioritize_task(&self, task_id: &str, priority: u32) -> Result<bool, Status> {
        self.inner
            .lock()
            .await
            .reprioritize_task(Request::new(pb::ReprioritizeTaskRequest {
                task_id: task_id.into(),
                priority,
            }))
            .await
            .map(|r| r.into_inner().updated)
    }

    pub async fn suspend_task(&self, task_id: &str) -> Result<bool, Status> {
        self.inner
            .lock()
            .await
            .suspend_task(Request::new(pb::SuspendTaskRequest {
                task_id: task_id.into(),
            }))
            .await
            .map(|r| r.into_inner().suspended)
    }

    pub async fn resume_task(&self, task_id: &str) -> Result<bool, Status> {
        self.inner
            .lock()
            .await
            .resume_task(Request::new(pb::ResumeTaskRequest {
                task_id: task_id.into(),
                position: Some("tail".into()),
            }))
            .await
            .map(|r| r.into_inner().resumed)
    }

    // -- Skills --

    pub async fn list_skills(&self) -> Result<Vec<pb::SkillInfo>, Status> {
        self.inner
            .lock()
            .await
            .list_skills(Request::new(pb::ListSkillsRequest {}))
            .await
            .map(|r| r.into_inner().skills)
    }

    pub async fn install_skill(&self, source_path: &str) -> Result<pb::InstallSkillResponse, Status> {
        self.inner
            .lock()
            .await
            .install_skill(Request::new(pb::InstallSkillRequest {
                source_path: source_path.into(),
            }))
            .await
            .map(|r| r.into_inner())
    }

    pub async fn remove_skill(&self, name: &str) -> Result<bool, Status> {
        self.inner
            .lock()
            .await
            .remove_skill(Request::new(pb::RemoveSkillRequest {
                name: name.into(),
            }))
            .await
            .map(|r| r.into_inner().removed)
    }

    pub async fn enable_skill(&self, name: &str) -> Result<bool, Status> {
        self.inner
            .lock()
            .await
            .enable_skill(Request::new(pb::EnableSkillRequest {
                name: name.into(),
            }))
            .await
            .map(|r| r.into_inner().enabled)
    }

    pub async fn disable_skill(&self, name: &str) -> Result<bool, Status> {
        self.inner
            .lock()
            .await
            .disable_skill(Request::new(pb::DisableSkillRequest {
                name: name.into(),
            }))
            .await
            .map(|r| r.into_inner().disabled)
    }

    // -- Architectures --

    pub async fn list_architectures(&self) -> Result<Vec<pb::ArchitectureInfo>, Status> {
        self.inner
            .lock()
            .await
            .list_architectures(Request::new(pb::ListArchitecturesRequest {}))
            .await
            .map(|r| r.into_inner().architectures)
    }

    pub async fn run_architecture(
        &self,
        name: &str,
        input: &str,
    ) -> Result<tonic::Response<Streaming<pb::RunArchitectureOutput>>, Status> {
        self.inner
            .lock()
            .await
            .run_architecture(Request::new(pb::RunArchitectureRequest {
                name: name.into(),
                input: input.into(),
            }))
            .await
    }

    // -- Events --

    pub async fn subscribe_events(
        &self,
    ) -> Result<tonic::Response<Streaming<pb::Event>>, Status> {
        self.inner
            .lock()
            .await
            .subscribe_events(Request::new(pb::SubscribeEventsRequest {
                event_types: vec![],
            }))
            .await
    }

    // -- Work status --

    pub async fn get_work_status(&self) -> Result<pb::WorkStatusSnapshot, Status> {
        self.inner
            .lock()
            .await
            .get_work_status(Request::new(pb::GetWorkStatusRequest {}))
            .await
            .map(|r| r.into_inner())
    }

    pub async fn watch_work_status(
        &self,
        interval_secs: u32,
    ) -> Result<tonic::Response<Streaming<pb::WorkStatusSnapshot>>, Status> {
        self.inner
            .lock()
            .await
            .watch_work_status(Request::new(pb::WatchWorkStatusRequest {
                interval_secs,
            }))
            .await
    }
}

// ---------------------------------------------------------------------------
// ChatHandle
// ---------------------------------------------------------------------------

pub struct ChatHandle {
    pub session_id: String,
    tx: mpsc::Sender<pb::ChatInput>,
    pub stream: Streaming<pb::ChatOutput>,
}

impl ChatHandle {
    pub fn new(
        session_id: String,
        tx: mpsc::Sender<pb::ChatInput>,
        response: tonic::Response<Streaming<pb::ChatOutput>>,
    ) -> Self {
        Self {
            session_id,
            tx,
            stream: response.into_inner(),
        }
    }

    pub async fn send_message(&self, text: &str) -> Result<(), Status> {
        let msg = pb::ChatInput {
            input: Some(pb::chat_input::Input::UserMessage(pb::UserMessage {
                session_id: self.session_id.clone(),
                text: text.into(),
                identity: None,
                architecture: None,
            })),
        };
        self.tx.send(msg).await.map_err(|_| {
            Status::internal("chat stream closed")
        })
    }

    pub async fn send_approval(&self, tool_call_id: &str, approved: bool) -> Result<(), Status> {
        let msg = pb::ChatInput {
            input: Some(pb::chat_input::Input::ToolApproval(pb::ToolApproval {
                session_id: self.session_id.clone(),
                tool_call_id: tool_call_id.into(),
                approved,
                reason: String::new(),
            })),
        };
        self.tx.send(msg).await.map_err(|_| {
            Status::internal("chat stream closed")
        })
    }

    pub async fn send_cancel(&self) -> Result<(), Status> {
        let msg = pb::ChatInput {
            input: Some(pb::chat_input::Input::Cancel(pb::CancelRequest {
                session_id: self.session_id.clone(),
            })),
        };
        self.tx.send(msg).await.map_err(|_| {
            Status::internal("chat stream closed")
        })
    }

    pub async fn send_set_mode(&self, mode: pb::AgentMode) -> Result<(), Status> {
        let msg = pb::ChatInput {
            input: Some(pb::chat_input::Input::SetMode(pb::SetMode {
                session_id: self.session_id.clone(),
                mode: mode as i32,
            })),
        };
        self.tx.send(msg).await.map_err(|_| {
            Status::internal("chat stream closed")
        })
    }

    pub async fn recv(&mut self) -> Option<Result<pb::ChatOutput, Status>> {
        use tokio_stream::StreamExt;
        self.stream.next().await
    }

    pub fn into_stream(self) -> Streaming<pb::ChatOutput> {
        self.stream
    }
}

/// Convenience: start a chat session in one call.
/// If `session_id` is provided, reuse that session; otherwise create a new one.
pub async fn start_chat(
    client: &MaixClient,
    text: &str,
    session_id: Option<&str>,
) -> Result<ChatHandle, Status> {
    let session_id = match session_id {
        Some(id) => id.to_string(),
        None => client.create_session().await?,
    };
    client.chat_with_message(&session_id, text).await
}
