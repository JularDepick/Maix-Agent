<div align="center">

# Maix-Agent
> A hybrid AI-Agent implementation with multiple AI architectures, powerful memory capabilities, and programmable features.

[![](https://img.shields.io/badge/Copyright-Maix--Agent-0066AA)](./COPYRIGHT)
[![](https://img.shields.io/badge/License-AGPL--3.0--or--later-yellow)](./LICENSE)
[![](https://img.shields.io/badge/Commercial-Closed--Source_Paid-red)](./COMMERCIAL.md)

[[English]](./README.md)
[[简体中文]](./README_zh-CN.md)

</div>

---

## Architecture

```
maix.exe  (Core engine, gRPC Server, Daemon)
  ├── maix-cli.exe      CLI client (stateless, gRPC)
  ├── maix-tui.exe      TUI client (interactive, gRPC)
  └── maix-gateway.exe  HTTP gateway (REST/SSE/WS, gRPC)
```

The core engine `maix.exe` runs as a daemon. All clients are stateless and communicate via gRPC.

## Features
- Single Agent operations for local tools (fs_read, fs_write, shell_exec, web_fetch)
- Multi-Agent parallel/async collaboration (Hierarchical / Collaborative / Debate)
- Three modes: Plan / Agent / YOLO with free switching
- Human-like long-term memory system (Episodic / Semantic / Working / SQLite storage)
- Multi-model routing: auto-detect task category, select best LLM
- Programmable single-Agent architecture (TOML DSL, 3 built-in topologies)
- Dynamic task queue: priority, dependencies, position insertion
- Skills system: maix-skill.toml + SKILL.md dual format
- Identity/personality system: natural language identity description, persistent storage
- Programmable multi-Agent collaboration topologies
- Real-time Agent work status viewing (EventBus + WebSocket)
- MCP protocol: JSON-RPC 2.0 client/server

## Quick Start

### Prerequisites
- Windows / Linux / macOS
- DeepSeek API Key (or other OpenAI-compatible provider)

### Configuration

Create `~/.maix/settings.json`:

```json
{
  "providers": {
    "anthropic": {
      "api_key": "sk-ant-xxx",
      "model": "claude-sonnet-4-20250514"
    },
    "openai": {
      "api_key": "sk-xxx",
      "model": "gpt-4o"
    },
    "deepseek": {
      "api_key": "sk-xxx",
      "model": "deepseek-chat",
      "base_url": "https://api.deepseek.com/v1"
    }
  }
}
```

Or use environment variables:

```bash
export MAIX_PROVIDERS_DEEPSEEK_API_KEY=sk-your-key
```

### CLI
```bash
# Set API Key
set MAIX_PROVIDERS_DEEPSEEK_API_KEY=sk-your-key

# One-time Q&A
maix -m deepseek ask "Explain Rust ownership"

# Interactive chat
maix chat

# Identity management
maix identity list
maix identity use Architect

# Memory management
maix memory list
maix memory search "keywords"

# Architecture DSL
maix architecture list
maix architecture show sequential
```

### TUI
```bash
# Double-click maix-tui.exe to start config wizard
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

## Crates (14)
| Crate | Layer | Description |
|-------|-------|-------------|
| `maix-core` | Infrastructure | Shared types, config, errors, ModelRouter, Identity, Architecture DSL |
| `maix-db` | Infrastructure | SQLite (rusqlite bundled), 7 tables, WAL mode |
| `maix-provider` | Domain | LLM providers (DeepSeek, MiniMax, OpenAI-compatible) |
| `maix-tools` | Domain | Built-in tools + MCP JSON-RPC client/server |
| `maix-memory` | Domain | MemoryStore trait, FileMemoryStore, SqliteMemoryStore |
| `maix-task-queue` | Domain | Priority/dependency/position queue with DB persistence |
| `maix-skills` | Domain | Skills loading/installing/enabling, TOML + Markdown dual format |
| `maix-monitor` | Domain | EventBus (256 channels), Monitor, AgentEvent tracking |
| `maix-agent` | Application | Agent runtime + multi-Agent orchestration |
| `maix-cli` | Client | CLI client, gRPC Client |
| `maix-tui` | Client | TUI terminal interface, gRPC Client |
| `maix-gateway` | Client | HTTP gateway, gRPC → HTTP conversion |
| `maix-server` | Core | maix.exe daemon, gRPC Server |

## Tech Stack
| Component | Technology |
|-----------|------------|
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
├── proto/               # Protobuf protocol definitions
├── Cargo.toml           # Workspace root config
├── Cargo.lock
├── Dockerfile
├── README.md            # English README (this file)
└── README_zh-CN.md      # Chinese README
```

## Name Origin
`Maix` = `Max` + `Mix`, meaning "maximum memory capability" and "hybrid architecture".

## License
- **AGPL-3.0-or-later** for open source use. See [[LICENSE]](./LICENSE)
- **Commercial closed-source licensing** available. See [[COMMERCIAL.md]](./COMMERCIAL.md)

## Links
- [[Report Issues and Requests]](https://github.com/JularDepick/Maix-Agent/issues)
- [[Apply for Commercial License]](./COMMERCIAL.md)

## Acknowledgments
- Thanks to open source projects [[DeepSeek-TUI]](https://github.com/Hmbown/DeepSeek-TUI) and [[OpenHanako]](https://github.com/liliMozi/openhanako) for providing implementation ideas and reference specifications for this project.
- Thanks to [[Xiaomi MiMo-V2.5 Series Open Source & Orbit Hundred Trillion Token Plan]]() for providing **1600M TOKEN** large model API service sponsorship for this project.
  <img src="./.github/image/MiMo-V2.5-API-Support.png"/>
- Thanks to [[DeepSeek Open Platform]](https://platform.deepseek.com) for providing high-quality, low-cost large model API service support for this project.
  <img src="./.github/image/DeepSeek-API-Support.png" width="50%"/>
- Thanks to [[Claude Code]](https://code.claude.com) for providing AI Agent programming support for this project.
