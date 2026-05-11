# Maix-Agent — AI可读项目清单

> 自动维护。英文版: README.ai.md | 架构图: Construction.md

## 当前版本: v0.2.0-dev (Phase 2全部完成)

### 架构: 3个薄入口 → 1个核心Maix调度器
```
┌──────────────────────────────────────────────────┐
│  maix_agent::Maix  (核心调度器)                    │
│  Config · ModelRouter · Agent · Memory · Skills   │
│  IdentityManager · Architecture DSL               │
└────────┬──────────────┬──────────────┬───────────┘
         │              │              │
    maix.exe        maix-tui.exe    maix-gateway.exe
    (CLI驱动)       (TUI驱动)       (HTTP驱动)
    8.4 MB          8.3 MB          8.8 MB
```
TUI和Gateway不拥有Agent逻辑 — 它们通过Maix的公开API驱动核心。

### 二进制
| 文件 | 大小 | 说明 |
|------|------|------|
| `maix.exe` | 8.4 MB | CLI: 自给自足，直接驱动Maix。`chat`, `ask`, `memory`, `config`, `identity`, `architecture`, `skill` |
| `maix-tui.exe` | 8.3 MB | TUI: 双击快速配置，包装Maix提供用户交互 |
| `maix-gateway.exe` | 8.8 MB | HTTP网关: 端口26506，30+端点，包装Maix提供HTTP访问 |

### 全部功能
- 3种运行模式: Plan/Agent/YOLO
- 4个内置工具: fs_read, fs_write, shell_exec, web_fetch
- 持久记忆: Episodic/Semantic/Working + SQLite存储
- 多模型路由: 自动检测任务类别，选择最佳模型
- 身份系统: 3种人格 (Maix, Code Reviewer, Architect)
- 架构DSL: TOML自定义多Agent拓扑
- TUI快速配置: 双击启动配置向导，已配置则直接启动
- Server: 端口26506可配置，30+端点含身份/架构
- MCP协议: JSON-RPC 2.0 client/server
- 技能系统: maix-skill.toml + SKILL.md双格式，CLI管理
- 架构DSL: 3种内置拓扑 + TOML序列化
- CLI管理: `architecture list/show/run`, `skill install/list/enable/disable`
- UTF-8控制台: Windows自动设置（解决乱码问题）
- 架构文档: Construction.md实时维护

### 测试
- `cargo test` — 57 tests, 0 warnings
- `cargo build --release` — clean
- Server: `maix-server` → http://localhost:26506/health
- CLI: `maix -m deepseek ask "你好"`
- CLI身份: `maix identity list`
- CLI架构: `maix architecture list | show <name> | run <name> <input>`
- CLI技能: `maix skill install <path> | list | enable/disable <name>`
