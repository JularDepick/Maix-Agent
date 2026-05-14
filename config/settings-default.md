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
    "mode": "agent"
  },

  "memory": {
    "dir": "",
    "max_episodic_entries": 500
  },

  "tools": {
    "shell_enabled": false
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

### memory — 记忆

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `dir` | string | `""` | 存储目录，空则 `~/.maix/memory` |
| `max_episodic_entries` | number | `500` | 最大条目数 |

### tools — 工具

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `shell_enabled` | boolean | `false` | 启用 Shell |

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
