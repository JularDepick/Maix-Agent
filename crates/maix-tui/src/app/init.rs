//! App initialization logic.

use super::*;
use crate::input::InputState;

impl App {
    pub async fn new(
        client: MaixClient,
        session_id: String,
        mode: i32,
        server_addr: String,
        resume_session: Option<String>,
    ) -> Self {
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();

        // Discover custom commands
        let home = dirs_home();
        let project_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let custom_cmds = maix_agent::commands::discover_commands(&project_root, &home);
        let custom_cmd_names: Vec<String> = custom_cmds.iter().map(|c| format!("/{}", c.name)).collect();

        let (tool_defs, memories) = tokio::join!(
            client.list_tools(),
            client.search_memory("", 50),
        );

        let tool_defs = tool_defs.unwrap_or_default();
        let memories = memories.unwrap_or_default();

        // Fetch config from server to get model name and provider info
        let (model_name, provider_caps) = match client.get_config().await {
            Ok(cfg) => {
                let caps = ProviderCaps {
                    max_context: 1_000_000,
                    supports_reasoning: true,
                    supports_tool_use: true,
                };
                (format!("{}/{}", cfg.active_provider, cfg.model), caps)
            }
            Err(_) => ("unknown".to_string(), ProviderCaps::default()),
        };

        let mut messages = vec![ChatMessage::System(format!(
            "Maix-Agent TUI | {} | 模型: {} | 服务: {}",
            mode_name(mode),
            model_name,
            server_addr,
        ))];

        // Check first run and show welcome
        let is_first_run = !dirs_home().join(".maix").join("config.toml").exists();
        if is_first_run {
            messages.push(ChatMessage::System(
                "欢迎使用 Maix-Agent! 🎉\n\n\
                快速开始:\n\
                - 输入消息开始对话\n\
                - 输入 / 查看所有命令\n\
                - Ctrl+P 打开命令面板\n\
                - Ctrl+F 搜索对话\n\
                - /help 查看帮助\n\n\
                输入 /tutorial 开始交互式教程".into()
            ));
        }

        // Resume session if requested
        if let Some(sid) = &resume_session {
            match client.get_session_messages(sid, 100).await {
                Ok(msgs) => {
                    if msgs.is_empty() {
                        messages.push(ChatMessage::System(format!("会话 {sid} 中没有消息")));
                    } else {
                        messages.push(ChatMessage::System(format!(
                            "已恢复会话 {} ({} 条消息)",
                            &sid[..sid.len().min(8)],
                            msgs.len()
                        )));
                        for m in &msgs {
                            match m.role.as_str() {
                                "user" => messages.push(ChatMessage::User(m.content.clone())),
                                "assistant" => messages.push(ChatMessage::Assistant(m.content.clone())),
                                _ => messages.push(ChatMessage::System(m.content.clone())),
                            }
                        }
                    }
                }
                Err(e) => {
                    messages.push(ChatMessage::System(format!("恢复会话失败: {e}")));
                }
            }
        }

        let mut input = InputState::new();
        input.custom_commands = custom_cmd_names;

        // Build initial search index before moving messages
        let mut search_index = SearchIndex::default();
        search_index.rebuild(&messages);

        App {
            model_name,
            mode,
            messages,
            memories,
            tool_defs,
            input,
            active_panel: ActivePanel::Memory,
            selected_index: None,
            is_streaming: Arc::new(AtomicBool::new(false)),
            provider_caps,
            agent_state: Some("Idle".into()),
            status_detail: None,
            total_tokens: 0,
            prompt_tokens: 0,
            completion_tokens: 0,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            total_cost: 0.0,
            round_count: 0,
            cost_tracker: CostTracker::new(Pricing::default()),
            session_id: session_id.clone(),
            server_addr,
            chat_scroll: 0,
            scroll_target: 0,
            scroll_animation: 1.0,
            tick_count: 0,
            show_reasoning: false,
            client,
            event_tx,
            event_rx,
            should_quit: false,
            custom_cmds,
            vim: crate::vim::VimState::new(),
            notifier: Notifier::new(NotificationConfig::default()),
            stream_renderer: StreamRenderer::new().0,
            desk: AgentDesk::new(std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))),
            layout: LayoutManager::new(),
            diff_renderer: DiffRenderer::new(crate::diff_view::DiffMode::Unified),
            pane_layout: PaneLayout::single(crate::pane::PaneContent::Chat),
            select_mode: false,
            select_start: None,
            select_end: None,
            palette: crate::palette::CommandPalette::new(),
            search_mode: false,
            search_query: String::new(),
            search_results: Vec::new(),
            search_result_index: 0,
            show_timestamps: false,
            folded_messages: std::collections::HashSet::new(),
            panel_width: 30,
            fullscreen: false,
            token_rate: 0.0,
            last_token_count: 0,
            last_rate_update: std::time::Instant::now(),
            pending_tool_approvals: Vec::new(),
            auto_approve_round: false,
            current_tool_call: None,
            last_failed_tool: None,
            aliases: std::collections::HashMap::new(),
            show_dividers: true,
            sessions: vec![SessionTab::new(session_id.clone(), "会话 1".to_string())],
            active_session: 0,
            max_messages: 10000,
            reminders: Vec::new(),
            next_reminder_id: 1,
            theme: crate::ui::Theme::dark(),
            layout_preset: "standard".to_string(),
            shortcut_scheme: "standard".to_string(),
            command_usage: std::collections::HashMap::new(),
            session_start: std::time::Instant::now(),
            habits: Vec::new(),
            tool_permissions: std::collections::HashMap::new(),
            favorite_tools: Vec::new(),
            tool_stats: std::collections::HashMap::new(),
            tool_cache: std::collections::HashMap::new(),
            completion_learning: std::collections::HashMap::new(),
            tool_chains: Vec::new(),
            tool_templates: std::collections::HashMap::new(),
            network_requests: Vec::new(),
            checkpoints: Vec::new(),
            recording: None,
            debug_log: Vec::new(),
            focused_message: None,
            auto_scroll: true,
            message_tags: std::collections::HashMap::new(),
            pinned_messages: Vec::new(),
            session_notes: String::new(),
            command_history: Vec::new(),
            history_index: None,
            expanded_tool_calls: std::collections::HashSet::new(),
            message_references: std::collections::HashMap::new(),
            command_favorites: Vec::new(),
            archived_messages: Vec::new(),
            layout_presets: std::collections::HashMap::new(),
            code_snippets: std::collections::HashMap::new(),
            git_status: crate::git_status::GitStatus::detect(),
            workflows: std::collections::HashMap::new(),
            macro_recording: None,
            macros: std::collections::HashMap::new(),
            dirty_regions: vec![DirtyRegion::Full],
            last_render_frame: 0,
            frame_number: 0,
            search_index,
        }
    }
}
