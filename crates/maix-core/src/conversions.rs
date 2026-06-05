use crate::proto::maix::common::v1 as pb_common;
use crate::proto::maix::core::v1 as pb_core;
use crate::types;
use crate::identity::Identity;

// ---------------------------------------------------------------------------
// Role
// ---------------------------------------------------------------------------

impl From<types::Role> for pb_common::Role {
    fn from(r: types::Role) -> Self {
        match r {
            types::Role::System => pb_common::Role::System,
            types::Role::User => pb_common::Role::User,
            types::Role::Assistant => pb_common::Role::Assistant,
            types::Role::Tool => pb_common::Role::Tool,
        }
    }
}

impl From<pb_common::Role> for types::Role {
    fn from(r: pb_common::Role) -> Self {
        match r {
            pb_common::Role::System => types::Role::System,
            pb_common::Role::User => types::Role::User,
            pb_common::Role::Assistant => types::Role::Assistant,
            pb_common::Role::Tool => types::Role::Tool,
            pb_common::Role::Unspecified => types::Role::User,
        }
    }
}

// ---------------------------------------------------------------------------
// MessageContent
// ---------------------------------------------------------------------------

impl From<types::MessageContent> for pb_common::MessageContent {
    fn from(mc: types::MessageContent) -> Self {
        match mc {
            types::MessageContent::Text(text) => pb_common::MessageContent {
                content: Some(pb_common::message_content::Content::Text(text)),
            },
            types::MessageContent::Parts(parts) => {
                let parts = parts.into_iter().map(|p| p.into()).collect();
                pb_common::MessageContent {
                    content: Some(pb_common::message_content::Content::Parts(
                        pb_common::ContentParts { parts },
                    )),
                }
            }
        }
    }
}

impl From<pb_common::MessageContent> for types::MessageContent {
    fn from(mc: pb_common::MessageContent) -> Self {
        match mc.content {
            Some(pb_common::message_content::Content::Text(text)) => {
                types::MessageContent::Text(text)
            }
            Some(pb_common::message_content::Content::Parts(parts)) => {
                let parts = parts.parts.into_iter().map(|p| p.into()).collect();
                types::MessageContent::Parts(parts)
            }
            None => types::MessageContent::Text(String::new()),
        }
    }
}

// ---------------------------------------------------------------------------
// ContentPart
// ---------------------------------------------------------------------------

impl From<types::ContentPart> for pb_common::ContentPart {
    fn from(p: types::ContentPart) -> Self {
        match p {
            types::ContentPart::Text { text } => pb_common::ContentPart {
                part: Some(pb_common::content_part::Part::Text(pb_common::TextPart { text })),
            },
            types::ContentPart::ImageUrl { image_url } => pb_common::ContentPart {
                part: Some(pb_common::content_part::Part::Image(pb_common::ImagePart {
                    url: image_url.url,
                    detail: image_url.detail,
                })),
            },
            types::ContentPart::ImageBase64 { source } => pb_common::ContentPart {
                part: Some(pb_common::content_part::Part::Image(pb_common::ImagePart {
                    url: format!("data:{};base64,{}", source.media_type, source.data),
                    detail: None,
                })),
            },
        }
    }
}

impl From<pb_common::ContentPart> for types::ContentPart {
    fn from(p: pb_common::ContentPart) -> Self {
        match p.part {
            Some(pb_common::content_part::Part::Text(t)) => types::ContentPart::Text {
                text: t.text,
            },
            Some(pb_common::content_part::Part::Image(img)) => types::ContentPart::ImageUrl {
                image_url: types::ImageUrl {
                    url: img.url,
                    detail: img.detail,
                },
            },
            None => types::ContentPart::Text {
                text: String::new(),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// ToolCall / FunctionCall
// ---------------------------------------------------------------------------

impl From<types::ToolCall> for pb_common::ToolCall {
    fn from(tc: types::ToolCall) -> Self {
        pb_common::ToolCall {
            id: tc.id,
            r#type: tc.call_type,
            function: Some(pb_common::FunctionCall {
                name: tc.function.name,
                arguments: tc.function.arguments,
            }),
        }
    }
}

impl From<pb_common::ToolCall> for types::ToolCall {
    fn from(tc: pb_common::ToolCall) -> Self {
        let function = tc.function.unwrap_or(pb_common::FunctionCall {
            name: String::new(),
            arguments: String::new(),
        });
        types::ToolCall {
            id: tc.id,
            call_type: tc.r#type,
            function: types::FunctionCall {
                name: function.name,
                arguments: function.arguments,
            },
        }
    }
}

// ---------------------------------------------------------------------------
// ToolDef / FunctionDef
// ---------------------------------------------------------------------------

pub fn prost_struct_to_json(s: prost_types::Struct) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for (k, v) in s.fields {
        map.insert(k, prost_value_to_json(v));
    }
    serde_json::Value::Object(map)
}

pub fn prost_value_to_json(v: prost_types::Value) -> serde_json::Value {
    use prost_types::value::Kind;
    match v.kind {
        Some(Kind::NullValue(_)) => serde_json::Value::Null,
        Some(Kind::NumberValue(n)) => serde_json::Value::Number(
            serde_json::Number::from_f64(n).unwrap_or(serde_json::Number::from(0)),
        ),
        Some(Kind::StringValue(s)) => serde_json::Value::String(s),
        Some(Kind::BoolValue(b)) => serde_json::Value::Bool(b),
        Some(Kind::StructValue(s)) => prost_struct_to_json(s),
        Some(Kind::ListValue(l)) => {
            serde_json::Value::Array(l.values.into_iter().map(prost_value_to_json).collect())
        }
        None => serde_json::Value::Null,
    }
}

pub fn json_to_prost_struct(v: serde_json::Value) -> prost_types::Struct {
    let mut fields = std::collections::BTreeMap::new();
    if let serde_json::Value::Object(map) = v {
        for (k, val) in map {
            fields.insert(k, json_to_prost_value(val));
        }
    }
    prost_types::Struct { fields }
}

fn json_to_prost_value(v: serde_json::Value) -> prost_types::Value {
    use prost_types::value::Kind;
    let kind = match v {
        serde_json::Value::Null => Kind::NullValue(0),
        serde_json::Value::Bool(b) => Kind::BoolValue(b),
        serde_json::Value::Number(n) => Kind::NumberValue(n.as_f64().unwrap_or(0.0)),
        serde_json::Value::String(s) => Kind::StringValue(s),
        serde_json::Value::Array(arr) => {
            Kind::ListValue(prost_types::ListValue {
                values: arr.into_iter().map(json_to_prost_value).collect(),
            })
        }
        serde_json::Value::Object(_) => Kind::StructValue(json_to_prost_struct(v)),
    };
    prost_types::Value { kind: Some(kind) }
}

impl From<types::ToolDef> for pb_common::ToolDef {
    fn from(td: types::ToolDef) -> Self {
        pb_common::ToolDef {
            r#type: td.tool_type,
            function: Some(pb_common::FunctionDef {
                name: td.function.name,
                description: td.function.description,
                parameters: Some(json_to_prost_struct(td.function.parameters)),
            }),
        }
    }
}

impl From<pb_common::ToolDef> for types::ToolDef {
    fn from(td: pb_common::ToolDef) -> Self {
        let function = td.function.unwrap_or(pb_common::FunctionDef {
            name: String::new(),
            description: String::new(),
            parameters: None,
        });
        types::ToolDef {
            tool_type: td.r#type,
            function: types::FunctionDef {
                name: function.name,
                description: function.description,
                parameters: function
                    .parameters
                    .map(prost_struct_to_json)
                    .unwrap_or_default(),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Message
// ---------------------------------------------------------------------------

impl From<types::Message> for pb_common::Message {
    fn from(m: types::Message) -> Self {
        let tool_calls = m.tool_calls.map(|tcs| tcs.into_iter().map(|tc| tc.into()).collect());
        pb_common::Message {
            role: (pb_common::Role::from(m.role)).into(),
            content: Some(m.content.into()),
            name: m.name,
            tool_call_id: m.tool_call_id,
            tool_calls: tool_calls.unwrap_or_default(),
        }
    }
}

impl From<pb_common::Message> for types::Message {
    fn from(m: pb_common::Message) -> Self {
        let role = pb_common::Role::try_from(m.role)
            .map(types::Role::from)
            .unwrap_or(types::Role::User);
        let tool_calls = if m.tool_calls.is_empty() {
            None
        } else {
            Some(m.tool_calls.into_iter().map(|tc| tc.into()).collect())
        };
        types::Message {
            role,
            content: m.content.map(types::MessageContent::from).unwrap_or(
                types::MessageContent::Text(String::new()),
            ),
            name: m.name,
            tool_call_id: m.tool_call_id,
            tool_calls,
            reasoning_content: None,
        }
    }
}

// ---------------------------------------------------------------------------
// TokenUsage
// ---------------------------------------------------------------------------

impl From<types::TokenUsage> for pb_common::TokenUsage {
    fn from(tu: types::TokenUsage) -> Self {
        pb_common::TokenUsage {
            prompt_tokens: tu.prompt_tokens,
            completion_tokens: tu.completion_tokens,
            total_tokens: tu.total_tokens,
            cache_read_tokens: tu.cache_read_tokens,
            cache_write_tokens: tu.cache_write_tokens,
        }
    }
}

impl From<pb_common::TokenUsage> for types::TokenUsage {
    fn from(tu: pb_common::TokenUsage) -> Self {
        types::TokenUsage {
            prompt_tokens: tu.prompt_tokens,
            completion_tokens: tu.completion_tokens,
            total_tokens: tu.total_tokens,
            cache_read_tokens: tu.cache_read_tokens,
            cache_write_tokens: tu.cache_write_tokens,
        }
    }
}

// ---------------------------------------------------------------------------
// AgentInfo (proto) <-> Identity (Rust)
// ---------------------------------------------------------------------------

impl From<Identity> for pb_core::AgentInfo {
    fn from(id: Identity) -> Self {
        pb_core::AgentInfo {
            name: id.name,
            description: id.description,
            tone: id.tone,
            traits: id.personality_traits.clone(),
            domains: id.knowledge_domains,
        }
    }
}

impl From<pb_core::AgentInfo> for Identity {
    fn from(info: pb_core::AgentInfo) -> Self {
        Identity::new(
            info.name.clone(),
            info.name,
            info.description,
            String::new(),
        )
        .with_traits(info.traits)
        .with_domains(info.domains)
        .with_tone(info.tone)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_roundtrip() {
        let roles = [
            types::Role::System,
            types::Role::User,
            types::Role::Assistant,
            types::Role::Tool,
        ];
        for r in roles {
            let pb: pb_common::Role = r.clone().into();
            let back: types::Role = pb.into();
            assert_eq!(r, back);
        }
    }

    #[test]
    fn test_token_usage_roundtrip() {
        let tu = types::TokenUsage {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
            cache_read_tokens: 80,
            cache_write_tokens: 20,
        };
        let pb: pb_common::TokenUsage = tu.clone().into();
        let back: types::TokenUsage = pb.into();
        assert_eq!(tu.prompt_tokens, back.prompt_tokens);
        assert_eq!(tu.completion_tokens, back.completion_tokens);
        assert_eq!(tu.total_tokens, back.total_tokens);
        assert_eq!(tu.cache_read_tokens, back.cache_read_tokens);
        assert_eq!(tu.cache_write_tokens, back.cache_write_tokens);
    }

    #[test]
    fn test_message_text_roundtrip() {
        let msg = types::Message {
            role: types::Role::User,
            content: types::MessageContent::Text("hello".into()),
            name: Some("user".into()),
            tool_call_id: None,
            tool_calls: None,
            reasoning_content: None,
        };
        let pb: pb_common::Message = msg.clone().into();
        let back: types::Message = pb.into();
        assert_eq!(back.role, msg.role);
        assert_eq!(back.name, msg.name);
        match back.content {
            types::MessageContent::Text(ref s) => assert_eq!(s, "hello"),
            _ => panic!("expected text content"),
        }
    }

    #[test]
    fn test_message_tool_call_roundtrip() {
        let msg = types::Message {
            role: types::Role::Assistant,
            content: types::MessageContent::Text("".into()),
            name: None,
            tool_call_id: None,
            tool_calls: Some(vec![types::ToolCall {
                id: "call_1".into(),
                call_type: "function".into(),
                function: types::FunctionCall {
                    name: "read_file".into(),
                    arguments: r#"{"path":"/tmp"}"#.into(),
                },
            }]),
            reasoning_content: None,
        };
        let pb: pb_common::Message = msg.clone().into();
        let back: types::Message = pb.into();
        let tcs = back.tool_calls.unwrap();
        assert_eq!(tcs.len(), 1);
        assert_eq!(tcs[0].id, "call_1");
        assert_eq!(tcs[0].function.name, "read_file");
    }

    #[test]
    fn test_identity_to_agent_info() {
        let id = Identity::new(
            "test".into(),
            "TestAgent".into(),
            "A test agent".into(),
            "system prompt".into(),
        )
        .with_traits(vec!["friendly".into()])
        .with_tone("casual");

        let info: pb_core::AgentInfo = id.into();
        assert_eq!(info.name, "TestAgent");
        assert_eq!(info.tone, "casual");
        assert_eq!(info.traits.len(), 1);
    }

    #[test]
    fn test_prost_value_null() {
        let v = prost_types::Value { kind: Some(prost_types::value::Kind::NullValue(0)) };
        let json = prost_value_to_json(v);
        assert_eq!(json, serde_json::Value::Null);
    }

    #[test]
    fn test_prost_value_none_kind() {
        let v = prost_types::Value { kind: None };
        let json = prost_value_to_json(v);
        assert_eq!(json, serde_json::Value::Null);
    }

    #[test]
    fn test_prost_value_bool() {
        let v = prost_types::Value { kind: Some(prost_types::value::Kind::BoolValue(true)) };
        assert_eq!(prost_value_to_json(v), serde_json::Value::Bool(true));
    }

    #[test]
    fn test_prost_value_string() {
        let v = prost_types::Value { kind: Some(prost_types::value::Kind::StringValue("hello".into())) };
        assert_eq!(prost_value_to_json(v), serde_json::Value::String("hello".into()));
    }

    #[test]
    fn test_prost_value_number() {
        let v = prost_types::Value { kind: Some(prost_types::value::Kind::NumberValue(42.5)) };
        let json = prost_value_to_json(v);
        assert_eq!(json, serde_json::json!(42.5));
    }

    #[test]
    fn test_prost_value_list() {
        let v = prost_types::Value {
            kind: Some(prost_types::value::Kind::ListValue(prost_types::ListValue {
                values: vec![
                    prost_types::Value { kind: Some(prost_types::value::Kind::NumberValue(1.0)) },
                    prost_types::Value { kind: Some(prost_types::value::Kind::StringValue("two".into())) },
                ],
            })),
        };
        let json = prost_value_to_json(v);
        assert_eq!(json, serde_json::json!([1.0, "two"]));
    }

    #[test]
    fn test_prost_struct_nested() {
        let inner = prost_types::Struct {
            fields: std::collections::BTreeMap::from([
                ("key".into(), prost_types::Value { kind: Some(prost_types::value::Kind::StringValue("val".into())) }),
            ]),
        };
        let outer = prost_types::Struct {
            fields: std::collections::BTreeMap::from([
                ("nested".into(), prost_types::Value { kind: Some(prost_types::value::Kind::StructValue(inner)) }),
            ]),
        };
        let json = prost_struct_to_json(outer);
        assert_eq!(json["nested"]["key"], "val");
    }

    #[test]
    fn test_prost_struct_empty() {
        let s = prost_types::Struct { fields: std::collections::BTreeMap::new() };
        let json = prost_struct_to_json(s);
        assert_eq!(json, serde_json::json!({}));
    }

    #[test]
    fn test_json_to_prost_value_null() {
        let pv = json_to_prost_value(serde_json::Value::Null);
        assert!(matches!(pv.kind, Some(prost_types::value::Kind::NullValue(_))));
    }

    #[test]
    fn test_json_to_prost_value_array() {
        let pv = json_to_prost_value(serde_json::json!([1, "two", true]));
        match pv.kind {
            Some(prost_types::value::Kind::ListValue(l)) => assert_eq!(l.values.len(), 3),
            _ => panic!("expected list"),
        }
    }
}
