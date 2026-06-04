//! App streaming and event handling.

use super::*;

impl App {
    pub async fn run(
        &mut self,
        mut terminal: Terminal<impl ratatui::backend::Backend>,
    ) -> io::Result<()> {
        let mut crossterm_events = crossterm::event::EventStream::new();
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(50));
        let mut memory_refresh = tokio::time::interval(std::time::Duration::from_secs(3));

        loop {
            self.frame_number += 1;

            // Incremental rendering: only redraw if there are changes
            if self.has_changes() || self.is_streaming.load(std::sync::atomic::Ordering::Relaxed) {
                terminal.draw(|f| crate::ui::render(f, self))?;
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

                            // Search mode
                            if self.search_mode {
                                match key.code {
                                    KeyCode::Esc => {
                                        self.search_mode = false;
                                        self.search_query.clear();
                                        self.search_results.clear();
                                    }
                                    KeyCode::Enter => {
                                        self.search_mode = false;
                                    }
                                    KeyCode::Char(c) => {
                                        self.search_query.push(c);
                                        self.update_search_results();
                                    }
                                    KeyCode::Backspace => {
                                        self.search_query.pop();
                                        self.update_search_results();
                                    }
                                    KeyCode::Down
                                        if !self.search_results.is_empty() => {
                                            self.search_result_index = (self.search_result_index + 1) % self.search_results.len();
                                        }
                                    KeyCode::Up
                                        if !self.search_results.is_empty() => {
                                            self.search_result_index = if self.search_result_index == 0 {
                                                self.search_results.len() - 1
                                            } else {
                                                self.search_result_index - 1
                                            };
                                        }
                                    _ => {}
                                }
                                continue;
                            }

                            // Global shortcuts
                            match (key.code, key.modifiers) {
                                (KeyCode::Char('q'), KeyModifiers::CONTROL) => {
                                    self.should_quit = true;
                                }
                                (KeyCode::Char('f'), KeyModifiers::CONTROL) => {
                                    self.search_mode = true;
                                    self.search_query.clear();
                                    self.search_results.clear();
                                }
                                (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                                    // Command palette
                                    self.palette.toggle();
                                }
                                (KeyCode::Char('a'), KeyModifiers::CONTROL) => {
                                    self.select_mode = true;
                                    self.select_start = Some((self.messages.len().saturating_sub(1), 0));
                                    self.select_end = Some((self.messages.len().saturating_sub(1), 0));
                                }
                                (KeyCode::Char('r'), KeyModifiers::CONTROL) => {
                                    self.show_reasoning = !self.show_reasoning;
                                }
                                (KeyCode::Char('t'), KeyModifiers::CONTROL) => {
                                    self.show_timestamps = !self.show_timestamps;
                                }
                                (KeyCode::Char('l'), KeyModifiers::CONTROL) => {
                                    self.messages.clear();
                                    self.messages.push(ChatMessage::System("已清屏".into()));
                                }
                                (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
                                    // New session
                                    self.handle_slash_command("/session new").await;
                                }
                                (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                                    self.input.buffer.clear();
                                    self.input.cursor = 0;
                                }
                                (KeyCode::Char('w'), KeyModifiers::CONTROL) => {
                                    // Delete previous word
                                    let before = self.input.buffer[..self.input.cursor].to_string();
                                    let trimmed = before.trim_end();
                                    let last_space = trimmed.rfind(' ').map(|i| i + 1).unwrap_or(0);
                                    self.input.buffer.drain(last_space..self.input.cursor);
                                    self.input.cursor = last_space;
                                }
                                (KeyCode::Char('k'), KeyModifiers::CONTROL) => {
                                    self.input.buffer.truncate(self.input.cursor);
                                }
                                (KeyCode::F(1), _) => {
                                    self.handle_slash_command("/help").await;
                                }
                                (KeyCode::F(2), _) => {
                                    self.focused_message = self.focused_message
                                        .map(|i| i.saturating_sub(1))
                                        .or(Some(self.messages.len().saturating_sub(1)));
                                }
                                (KeyCode::F(3), _) => {
                                    self.focused_message = self.focused_message
                                        .map(|i| (i + 1).min(self.messages.len().saturating_sub(1)))
                                        .or(Some(0));
                                }
                                (KeyCode::F(4), _) => {
                                    self.focused_message = None;
                                }
                                (KeyCode::F(11), _) => {
                                    self.fullscreen = !self.fullscreen;
                                }
                                (KeyCode::Esc, _) => {
                                    if self.is_streaming.load(Ordering::Relaxed) {
                                        // Cancel streaming
                                        self.is_streaming.store(false, Ordering::SeqCst);
                                        self.agent_state = Some("Idle".into());
                                        self.status_detail = None;
                                        self.messages.push(ChatMessage::System("已中断生成".into()));
                                    } else if !self.input.buffer.is_empty() {
                                        self.input.buffer.clear();
                                        self.input.cursor = 0;
                                    } else {
                                        self.chat_scroll = 0;
                                    }
                                }
                                (KeyCode::PageUp, _) => {
                                    self.chat_scroll = self.chat_scroll.saturating_add(10);
                                }
                                (KeyCode::PageDown, _) => {
                                    self.chat_scroll = self.chat_scroll.saturating_sub(10).min(self.messages.len());
                                }
                                (KeyCode::Home, KeyModifiers::CONTROL) => {
                                    self.chat_scroll = self.messages.len();
                                }
                                (KeyCode::End, KeyModifiers::CONTROL) => {
                                    self.chat_scroll = 0;
                                }
                                _ => {
                                    // Pass to input handling
                                    self.handle_key_event(key).await;
                                }
                            }
                        }
                        Some(Ok(Event::Resize(_, _))) => {
                            self.mark_dirty(DirtyRegion::Full);
                        }
                        _ => {}
                    }
                }
                // Tick for animations and reminders
                _ = tick.tick() => {
                    self.tick_count += 1;

                    // Check reminders
                    let mut triggered = Vec::new();
                    for reminder in &mut self.reminders {
                        if !reminder.triggered && reminder.is_due() {
                            reminder.triggered = true;
                            triggered.push(reminder.clone());
                        }
                    }
                    for reminder in triggered {
                        self.messages.push(ChatMessage::System(format!(
                            "⏰ 提醒 #{}: {}", reminder.id, reminder.message
                        )));
                        self.notifier.task_complete(&format!("提醒: {}", reminder.message));
                        self.notifier.play_sound(crate::notify::NotifyKind::Success);
                    }

                    // Scroll animation
                    if self.scroll_animation < 1.0 {
                        self.scroll_animation = (self.scroll_animation + 0.1).min(1.0);
                    }

                    // Auto-save periodically
                    if self.tick_count.is_multiple_of(600) {
                        // Auto-save every 30 seconds (600 * 50ms)
                        let save_dir = dirs_home().join(".maix").join("autosave");
                        let _ = std::fs::create_dir_all(&save_dir);
                        let save_file = save_dir.join(format!("{}.json", self.session_id));
                        let data = serde_json::json!({
                            "session_id": self.session_id,
                            "messages": self.messages.iter().map(|m| match m {
                                ChatMessage::User(t) => serde_json::json!({"role": "user", "content": t}),
                                ChatMessage::Assistant(t) => serde_json::json!({"role": "assistant", "content": t}),
                                ChatMessage::System(t) => serde_json::json!({"role": "system", "content": t}),
                                _ => serde_json::json!({"role": "system", "content": ""}),
                            }).collect::<Vec<_>>(),
                            "saved_at": chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                        });
                        if let Err(e) = std::fs::write(&save_file, serde_json::to_string_pretty(&data).unwrap_or_default()) {
                            tracing::warn!("Failed to autosave chat: {e}");
                        }
                    }
                }
                // Memory refresh
                _ = memory_refresh.tick() => {
                    self.refresh_memories().await;
                    // Truncate old messages to prevent memory issues
                    self.truncate_messages();
                }
                // App events from spawned tasks
                Some(event) = self.event_rx.recv() => {
                    self.handle_app_event(event).await;
                }
            }
        }
    }

    pub async fn send_message(&mut self, text: String) {
        self.messages.push(ChatMessage::User(text.clone()));
        self.round_count += 1;
        if self.auto_scroll {
            self.chat_scroll = 0;
        }
        self.stream_renderer.clear();
        self.mark_dirty(DirtyRegion::Chat);
        self.mark_dirty(DirtyRegion::StatusBar);
        self.search_index.mark_dirty();

        // Save to command history
        if !text.is_empty() {
            self.command_history.push(text.clone());
            if self.command_history.len() > 500 {
                self.command_history.remove(0);
            }
        }

        // Auto-name session from first user message
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

    pub async fn handle_app_event(&mut self, event: AppEvent) {
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
                if self.messages.len().is_multiple_of(100) {
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
                self.current_tool_call = Some(ToolCallInfo {
                    name: name.clone(),
                    args: args.clone(),
                    start_time: std::time::Instant::now(),
                });
                if self.auto_approve_round {
                    self.messages.push(ChatMessage::ToolCall { name: name.clone(), args: args.clone() });
                } else {
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
                let elapsed_info = if let Some(tool_call) = self.current_tool_call.take() {
                    let elapsed = tool_call.start_time.elapsed();
                    let elapsed_str = if elapsed.as_secs() > 0 {
                        format!(" ({}.{:01}s)", elapsed.as_secs(), elapsed.subsec_millis() / 100)
                    } else {
                        format!(" ({}ms)", elapsed.as_millis())
                    };
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

                let mut error_lines = vec![
                    format!("✗ 错误"),
                    format!("  {}", e),
                ];

                if !fix_suggestion.is_empty() {
                    error_lines.push(String::new());
                    error_lines.push(format!("💡 建议: {}", fix_suggestion));
                }

                if e.contains("connection") || e.contains("timeout") {
                    error_lines.push(String::new());
                    error_lines.push("📍 上下文:".to_string());
                    error_lines.push(format!("  会话: {}", &self.session_id[..8.min(self.session_id.len())]));
                    error_lines.push(format!("  模型: {}", self.model_name));
                    error_lines.push(format!("  服务: {}", self.server_addr));
                }

                if e.len() > 100 {
                    error_lines.push(String::new());
                    error_lines.push("📋 按 Ctrl+A 选择, Ctrl+C 复制完整错误信息".to_string());
                }

                self.messages.push(ChatMessage::System(error_lines.join("\n")));
                self.agent_state = Some("Errored".into());
                self.status_detail = None;
                self.notifier.error(&e);
                self.notifier.play_sound(crate::notify::NotifyKind::Error);
            }
            AppEvent::StatusUpdate { state } => {
                self.status_detail = match state {
                    1 => None,
                    2 => Some("思考中".into()),
                    3 => Some("执行工具中".into()),
                    4 => Some("等待审批".into()),
                    5 => Some("生成回复中".into()),
                    6 => Some("更新记忆中".into()),
                    7 => Some("错误".into()),
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
