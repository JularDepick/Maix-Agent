# Agent开发时守则细则

- 不得修改本守则内容,除非用户明确要求维护本守则,并且维护不得丢失本守则的细节
- 本守则所在文档可能存在绑定于具体项目的信息,需要根据项目更新维护这些信息(懒维护)
- Agent的思考过程和结果输出必须全程使用用户所使用的语言,除非系统限制或用户明确指定思考/输出的语言
- 新会话中,开始操作前,先确认有哪些读写工具可用,并选择合适可用的读写工具,避免因工具问题干扰后续工作
- 向用户确认本项目是否有缩写或简称,方便创建文件出现未明确名称时直接使用命名
- 每当用户追加新任务时,不要阻塞或打断旧任务,确保完成旧任务后再执行新任务
- 相对路径原则:项目各处涉及项目内路径问题优先使用相对路径,避免环境依赖,保证项目迁移部署后仍正常工作
- git权限分级:[读取] git log/status/diff 可随时使用;[写入] git add/commit/push/reset/amend 需用户当次对话明确授权（如"提交"、"push"、"合并"）,授权仅限本次请求,完成后立即失效,不得跨请求复用
- git提交规范:commit前先检查维护git忽略文件,commit标题包含版本号,body要记录功能性变化(与上一版本比较),用户未明确要求时不要动tag和release也不要push
- 维护任意md文档时,应该全量或逐段落加载文档内容,避免遗漏导致部分内容过时或有误
- 维护任意md文档时,允许改写、转换说法,但是不得丢失细节,除非用户明确提出额外要求
- 维护任意md文档时,自然语言描述部分尽量使用用户所使用的语言,避免非必要的英文表述,例如 `Phrase 1` 是非必要的,而专业术语 `MySQL` 是必要的
- 维护任意md文档时,如果使用到表格,应该默认使用 `|:---:|` 单元格居中
- 项目README文档的维护应该以 `README_zh-CN.md` 为核心,最后再翻译成英文版的 `README.md`
- 项目README文档内介绍功能特性的位置不要介绍非功能性的细节
- 项目README文档可以参考本文档内的非守则内容,但不要照搬,而是针对产品用户和社区协作者的群体特性选取
- 项目文档多语言版本维护规则:允许同一文档的不同语言版本之间通过链接相互跳转,跨文档链接要保证语言一致性(如果需要);例如 `README_zh-CN.md` 内允许通过链接跳转到不同语言版本的README,但是只链接到中文版的HELP文档(如果有)
- 遇到文档中有过时内容时,及时清理
- 当不确定版本号时,应该从git log查看后向用户确认正在开发的项目的版本号,不允许自动迭代版本号
- 只有在用户明确重新指定新版本号后才能弃用旧版本号,新版本号要及时同步到项目源码和文档各处
- 当项目最新状态不兼容旧版本(有冲突)时,仅提醒用户注意迭代版本号,但不做版本号迭代兜底
- 版本号迭代后,及时更新项目文档中的全量内容到最新状态
- 工作目录下,如果用户没有特别指定,那么 `src/` 就是功能性的源码目录,其余内容则是辅助性和说明性的内容,另外要明确目录结构,严禁混淆使用
- 必须明确开发技术栈,每当有变更技术栈的需求时需要提醒用户进行确认
- 代码中可个性化修改但不影响项目核心功能的设计细节(指后文定义的设计细节),需要以全局常量/宏/独立代码文件之一的形式隔离储存,便于开发者知悉和维护
- 代码内禁止使用emoji,并避免代码内的无用连续空白符
- 代码注释根据语言全部使用跨行注释,精简注释内容
- 合理组织代码保证代码结构化,避免结构混乱不利于后续开发
- 充分发挥面向对象思维,开发过程中及时封装对象
- 在模块内具有复用价值的对象和功能要提取成模板转移进独立代码文件,方便后续开发复用引入
- 不要撰写任何更新日志,不要储存项目状态,避免过期内容污染项目
- 如果用户明确要求写更新日志,应该以版本号和日期 `vx.x.x yyyy-MM-dd HH:mm` 为索引写入 `AGENTS-CHANGELOG.md` ,并阐明更新变动
- 需要写更新计划时,应该以目标版本号和日期 `vx.x.x yyyy-MM-dd HH:mm` 为索引写入 `AGENTS-UPDATES.md`
- 前端网页项目不要使用浏览器原生弹窗提醒,而是使用自定义飘窗提醒
- 前端网页项目不要使用浏览器原生弹窗进行二次确认,而是在原按钮上执行"替换为确认按钮-3s内点击确认-超时回归初始状态"流程
- 前端网页项目默认隐藏浏览器侧边滚动条(如果有),然后告知用户(允许用户回退该操作)
- 前端网页项目的表格、select控件默认文本水平居中
- 日期格式化:默认使用"yyyy-MM-dd HH:mm:ss+HH:mm"格式,除非用户明确指定使用别的格式
- 合理利用子代理(如果有)并行任务,以加快项目进程或避免已有上下文污染思考,注意设定子代理个数上限(默认5)避免并发超限
- 处理好子代理的上下文继承关系,已过期用不上的子代理及时关闭或销毁,避免占用资源
- 当项目多次尝试修复同一个问题未成功解决时,完整阅读所有代码后再动手
- 项目作者[JularDepick](https://github.com/JularDepick)
- 前后端项目请在后端代码注释头、每一个前端页面底部标注作者信息,并在控制前端页面的代码里定义宏或常量方便开发者动态替换前端页面作者信息

---

<!-- 以下内容由Agent自行维护到项目最新状态,供后继Agent会话继承开发 -->
<!-- 注释内容是文档或段落的内容结构相关说明和规范,本身也符合md语法,通常不需要维护和修改,且不计入开发时参考 -->

# 概述

Maix-Agent TUI 是基于 terminal-kit 的命令行 AI 编程助手，集成 OpenAI/Anthropic Provider，支持多会话、流式对话、工具调用、记忆系统、主题切换、多Agent协作、MCP协议等功能。架构上核心引擎（core/provider/agent/tools）与表现层（tui）分离，核心逻辑可复用于不同客户端。

# 技术栈
<!--
| th1 | th2 | ...
|:---:|:---:| ...
| td1 | td2 | ...
...
> 当项目技术栈发生变化时需要自主更新并告知用户
-->

| 类别 | 技术 |
|:---:|:---:|
| 运行时 | Node.js |
| 语言 | TypeScript (strict mode, ESM) |
| 终端 UI | terminal-kit |
| 数据库 | sql.js (SQLite WASM) |
| LLM SDK | openai, @anthropic-ai/sdk (^0.106.0) |
| Markdown 渲染 | marked, highlight.js |
| WebSocket | ws |
| 包管理 | pnpm |
| 构建工具 | Bun (交叉编译), esbuild (打包) |

# 架构
<!-- 主要指前后端架构、服务架构
> 当项目架构发生变化时需要自主更新并告知用户
-->

Backend 与 TUI 完全解耦，通过 API 适配层通信。构建脚本选择耦合方式：
- `tui-shell`：TUI 通过 HTTP/WebSocket 连接远程 Backend
- `backend-solo`：Backend 独立运行，暴露 API 供任意客户端调用
- `tui-with-backend`：TUI 与 Backend 捆绑，本地直连

```
src/
├── backend/                后端核心
│   ├── core/               核心层：类型定义、配置加载、错误体系、日志、事件总线
│   ├── provider/           核心层：LLM Provider 抽象与实现（OpenAI / Anthropic）、模型路由
│   ├── agent/              核心层：Agent 主循环、会话管理、记忆存储、上下文压缩、模式系统、任务队列、多Agent协作
│   ├── tools/              核心层：工具系统（文件、命令、搜索、审批机制）
│   ├── mcp/                核心层：MCP 协议客户端
│   └── monitor/            核心层：WebSocket 监控服务
├── tui/                    前端 TUI
│   ├── app.ts              TUI 主应用
│   ├── api/                API 适配层
│   │   ├── types.ts        前端类型定义（BackendAPI 接口）
│   │   ├── local.ts        本地直连适配器
│   │   ├── http.ts         HTTP API 适配器
│   │   └── ws.ts           WebSocket 适配器
│   ├── panels/             状态面板
│   ├── themes/             主题管理（dark/light）
│   └── utils/              工具函数（快捷键、Markdown 渲染）
```

# 目录结构
<!-- 默认采用Agent开发时守则中的 `src/` 源码架构,具体以本段落和项目状态优先
> 当目录结构发生变化时需要自主更新并告知用户
-->

```
src/
├── backend/                # 后端核心
│   ├── core/               #   核心类型、配置、错误、日志
│   ├── provider/           #   LLM Provider
│   ├── agent/              #   Agent 核心
│   ├── tools/              #   工具系统
│   ├── mcp/                #   MCP 协议
│   └── monitor/            #   监控服务
├── tui/                    # 前端 TUI
│   ├── app.ts              #   TUI 主应用
│   ├── api/                #   API 适配层
│   │   ├── types.ts        #     前端类型定义
│   │   ├── local.ts        #     本地直连适配器
│   │   ├── http.ts         #     HTTP API 适配器
│   │   └── ws.ts           #     WebSocket 适配器
│   ├── panels/             #   状态面板
│   ├── themes/             #   主题管理（dark/light）
│   └── utils/              #   工具函数

scripts/                    # 构建脚本
├── bunbuild-tui-shell-windows_x64.mjs       # Bun 构建：TUI 壳
├── bunbuild-backend-solo-windows_x64.mjs    # Bun 构建：Backend 独立
├── bunbuild-tui-with-backend-windows_x64.mjs # Bun 构建：TUI + Backend 捆绑
├── esbuild-tui-shell.mjs                     # esbuild 打包：TUI 壳
├── esbuild-backend-solo.mjs                  # esbuild 打包：Backend 独立
└── esbuild-tui-with-backend.mjs              # esbuild 打包：TUI + Backend 捆绑

build/                      # Bun 编译产物
dist/                       # esbuild 打包产物

package.json                # 项目依赖、脚本命令
tsconfig.json               # TypeScript 编译配置
.env.example                # 环境变量模板
```

# 配置文件
<!-- 主要指影响项目核心功能的配置文件,此处只需要给出具体文件列表和功能性说明即可,无需给出文件具体内容 -->

| 文件 | 说明 |
|:---:|:---:|
| `package.json` | 项目依赖、脚本命令、版本号 |
| `tsconfig.json` | TypeScript 编译配置（strict mode, ESM） |
| `.env.example` | 环境变量模板（带 `MAIX_AGENT_` 前缀） |
| `.pnpmrc` | pnpm 构建依赖白名单 |

# 配置优先级（从低到高）
<!-- 环境变量配置优先级规则，优先级高的覆盖优先级低的同名配置 -->

1. **用户目录** `~/.maix-agent/config.env`（全局默认配置）
2. **项目目录** `.maix-agent/config.env`（项目级覆盖）
3. **系统环境变量** `MAIX_AGENT_*`（系统级覆盖）
4. **工作目录** `.env`（本地开发覆盖，最高优先级）

环境变量使用 `MAIX_AGENT_` 前缀避免冲突，例如 `MAIX_AGENT_OPENAI_API_KEY`。同时支持不带前缀的变量名作为兼容。

# 设计细节
<!-- 主要指可个性化修改但不影响项目核心功能的设计细节,某个项第一次使用时一般需要取默认值方便开发者知悉和维护,具体包括但不限于(如果有):
- 项目产物文件名 `<main_name>(.<type>)?` ,默认取 `<项目名称>(.<type>)?`
- 项目产物运行时:
  - 占用的文件系统文件夹:
    - 用户目录 `~/<main_name>/` ,默认取 `~/<项目名称>/`
    - 工作目录 `<workspace>/<main_name>/` ,默认取 `<workspace>/<项目名称>/`
  - 监听的地址和端口 `<address>:<port>` ,默认取 `localhost:8080`
> 本段注释行内代码块 `...` 内容包含路径匹配、参数匹配、变量匹配、正则匹配的混合语法,需要区分理解,例如 `~/` 表示工作目录, `<...>` 表示参数或变量, `(...)?` 表示内容正则匹配存在或不存在
> 当项目设计细节具有全局常量/宏/独立代码文件的定义形式时,需要在本段落具体内容末尾添加索引性说明(文件路径,行数,宏/量名称)
> 特别地,当项目状态中的设计细节具体值与本段落设计细节值发生冲突时,需要向用户报告请求决策,不要自行决定
-->

- 主题色定义（dark/light），索引：`src/tui/themes/index.ts`
- 危险命令过滤列表，索引：`src/backend/tools/shell.ts:9-17`
- 默认系统提示词，索引：`src/backend/agent/agent.ts:48-56`
- Token 估算系数（4字符/token），索引：`src/backend/agent/context.ts:13`、`src/backend/provider/base.ts:24`
- 最大工具调用轮次 16，索引：`src/backend/agent/agent.ts:10`
- Agent 模式配置（Plan/Agent/YOLO），索引：`src/backend/agent/modes.ts:12-37`
- 事件总线最大监听器数量 100，索引：`src/backend/core/event-bus.ts:25`
- WebSocket 监控端口 8765，索引：`src/backend/monitor/ws.ts:54`

# 快捷命令
<!-- 主要指Agent开发时使用的命令,如安装依赖、热重载、构建产物、清理遗留,需要按照项目实际需求选择性补充,注意适配开发环境的命令行类型 -->
```
pnpm install          # 安装依赖
pnpm build            # 使用 Bun 交叉编译 TUI + Backend 到 build/
pnpm build:tui-shell  # 使用 Bun 交叉编译 TUI 壳到 build/
pnpm build:backend-solo # 使用 Bun 交叉编译 Backend 独立到 build/
pnpm esbuild          # 使用 esbuild 打包 TUI + Backend 到 dist/
pnpm typecheck        # 类型检查（不生成产物）
```

# 
