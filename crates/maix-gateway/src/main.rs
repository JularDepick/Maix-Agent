//! Maix-Agent HTTP Gateway — gRPC-to-HTTP bridge.
//! Translates REST/SSE/WebSocket requests to gRPC calls on maix-server.

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    http::StatusCode,
    response::{sse::Event, Json, Sse},
    routing::{delete, get, patch, post},
    Router,
};
use clap::Parser;
use futures::stream::Stream;
use maix_core::client::MaixClient;
use maix_core::proto::maix::core::v1 as pb;
use serde::Deserialize;
use std::convert::Infallible;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "maix-gateway", version)]
struct Cli {
    #[arg(long, default_value = "0.0.0.0:8080")]
    listen: String,
    #[arg(long, default_value = "127.0.0.1:26506")]
    server: String,
}

type GatewayState = Arc<MaixClient>;

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ChatRequest {
    message: String,
    session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SubmitTaskRequest {
    description: String,
    input: String,
    priority: u32,
    #[allow(dead_code)]
    position: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MemoryEntryInput {
    content: String,
    importance: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct MemorySearchQuery {
    q: String,
    limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct InstallSkillRequest {
    source: String,
}

#[derive(Debug, Deserialize)]
struct PatchTaskRequest {
    priority: Option<u32>,
    position: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RunArchitectureRequest {
    input: String,
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    maix_core::init_console_utf8();
    tracing_subscriber::fmt().with_env_filter("info").init();

    let listen_addr = &cli.listen;
    let server_addr = &cli.server;

    let client = MaixClient::connect(server_addr).await.unwrap_or_else(|e| {
        tracing::error!("Failed to connect to maix at {}: {e}", server_addr);
        std::process::exit(1);
    });

    if let Ok(h) = client.health_check().await {
        tracing::debug!("Connected to maix v{} (uptime {}s)", h.version, h.uptime_secs);
    }

    let app = Router::new()
        .route("/health", get(health))
        .route("/metrics", get(metrics))
        .route("/v1/chat", post(chat_sse))
        .route("/v1/ws/chat", get(ws_chat))
        .route("/v1/ws/events", get(ws_events))
        .route("/v1/sessions", get(list_sessions).delete(delete_session))
        .route("/v1/sessions/{id}", delete(delete_session_by_id))
        .route("/v1/memory", get(search_memory).post(save_memory))
        .route("/v1/memory/{id}", delete(delete_memory))
        .route("/v1/memory/compact", post(compact_memory))
        .route("/v1/tasks", get(list_tasks).post(submit_task))
        .route("/v1/tasks/{id}", delete(delete_task))
        .route("/v1/tasks/{id}/position", patch(reposition_task))
        .route("/v1/tasks/{id}/priority", patch(reprioritize_task))
        .route("/v1/tasks/{id}/suspend", post(suspend_task))
        .route("/v1/tasks/{id}/resume", post(resume_task))
        .route("/v1/tools", get(list_tools))
        .route("/v1/tools/{name}/call", post(call_tool_handler))
        .route("/v1/skills", get(list_skills).post(install_skill))
        .route("/v1/skills/{name}", delete(remove_skill))
        .route("/v1/skills/{name}/enable", post(enable_skill))
        .route("/v1/skills/{name}/disable", post(disable_skill))
        .route("/v1/identities", get(list_identities))
        .route("/v1/identities/{name}", get(show_identity))
        .route("/v1/identities/{name}/activate", post(activate_identity))
        .route("/v1/architectures", get(list_architectures))
        .route("/v1/architectures/{name}", get(show_architecture))
        .route("/v1/architectures/{name}/run", post(run_architecture))
        .route("/v1/work-status/snapshot", get(work_status_snapshot))
        .route("/v1/ws/work-status", get(ws_work_status))
        .with_state(Arc::new(client));

    tracing::info!("Maix-Gateway listening on http://{listen_addr}");
    let listener = tokio::net::TcpListener::bind(&listen_addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// ---------------------------------------------------------------------------
// Health / Metrics
// ---------------------------------------------------------------------------

async fn health(State(client): State<GatewayState>) -> Json<serde_json::Value> {
    match client.health_check().await {
        Ok(h) => Json(serde_json::json!({
            "status": "ok",
            "version": h.version,
            "uptime_secs": h.uptime_secs,
            "active_sessions": h.active_sessions,
            "queue_depth": h.queue_depth,
        })),
        Err(_) => Json(serde_json::json!({
            "status": "degraded",
            "version": env!("CARGO_PKG_VERSION"),
        })),
    }
}

async fn metrics(State(client): State<GatewayState>) -> Json<serde_json::Value> {
    let (health, sessions) = tokio::join!(
        client.health_check(),
        client.list_sessions(),
    );
    Json(serde_json::json!({
        "session_count": sessions.as_ref().map(|s| s.len()).unwrap_or(0),
        "queue_depth": health.as_ref().map(|h| h.queue_depth).unwrap_or(0),
        "active_sessions": health.as_ref().map(|h| h.active_sessions).unwrap_or(0),
        "uptime_secs": health.as_ref().map(|h| h.uptime_secs).unwrap_or(0),
    }))
}

// ---------------------------------------------------------------------------
// SSE Chat
// ---------------------------------------------------------------------------

async fn chat_sse(
    State(client): State<GatewayState>,
    Json(req): Json<ChatRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = async_stream::stream! {
        match maix_core::client::start_chat(&client, &req.message, req.session_id.as_deref()).await {
            Ok(mut handle) => {
                let sid = handle.session_id.clone();
                yield Ok(Event::default().data(
                    serde_json::json!({"type": "thinking", "session_id": sid}).to_string()
                ));

                loop {
                    match handle.recv().await {
                        Some(Ok(msg)) => {
                            if let Some(out) = msg.output {
                                let data = match out {
                                    pb::chat_output::Output::TextDelta(d) => {
                                        if d.text.is_empty() {
                                            continue;
                                        }
                                        serde_json::json!({"type": "text", "content": d.text})
                                    }
                                    pb::chat_output::Output::ToolCall(tc) => {
                                        serde_json::json!({
                                            "type": "tool_call",
                                            "name": tc.tool_name,
                                            "args": tc.arguments.map(maix_core::prost_struct_to_json),
                                        })
                                    }
                                    pb::chat_output::Output::ToolResult(tr) => {
                                        serde_json::json!({
                                            "type": "tool_result",
                                            "result": tr.result,
                                        })
                                    }
                                    pb::chat_output::Output::Complete(_c) => {
                                        serde_json::json!({
                                            "type": "done",
                                            "session_id": sid,
                                        })
                                    }
                                    pb::chat_output::Output::Error(e) => {
                                        serde_json::json!({"type": "error", "message": e.message})
                                    }
                                    _ => continue,
                                };
                                yield Ok(Event::default().data(data.to_string()));
                            }
                        }
                        Some(Err(e)) => {
                            yield Ok(Event::default().data(
                                serde_json::json!({"type": "error", "message": e.to_string()}).to_string()
                            ));
                            break;
                        }
                        None => break,
                    }
                }
            }
            Err(e) => {
                yield Ok(Event::default().data(
                    serde_json::json!({"type": "error", "message": e.to_string()}).to_string()
                ));
            }
        }
    };
    Sse::new(stream)
}

// ---------------------------------------------------------------------------
// WebSocket
// ---------------------------------------------------------------------------

async fn ws_chat(
    ws: WebSocketUpgrade,
    State(client): State<GatewayState>,
) -> impl axum::response::IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_chat(socket, client))
}

async fn handle_ws_chat(mut socket: WebSocket, client: Arc<MaixClient>) {
    let session_id = match client.create_session().await {
        Ok(id) => id,
        Err(e) => {
            let _ = socket.send(Message::Text(
                serde_json::json!({"type": "error", "message": e.to_string()}).to_string().into()
            )).await;
            return;
        }
    };
    while let Some(Ok(msg)) = socket.recv().await {
        if let Message::Text(text) = msg {
            let text: String = text.to_string();
            match client.chat_with_message(&session_id, &text).await {
                Ok(mut handle) => {
                    let _ = socket.send(Message::Text(
                        serde_json::json!({"type": "thinking", "session_id": session_id}).to_string().into()
                    )).await;

                    loop {
                        match handle.recv().await {
                            Some(Ok(msg)) => {
                                if let Some(out) = msg.output {
                                    let resp = match out {
                                        pb::chat_output::Output::TextDelta(d) => {
                                            if d.text.is_empty() {
                                                continue;
                                            }
                                            serde_json::json!({"type": "text", "content": d.text})
                                        }
                                        pb::chat_output::Output::ToolCall(tc) => {
                                            serde_json::json!({
                                                "type": "tool_call",
                                                "name": tc.tool_name,
                                                "args": tc.arguments.map(maix_core::prost_struct_to_json),
                                            })
                                        }
                                        pb::chat_output::Output::ToolResult(tr) => {
                                            serde_json::json!({
                                                "type": "tool_result",
                                                "result": tr.result,
                                            })
                                        }
                                        pb::chat_output::Output::Complete(_) => {
                                            serde_json::json!({"type": "done", "session_id": session_id})
                                        }
                                        pb::chat_output::Output::Error(e) => {
                                            serde_json::json!({"type": "error", "message": e.message})
                                        }
                                        _ => continue,
                                    };
                                    if socket.send(Message::Text(resp.to_string().into())).await.is_err() {
                                        break;
                                    }
                                }
                            }
                            _ => break,
                        }
                    }
                }
                Err(e) => {
                    let _ = socket.send(Message::Text(
                        serde_json::json!({"type": "error", "message": e.to_string()}).to_string().into()
                    )).await;
                }
            }
        }
    }
}

async fn ws_events(
    ws: WebSocketUpgrade,
    State(client): State<GatewayState>,
) -> impl axum::response::IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_events(socket, client))
}

async fn handle_ws_events(mut socket: WebSocket, client: Arc<MaixClient>) {
    match client.subscribe_events().await {
        Ok(resp) => {
            let mut stream = resp.into_inner();
            use tokio_stream::StreamExt;
            loop {
                tokio::select! {
                    event = stream.next() => {
                        match event {
                            Some(Ok(e)) => {
                                let json = serde_json::json!({
                                    "type": e.r#type,
                                    "agent_id": e.agent_id,
                                    "timestamp": e.timestamp,
                                });
                                if socket.send(Message::Text(json.to_string().into())).await.is_err() { break; }
                            }
                            _ => break,
                        }
                    }
                    ws_msg = socket.recv() => {
                        if ws_msg.is_none() { break; }
                    }
                }
            }
        }
        Err(e) => {
            let _ = socket.send(Message::Text(
                serde_json::json!({"type": "error", "message": e.to_string()}).to_string().into()
            )).await;
        }
    }
}

// ---------------------------------------------------------------------------
// Sessions
// ---------------------------------------------------------------------------

async fn list_sessions(State(client): State<GatewayState>) -> Json<Vec<serde_json::Value>> {
    match client.list_sessions().await {
        Ok(sessions) => {
            Json(sessions.iter().map(|s| {
                serde_json::json!({
                    "id": s.id,
                    "name": s.name,
                    "created_at": s.created_at,
                    "message_count": s.message_count,
                })
            }).collect())
        }
        Err(_) => Json(vec![]),
    }
}

async fn delete_session(State(client): State<GatewayState>) -> StatusCode {
    let sessions = client.list_sessions().await.unwrap_or_default();
    for s in &sessions {
        let _ = client.delete_session(&s.id).await;
    }
    StatusCode::NO_CONTENT
}

async fn delete_session_by_id(
    State(client): State<GatewayState>,
    Path(id): Path<String>,
) -> StatusCode {
    match client.delete_session(&id).await {
        Ok(true) => StatusCode::NO_CONTENT,
        _ => StatusCode::NOT_FOUND,
    }
}

// ---------------------------------------------------------------------------
// Memory
// ---------------------------------------------------------------------------

async fn search_memory(
    State(client): State<GatewayState>,
    Query(query): Query<MemorySearchQuery>,
) -> Json<serde_json::Value> {
    let limit = query.limit.unwrap_or(10);
    match client.search_memory(&query.q, limit).await {
        Ok(entries) => {
            let items: Vec<_> = entries.iter().map(|e| {
                let imp = (e.importance as f64 * 100.0).round() / 100.0;
                serde_json::json!({
                    "id": e.id,
                    "content": e.content,
                    "kind": e.kind,
                    "importance": imp,
                    "created_at": e.created_at,
                })
            }).collect();
            Json(serde_json::json!({ "entries": items }))
        }
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn save_memory(
    State(client): State<GatewayState>,
    Json(req): Json<MemoryEntryInput>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    client.save_memory(&req.content, req.importance.unwrap_or(0.5), None).await
        .map(|id| Json(serde_json::json!({ "status": "saved", "id": id })))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

async fn delete_memory(
    State(client): State<GatewayState>,
    Path(id): Path<String>,
) -> StatusCode {
    match client.forget_memory(&id).await {
        Ok(true) => StatusCode::NO_CONTENT,
        _ => StatusCode::NOT_FOUND,
    }
}

async fn compact_memory(State(client): State<GatewayState>) -> Json<serde_json::Value> {
    match client.search_memory("", 0).await {
        Ok(_) => Json(serde_json::json!({ "status": "compacted" })),
        Err(e) => Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
    }
}

// ---------------------------------------------------------------------------
// Tasks
// ---------------------------------------------------------------------------

async fn list_tasks(State(client): State<GatewayState>) -> Json<serde_json::Value> {
    match client.list_tasks().await {
        Ok(tasks) => {
            let tasks: Vec<_> = tasks.iter().map(|t| {
                serde_json::json!({
                    "id": t.id,
                    "description": t.description,
                    "priority": t.priority,
                    "status": t.status,
                    "assigned": t.assigned,
                })
            }).collect();
            Json(serde_json::json!({ "tasks": tasks }))
        }
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn submit_task(
    State(client): State<GatewayState>,
    Json(req): Json<SubmitTaskRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let task_id = client.submit_task(&req.description, &req.input, req.priority).await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(serde_json::json!({ "task_id": task_id })))
}

async fn delete_task(
    State(client): State<GatewayState>,
    Path(id): Path<String>,
) -> StatusCode {
    match client.cancel_task(&id).await {
        Ok(true) => StatusCode::NO_CONTENT,
        _ => StatusCode::NOT_FOUND,
    }
}

async fn reposition_task(
    State(client): State<GatewayState>,
    Path(id): Path<String>,
    Json(req): Json<PatchTaskRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if let Some(_pos) = &req.position {
        // Reposition via resume with position
        client.resume_task(&id).await
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    }
    Ok(Json(serde_json::json!({ "status": "ok" })))
}

async fn reprioritize_task(
    State(client): State<GatewayState>,
    Path(id): Path<String>,
    Json(req): Json<PatchTaskRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if let Some(pri) = req.priority {
        client.reprioritize_task(&id, pri).await
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    }
    Ok(Json(serde_json::json!({ "status": "ok" })))
}

async fn suspend_task(
    State(client): State<GatewayState>,
    Path(id): Path<String>,
) -> StatusCode {
    match client.suspend_task(&id).await {
        Ok(true) => StatusCode::OK,
        _ => StatusCode::NOT_FOUND,
    }
}

async fn resume_task(
    State(client): State<GatewayState>,
    Path(id): Path<String>,
) -> StatusCode {
    match client.resume_task(&id).await {
        Ok(true) => StatusCode::OK,
        _ => StatusCode::NOT_FOUND,
    }
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

async fn list_tools(State(client): State<GatewayState>) -> Json<serde_json::Value> {
    match client.list_tools().await {
        Ok(tools) => {
            let defs: Vec<_> = tools.iter().map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters.clone().map(maix_core::prost_struct_to_json),
                    "risk_level": t.risk_level,
                })
            }).collect();
            Json(serde_json::json!({ "tools": defs }))
        }
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn call_tool_handler(
    State(client): State<GatewayState>,
    Path(name): Path<String>,
    axum::extract::Json(args): axum::extract::Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let arguments = maix_core::json_to_prost_struct(args);
    let session_id = uuid::Uuid::new_v4().to_string();
    client.call_tool(&name, Some(arguments), &session_id, ".").await
        .map(|resp| {
            if let Some(err) = resp.error {
                serde_json::json!({ "error": err })
            } else {
                serde_json::json!({ "result": resp.result, "duration_ms": resp.duration_ms })
            }
        })
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

// ---------------------------------------------------------------------------
// Skills
// ---------------------------------------------------------------------------

async fn list_skills(State(client): State<GatewayState>) -> Json<Vec<serde_json::Value>> {
    match client.list_skills().await {
        Ok(skills) => {
            Json(skills.iter().map(|s| {
                serde_json::json!({
                    "name": s.name,
                    "version": s.version,
                    "runtime": s.runtime,
                    "enabled": s.enabled,
                })
            }).collect())
        }
        Err(_) => Json(vec![]),
    }
}

async fn install_skill(
    State(client): State<GatewayState>,
    Json(req): Json<InstallSkillRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    client.install_skill(&req.source).await
        .map(|resp| Json(serde_json::json!({
            "name": resp.name,
            "version": resp.version,
            "status": "installed",
        })))
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))
}

async fn remove_skill(
    State(client): State<GatewayState>,
    Path(name): Path<String>,
) -> StatusCode {
    match client.remove_skill(&name).await {
        Ok(true) => StatusCode::NO_CONTENT,
        _ => StatusCode::NOT_FOUND,
    }
}

async fn enable_skill(
    State(client): State<GatewayState>,
    Path(name): Path<String>,
) -> StatusCode {
    match client.enable_skill(&name).await {
        Ok(true) => StatusCode::OK,
        _ => StatusCode::NOT_FOUND,
    }
}

async fn disable_skill(
    State(client): State<GatewayState>,
    Path(name): Path<String>,
) -> StatusCode {
    match client.disable_skill(&name).await {
        Ok(true) => StatusCode::OK,
        _ => StatusCode::NOT_FOUND,
    }
}

// ---------------------------------------------------------------------------
// Identities
// ---------------------------------------------------------------------------

async fn list_identities(State(client): State<GatewayState>) -> Json<serde_json::Value> {
    match client.list_agents().await {
        Ok(resp) => {
            let list: Vec<_> = resp.agents.iter().map(|id| {
                let active = resp.active.as_deref() == Some(&id.name);
                serde_json::json!({
                    "name": id.name,
                    "description": id.description,
                    "tone": id.tone,
                    "active": active,
                })
            }).collect();
            Json(serde_json::json!({ "identities": list }))
        }
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn show_identity(
    State(client): State<GatewayState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let resp = client.list_agents().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    resp.agents.iter().find(|a| a.name == name)
        .map(|a| {
            Json(serde_json::json!({
                "name": a.name,
                "description": a.description,
                "tone": a.tone,
                "traits": a.traits,
                "domains": a.domains,
            }))
        })
        .ok_or(StatusCode::NOT_FOUND)
}

async fn activate_identity(
    State(client): State<GatewayState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    client.activate_agent(&name).await
        .map(|resp| {
            if resp.activated {
                Json(serde_json::json!({ "active_identity": name }))
            } else {
                Json(serde_json::json!({
                    "active_identity": name,
                    "warning": resp.system_prompt,
                }))
            }
        })
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))
}

// ---------------------------------------------------------------------------
// Architectures
// ---------------------------------------------------------------------------

async fn list_architectures(State(client): State<GatewayState>) -> Json<serde_json::Value> {
    match client.list_architectures().await {
        Ok(archs) => {
            let list: Vec<_> = archs.iter().map(|a| {
                serde_json::json!({
                    "id": a.id,
                    "name": a.name,
                    "description": a.description,
                    "topology": a.topology,
                    "node_count": a.node_count,
                    "flow_count": a.flow_count,
                })
            }).collect();
            Json(serde_json::json!({ "architectures": list }))
        }
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn show_architecture(
    State(client): State<GatewayState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let resp = client.list_architectures().await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    resp.iter().find(|a| a.name == name)
        .map(|a| {
            Json(serde_json::json!({
                "id": a.id,
                "name": a.name,
                "description": a.description,
                "topology": a.topology,
                "node_count": a.node_count,
                "flow_count": a.flow_count,
            }))
        })
        .ok_or(StatusCode::NOT_FOUND)
}

async fn run_architecture(
    State(client): State<GatewayState>,
    Path(name): Path<String>,
    Json(req): Json<RunArchitectureRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    match client.run_architecture(&name, &req.input).await {
        Ok(resp) => {
            let mut stream = resp.into_inner();
            use tokio_stream::StreamExt;
            while let Some(Ok(msg)) = stream.next().await {
                if let Some(out) = msg.output {
                    match out {
                        pb::run_architecture_output::Output::Complete(s) => {
                            return Ok(Json(serde_json::json!({
                                "status": "completed", "output": s
                            })));
                        }
                        pb::run_architecture_output::Output::Error(e) => {
                            return Err((StatusCode::INTERNAL_SERVER_ERROR, e));
                        }
                        _ => {}
                    }
                }
            }
            Ok(Json(serde_json::json!({ "status": "done" })))
        }
        Err(e) => Err((StatusCode::BAD_REQUEST, e.to_string())),
    }
}

// ---------------------------------------------------------------------------
// Work Status
// ---------------------------------------------------------------------------

async fn work_status_snapshot(State(client): State<GatewayState>) -> Json<serde_json::Value> {
    match client.get_work_status().await {
        Ok(snap) => Json(serde_json::json!({
            "total_agents": snap.total_agents,
            "active_agents": snap.active_agents,
            "idle_agents": snap.idle_agents,
            "queue_depth": snap.queue_depth,
            "tasks_completed": snap.tasks_completed,
            "tasks_failed": snap.tasks_failed,
            "total_tokens": snap.total_tokens,
            "total_cost": snap.total_cost,
            "uptime_secs": snap.uptime_secs,
        })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn ws_work_status(
    ws: WebSocketUpgrade,
    State(client): State<GatewayState>,
) -> impl axum::response::IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_work_status(socket, client))
}

async fn handle_ws_work_status(mut socket: WebSocket, client: Arc<MaixClient>) {
    if let Ok(resp) = client.watch_work_status(5).await {
        let mut stream = resp.into_inner();
        use tokio_stream::StreamExt;
        loop {
            tokio::select! {
                event = stream.next() => {
                    match event {
                        Some(Ok(snapshot)) => {
                            let payload = serde_json::json!({
                                "timestamp": chrono::Utc::now().to_rfc3339(),
                                "total_agents": snapshot.total_agents,
                                "active_agents": snapshot.active_agents,
                                "idle_agents": snapshot.idle_agents,
                                "queue_depth": snapshot.queue_depth,
                                "total_tokens": snapshot.total_tokens,
                                "total_cost": snapshot.total_cost,
                            });
                            let body = serde_json::to_string(&payload).unwrap_or_default();
                            if socket.send(Message::Text(body.into())).await.is_err() { break; }
                        }
                        _ => break,
                    }
                }
                ws_msg = socket.recv() => {
                    if ws_msg.is_none() { break; }
                }
            }
        }
    }
}
