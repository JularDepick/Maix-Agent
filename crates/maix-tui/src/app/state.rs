//! App state management functions.

use super::*;

impl App {
    /// Get context-aware command suggestions based on conversation content.
    pub fn get_context_suggestions(&self) -> Vec<String> {
        let mut suggestions = Vec::new();
        let recent_messages: Vec<&ChatMessage> = self.messages.iter().rev().take(10).collect();

        for msg in &recent_messages {
            let content = match msg {
                ChatMessage::User(text) | ChatMessage::Assistant(text) => text.to_lowercase(),
                _ => continue,
            };

            // Suggest /diff if code changes discussed
            if (content.contains("修改") || content.contains("改动") || content.contains("diff"))
                && !suggestions.contains(&"/diff".to_string()) {
                    suggestions.push("/diff".to_string());
                }

            // Suggest /compact if context might be long
            if (self.messages.len() > 100 || content.contains("上下文") || content.contains("token"))
                && !suggestions.contains(&"/compact".to_string()) {
                    suggestions.push("/compact".to_string());
                }

            // Suggest /todo if tasks mentioned
            if (content.contains("任务") || content.contains("todo") || content.contains("待办"))
                && !suggestions.contains(&"/todo".to_string()) {
                    suggestions.push("/todo".to_string());
                }

            // Suggest /remind if time-related
            if (content.contains("提醒") || content.contains("remember") || content.contains("别忘"))
                && !suggestions.contains(&"/remind".to_string()) {
                    suggestions.push("/remind".to_string());
                }

            // Suggest /export if sharing discussed
            if (content.contains("分享") || content.contains("导出") || content.contains("export"))
                && !suggestions.contains(&"/export".to_string()) {
                    suggestions.push("/export".to_string());
                }

            // Suggest /note if important info
            if (content.contains("重要") || content.contains("记录") || content.contains("note"))
                && !suggestions.contains(&"/note".to_string()) {
                    suggestions.push("/note add".to_string());
                }

            // Suggest /config if settings discussed
            if (content.contains("配置") || content.contains("设置") || content.contains("config"))
                && !suggestions.contains(&"/config".to_string()) {
                    suggestions.push("/config".to_string());
                }

            // Suggest /theme if appearance discussed
            if (content.contains("主题") || content.contains("颜色") || content.contains("theme"))
                && !suggestions.contains(&"/theme".to_string()) {
                    suggestions.push("/theme".to_string());
                }

            // Suggest /habit if habits discussed
            if (content.contains("习惯") || content.contains("habit") || content.contains("每天"))
                && !suggestions.contains(&"/habit".to_string()) {
                    suggestions.push("/habit".to_string());
                }

            // Suggest /calendar if dates discussed
            if (content.contains("日历") || content.contains("日程") || content.contains("calendar"))
                && !suggestions.contains(&"/calendar".to_string()) {
                    suggestions.push("/calendar".to_string());
                }
        }

        // Limit to top 5 suggestions
        suggestions.truncate(5);
        suggestions
    }

    /// Get smart history suggestions based on time patterns.
    pub fn get_smart_history_suggestions(&self) -> Vec<String> {
        let mut suggestions = Vec::new();
        let now = chrono::Local::now();
        let hour = now.hour();

        // Morning suggestions (6-10)
        if (6..10).contains(&hour) {
            if !self.command_usage.contains_key("/todo") {
                suggestions.push("/todo list - 查看今日待办".to_string());
            }
            if !self.command_usage.contains_key("/calendar") {
                suggestions.push("/calendar - 查看今日日程".to_string());
            }
        }

        // Afternoon suggestions (12-14)
        if (12..14).contains(&hour)
            && !self.command_usage.contains_key("/usage") {
                suggestions.push("/usage - 查看今日使用统计".to_string());
            }

        // Evening suggestions (18-22)
        if (18..22).contains(&hour) {
            if !self.command_usage.contains_key("/habit") {
                suggestions.push("/habit - 检查今日习惯完成情况".to_string());
            }
            if !self.command_usage.contains_key("/export") {
                suggestions.push("/export - 导出今日对话记录".to_string());
            }
        }

        // Session duration suggestions
        let session_duration = self.session_start.elapsed();
        if session_duration.as_secs() > 3600 && !self.command_usage.contains_key("/compact") {
            suggestions.push("/compact - 会话已超过1小时，建议压缩上下文".to_string());
        }

        // High token usage suggestions
        if self.total_tokens > 100000 && !self.command_usage.contains_key("/compact") {
            suggestions.push("/compact - token用量较高，建议压缩上下文".to_string());
        }

        // Low usage suggestions
        if self.command_usage.is_empty() && self.messages.len() > 10 {
            suggestions.push("/help - 查看可用命令".to_string());
            suggestions.push("/tutorial - 开始交互式教程".to_string());
        }

        suggestions.truncate(3);
        suggestions
    }

    pub fn current_panel_item_count(&self) -> usize {
        match self.active_panel {
            ActivePanel::Memory => self.memories.len(),
            ActivePanel::Tools => self.tool_defs.len(),
            ActivePanel::Stats => 0,
            ActivePanel::Desk => self.desk.sticky_notes.len() + self.desk.pinned_files.len() + self.desk.task_board.tasks.len(),
        }
    }

    pub fn tab_next(&mut self) {
        let count = self.current_panel_item_count();
        match self.selected_index {
            None => {
                // Input → first item (or next panel if empty)
                if count > 0 {
                    self.selected_index = Some(0);
                } else {
                    self.active_panel = self.active_panel.next();
                    if self.current_panel_item_count() > 0 {
                        self.selected_index = Some(0);
                    }
                }
            }
            Some(i) => {
                if i + 1 < count {
                    // Next item in current panel
                    self.selected_index = Some(i + 1);
                } else {
                    // Wrap to next panel
                    self.active_panel = self.active_panel.next();
                    let new_count = self.current_panel_item_count();
                    if new_count > 0 {
                        self.selected_index = Some(0);
                    } else {
                        self.selected_index = None;
                    }
                }
            }
        }
    }

    #[allow(dead_code)]
    pub fn navigate_up(&mut self) {
        if let Some(i) = self.selected_index {
            if i > 0 {
                self.selected_index = Some(i - 1);
            } else {
                // Wrap to input
                self.selected_index = None;
            }
        }
    }

    #[allow(dead_code)]
    pub fn navigate_down(&mut self) {
        let count = self.current_panel_item_count();
        if count == 0 {
            return;
        }
        match self.selected_index {
            None => self.selected_index = Some(0),
            Some(i) if i + 1 < count => self.selected_index = Some(i + 1),
            _ => {}
        }
    }

    pub fn select_move(&mut self, delta: i32) {
        if self.messages.is_empty() {
            return;
        }
        if let Some((msg_idx, _offset)) = self.select_end {
            let new_idx = (msg_idx as i32 + delta).max(0).min(self.messages.len() as i32 - 1) as usize;
            self.select_end = Some((new_idx, 0));
        }
    }

    pub fn select_extend(&mut self, delta: i32) {
        if let Some((msg_idx, offset)) = self.select_end {
            let msg_text = match &self.messages.get(msg_idx) {
                Some(ChatMessage::Assistant(t)) | Some(ChatMessage::User(t)) | Some(ChatMessage::System(t)) => t.as_str(),
                _ => return,
            };
            let char_count = msg_text.chars().count();
            let new_offset = (offset as i32 + delta).max(0).min(char_count as i32) as usize;
            self.select_end = Some((msg_idx, new_offset));
        }
    }

    pub fn copy_selection(&self) {
        let (start, end) = match (self.select_start, self.select_end) {
            (Some(s), Some(e)) => (s, e),
            _ => return,
        };
        let mut text = String::new();
        for idx in start.0..=end.0 {
            if let Some(msg) = self.messages.get(idx) {
                let msg_text = match msg {
                    ChatMessage::Assistant(t) | ChatMessage::User(t) | ChatMessage::System(t) => t.as_str(),
                    _ => continue,
                };
                // Convert char offsets to byte offsets
                let from_byte = if idx == start.0 {
                    msg_text.char_indices().nth(start.1).map(|(i, _)| i).unwrap_or(msg_text.len())
                } else {
                    0
                };
                let to_byte = if idx == end.0 {
                    let char_idx = end.1.min(msg_text.chars().count());
                    msg_text.char_indices().nth(char_idx).map(|(i, _)| i).unwrap_or(msg_text.len())
                } else {
                    msg_text.len()
                };
                if from_byte < to_byte && to_byte <= msg_text.len() {
                    text.push_str(&msg_text[from_byte..to_byte]);
                }
                if idx < end.0 {
                    text.push('\n');
                }
            }
        }
        if !text.is_empty() {
            // Copy to clipboard using platform command
            #[cfg(target_os = "windows")]
            {
                use std::io::Write;
                if let Ok(mut child) = std::process::Command::new("clip")
                    .stdin(std::process::Stdio::piped())
                    .spawn()
                {
                    if let Some(mut stdin) = child.stdin.take() {
                        // clip.exe expects UTF-16LE
                        let utf16: Vec<u16> = text.encode_utf16().collect();
                        let bytes: &[u8] = unsafe {
                            std::slice::from_raw_parts(utf16.as_ptr() as *const u8, utf16.len() * 2)
                        };
                        let _ = stdin.write_all(bytes);
                    }
                    let _ = child.wait();
                }
            }
            #[cfg(target_os = "macos")]
            {
                use std::io::Write;
                if let Ok(mut child) = std::process::Command::new("pbcopy")
                    .stdin(std::process::Stdio::piped())
                    .spawn()
                {
                    if let Some(mut stdin) = child.stdin.take() {
                        let _ = stdin.write_all(text.as_bytes());
                    }
                    let _ = child.wait();
                }
            }
            #[cfg(target_os = "linux")]
            {
                use std::io::Write;
                if let Ok(mut child) = std::process::Command::new("xclip")
                    .args(["-selection", "clipboard"])
                    .stdin(std::process::Stdio::piped())
                    .spawn()
                {
                    if let Some(mut stdin) = child.stdin.take() {
                        let _ = stdin.write_all(text.as_bytes());
                    }
                    let _ = child.wait();
                }
            }
        }
    }

    #[allow(dead_code)]
    pub async fn handle_selected_item(&mut self) {
        let idx = match self.selected_index {
            Some(i) => i,
            None => return,
        };
        match self.active_panel {
            ActivePanel::Memory => {
                if let Some(m) = self.memories.get(idx) {
                    self.messages.push(ChatMessage::System(format!(
                        "[{}] kind={} {}",
                        m.id.chars().take(8).collect::<String>(),
                        m.kind,
                        m.content,
                    )));
                }
            }
            ActivePanel::Tools => {
                if let Some(t) = self.tool_defs.get(idx) {
                    self.messages.push(ChatMessage::System(format!(
                        "{}: {} (risk={})",
                        t.name, t.description, t.risk_level,
                    )));
                }
            }
            ActivePanel::Stats => {}
            ActivePanel::Desk => {}
        }
    }

    pub fn tab_prev(&mut self) {
        match self.selected_index {
            None => {
                // Input → last item of previous panel
                self.active_panel = match self.active_panel {
                    ActivePanel::Memory => ActivePanel::Desk,
                    ActivePanel::Tools => ActivePanel::Memory,
                    ActivePanel::Stats => ActivePanel::Tools,
                    ActivePanel::Desk => ActivePanel::Stats,
                };
                let count = self.current_panel_item_count();
                if count > 0 {
                    self.selected_index = Some(count - 1);
                }
            }
            Some(0) => {
                // First item → back to input
                self.selected_index = None;
            }
            Some(i) => {
                self.selected_index = Some(i - 1);
            }
        }
    }

    pub async fn refresh_memories(&mut self) {
        if let Ok(mems) = self.client.search_memory("", 50).await {
            self.memories = mems;
        }
    }
}
