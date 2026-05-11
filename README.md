<div align="center">

# Maix-Agent
> A hybrid AI-Agent implementation with persistent memory, programmable architecture, and multi-model routing.

[![](https://img.shields.io/badge/Copyright-Maix--Agent-0066AA)](./COPYRIGHT)
[![](https://img.shields.io/badge/License-AGPL--3.0--or--later-yellow)](./LICENSE)
[![](https://img.shields.io/badge/Commercial-Closed--Source_Paid-red)](./COMMERCIAL.md)

[[English]](./README.md)
[[简体中文]](./README_zh-CN.md)

</div>

---

## Architecture

```
maix_agent::Maix  (central orchestrator)
  ├── maix.exe          CLI driver (self-sufficient)
  ├── maix-tui.exe      TUI driver (user interaction)
  └── maix-gateway.exe  HTTP driver (REST/SSE/WebSocket)
```

The core scheduler `Maix` lives in `maix-agent`. All three entry points are thin wrappers — TUI and Gateway do NOT own Agent scheduling logic.

## Features
- Single-agent with local tools (fs_read, fs_write, shell_exec, web_fetch)
- Multi-agent orchestration (Hierarchical / Collaborative / Debate)
- Plan / Agent / YOLO three modes
- Human-like persistent memory (Episodic / Semantic / Working / SQLite)
- Multi-model routing: auto-detect task category, route to best LLM
- Programmable single-agent architecture via TOML DSL (3 built-in topologies)
- Dynamic task queue with priority, dependencies, and position insertion
- Skill system: maix-skill.toml + SKILL.md dual format
- Identity system: natural-language persona definitions with persistence
- Programmable multi-agent collaboration topologies
- Real-time agent work status via EventBus + WebSocket
- MCP protocol: JSON-RPC 2.0 client/server

## Quick Start

### Prerequisites
- Windows / Linux / macOS
- DeepSeek API key (or any OpenAI-compatible provider)

### CLI
```bash
# Set API key
set MAIX_PROVIDERS_DEEPSEEK_API_KEY=sk-your-key

# One-shot question
maix -m deepseek ask "explain Rust ownership"

# Interactive chat
maix chat

# Identity management
maix identity list
maix identity use Architect

# Memory management
maix memory list
maix memory search "keyword"

# Architecture DSL
maix architecture list
maix architecture show sequential
```

### TUI
```bash
# Double-click maix-tui.exe for config wizard
# Or run from terminal:
maix-tui
```

### HTTP Gateway
```bash
maix-gateway
# → http://localhost:26506/health
# → http://localhost:26506/v1/identities
# → http://localhost:26506/v1/architectures
```

## Crates (14 total)
| Crate | Layer | Description |
|-------|-------|-------------|
| `maix-core` | Infrastructure | Shared types, config, error, ModelRouter, Identity, Architecture DSL |
| `maix-db` | Infrastructure | SQLite (rusqlite bundled), 7 tables, WAL mode |
| `maix-provider` | Domain | LLM providers (DeepSeek, MiniMax, OpenAICompat) |
| `maix-tools` | Domain | Builtin tools + MCP JSON-RPC client/server |
| `maix-memory` | Domain | MemoryStore trait, FileMemoryStore, SqliteMemoryStore |
| `maix-task-queue` | Domain | Priority/deps/position queue with DB persistence |
| `maix-skills` | Domain | Skill load/install/enable, TOML + Markdown dual format |
| `maix-monitor` | Domain | EventBus (256 chan), Monitor, AgentEvent tracking |
| `maix-agent` | Application | Agent runtime + multi-agent orchestrator + message router |
| `maix-cli` | Entry | CLI (maix.exe), thin driver over Maix |
| `maix-tui` | Entry | TUI (maix-tui.exe), config wizard, thin driver over Maix |
| `maix-gateway` | Entry | HTTP Gateway (maix-gateway.exe), 30+ endpoints, thin driver |
| `maix-server` | Legacy | (superseded by maix-gateway) |

## Tech Stack
| Component | Technology |
|-----------|-----------|
| Language | Rust (edition 2021) |
| Database | rusqlite (bundled SQLite) |
| CLI | clap |
| TUI | ratatui + crossterm |
| HTTP | axum 0.8 (REST + SSE + WebSocket) |
| Serialization | serde + serde_json + toml |
| Async | tokio |
| Logging | tracing |

## Project Structure
```
maix-agent/
├── crates/              # 13 workspace crates
├── config/              # default.toml
├── Cargo.toml           # workspace root
├── Cargo.lock
├── Dockerfile
├── README.md            # this file
├── README_zh-CN.md      # Chinese README
├── README.ai.md         # AI-readable manifest (EN)
├── README_zh-CN.ai.md   # AI-readable manifest (CN)
└── Construction.md      # Architecture diagrams & changelog
```

## Name
`Maix` = `Max` + `Mix` → maximum memory capacity + hybrid architecture.

## License
- **AGPL-3.0-or-later** for open-source use. See [LICENSE](./LICENSE)
- **Commercial closed-source** licensing available. See [COMMERCIAL.md](./COMMERCIAL.md)

## Links
- [Report bugs & feature requests](https://github.com/JularDepick/Maix-Agent/issues)
- [Commercial licensing](./COMMERCIAL.md)

## Acknowledgments
- [DeepSeek-TUI](https://github.com/Hmbown/DeepSeek-TUI)
- [OpenHanako](https://github.com/liliMozi/openhanako)
