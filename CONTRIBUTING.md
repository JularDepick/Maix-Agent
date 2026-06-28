# 贡献指南

感谢你对 Maix-Agent 的关注。

## 开发环境

```bash
# 前置条件
# - Node.js >= 18
# - pnpm >= 8
# - Bun（构建二进制时需要）

# 克隆仓库
git clone https://github.com/JularDepick/Maix-Agent.git
cd Maix-Agent

# 安装依赖
pnpm install

# 配置环境变量
cp .env.example .env
# 编辑 .env 填入 API Key
```

## 常用命令

| 命令 | 说明 |
|:---:|:---:|
| `pnpm build` | Bun 交叉编译 TUI + Backend |
| `pnpm build:tui-shell` | Bun 交叉编译 TUI 壳 |
| `pnpm build:backend-solo` | Bun 交叉编译 Backend 独立 |
| `pnpm esbuild` | esbuild 打包 TUI + Backend |
| `pnpm typecheck` | 类型检查 |

## 构建流程

### esbuild 打包

```bash
pnpm esbuild
```

使用 esbuild 打包，输出到 `dist/`，用于快速构建。

### Bun 交叉编译

```bash
pnpm build
```

两阶段构建，生成独立 Windows 可执行文件：

```
阶段 1: esbuild 打包（处理 terminal-kit 兼容性）
  src/tui/app.ts ──► dist/esbuild-tui-with-backend/index.mjs

阶段 2: Bun 编译
  index.mjs ──► build/maix-agent-all-win-x64.exe（~95MB）
```

### 构建产物

| 文件 | 说明 |
|:---:|:---:|
| `build/maix-agent-all-win-x64.exe` | 主程序（TUI + Backend 捆绑） |
| `build/maix-agent-tui-win-x64.exe` | TUI 壳（通过 API 连接远程 Backend） |
| `build/maix-agent-backend-win-x64.exe` | Backend 独立（暴露 API） |

### 分发

将可执行文件 + `.env` 放在同一目录，运行可执行文件。

## 代码规范

- TypeScript strict mode，ESM 模块
- 代码内禁止使用 emoji
- 注释使用跨行注释，精简内容
- 面向对象设计，及时封装可复用对象
- 模块内具有复用价值的功能提取为独立文件

## 提交规范

- commit 标题包含版本号
- body 记录功能性变化（与上一版本比较）
- 提交前检查 `.gitignore` 是否覆盖敏感文件

## 分支策略

- `main`: 稳定发布分支
- `dev`: 开发分支
- 功能分支从 `dev` 创建，完成后合并回 `dev`

## 提交 Pull Request

1. Fork 本仓库
2. 从 `dev` 创建功能分支
3. 完成开发后运行 `pnpm typecheck` 确保类型正确
4. 提交 PR 到 `dev` 分支，说明变更内容

## 许可证

贡献的代码将遵循项目的 [AGPL-3.0-or-later](./LICENSE) 许可证。
商业闭源授权请参阅 [COMMERCIAL.md](./COMMERCIAL.md)。
