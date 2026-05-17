# Maix-Agent 使用指南

## 目录

- [架构总览](#架构总览)
- [安装与配置](#安装与配置)
- [CLI 命令行](#cli-命令行)
- [TUI 终端界面](#tui-终端界面)
- [HTTP 网关](#http-网关)
- [Agent 模式](#agent-模式)
- [记忆系统](#记忆系统)
- [技能系统](#技能系统)
- [身份系统](#身份系统)
- [多Agent协作](#多agent协作)
- [MCP协议](#mcp协议)
- [配置参考](#配置参考)

---

## 架构总览

```
maix.exe        核心引擎，gRPC Server，守护进程
  ├── maix-cli.exe      CLI客户端（无状态，gRPC）
  ├── maix-tui.exe      TUI客户端（交互式，gRPC）
  └── maix-gateway.exe  HTTP网关（REST/SSE/WS，gRPC）
```

核心引擎 `maix.exe` 以守护进程运行在 `127.0.0.1:26506`，所有客户端通过 gRPC 通信。

---

## 安装与配置

### 1. 启动核心引擎

```cmd
maix.exe
```

### 2. 配置 API 密钥

创建 `~/.maix/settings.json`：

```json
{
  "providers": {
    "deepseek": {
      "api_key": "sk-your-key",
      "model": "deepseek-chat",
      "base_url": "https://api.deepseek.com/v1"
    },
    "openai": {
      "api_key": "sk-your-key",
      "model": "gpt-4o"
    }
  }
}
```

或使用环境变量：

```cmd
set MAIX_PROVIDERS_DEEPSEEK_API_KEY=sk-your-key
```

### 3. 验证连接

```cmd
maix-cli.exe doctor
```

---

## CLI 命令行

### 基本用法

```cmd
maix-cli.exe <命令> [选项]
```

可简写为 `maix`。

### 核心命令

#### ask / q - 提问

```cmd
# 一次性问答
maix ask "解释Rust所有权"

# 指定模式
maix ask --mode plan "设计一个REST API"

# 指定模型
maix ask --model deepseek "你好"

# 从文件读取上下文
maix ask --file main.rs "解释这段代码"

# 显示推理过程
maix ask --verbose "为什么天空是蓝的"

# JSON输出
maix ask --output-format json "总结一下"

# 非交互模式（直接输出）
maix ask --print "简单回答"

# 继续上次会话
maix ask --continue "继续刚才的讨论"
```

#### chat - 交互式对话

```cmd
maix chat
```

进入交互式对话模式，输入 `/exit` 退出。

#### init - 初始化项目

```cmd
maix init
```

自动生成 `MAIX.md` 项目文件，检测项目类型并配置。

### 记忆管理

```cmd
# 列出所有记忆
maix memory list

# 搜索记忆
maix memory search "关键词"

# 保存记忆
maix memory save "重要信息"

# 压缩记忆
maix memory compact
```

### 身份管理

```cmd
# 列出所有身份
maix identity list

# 查看身份详情
maix identity show Architect

# 激活身份
maix identity activate Architect
```

### 架构管理

```cmd
# 列出可用架构
maix architecture list

# 查看架构详情
maix architecture show sequential

# 运行架构
maix architecture run sequential "设计一个微服务系统"
```

### 技能管理

```cmd
# 列出已安装技能
maix skill list

# 安装技能
maix skill install ./my-skill

# 启用/禁用技能
maix skill enable my-skill
maix skill disable my-skill
```

### 会话管理

```cmd
# 列出会话
maix session list

# 查看会话详情
maix session show <session-id>

# 删除会话
maix session delete <session-id>
```

### 任务管理

```cmd
# 列出任务
maix task list

# 提交任务
maix task submit "分析代码库"

# 取消任务
maix task cancel <task-id>
```

### 工具管理

```cmd
# 列出可用工具
maix tool list

# 调用工具
maix tool call <tool-name> <args>
```

### 配置管理

```cmd
# 查看配置
maix config show

# 设置配置
maix config set providers.deepseek.api_key sk-xxx

# 验证配置
maix config validate

# 导出配置
maix config export backup.json

# 导入配置
maix config import backup.json

# 对比配置
maix config diff
```

### 系统检查

```cmd
# 健康检查
maix health

# 诊断问题
maix doctor
```

---

## TUI 终端界面

### 启动

```cmd
maix-tui.exe
```

首次启动会进入配置向导。

### 界面布局

```
┌─────────────────────────────────────────┐
│  Chat Panel (主对话区)                   │
│                                         │
├─────────────────────────────────────────┤
│  Input Area (输入区)                     │
└─────────────────────────────────────────┘
```

侧边栏可切换：Memory / Tools / Stats / Desk

### 快捷键

| 快捷键 | 功能 |
|--------|------|
| `Ctrl+P` | 命令面板（模糊搜索） |
| `Ctrl+F` | 搜索模式 |
| `Ctrl+V` | 粘贴剪贴板图片 |
| `Ctrl+Tab` | 切换会话 |
| `Ctrl+T` | 切换时间戳显示 |
| `F11` | 全屏切换 |
| `Esc` | 退出当前模式 |

### Vim 模式

支持 Normal / Insert / Visual 三种模式切换。

### 斜杠命令

| 命令 | 功能 |
|------|------|
| `/help` | 显示帮助 |
| `/compact` | 压缩上下文 |
| `/clear` | 清空对话 |
| `/mode` | 切换模式 |
| `/git status` | 查看Git状态 |
| `/undo` | 撤销操作 |
| `/task list` | 查看任务列表 |

### 自定义命令

在 `~/.maix/commands/` 或 `.maix/commands/` 目录下创建 `.md` 文件：

```markdown
# my-command

这是一个自定义命令，参数：$ARGUMENTS
```

使用：`/my-command 参数`

### Desk 工作区

便签式工作区，用于记录临时想法和任务。

---

## HTTP 网关

### 启动

```cmd
maix-gateway.exe
```

默认端口：26507

### API 端点

#### 对话

| 端点 | 方法 | 说明 |
|------|------|------|
| `/v1/chat` | POST | SSE 流式对话 |
| `/v1/ws/chat` | WebSocket | WebSocket 对话 |
| `/v1/ws/events` | WebSocket | 事件流 |

#### 会话

| 端点 | 方法 | 说明 |
|------|------|------|
| `/v1/sessions` | GET | 列出会话 |
| `/v1/sessions/:id` | GET | 获取会话详情 |
| `/v1/sessions/:id` | DELETE | 删除会话 |

#### 记忆

| 端点 | 方法 | 说明 |
|------|------|------|
| `/v1/memory/search` | POST | 搜索记忆 |
| `/v1/memory/save` | POST | 保存记忆 |
| `/v1/memory/:id` | DELETE | 删除记忆 |
| `/v1/memory/compact` | POST | 压缩记忆 |

#### 任务

| 端点 | 方法 | 说明 |
|------|------|------|
| `/v1/tasks` | GET | 列出任务 |
| `/v1/tasks` | POST | 提交任务 |
| `/v1/tasks/:id/cancel` | POST | 取消任务 |
| `/v1/tasks/:id/suspend` | POST | 暂停任务 |
| `/v1/tasks/:id/resume` | POST | 恢复任务 |
| `/v1/tasks/:id/reprioritize` | POST | 调整优先级 |

#### 工具

| 端点 | 方法 | 说明 |
|------|------|------|
| `/v1/tools` | GET | 列出工具 |
| `/v1/tools/call` | POST | 调用工具 |

#### 技能

| 端点 | 方法 | 说明 |
|------|------|------|
| `/v1/skills` | GET | 列出技能 |
| `/v1/skills/install` | POST | 安装技能 |
| `/v1/skills/:id/enable` | POST | 启用技能 |
| `/v1/skills/:id/disable` | POST | 禁用技能 |

#### 身份

| 端点 | 方法 | 说明 |
|------|------|------|
| `/v1/identities` | GET | 列出身份 |
| `/v1/identities/:id` | GET | 获取身份详情 |
| `/v1/identities/:id/activate` | POST | 激活身份 |

#### 架构

| 端点 | 方法 | 说明 |
|------|------|------|
| `/v1/architectures` | GET | 列出架构 |
| `/v1/architectures/:id` | GET | 获取架构详情 |
| `/v1/architectures/:id/run` | POST | 运行架构 |

#### 状态

| 端点 | 方法 | 说明 |
|------|------|------|
| `/v1/work-status` | GET | 获取工作状态快照 |
| `/v1/work-status/stream` | WebSocket | 实时状态流 |
| `/health` | GET | 健康检查 |

### 平台桥接

支持 Telegram 和 Feishu（飞书）平台，通过 webhook 接入。

配置示例（`settings.json`）：

```json
{
  "bridges": {
    "telegram": {
      "bot_token": "your-bot-token",
      "webhook_url": "https://your-domain/webhook/telegram",
      "allowed_users": ["user_id_1"]
    },
    "feishu": {
      "app_id": "your-app-id",
      "app_secret": "your-app-secret",
      "webhook_url": "https://your-domain/webhook/feishu"
    }
  }
}
```

---

## Agent 模式

### Agent 模式（默认）

平衡模式，适合大多数任务。Agent 会根据需要使用工具，但会请求用户确认。

```cmd
maix ask --mode agent "帮我分析这个代码库"
```

### Plan 模式

生成多步骤计划，每个步骤有状态追踪：

- Draft → Approved → Executing → Completed
- 每步状态：Pending → InProgress → Done / Failed / Skipped

```cmd
maix ask --mode plan "设计一个微服务架构"
```

适合复杂任务，可以审查和修改计划后再执行。

### YOLO 模式

自主模式，最小化确认，适合快速执行。

```cmd
maix ask --mode yolo "快速修复这个bug"
```

---

## 记忆系统

### 三种记忆类型

| 类型 | 说明 | 作用域 |
|------|------|--------|
| Episodic | 情景记忆，记录对话历史 | 每次会话 |
| Semantic | 语义记忆，存储知识 | 全局 |
| Working | 工作记忆，临时草稿 | 当前会话 |

### 存储方式

- 文件存储：`~/.maix/memory/`
- SQLite 存储：`~/.maix/memory.db`

### 使用方法

```cmd
# 搜索记忆
maix memory search "Rust所有权"

# 保存重要信息
maix memory save "项目使用 Rust 2021 edition"

# 压缩记忆（清理冗余）
maix memory compact

# 列出所有记忆
maix memory list
```

### 自动特性

- 重要性评分：自动评估记忆重要性
- 嵌入检索：基于向量相似度搜索
- 自动压缩：定期清理低价值记忆

---

## 技能系统

### 技能格式

支持两种格式：

1. **maix-skill.toml** - 技能清单文件
2. **SKILL.md** - Markdown 格式技能

### 安装技能

```cmd
# 从本地路径安装
maix skill install ./my-skill

# 从 GitHub 安装
maix skill install github:user/repo
```

### 技能清单示例

```toml
[skill]
name = "my-skill"
version = "1.0.0"
runtime = "native"  # wasm / native / prompt-only

[prompt]
system = "你是一个专业助手"

[tools]
allowed = ["fs_read", "fs_write"]

[sandbox]
fs_permissions = ["./workspace"]
```

### 管理技能

```cmd
# 列出已安装技能
maix skill list

# 启用技能
maix skill enable my-skill

# 禁用技能
maix skill disable my-skill
```

---

## 身份系统

### 概念

身份是命名的 Agent 配置文件，包含：

- 描述（description）
- 语气（tone）
- 特征（traits）
- 领域专长（domain expertise）

### 使用身份

```cmd
# 列出可用身份
maix identity list

# 查看身份详情
maix identity show Architect

# 激活身份
maix identity activate Architect
```

激活后，Agent 会以该身份的特征和专长回应。

---

## 多Agent协作

### 预定义角色

| 角色 | 说明 |
|------|------|
| architect | 架构师，负责系统设计 |
| coder | 程序员，负责代码实现 |
| reviewer | 审查员，负责代码审查 |
| researcher | 研究员，负责调研分析 |

### 架构拓扑

支持可编程的多Agent协作拓扑：

```cmd
# 列出可用架构
maix architecture list

# 查看架构详情
maix architecture show sequential

# 运行架构
maix architecture run sequential "设计一个电商系统"
```

### 内置拓扑

- **sequential** - 顺序执行
- **parallel** - 并行执行
- **hierarchical** - 层级协作

---

## MCP协议

### 概述

MCP（Model Context Protocol）是 JSON-RPC 2.0 协议，用于扩展工具能力。

### 作为客户端

连接外部 MCP 服务器：

```json
{
  "mcp": {
    "servers": {
      "my-server": {
        "command": "node",
        "args": ["server.js"]
      }
    }
  }
}
```

### 作为服务器

Maix-Agent 可以作为 MCP 服务器，暴露自己的工具。

### 工具桥接

`McpToolBridge` 将外部 MCP 工具桥接到本地工具注册表。

---

## 配置参考

### 配置层级

1. `config/system.toml` - 系统配置（服务器地址/端口）
2. `~/.maix/settings.json` - 用户配置（提供商、模型、功能）
3. `MAIX_*` 环境变量 - 环境覆盖

### system.toml

```toml
[server]
listen_addr = "127.0.0.1"
listen_port = 26506

[transport]
type = "grpc"  # grpc / http
```

### settings.json

```json
{
  "providers": {
    "deepseek": {
      "api_key": "sk-xxx",
      "model": "deepseek-chat",
      "base_url": "https://api.deepseek.com/v1"
    }
  },
  "agent": {
    "max_tool_rounds": 16,
    "context_threshold": 0.9,
    "mode": "agent"
  },
  "memory": {
    "dir": "",
    "backend": "sqlite"
  },
  "tools": {
    "shell_enabled": false
  }
}
```

### 环境变量

| 变量 | 说明 |
|------|------|
| `MAIX_PROVIDERS_DEEPSEEK_API_KEY` | DeepSeek API 密钥 |
| `MAIX_PROVIDERS_OPENAI_API_KEY` | OpenAI API 密钥 |
| `MAIX_AGENT_MODE` | 默认模式 |
| `MAIX_MEMORY_DIR` | 记忆存储目录 |
| `MAIX_TOOLS_SHELL_ENABLED` | 是否启用 Shell 工具 |

---

## 内置工具

| 工具 | 说明 |
|------|------|
| `fs_read` | 读取文件 |
| `fs_write` | 写入文件 |
| `shell_exec` | 执行 Shell 命令 |
| `web_fetch` | 获取网页内容 |

---

## 常见问题

### Q: 如何切换 API 提供商？

编辑 `~/.maix/settings.json`，添加新的 provider 配置。

### Q: 如何启用 Shell 工具？

在 `settings.json` 中设置：

```json
{
  "tools": {
    "shell_enabled": true
  }
}
```

### Q: 记忆存储在哪里？

默认在 `~/.maix/memory.db`（SQLite）或 `~/.maix/memory/`（文件）。

### Q: 如何查看 Agent 的推理过程？

使用 `--verbose` 参数：

```cmd
maix ask --verbose "你的问题"
```

### Q: TUI 如何粘贴图片？

在 Insert 模式下按 `Ctrl+V` 粘贴剪贴板中的图片。

---

## 更新日志

### v0.1.1

- TUI 优化
- Agent 功能增强
- 文档更新

### v0.1.0

- 初始发布
- CLI / TUI / HTTP 网关
- 记忆系统
- 技能系统
- 多Agent协作
