# Maix-Agent — AI-readable project manifest

> Auto-maintained. Updated every iteration.
> 中文版: README_zh-CN.ai.md | 架构: Construction.md

## Current version: v0.2.0-dev

### Architecture: 3 thin entry points → 1 central Maix orchestrator
```
┌────────────────────────────────────────────────────┐
│  maix_agent::Maix  (central orchestrator)          │
│  Config · ModelRouter · Agent · Memory · Skills    │
│  IdentityManager · Architecture DSL                │
└────────┬──────────────┬────────────────┬───────────┘
         │              │                │
    maix.exe        maix-tui.exe    maix-gateway.exe
    (CLI driver)    (TUI driver)    (HTTP driver)
    8.4 MB          8.3 MB          8.8 MB
```
TUI and Gateway do NOT own Agent logic — they drive `Maix` via its public API.

### Binary entry points
| Binary | Crate | Size | Purpose |
|--------|-------|------|---------|
| `maix.exe` | `maix-cli` | 8.4 MB | CLI: self-sufficient, drives Maix directly. `chat`, `ask`, `memory`, `config`, `identity`, `architecture`, `skill` |
| `maix-tui.exe` | `maix-tui` | 8.3 MB | TUI: double-click wizard, wraps Maix for user interaction |
| `maix-gateway.exe` | `maix-gateway` | 8.8 MB | HTTP Gateway: port 26506, 30+ endpoints, wraps Maix for HTTP access |

### Crate dependency graph (14 crates)
```
maix-core         — Config, MaixError, ModelRouter, Identity, Architecture DSL, util
maix-db           — [2.1] SQLite rusqlite (bundled), 7 tables, V1 migration
maix-provider     — LLM providers (OpenAICompat, DeepSeek, MiniMax), global HTTP client pool
maix-tools        — ToolRegistry (4 builtins) + MCP JSON-RPC client/server
maix-memory       — MemoryStore trait: FileMemoryStore + SqliteMemoryStore [2.1b]
maix-task-queue   — TaskQueue with priority/deps/position + JSON/DB persistence [2.1c]
maix-skills       — SkillRegistry + maix-skill.toml + SKILL.md [2.5]
maix-monitor      — EventBus (256 channel) + Monitor + AgentEvent tracking
maix-agent        — Single-agent loop (plan/agent/yolo) + Maix facade [2.7b]
maix-multi-agent  — Multi-agent orchestrator (Hierarchical/Collaborative/Debate)
maix-cli          — CLI: thin wrapper driving Maix [2.2][2.3][2.7]
maix-tui          — TUI: thin wrapper driving Maix, config wizard [2.8]
maix-server       — (legacy) HTTP server, superseded by maix-gateway
maix-gateway      — HTTP gateway: thin wrapper driving Maix, 30+ endpoints [2.8b]
```

### Full Phase completion
| Phase | Details | Tests |
|-------|---------|-------|
| 1: Foundation | core/provider/tools/memory/agent/cli/tui | 43 |
| 2.1: SQLite DB | maix-db + SqliteMemoryStore + TaskQueue DB | 49 |
| 2.2: Multi-model | ModelRouter + TaskCategory + CLI auto-routing | — |
| 2.3: Identity | Identity/IdentityManager + 3 defaults + CLI commands | 53 |
| 2.4: Architecture DSL | TOML-based agent topology: Sequential/Debate/Router | 57 |
| 2.5: SKILL.md | from_skill_md() parser + from_dir() auto-detect | — |
| 2.6: Monitoring | EventBus + Monitor snapshots + metrics endpoint | — |
| 2.7: CLI enhanced | identity list/use, UTF-8 console init, config show | — |
| 2.8: GUI prep | TUI double-click wizard, direct launch support | — |
| 3: Multi-agent | Orchestrator + 3 modes + task queue | — |
| 4: HTTP Server | axum 0.8 + 25+ endpoints + SSE/WS | — |
| 5: Polish | Pooling/persistence/masking/credentials/Docker | — |

### Features summary
- **3 modes**: Plan/Agent/YOLO with tool approval
- **4 builtin tools**: fs_read, fs_write, shell_exec, web_fetch
- **Memory**: Episodic/Semantic/Working + SQLite persistence
- **Model routing**: Auto-detect task category, route to best model
- **Identity system**: 3 personas (Maix, Code Reviewer, Architect)
- **Architecture DSL**: TOML-based custom multi-agent topologies
- **TUI**: Double-click quick-config, command-line temporary mode
- **Server**: Configurable address/port (default 26506), 30+ endpoints including identities/architectures
- **MCP**: JSON-RPC 2.0 client/server protocol
- **Skills**: maix-skill.toml + SKILL.md dual format with CLI management
- **Architecture DSL**: 3 built-in topologies + TOML roundtrip
- **CLI management**: `architecture list/show/run`, `skill install/list/enable/disable`
- **UTF-8 console**: Auto-set on Windows (fixes garbled output)

### Database (maix-db V1)
7 tables: memories, tasks, sessions, messages, identities, skills, agent_architectures

### Config
```toml
[providers.deepseek]  # DeepSeek-V4
[providers.minimax]   # MiniMax-M2.5
[server]              # listen_addr, listen_port (26506)
[agent]               # max_tool_rounds, context_threshold, mode
[memory]              # dir
[tools]               # shell_enabled
```

### Testing
- `cargo test` — 57 tests, 0 warnings
- `cargo build --release` — clean
- Server: `maix-server` → http://localhost:26506/health
- CLI: `maix -m deepseek ask "hello"`
- CLI identity: `maix identity list`
- CLI architecture: `maix architecture list | show <name> | run <name> <input>`
- CLI skill: `maix skill install <path> | list | enable/disable <name>`
