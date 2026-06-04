//! App navigation and input handling.

use super::*;

impl App {
    pub async fn handle_key_event(&mut self, key: KeyEvent) {
        // Vim mode
        if self.vim.enabled && self.input.buffer.is_empty() {
            match self.vim.handle_key(key, &mut self.input.cursor, &mut self.input.buffer) {
                crate::vim::VimAction::Passthrough => {}
                crate::vim::VimAction::None => return,
                crate::vim::VimAction::Submit => {
                    let text = self.input.buffer.trim().to_string();
                    if !text.is_empty() {
                        self.input.history.push(text.clone());
                        self.input.history_index = None;
                        self.input.buffer.clear();
                        self.input.cursor = 0;
                        self.input.completions.clear();

                        if text.starts_with('/') {
                            self.handle_slash_command(&text).await;
                        } else {
                            self.send_message(text).await;
                        }
                    }
                    return;
                }
                crate::vim::VimAction::Yank(_) => return,
                crate::vim::VimAction::SelectionChanged => return,
            }
        }

        match key.code {
            KeyCode::Enter => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    // Multi-line input
                    self.input.buffer.push('\n');
                    self.input.cursor = self.input.buffer.len();
                } else {
                    let text = self.input.buffer.trim().to_string();
                    if !text.is_empty() {
                        self.input.history.push(text.clone());
                        self.input.history_index = None;
                        self.input.buffer.clear();
                        self.input.cursor = 0;
                        self.input.completions.clear();

                        if text.starts_with('/') {
                            self.handle_slash_command(&text).await;
                        } else {
                            self.send_message(text).await;
                        }
                    }
                }
            }
            KeyCode::Char(c)
                if c.is_ascii_digit() && !self.input.completions.is_empty() =>
            {
                // Direct completion selection with 1-9
                let idx = (c as usize) - ('0' as usize);
                if idx > 0 && idx <= self.input.completions.len() {
                    self.input.buffer = self.input.completions[idx - 1].name.clone();
                    self.input.cursor = self.input.buffer.len();
                    self.input.completions.clear();
                }
            }
            KeyCode::Char(c) => {
                self.input.buffer.insert(self.input.cursor, c);
                self.input.cursor += 1;
                self.input.auto_complete();
            }
            KeyCode::Backspace
                if self.input.cursor > 0 => {
                    self.input.cursor -= 1;
                    self.input.buffer.remove(self.input.cursor);
                    self.input.auto_complete();
                }
            KeyCode::Delete
                if self.input.cursor < self.input.buffer.len() => {
                    self.input.buffer.remove(self.input.cursor);
                    self.input.auto_complete();
                }
            KeyCode::Left
                if self.input.cursor > 0 => {
                    self.input.cursor -= 1;
                }
            KeyCode::Right
                if self.input.cursor < self.input.buffer.len() => {
                    self.input.cursor += 1;
                }
            KeyCode::Home => {
                self.input.cursor = 0;
            }
            KeyCode::End => {
                self.input.cursor = self.input.buffer.len();
            }
            KeyCode::Up
                // History navigation
                if !self.input.history.is_empty() => {
                    let idx = self.input.history_index.unwrap_or(self.input.history.len());
                    if idx > 0 {
                        let new_idx = idx - 1;
                        self.input.history_index = Some(new_idx);
                        self.input.buffer = self.input.history[new_idx].clone();
                        self.input.cursor = self.input.buffer.len();
                    }
                }
            KeyCode::Down => {
                // History navigation
                if let Some(idx) = self.input.history_index {
                    if idx + 1 < self.input.history.len() {
                        let new_idx = idx + 1;
                        self.input.history_index = Some(new_idx);
                        self.input.buffer = self.input.history[new_idx].clone();
                        self.input.cursor = self.input.buffer.len();
                    } else {
                        self.input.history_index = None;
                        self.input.buffer.clear();
                        self.input.cursor = 0;
                    }
                }
            }
            KeyCode::Tab => {
                // Tab completion
                if !self.input.completions.is_empty() {
                    let idx = self.input.completion_index % self.input.completions.len();
                    self.input.buffer = self.input.completions[idx].name.clone();
                    self.input.cursor = self.input.buffer.len();
                    self.input.completion_index += 1;
                } else {
                    self.tab_next();
                }
            }
            KeyCode::BackTab => {
                self.tab_prev();
            }
            _ => {}
        }

        // Auto-complete on input change
        self.input.auto_complete();
    }

    pub fn update_search_results(&mut self) {
        self.search_results.clear();
        self.search_result_index = 0;

        if self.search_query.is_empty() {
            return;
        }

        if self.search_index.dirty {
            self.search_index.rebuild(&self.messages);
        }

        self.search_results = self.search_index.search(&self.search_query);
    }

    pub async fn handle_slash_command(&mut self, cmd: &str) {
        // Track command usage
        let cmd_name = cmd.split_whitespace().next().unwrap_or(cmd);
        *self.command_usage.entry(cmd_name.to_string()).or_insert(0) += 1;

        // Input validation
        if cmd.starts_with('/') {
            let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
            let command = parts[0];
            let args = parts.get(1).unwrap_or(&"").trim();

            match command {
                "/mode" if args.is_empty() => {
                    self.messages.push(ChatMessage::System("用法: /mode <plan|agent|yolo>".into()));
                    return;
                }
                "/model" if args.is_empty() => {
                    self.messages.push(ChatMessage::System("用法: /model <name>".into()));
                    return;
                }
                "/branch" if args.is_empty() => {
                    self.messages.push(ChatMessage::System("用法: /branch <name>".into()));
                    return;
                }
                "/tag" if args.is_empty() => {
                    self.messages.push(ChatMessage::System("用法: /tag <name>\n  /tag msg <index> <tag>".into()));
                    return;
                }
                "/template" if args.is_empty() => {
                    self.messages.push(ChatMessage::System("用法: /template <name>".into()));
                    return;
                }
                "/theme" if args.is_empty() => {
                    let themes = crate::ui::Theme::available_themes();
                    self.messages.push(ChatMessage::System(format!("用法: /theme <name>\n可用主题: {}", themes.join(", "))));
                    return;
                }
                "/remind" if args.is_empty() => {
                    self.messages.push(ChatMessage::System("用法: /remind <time> <message>\n示例: /remind 5m 检查构建".into()));
                    return;
                }
                "/checkpoint save" | "/checkpoint load" | "/checkpoint rm" if args.is_empty() => {
                    self.messages.push(ChatMessage::System(format!("用法: {} <name>", command)));
                    return;
                }
                _ => {}
            }
        }

        // Resolve aliases
        let _resolved_cmd = if cmd.starts_with('/') {
            let alias_name = cmd.split_whitespace().next().unwrap_or(cmd);
            if let Some(resolved) = self.aliases.get(alias_name) {
                let args = cmd[alias_name.len()..].trim();
                if args.is_empty() {
                    resolved.clone()
                } else {
                    format!("{} {}", resolved, args)
                }
            } else {
                cmd.to_string()
            }
        } else {
            cmd.to_string()
        };

        // Handle !! and !N history shortcuts
        let cmd = if cmd == "!!" {
            self.input.history.last().cloned().unwrap_or_default()
        } else if cmd.starts_with('!') && cmd[1..].chars().all(|c| c.is_ascii_digit()) {
            let idx: usize = cmd[1..].parse().unwrap_or(0);
            if idx > 0 && idx <= self.input.history.len() {
                self.input.history[idx - 1].clone()
            } else {
                cmd.to_string()
            }
        } else {
            cmd.to_string()
        };
        let cmd = cmd.as_str();

        // Check for custom commands
        if cmd.starts_with("/user:") || cmd.starts_with("/project:") {
            let (cmd_name, arguments) = match cmd.find(' ') {
                Some(pos) => (&cmd[..pos], &cmd[pos + 1..]),
                None => (cmd, ""),
            };
            let cmd_name_without_slash = &cmd_name[1..];
            if let Some(custom) = self.custom_cmds.iter().find(|c| c.name == cmd_name_without_slash) {
                let rendered = maix_agent::commands::render_command(custom, arguments);
                self.messages.push(ChatMessage::System(format!("执行自定义命令: {}", cmd_name)));
                self.send_message(rendered).await;
                return;
            }
        }

        match cmd {
            "/quit" | "/exit" => self.should_quit = true,
            "/mode plan" => {
                self.mode = MODE_PLAN;
                self.messages.push(ChatMessage::System("已切换到计划模式".into()));
            }
            "/mode agent" => {
                self.mode = MODE_AGENT;
                self.messages.push(ChatMessage::System("已切换到智能体模式".into()));
            }
            "/mode yolo" => {
                self.mode = MODE_YOLO;
                self.messages.push(ChatMessage::System("已切换到自主模式".into()));
            }
            "/memory" => {
                self.active_panel = ActivePanel::Memory;
                self.refresh_memories().await;
            }
            "/tools" => self.active_panel = ActivePanel::Tools,
            "/stats" => self.active_panel = ActivePanel::Stats,
            "/desk" => {
                let desk_info = self.desk.format_desk();
                self.messages.push(ChatMessage::System(format!("工作台:\n{}", desk_info)));
            }
            "/clear" => {
                self.messages.clear();
                self.messages.push(ChatMessage::System("已清空对话".into()));
            }
            "/timestamp" => {
                self.show_timestamps = !self.show_timestamps;
                let status = if self.show_timestamps { "开启" } else { "关闭" };
                self.messages.push(ChatMessage::System(format!("时间戳显示已{status}")));
            }
            "/fullscreen" => {
                self.fullscreen = !self.fullscreen;
                let status = if self.fullscreen { "开启" } else { "关闭" };
                self.messages.push(ChatMessage::System(format!("全屏模式已{status}")));
            }
            "/vim" => {
                self.vim.toggle();
                let status = if self.vim.enabled { "开启" } else { "关闭" };
                self.messages.push(ChatMessage::System(format!("Vim 模式已{status}")));
            }
            "/sound" => {
                let new_state = !self.notifier.sound_enabled();
                self.notifier.set_sound(new_state);
                let status = if new_state { "开启" } else { "关闭" };
                self.messages.push(ChatMessage::System(format!("声音提醒已{status}")));
            }
            "/divider" => {
                self.show_dividers = !self.show_dividers;
                let status = if self.show_dividers { "开启" } else { "关闭" };
                self.messages.push(ChatMessage::System(format!("消息分隔线已{status}")));
            }
            "/git" => {
                // Refresh git status
                self.git_status = crate::git_status::GitStatus::detect();
                match &self.git_status {
                    Some(git) => {
                        let info = format!(
                            "分支: {}\n暂存: {} 文件\n修改: {} 文件\n未跟踪: {} 文件\n状态: {}",
                            git.branch, git.staged, git.modified, git.untracked,
                            if git.dirty { "有未提交更改" } else { "干净" }
                        );
                        self.messages.push(ChatMessage::System(info));
                    }
                    None => {
                        self.messages.push(ChatMessage::System("不在 git 仓库中".into()));
                    }
                }
            }
            "/compact" => {
                let before = self.messages.len();
                let keep_count = 20;
                if self.messages.len() > keep_count {
                    let system_msgs: Vec<_> = self.messages.iter()
                        .enumerate()
                        .filter(|(_, m)| matches!(m, ChatMessage::System(_)))
                        .map(|(i, _)| i)
                        .collect();
                    let mut keep_indices: Vec<usize> = system_msgs;
                    let start = self.messages.len().saturating_sub(keep_count);
                    for i in start..self.messages.len() {
                        if !keep_indices.contains(&i) {
                            keep_indices.push(i);
                        }
                    }
                    keep_indices.sort();
                    keep_indices.dedup();

                    let new_messages: Vec<_> = keep_indices.iter().filter_map(|&i| self.messages.get(i).cloned()).collect();
                    let removed = before - new_messages.len();
                    self.messages = new_messages;
                    self.chat_scroll = 0;
                    self.message_references.clear();
                    self.messages.push(ChatMessage::System(format!(
                        "上下文已压缩: 移除 {} 条消息, 保留 {} 条",
                        removed, self.messages.len() - 1
                    )));
                } else {
                    self.messages.push(ChatMessage::System("上下文无需压缩".into()));
                }
            }
            "/sessions" => {
                match self.client.list_sessions().await {
                    Ok(sessions) => {
                        if sessions.is_empty() {
                            self.messages.push(ChatMessage::System("(没有已保存的会话)".into()));
                        } else {
                            let mut lines = vec!["已保存的会话:".to_string()];
                            for s in &sessions {
                                lines.push(format!(
                                    "  {} | {} | 消息: {} | {}",
                                    &s.id[..s.id.len().min(8)],
                                    s.name,
                                    s.message_count,
                                    s.updated_at
                                ));
                            }
                            self.messages.push(ChatMessage::System(lines.join("\n")));
                        }
                    }
                    Err(e) => {
                        self.messages.push(ChatMessage::System(format!("列出会话失败: {e}")));
                    }
                }
            }
            "/health" => {
                match self.client.health_check().await {
                    Ok(h) => {
                        let lines = ["健康状态:".to_string(),
                            format!("  状态: {}", h.status),
                            format!("  版本: {}", h.version),
                            format!("  运行时间: {}s", h.uptime_secs),
                            format!("  活跃会话: {}", h.active_sessions),
                            format!("  队列深度: {}", h.queue_depth)];
                        self.messages.push(ChatMessage::System(lines.join("\n")));
                    }
                    Err(e) => {
                        self.messages.push(ChatMessage::System(format!("健康检查失败: {e}")));
                    }
                }
            }
            "/cost" => {
                let tracker = &self.cost_tracker;
                let total = tracker.total_usage();
                let cache_pct = total.cache_hit_rate();
                let savings = tracker.total_cache_savings();
                let pricing = &tracker.pricing;

                let mut lines = vec![
                    "会话费用明细:".to_string(),
                    format!("  输入 token:    {:>10}    ¥{:.4}", self.prompt_tokens, (self.prompt_tokens.saturating_sub(self.cache_read_tokens)) as f64 * pricing.input_per_million / 1_000_000.0),
                    format!("  输出 token:    {:>10}    ¥{:.4}", self.completion_tokens, self.completion_tokens as f64 * pricing.output_per_million / 1_000_000.0),
                    format!("  缓存读取:     {:>10}    ¥{:.4}  ({:.1}%)", self.cache_read_tokens, self.cache_read_tokens as f64 * pricing.cache_read_per_million / 1_000_000.0, cache_pct),
                    format!("  缓存写入:     {:>10}    ¥{:.4}", self.cache_write_tokens, self.cache_write_tokens as f64 * pricing.cache_write_per_million / 1_000_000.0),
                    "  ────────────────────────────────".to_string(),
                    format!("  总计:                       ¥{:.4}", self.total_cost),
                    format!("  缓存节省:                   ¥{:.4}", savings),
                    format!("  轮次: {}", self.round_count),
                ];

                if !tracker.turns.is_empty() {
                    lines.push("".to_string());
                    lines.push("Per-turn 明细:".to_string());
                    for t in &tracker.turns {
                        lines.push(format!("  Turn {}: {} in / {} out / cache {} / ¥{:.4}",
                            t.turn + 1, t.usage.prompt_tokens, t.usage.output_tokens(), t.usage.cache_read_tokens, t.cost));
                    }
                }

                self.messages.push(ChatMessage::System(lines.join("\n")));
            }
            "/export" => {
                let mut markdown = String::new();
                markdown.push_str("# Maix-Agent 对话导出\n\n");
                markdown.push_str(&format!("会话ID: {}\n", self.session_id));
                markdown.push_str(&format!("模型: {}\n", self.model_name));
                markdown.push_str(&format!("时间: {}\n\n", chrono::Local::now().format("%Y-%m-%d %H:%M:%S")));

                for msg in &self.messages {
                    match msg {
                        ChatMessage::User(text) => {
                            markdown.push_str(&format!("## 用户\n\n{}\n\n", text));
                        }
                        ChatMessage::Assistant(text) => {
                            markdown.push_str(&format!("## 助手\n\n{}\n\n", text));
                        }
                        ChatMessage::System(text) => {
                            markdown.push_str(&format!("> {}\n\n", text));
                        }
                        _ => {}
                    }
                }

                let filename = format!("maix-chat-{}.md", chrono::Local::now().format("%Y%m%d-%H%M%S"));
                match std::fs::write(&filename, &markdown) {
                    Ok(_) => {
                        self.messages.push(ChatMessage::System(format!("已导出对话到: {}", filename)));
                    }
                    Err(e) => {
                        self.messages.push(ChatMessage::System(format!("导出失败: {}", e)));
                    }
                }
            }
            "/help" => {
                self.messages.push(ChatMessage::System(
                    "命令列表:\n\
                    \n\
                    /mode <plan|agent|yolo>  切换模式\n\
                    /model <name>            切换模型\n\
                    /vim                     切换 Vim 模式\n\
                    /init [force]            生成 MAIX.md 项目约定\n\
                    /compact                 压缩上下文\n\
                    /memory                  显示记忆面板\n\
                    /tools                   显示工具面板\n\
                    /stats                   显示统计面板\n\
                    /desk                    显示工作台\n\
                    /timestamp               开关时间戳\n\
                    /sessions                列出已保存会话\n\
                    /health                  健康检查\n\
                    /cost                    显示 token 用量与费用\n\
                    /export                  导出对话\n\
                    /sound                   开关声音提醒\n\
                    /theme <name>            切换主题\n\
                    /layout <preset>         切换布局\n\
                    /clear                   清空对话\n\
                    /quit                    退出\n\
                    \n\
                    快捷键:\n\
                    Esc                    中断生成/清空输入\n\
                    Shift+Enter            换行（多行输入）\n\
                    Tab                    循环补全\n\
                    Ctrl+U                 清空当前行\n\
                    Ctrl+W                 删除前一个单词\n\
                    Ctrl+K                 删除到行尾\n\
                    Ctrl+A                 文本选择模式\n\
                    Ctrl+F                 搜索对话\n\
                    Ctrl+P                 命令面板\n\
                    Ctrl+R                 显示/隐藏推理过程\n\
                    Ctrl+Q                 退出"
                        .into(),
                ));
            }
            other if other.starts_with("/mode ") => {
                self.messages.push(ChatMessage::System("未知模式。可选: /mode plan, /mode agent, /mode yolo".to_string()));
            }
            other => {
                let cmd_name = other.split_whitespace().next().unwrap_or(other);
                let all_commands = vec![
                    "/help", "/quit", "/exit", "/mode", "/model", "/vim", "/init",
                    "/compact", "/memory", "/tools", "/stats", "/desk",
                    "/timestamp", "/fullscreen", "/sessions", "/branch", "/tag", "/template", "/search",
                    "/session_stats", "/session merge", "/session compare", "/session replay", "/session share",
                    "/resume", "/cost", "/config", "/doctor", "/identity", "/architecture",
                    "/skill", "/task", "/health", "/export", "/clear", "/note", "/pin", "/task_add",
                    "/sound", "/remind", "/reminders", "/todo", "/theme", "/layout",
                    "/tutorial", "/quickstart", "/tips", "/usage", "/feedback", "/profile",
                    "/calendar", "/habit", "/tool_chain", "/tool_template", "/tool_parallel",
                    "/debug", "/net", "/checkpoint", "/record", "/perf",
                ];
                let suggestion = all_commands.iter()
                    .min_by_key(|cmd| levenshtein_distance(cmd_name, cmd))
                    .filter(|cmd| levenshtein_distance(cmd_name, cmd) <= 3);

                if let Some(suggested) = suggestion {
                    self.messages.push(ChatMessage::System(format!(
                        "未知命令: {other}。你是不是想输入: {}?", suggested
                    )));
                } else {
                    self.messages.push(ChatMessage::System(format!(
                        "未知命令: {other}。输入 /help 查看可用命令。"
                    )));
                }
            }
        }
    }
}
