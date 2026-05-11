# Maix-Agent 更新计划

基于 `docs/devOutline` 目标架构，结合当前代码库实现状态，将 v0.1.0 → v0.5.0 拆分为多期任务。每期完成后须构建测试、修复缺陷、确保功能正常再进入下一期。

---

## 第1期 — 核心基础设施精化 (v0.1.0 → v0.1.1)

**目标**: 补齐 maix-core / maix-db 缺失能力，修复现有问题，为 gRPC 改造打基础。

### 1.1 maix-core 权限系统
- 新建 `crates/maix-core/src/permissions.rs`
- 实现 `Permission` / `PermissionSet` 结构体
- 实现工具级、技能级、路径级三层权限校验

### 1.2 maix-core 公共 Trait
- 新建 `crates/maix-core/src/traits.rs`
- 定义 `ToolProvider` / `SkillProvider` / `MemoryProvider` 公共 trait 抽象

### 1.3 maix-db 补全
- `messages` 表写入接口（insert_message）
- `sessions` 表 message_count 自动更新
- 数据库连接池（r2d2 + rusqlite）

### 1.4 构建与测试
- `cargo build --release` 全量编译通过
- `cargo test` 全部测试通过
- 修复所有编译警告

---

## 第2期 — Protobuf 协议 + gRPC IPC 总线 (v0.1.1 → v0.2.0)

**目标**: 引入 prost + tonic，定义全部 .proto 接口，生成 Rust 代码，建立 gRPC IPC 通信基础。

### 2.1 Proto 文件编写
- 创建 `proto/` 目录，按 `5.交互层接口规范.md` 编写全部 .proto：
  - `common.proto` — 公共类型（UUID, Role, Message, ToolDef, TokenUsage）
  - `maix.proto` — CoreService RPC 定义（生命周期/会话/聊天/Agent/工具/记忆/任务/技能/架构/事件/状态）
  - `session.proto` — 会话管理消息
  - `tool.proto` — 工具调用消息
  - `memory.proto` — 记忆操作消息
  - `task.proto` — 任务队列消息
  - `skill.proto` — 技能管理消息
  - `agent.proto` — 多Agent架构消息

### 2.2 maix-core 集成 prost + tonic-build
- 添加依赖: `prost`, `prost-types`, `tonic`, `tonic-build`
- 添加 `build.rs` — tonic-build 自动生成 proto 代码
- `lib.rs` 中 re-export 生成代码
- 将现有 Rust 类型与 proto 类型建立双向转换 trait（`From` / `Into`）

### 2.3 maix-core 版本更新
- crate 版本 0.1.0 → 0.2.0

### 2.4 构建与测试
- `cargo build -p maix-core` 编译通过
- `cargo test -p maix-core` 测试通过
- protobuf 生成代码可正常使用

---

## 第3期 — gRPC 服务端实现 + 守护进程 (v0.2.0 → v0.3.0)

**目标**: maix-server 从 HTTP 服务器改造为 gRPC 守护进程，实现 CoreService 全部 RPC 方法。

### 3.1 gRPC CoreService 实现
- `crates/maix-server/src/server.rs` — tonic gRPC server 实现
- 实现全部 CoreService RPC 方法（HealthCheck, CreateSession, Chat(stream), ListTools, CallTool, SearchMemory, SubmitTask 等）
- IPC 传输: Unix Socket (Linux/macOS) / Named Pipe (Windows)，可选 TCP
- gRPC 反射支持（tonic-reflection）

### 3.2 守护进程生命周期
- `crates/maix-server/src/daemon.rs` — 守护进程管理
- Foreground 模式（开发调试）
- Daemon 模式（Linux daemonize / Windows Service）
- 优雅关闭（等待当前任务完成后退出）

### 3.3 客户端自动拉起
- `crates/maix-server/src/auto_launch.rs`
- 客户端启动时检测 maix.exe 是否运行
- 未运行时自动后台拉起守护进程
- 等待 IPC socket/pipe 就绪后连接

### 3.4 构建与测试
- `cargo build -p maix-server` 编译通过
- gRPC 服务启动、HealthCheck、基本 Chat 流测试通过

---

## 第4期 — 客户端层 gRPC 解耦 (v0.3.0 → v0.4.0)

**目标**: 三个客户端从直接依赖 maix-agent 改为仅依赖 maix-core (proto types) + tonic client，实现架构解耦。

### 4.1 maix-cli 重构
- 移除对 `maix-agent` 的依赖
- 替换为 tonic gRPC client stub → 连接 maix.exe
- 保持所有原有子命令功能（chat, ask, memory, config, identity, architecture, skill）
- Chat 双向流实现

### 4.2 maix-tui 重构
- 移除对 `maix-agent` / `maix-provider` / `maix-tools` / `maix-memory` 的直接依赖
- 替换为 tonic gRPC client stub
- 保持 TUI 界面布局（Chat + Memory/Tools/Stats 面板）
- 流式输出适配

### 4.3 maix-gateway 重构
- 作为 gRPC→HTTP 桥接层（不拥有业务逻辑）
- gRPC 双向流 → SSE / WebSocket 转换
- REST 端点 → gRPC unary call 转换
- 路由表对齐 `5.交互层接口规范.md` 第五节

### 4.4 构建与测试
- `cargo build --release` 全量编译通过（全部 14 crate）
- `cargo test` 全部测试通过
- CLI 连接 daemon → Chat 端到端测试通过
- Gateway HTTP 端点手动测试通过

---

## 第5期 — 领域层安全强化 (v0.4.0 → v0.4.1)

**目标**: 实现多层纵深沙箱防护，补全技能载入器，增强工具安全。

### 5.1 maix-tools 工作目录沙箱
- `crates/maix-tools/src/sandbox.rs` — 路径沙箱校验
- `path-absolutize` 路径规范化 + 穿越检测
- 禁止 `../` 穿越、符号链接跳转
- 限制所有文件操作在 `~/.maix/workspace/` 内
- 安装目录只读白名单

### 5.2 技能载入器补全
- `crates/maix-skills/src/loaders/` 目录
  - `rust_loader.rs` — 内置 Rust 技能载入器
  - `md_loader.rs` — SKILL.md 声明式技能载入器（已有 from_skill_md）
  - `exe_loader.rs` — 独立可执行文件载入器（子进程 stdio/gRPC 通信）
  - `bash_loader.rs` — Bash 脚本载入器（引导 Python/NodeJS）
- 载入器注册表 `loader_registry.rs`

### 5.3 Skill Scheduler 统一调度器
- `crates/maix-skills/src/scheduler.rs`
- 权限校验 → 工作目录沙箱拦截 → 参数校验 → 超时限流 → 审计日志

### 5.4 外部技能进程池
- 独立子进程隔离运行
- 资源限制（超时、内存限制）
- 进程崩溃不影响核心引擎

### 5.5 构建与测试
- `cargo test -p maix-tools` 沙箱测试
- `cargo test -p maix-skills` 载入器测试

---

## 第6期 — 用户目录 + 安装包 + 发布 (v0.4.1 → v0.5.0)

**目标**: 实现双目录分离、安装包规范、发布打包。

### 6.1 用户目录初始化
- `~/.maix/` 首次运行时自动创建
- 按 `i+1.用户目录结构规范.md` 创建完整目录树
- 出厂配置 `config/default.toml` 模板
- 默认 Agent 模板、架构模板

### 6.2 安装包生成（Windows）
- 构建 `maix.exe`, `maix-cli.exe`, `maix-tui.exe`, `maix-gateway.exe`
- 生成安装目录布局
- Windows Service 注册脚本
- 发布包打包（zip/msi 可选）

### 6.3 发布文档
- RELEASE_NOTES.md
- 快速开始指南
- 配置文件说明

### 6.4 全链路测试
- 守护进程启动 → 客户端连接 → Chat 交互 → 工具调用 → 记忆读写
- 多客户端并发连接测试
- 异常恢复测试

---

## 版本路线图总结

| 版本 | 里程碑 | 关键交付 |
|------|--------|---------|
| v0.1.0 | 当前基线 | 14 crate 同进程函数调用架构 |
| v0.1.1 | 基础设施精化 | 权限系统、公共Trait、DB连接池 |
| v0.2.0 | Protobuf + gRPC | proto定义、代码生成、类型转换 |
| v0.3.0 | gRPC守护进程 | CoreService实现、IPC传输、守护生命周期 |
| v0.4.0 | 客户端解耦 | CLI/TUI/Gateway 仅依赖proto types |
| v0.4.1 | 安全强化 | 沙箱、技能载入器、进程隔离 |
| v0.5.0 | 发布 | 安装包、文档、全链路测试 |
