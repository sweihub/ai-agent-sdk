# AI Agent SDK (Rust 版 Claude Code SDK)

[![Crates.io](https://img.shields.io/crates/v/ai-agent)](https://crates.io/crates/ai-agent)
[![Rust](https://img.shields.io/badge/rust-%3E%3D1.70-blue)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue)](./LICENSE)

[English](README.md) | [中文](READCN.md)

idiomatic Rust SDK — Claude Code 的 1:1 翻译。**进程内**运行完整 agent 循环，内置 37+ 工具。可部署到任意环境：云、无服务器、Docker、CI/CD。

AI Coding CLI: [ai-code](https://github.com/sweihub/ai-code)

## 快速开始

```bash
cargo add ai-agent
export AI_AUTH_TOKEN=your-api-key
export AI_MODEL=MiniMaxAI/MiniMax-M2.5
# 可选：AI_BASE_URL=https://api.minimax.chat/v1
```

```rust
use ai_agent::Agent;
let mut agent = Agent::new("MiniMaxAI/MiniMax-M2.5", 10);
agent.prompt("列出10个文件").await?;
```

更多示例见 [使用示例](#使用示例)。

## 核心功能

| 功能 | 描述 |
|------|------|
| **Agent** | 使用自定义模型、工具和提示词创建 Agent |
| **Subagent** | 生成子 Agent 用于并行或专业任务 |
| **Session** | 在磁盘上持久化、恢复、分叉对话 |
| **Context Compact** | 接近上下文限制时自动对话摘要 |
| **Skills** | 加载外部技能或使用 15+ 内置技能 |
| **Hooks** | 20+ 生命周期事件 (PreToolUse, PostToolUse, SessionStart 等) |
| **Tools** | 37 内置工具，10 大类别（文件操作、Shell、Web、LSP、多 Agent、任务管理、规划、调度、Git、MCP 等） |
| **Memory** | 通过 MEMORY.md 进行基于文件的持久化上下文 |
| **Permissions** | 工具访问控制，支持允许/拒绝规则 |
| **Plugins** | 加载包含命令、技能、MCP 服务器的插件 |
| **MCP** | 连接到 Model Context Protocol 服务器 |
| **Cost Tracking** | 实时令牌使用量和成本估算 |

## 内置工具

SDK 内置 **37 个工具**，分为 10 大类别。所有工具开箱即用，带有完整的参数验证和类型安全 Schema。

### 文件操作
| 工具 | 描述 |
|------|------|
| `Read` | 读取文件——支持文本、图片（PNG/JPG/GIF/WebP）、PDF、Jupyter 笔记本 |
| `Write` | 向文件写入内容，精确控制路径 |
| `Edit` | 在文件中执行精确字符串替换（单次或全部匹配） |
| `NotebookEdit` | 编辑 Jupyter 笔记本单元格——替换、插入或删除 |

### 文件发现与搜索
| 工具 | 描述 |
|------|------|
| `Glob` | 按 glob 模式查找文件（如 `**/*.ts`） |
| `Grep` | 通过 ripgrep 搜索文件内容，支持正则表达式——支持上下文行号、文件过滤 |

### Shell 与命令执行
| 工具 | 描述 |
|------|------|
| `Bash` | 执行 Shell 命令，内置沙箱、超时控制和破坏性命令安全检查 |
| `PowerShell` | 执行 PowerShell 命令（Windows，含 Git 安全和安全检查） |

### Web
| 工具 | 描述 |
|------|------|
| `WebFetch` | 从任意 URL 获取并提取内容（HTML → Markdown、JSON、纯文本） |
| `WebSearch` | 搜索网络获取最新信息 |
| `WebBrowser` | 无头浏览器自动化——导航、截图、点击、填写、执行 JS、管理标签页 |

### 代码智能
| 工具 | 描述 |
|------|------|
| `LSP` | 语言服务器协议操作——跳转定义、查找引用、悬停提示、文档/工作空间符号、调用层次、实现查找 |

### 多 Agent 编排
| 工具 | 描述 |
|------|------|
| `Agent` | 启动具有专业能力的子 Agent（Explore、Plan、code-review、verification 等） |
| `TeamCreate` / `TeamDelete` | 创建和删除并行工作的 Agent 团队 |
| `SendMessage` | 在团队内的 Agent 之间发送消息 |

### 任务管理
| 工具 | 描述 |
|------|------|
| `TaskCreate` | 创建新任务，包含主题、描述和活跃形式 |
| `TaskList` | 列出所有任务的状态和依赖关系 |
| `TaskUpdate` | 更新任务状态、详情或依赖关系（pending → in_progress → completed） |
| `TaskGet` | 获取指定任务的完整详情 |
| `TaskStop` | 按 ID 停止运行中的后台任务 |
| `TaskOutput` | 获取已完成或运行中后台任务的输出 |

### 规划与用户交互
| 工具 | 描述 |
|------|------|
| `EnterPlanMode` | 进入规划模式，用于多步骤实现方案设计 |
| `ExitPlanMode` | 提交方案供用户审批并开始执行 |
| `AskUserQuestion` | 向用户发起多选提问，支持预览和多选 |

### 调度
| 工具 | 描述 |
|------|------|
| `CronCreate` | 使用 cron 表达式创建周期性或一次性定时任务 |
| `CronDelete` | 取消已创建的定时任务 |
| `CronList` | 列出所有定时任务 |

### Git 与 Worktree
| 工具 | 描述 |
|------|------|
| `EnterWorktree` | 创建隔离的 git worktree 用于功能开发 |
| `ExitWorktree` | 退出并清理 worktree |

### 技能与配置
| 工具 | 描述 |
|------|------|
| `Skill` | 按名称调用技能（如 brainstorming、TDD、debugging、security-review） |
| `Config` | 读取或更新工作区配置（权限、Hooks、环境变量） |

### 系统
| 工具 | 描述 |
|------|------|
| `Monitor` | 监控系统资源和性能 |
| `ToolSearch` | 获取延迟加载工具的完整 Schema（懒加载工具发现） |

### MCP（Model Context Protocol）
| 工具 | 描述 |
|------|------|
| `ListMcpResourcesTool` | 列出已配置 MCP 服务器的可用资源 |
| `ReadMcpResourceTool` | 通过 URI 从 MCP 服务器读取指定资源 |

### 远程 / 云端
| 工具 | 描述 |
|------|------|
| `RemoteTrigger` | 通过 CCR API 管理定时远程 Claude Code Agent——列表、创建、更新、运行 |

## 使用示例

> Agent 自动使用 37+ 内置工具，覆盖 10 大类别，来完成任务。

### 多轮对话
```rust
let mut agent = Agent::new("MiniMaxAI/MiniMax-M2.5", 5);
agent.prompt("创建 /tmp/hello.txt 内容为 'Hello'").await?;
agent.prompt("读取刚才创建的文件").await?;
println!("消息数: {}", agent.get_messages().len());
```

### 自定义工具
```rust
let calculator = ai_agent::Tool {
    name: "Calculator".into(),
    description: "计算数学表达式".into(),
    input_schema: ToolInputSchema::Json(serde_json::json!({
        "type": "object",
        "properties": {"expression": {"type": "string"}},
        "required": ["expression"]
    })),
    executor: Box::new(|input, _ctx| async move {
        Ok(ToolResult { /* ... */ })
    }),
};
```

### MCP 服务器
```rust
let config = McpServerConfig::Stdio(McpStdioConfig {
    command: "npx".into(),
    args: Some(vec!["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]),
    ..Default::default()
});
```

### Hooks
```rust
registry.register("PreToolUse", HookDefinition {
    command: Some("echo pre-tool".into()),
    timeout: Some(5000),
    matcher: Some("Read.*".into()),
});
```

### 中断 Agent 执行

从其他任务调用 `agent.interrupt()` 可以取消正在运行的 `prompt()` 或 `submit_message()`。
操作会返回 `AgentError::UserAborted`。

```rust
use ai_agent::Agent;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

let agent = Arc::new(Mutex::new(Agent::new("MiniMaxAI/MiniMax-M2.5", 10)));
let interrupt_agent = Arc::clone(&agent);

// 派生一个在 5 秒后中断的任务
let interrupt_task = tokio::spawn(async move {
    tokio::time::sleep(Duration::from_secs(5)).await;
    interrupt_agent.lock().await.interrupt();
});

// 以独占方式运行 prompt
let result = {
    let mut ag = agent.lock().await;
    ag.prompt("处理大型代码库").await
};

let _ = tokio::time::timeout(Duration::from_secs(10), interrupt_task).await;
```

参见 `examples/27_interrupt.rs` 获取完整可运行示例。

## 配置

### Agent 选项
| 选项 | 默认值 | 描述 |
|------|--------|------|
| `model` | MiniMaxAI/MiniMax-M2.5 | LLM 模型 ID |
| `max_turns` | 10 | 最大 agent 轮次 |
| `max_tokens` | 16384 | 最大响应令牌数 |
| `max_budget_usd` | — | 支出上限 |
| `system_prompt` | — | 自定义系统提示词 |
| `cwd` | process.cwd() | 工作目录 |
| `allowed_tools` | 全部 | 允许的工具列表 |
| `disallowed_tools` | — | 禁用的工具列表 |

### 环境变量

| 变量 | 默认值 | 描述 |
|------|--------|------|
| `AI_AUTH_TOKEN` | — | API 密钥 (必填) |
| `AI_MODEL` | MiniMaxAI/MiniMax-M2.5 | 模型名称 |
| `AI_BASE_URL` | — | 自定义 API 端点 |
| `AI_CONTEXT_WINDOW` | 200000 | 上下文窗口大小 |
| `AI_DISABLE_AUTO_MEMORY` | false | 禁用自动记忆 |
| `AI_MEMORY_PATH_OVERRIDE` | ~/.ai | 记忆目录 |
| `AI_AUTO_COMPACT_WINDOW` | 模型默认值 | 压缩触发窗口 |
| `AI_AUTOCOMPACT_PCT_OVERRIDE` | — | 阈值百分比 (0-100) |
| `AI_DISABLE_COMPACT` | false | 禁用压缩 |
| `AI_CODE_DISABLE_BACKGROUND_TASKS` | false | 禁用后台任务 |

## 架构

```
┌─────────────────────────────────────┐
│         你的应用程序                 │
│   use ai_agent::Agent            │
└──────────────┬──────────────────────┘
               │
    ┌──────────▼──────────┐
    │       Agent         │  会话、工具、MCP
    │    prompt()         │
    └──────────┬──────────┘
               │
    ┌──────────▼──────────┐
    │    QueryEngine      │  Agent 循环: API → tools → repeat
    └──────────┬──────────┘
               │
    ┌──────────┼──────────┐
    │          │          │
┌───▼───┐  ┌───▼────┐  ┌──▼────┐
│  LLM  │  │ 37+    │  │  MCP  │
│  API  │  │Tools   │  │Server │
└───────┘  └───────┘  └───────┘
```

## 示例

```bash
cargo run --example 01_simple_query
cargo run --example 06_mcp_server
cargo run --example 09_subagents
```

## 示例

```bash
cargo run --example 01_simple_query
cargo run --example 18_plugin
cargo run --example 19_hooks
```

## API 兼容性

SDK 使用 OpenAI 格式与 LLM 通信，兼容：

- [MiniMax](https://platform.minimax.chat)
- [Anthropic](https://www.anthropic.com) (通过兼容端点)
- [OpenAI](https://openai.com) (兼容模式)
- 任何提供 OpenAI `/v1/chat/completions` 端点的提供商

## 许可证

MIT
