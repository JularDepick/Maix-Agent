# Maix-Agent 用户配置说明

配置文件路径: `~/.maix/settings.json`

## 配置模板

```json
{
  "provider": "my-provider",
  "api_key": "sk-xxx",
  "api_base": "https://api.example.com/v1",
  "model": "model-name",

  "agent": {
    "max_tool_rounds": 16,
    "context_threshold": 0.9,
    "mode": "agent",
    "auto_mode": {
      "enabled": false,
      "cheap_model": "gpt-4o-mini",
      "cheap_provider": "",
      "capable_model": "claude-sonnet-4-6",
      "capable_provider": "anthropic"
    }
  },

  "memory": {
    "dir": "",
    "max_episodic_entries": 500
  },

  "tools": {
    "shell_enabled": false,
    "mcp_servers": []
  },

  "hooks": {
    "PreToolUse": [
      {
        "matcher": "fs_write",
        "command": "echo 'about to write $MAIX_FILE_PATH'",
        "timeout_ms": 5000
      }
    ],
    "PostToolUse": [
      {
        "matcher": "fs_edit",
        "command": "prettier --write $MAIX_FILE_PATH",
        "timeout_ms": 30000
      }
    ],
    "Stop": [
      {
        "matcher": "",
        "command": "notify-send 'Maix task complete'"
      }
    ]
  },

  "env": {}
}
```

## 配置项说明

| 字段 | 类型 | 说明 |
|------|------|------|
| `provider` | string | 服务商名称（自定义标识） |
| `api_key` | string | API 密钥 |
| `api_base` | string | API 地址 |
| `model` | string | 模型名称 |

### agent — 智能体

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `max_tool_rounds` | number | `16` | 最大工具调用轮次 |
| `context_threshold` | number | `0.9` | 上下文压缩阈值 |
| `mode` | string | `"agent"` | 默认模式: agent / plan / yolo |

#### auto_mode — 自动模式路由

每轮对话根据任务复杂度自动选择 cheap（快速）或 capable（强力）模型。

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `enabled` | boolean | `false` | 启用自动模式路由 |
| `cheap_model` | string | `""` | 简单任务使用的快速模型 |
| `cheap_provider` | string | `""` | 快速模型的服务商（空则用默认） |
| `capable_model` | string | `""` | 复杂任务使用的强力模型 |
| `capable_provider` | string | `""` | 强力模型的服务商（空则用默认） |

路由逻辑:
- **Off** (简单): 问候、简短问题 → cheap_model
- **High** (复杂): 编码、调试、多步任务 → capable_model
- **Max** (深度推理): "think step by step"、架构设计 → capable_model

### memory — 记忆

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `dir` | string | `""` | 存储目录，空则 `~/.maix/memory` |
| `max_episodic_entries` | number | `500` | 最大条目数 |

### tools — 工具

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `shell_enabled` | boolean | `false` | 启用 Shell |
| `mcp_servers` | array | `[]` | MCP 服务器配置 |

#### mcp_servers 示例

```json
"mcp_servers": [
  {
    "name": "filesystem",
    "command": "npx",
    "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"],
    "env": {}
  }
]
```

### hooks — 生命周期钩子

在工具执行前后运行用户自定义命令。

| 钩子 | 触发时机 | 用途 |
|------|----------|------|
| `PreToolUse` | 工具执行前 | 拦截危险操作、日志记录 |
| `PostToolUse` | 工具执行后 | 自动格式化、通知 |
| `Stop` | Agent 循环结束 | 发送通知、清理 |

环境变量:
- `MAIX_TOOL_NAME` — 当前工具名
- `MAIX_FILE_PATH` — 操作的文件路径（如适用）
- `MAIX_TOOL_INPUT` — JSON 格式工具输入
- `MAIX_TOOL_OUTPUT` — JSON 格式工具输出（仅 PostToolUse）
- `MAIX_WORKING_DIR` — 工作目录

PreToolUse hook 非零退出码会阻止工具执行。

### env — 环境变量覆盖

在 settings.json 中定义环境变量，系统环境变量优先级更高。

## 环境变量

所有环境变量以 `MAIX_` 开头：

| 环境变量 | 对应配置 |
|----------|----------|
| `MAIX_API_KEY` | `api_key` |
| `MAIX_API_BASE` | `api_base` |
| `MAIX_MODEL` | `model` |
| `MAIX_PROVIDER` | `provider` |
| `MAIX_AGENT_MAX_TOOL_ROUNDS` | `agent.max_tool_rounds` |
| `MAIX_AGENT_CONTEXT_THRESHOLD` | `agent.context_threshold` |
| `MAIX_AGENT_MODE` | `agent.mode` |
| `MAIX_TOOLS_SHELL_ENABLED` | `tools.shell_enabled` |

优先级：系统环境变量 > settings.json env 字段 > settings.json 直接配置
