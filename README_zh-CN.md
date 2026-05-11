<div align="center">

# Maix-Agent
> 一个混合了多种人工智能架构和组件的、具有强大记忆能力的、支持编程化的AI-Agent实现。

[![](https://img.shields.io/badge/Copyright-Maix--Agent-0066AA)](./COPYRIGHT)
[![](https://img.shields.io/badge/License-AGPL--3.0--or--later-yellow)](./LICENSE)
[![](https://img.shields.io/badge/Commercial-Closed--Source_Paid-red)](./COMMERCIAL.md)

[[English]](./README.md)
[[简体中文]](./README_zh-CN.md)

</div>

---

## 架构

```
maix_agent::Maix  (核心调度器)
  ├── maix.exe          CLI驱动（自给自足）
  ├── maix-tui.exe      TUI驱动（用户交互）
  └── maix-gateway.exe  HTTP驱动（REST/SSE/WebSocket）
```

核心调度器 `Maix` 位于 `maix-agent`。三个入口点均为薄封装 — TUI和Gateway不拥有Agent调度逻辑。

## 功能支持
- 单Agent操作本地工具（fs_read、fs_write、shell_exec、web_fetch）
- 多Agent并行/异步协作（Hierarchical / Collaborative / Debate）
- Plan / Agent / YOLO 三种模式自由切换
- 类人脑长期记忆系统（Episodic / Semantic / Working / SQLite存储）
- 多模型路由：自动检测任务类别，选择最佳LLM
- 可编程单Agent架构（TOML DSL，3种内置拓扑）
- 动态任务队列：优先级、依赖、位置插入
- 技能系统：maix-skill.toml + SKILL.md 双格式
- 身份人格系统：自然语言描述的身份设定，持久化存储
- 可编程多Agent协作拓扑
- 实时Agent工作状态查看（EventBus + WebSocket）
- MCP协议：JSON-RPC 2.0 client/server

## 快速开始

### 前置条件
- Windows / Linux / macOS
- DeepSeek API Key（或其他OpenAI兼容提供商）

### CLI
```bash
# 设置API Key
set MAIX_PROVIDERS_DEEPSEEK_API_KEY=sk-your-key

# 一次性问答
maix -m deepseek ask "解释Rust所有权"

# 交互式对话
maix chat

# 身份管理
maix identity list
maix identity use Architect

# 记忆管理
maix memory list
maix memory search "关键词"

# 架构DSL
maix architecture list
maix architecture show sequential
```

### TUI
```bash
# 双击 maix-tui.exe 启动配置向导
# 或从终端运行：
maix-tui
```

### HTTP网关
```bash
maix-gateway
# → http://localhost:26506/health
# → http://localhost:26506/v1/identities
# → http://localhost:26506/v1/architectures
```

## Crates（14个）
| Crate | 层级 | 说明 |
|-------|------|------|
| `maix-core` | 基础设施 | 共享类型、配置、错误、ModelRouter、Identity、Architecture DSL |
| `maix-db` | 基础设施 | SQLite（rusqlite bundled），7张表，WAL模式 |
| `maix-provider` | 领域层 | LLM提供商（DeepSeek、MiniMax、OpenAI兼容） |
| `maix-tools` | 领域层 | 内置工具 + MCP JSON-RPC client/server |
| `maix-memory` | 领域层 | MemoryStore trait、FileMemoryStore、SqliteMemoryStore |
| `maix-task-queue` | 领域层 | 优先级/依赖/位置队列，支持DB持久化 |
| `maix-skills` | 领域层 | 技能加载/安装/启用，TOML + Markdown双格式 |
| `maix-monitor` | 领域层 | EventBus（256通道）、Monitor、AgentEvent追踪 |
| `maix-agent` | 应用层 | 单Agent循环 + `Maix` facade（核心调度器） |
| `maix-multi-agent` | 应用层 | 多Agent编排器（3种模式） |
| `maix-cli` | 入口 | CLI（maix.exe），基于Maix的薄驱动 |
| `maix-tui` | 入口 | TUI（maix-tui.exe），配置向导，基于Maix的薄驱动 |
| `maix-gateway` | 入口 | HTTP网关（maix-gateway.exe），30+端点，基于Maix的薄驱动 |
| `maix-server` | 遗留 | （已被maix-gateway取代） |

## 技术栈
| 组件 | 技术 |
|------|------|
| 语言 | Rust（edition 2021） |
| 数据库 | rusqlite（bundled SQLite） |
| CLI | clap |
| TUI | ratatui + crossterm |
| HTTP | axum 0.8（REST + SSE + WebSocket） |
| 序列化 | serde + serde_json + toml |
| 异步 | tokio |
| 日志 | tracing |

## 项目结构
```
maix-agent/
├── crates/              # 14个工作区crates
├── config/              # default.toml
├── Cargo.toml           # 工作区根配置
├── Cargo.lock
├── Dockerfile
├── README.md            # 英文README
├── README_zh-CN.md      # 中文README（本文件）
├── README.ai.md         # AI可读清单（英文）
├── README_zh-CN.ai.md   # AI可读清单（中文）
└── Construction.md      # 架构图与变更记录
```

## 名称来源
`Maix` = `Max` + `Mix`，寓意「最大记忆能力」和「混合型架构」。

## 许可证
- **AGPL-3.0-or-later** 用于开源使用。详见 [LICENSE](./LICENSE)
- **商业闭源授权** 可用。详见 [COMMERCIAL.md](./COMMERCIAL.md)

## 链接
- [报告漏洞和提出期望](https://github.com/JularDepick/Maix-Agent/issues)
- [申请商用闭源](./COMMERCIAL.md)

## 致谢
- [DeepSeek-TUI](https://github.com/Hmbown/DeepSeek-TUI)
- [OpenHanako](https://github.com/liliMozi/openhanako)
