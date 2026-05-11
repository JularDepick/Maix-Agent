use maix_agent::{AgentEvent, AgentMode};
use maix_core::proto::maix::core::v1 as pb;
use maix_core::TokenUsage;
use tonic::Status;

/// Convert an AgentEvent into the corresponding ChatOutput proto message.
pub fn agent_event_to_chat_output(session_id: &str, event: AgentEvent) -> pb::ChatOutput {
    let output = match event {
        AgentEvent::Thinking => pb::chat_output::Output::Status(pb::StatusUpdate {
            session_id: session_id.into(),
            state: pb::AgentState::Thinking.into(),
        }),
        AgentEvent::TextDelta(text) => pb::chat_output::Output::TextDelta(pb::TextDelta {
            session_id: session_id.into(),
            text,
        }),
        AgentEvent::ReasoningDelta(text) => {
            pb::chat_output::Output::ReasoningDelta(pb::ReasoningDelta {
                session_id: session_id.into(),
                text,
            })
        }
        AgentEvent::WaitingApproval => {
            pb::chat_output::Output::Status(pb::StatusUpdate {
                session_id: session_id.into(),
                state: pb::AgentState::WaitingApproval.into(),
            })
        }
        AgentEvent::ToolCallRequested {
            id,
            name,
            args,
            needs_approval,
        } => pb::chat_output::Output::ToolCall(pb::ToolCallRequest {
            session_id: session_id.into(),
            tool_call_id: id,
            tool_name: name,
            arguments: Some(maix_core::json_to_prost_struct(args)),
            needs_approval,
        }),
        AgentEvent::ToolCallApproved | AgentEvent::ToolCallDenied(_) => {
            // These are intermediate events; they map to nothing visible
            // The approval flow is communicated via ToolCallRequest + Status
            pb::chat_output::Output::Status(pb::StatusUpdate {
                session_id: session_id.into(),
                state: pb::AgentState::ExecutingTool.into(),
            })
        }
        AgentEvent::ToolResult { id, result } => {
            pb::chat_output::Output::ToolResult(pb::ToolResult {
                session_id: session_id.into(),
                tool_call_id: id,
                result,
            })
        }
        AgentEvent::ResponseComplete { text, usage } => {
            pb::chat_output::Output::Complete(pb::ResponseComplete {
                session_id: session_id.into(),
                text,
                usage: Some(pb_usage(usage)),
            })
        }
        AgentEvent::MemoryUpdated { summary: _ } => {
            pb::chat_output::Output::Status(pb::StatusUpdate {
                session_id: session_id.into(),
                state: pb::AgentState::UpdatingMemory.into(),
            })
        }
        AgentEvent::Error(e) => pb::chat_output::Output::Error(pb::ErrorEvent {
            session_id: session_id.into(),
            code: "AGENT_ERROR".into(),
            message: e,
        }),
    };

    pb::ChatOutput { output: Some(output) }
}

fn pb_usage(u: TokenUsage) -> maix_core::proto::maix::common::v1::TokenUsage {
    maix_core::proto::maix::common::v1::TokenUsage {
        prompt_tokens: u.prompt_tokens,
        completion_tokens: u.completion_tokens,
        total_tokens: u.total_tokens,
    }
}

/// Read the first `ChatInput` from the client stream and extract a `UserMessage`.
pub async fn read_first_user_message(
    stream: &mut tonic::Streaming<pb::ChatInput>,
) -> Result<(String, String, Option<String>), Status> {
    use futures::StreamExt;
    match stream.next().await {
        Some(Ok(msg)) => {
            if let Some(pb::chat_input::Input::UserMessage(um)) = msg.input {
                Ok((um.session_id, um.text, um.identity))
            } else {
                Err(Status::invalid_argument(
                    "first message must be a UserMessage",
                ))
            }
        }
        Some(Err(e)) => Err(Status::internal(format!("stream error: {e}"))),
        None => Err(Status::cancelled("client closed stream")),
    }
}

/// Read a `ChatInput` from the client stream and dispatch to the appropriate handler.
#[derive(Debug)]
pub enum InboundEvent {
    #[allow(dead_code)]
    UserMessage {
        text: String,
        identity: Option<String>,
    },
    ToolApproval {
        tool_call_id: String,
        approved: bool,
    },
    Cancel,
    SetMode(AgentMode),
}

pub async fn read_next_inbound(
    stream: &mut tonic::Streaming<pb::ChatInput>,
) -> Result<Option<InboundEvent>, Status> {
    use futures::StreamExt;
    match stream.next().await {
        Some(Ok(msg)) => match msg.input {
            Some(pb::chat_input::Input::UserMessage(um)) => Ok(Some(InboundEvent::UserMessage {
                text: um.text,
                identity: um.identity,
            })),
            Some(pb::chat_input::Input::ToolApproval(ta)) => {
                Ok(Some(InboundEvent::ToolApproval {
                    tool_call_id: ta.tool_call_id,
                    approved: ta.approved,
                }))
            }
            Some(pb::chat_input::Input::Cancel(_)) => Ok(Some(InboundEvent::Cancel)),
            Some(pb::chat_input::Input::SetMode(sm)) => {
                let mode = match pb::AgentMode::try_from(sm.mode) {
                    Ok(pb::AgentMode::Plan) => AgentMode::Plan,
                    Ok(pb::AgentMode::Yolo) => AgentMode::Yolo,
                    _ => AgentMode::Agent,
                };
                Ok(Some(InboundEvent::SetMode(mode)))
            }
            None => Ok(None),
        },
        Some(Err(e)) => Err(Status::internal(format!("stream error: {e}"))),
        None => Ok(None),
    }
}
