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
        cache_read_tokens: u.cache_read_tokens,
        cache_write_tokens: u.cache_write_tokens,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_event_text_delta() {
        let output = agent_event_to_chat_output("s1", AgentEvent::TextDelta("hello".into()));
        match output.output {
            Some(pb::chat_output::Output::TextDelta(td)) => {
                assert_eq!(td.session_id, "s1");
                assert_eq!(td.text, "hello");
            }
            _ => panic!("expected TextDelta"),
        }
    }

    #[test]
    fn test_agent_event_thinking() {
        let output = agent_event_to_chat_output("s1", AgentEvent::Thinking);
        match output.output {
            Some(pb::chat_output::Output::Status(s)) => {
                assert_eq!(s.session_id, "s1");
            }
            _ => panic!("expected Status"),
        }
    }

    #[test]
    fn test_agent_event_error() {
        let output = agent_event_to_chat_output("s1", AgentEvent::Error("fail".into()));
        match output.output {
            Some(pb::chat_output::Output::Error(e)) => {
                assert_eq!(e.session_id, "s1");
                assert_eq!(e.message, "fail");
                assert_eq!(e.code, "AGENT_ERROR");
            }
            _ => panic!("expected Error"),
        }
    }

    #[test]
    fn test_agent_event_tool_result() {
        let output = agent_event_to_chat_output(
            "s1",
            AgentEvent::ToolResult {
                id: "call-1".into(),
                result: "ok".into(),
            },
        );
        match output.output {
            Some(pb::chat_output::Output::ToolResult(tr)) => {
                assert_eq!(tr.tool_call_id, "call-1");
                assert_eq!(tr.result, "ok");
            }
            _ => panic!("expected ToolResult"),
        }
    }

    #[test]
    fn test_agent_event_response_complete() {
        let usage = TokenUsage {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
        };
        let output = agent_event_to_chat_output(
            "s1",
            AgentEvent::ResponseComplete {
                text: "done".into(),
                usage,
            },
        );
        match output.output {
            Some(pb::chat_output::Output::Complete(c)) => {
                assert_eq!(c.text, "done");
                let u = c.usage.unwrap();
                assert_eq!(u.total_tokens, 150);
            }
            _ => panic!("expected Complete"),
        }
    }

    #[test]
    fn test_agent_event_reasoning_delta() {
        let output = agent_event_to_chat_output("s1", AgentEvent::ReasoningDelta("thinking...".into()));
        match output.output {
            Some(pb::chat_output::Output::ReasoningDelta(rd)) => {
                assert_eq!(rd.text, "thinking...");
            }
            _ => panic!("expected ReasoningDelta"),
        }
    }

    #[test]
    fn test_pb_usage() {
        let u = TokenUsage {
            prompt_tokens: 10,
            completion_tokens: 20,
            total_tokens: 30,
            cache_read_tokens: 5,
            cache_write_tokens: 3,
        };
        let pb = pb_usage(u);
        assert_eq!(pb.prompt_tokens, 10);
        assert_eq!(pb.completion_tokens, 20);
        assert_eq!(pb.total_tokens, 30);
        assert_eq!(pb.cache_read_tokens, 5);
        assert_eq!(pb.cache_write_tokens, 3);
    }
}
