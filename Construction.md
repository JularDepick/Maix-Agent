# Maix-Agent Construction / Architecture

> 架构变更记录。每次架构变动时更新。

## v0.2.0-dev 架构 (Phase 2 — 架构重构)

> **核心原则**: maix.exe 自给自足，TUI 和 Gateway 只做薄封装，不拥有 Agent 调度逻辑。

```
┌─────────────────────────────────────────────────────────────┐
│                   Central Orchestrator                       │
│  ┌──────────────────────────────────────────────────────┐   │
│  │               maix_agent::Maix                       │   │
│  │  Config · ModelRouter · Agent · Memory · Tools       │   │
│  │  IdentityManager · SkillRegistry · Architecture DSL  │   │
│  └──────────────────────────────────────────────────────┘   │
└────────┬──────────────────┬──────────────────────┬──────────┘
         │                  │                      │
         ▼                  ▼                      ▼
┌─────────────────────────────────────────────────────────────┐
│                     Entry Points (thin wrappers)             │
│  ┌──────────┐  ┌───────────┐  ┌────────────────────────┐   │
│  │maix-cli  │  │maix-tui   │  │maix-gateway            │   │
│  │maix.exe  │  │maix-tui   │  │maix-gateway.exe        │   │
│  │CLI驱动   │  │TUI驱动    │  │HTTP驱动                │   │
│  └──────────┘  └───────────┘  └────────────────────────┘   │
│  自给自足调度   用户交互封装    HTTP/WS访问封装              │
└─────────────────────────────────────────────────────────────┘
         │                                                 
┌───────┴─────────────────────────────────────────────────────┐
│                     Domain Layer                            │
│  ┌────────────┐ ┌───────────┐ ┌──────────┐ ┌───────────┐  │
│  │maix-provider│ │maix-tools │ │maix-memory│ │maix-skills│  │
│  │DeepSeek    │ │fs_read/   │ │File +     │ │TOML + MD  │  │
│  │MiniMax     │ │write/shell│ │SqliteStore│ │registry   │  │
│  │OpenAICompat│ │/web_fetch │ │+embedding │ │           │  │
│  └────────────┘ │+MCP       │ └──────────┘ └───────────┘  │
│                  └───────────┘                              │
│  ┌────────────┐ ┌───────────────┐                           │
│  │maix-monitor│ │maix-task-queue│                          │
│  │EventBus +  │ │priority/deps/ │                          │
│  │Snapshot    │ │insert+DB pers │                          │
│  └────────────┘ └───────────────┘                           │
└─────────────────────────────────────────────────────────────┘
        │
┌───────┴─────────────────────────────────────────────────────┐
│                     Infrastructure Layer                    │
│  ┌──────────┐  ┌──────────────────────────────────────┐    │
│  │ maix-db  │  │ maix-core (shared types)              │    │
│  │ SQLite   │  │ Config, Error, ModelRouter, Identity, │    │
│  │ 7 tables │  │ Architecture DSL, Credentials, Util   │    │
│  │ WAL mode │  └──────────────────────────────────────┘    │
│  └──────────┘                                              │
└─────────────────────────────────────────────────────────────┘
```

### Crate层数: 3层 (Application → Domain → Infrastructure)
### 入口点哲学: maix.exe 独立 → TUI/Gateway 依赖 maix-core

### 变更记录
- **v0.2.1-dev**: 架构重构 — 抽取 `Maix` facade 到 maix-agent，CLI/TUI/Gateway 均作为薄封装；修复 tool_call_id 缺失导致 API 400 错误；新增 maix-gateway 替代 maix-server
- **v0.2.0-dev**: 完成 Phase 2.1-2.8 全部子阶段

### 数据流
```
User Input → CLI/TUI/Gateway → Maix.agent.run()
  → assemble_messages() (system prompt + history + memory context)
  → Provider.chat_stream() (LLM API call)
  → Tool execution loop (max 16 rounds)
  → Memory update (episodic save)
  → Response → User
```
Maix 是所有入口点共享的单一调度器，避免逻辑重复。

### 记忆系统
```
MemoryStore trait
├── FileMemoryStore (Phase 1, JSONL files)
└── SqliteMemoryStore (Phase 2.1, SQLite)
    └── maix-db::Database
        ├── memories table (episodic/semantic/working)
        └── import_memory_jsonl() (Phase 1→2 migration)
```

### 多模型路由 (Phase 2.2)
```
User Input → detect_category() → TaskCategory
  ├── Chat → deepseek-chat
  ├── Coding → deepseek-v4-flash
  ├── Reasoning → deepseek-v4-pro
  ├── Research → deepseek-v4-pro
  └── FastReply → deepseek-chat
```

### 身份系统 (Phase 2.3)
```
IdentityManager
├── Maix (default, professional, general programming)
├── Code Reviewer (meticulous, constructive)
└── Architect (analytical, pragmatic)
```

### 架构DSL (Phase 2.4)
```
Architecture (TOML-based)
├── Sequential (analyzer → executor)
├── Debate (moderator ← debater_0, debater_1)
└── Router (router → coder | reasoner)
```
