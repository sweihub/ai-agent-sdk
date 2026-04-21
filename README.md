# AI Agent SDK (Claude Code SDK in Rust)

[![Crates.io](https://img.shields.io/crates/v/ai-agent)](https://crates.io/crates/ai-agent)
[![Rust](https://img.shields.io/badge/rust-%3E%3D1.70-blue)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue)](./LICENSE)

[English](README.md) | [中文](READCN.md)

Idiomatic Rust SDK — 1:1 translation of Claude Code. Runs the full agent loop **in-process** with 37+ built-in tools. Deploy anywhere: cloud, serverless, Docker, CI/CD.

AI Coding CLI: [ai-code](https://github.com/sweihub/ai-code)

## Quick Start

```bash
cargo add ai-agent
export AI_AUTH_TOKEN=your-api-key
export AI_MODEL=MiniMaxAI/MiniMax-M2.5
# Optional: AI_BASE_URL=https://api.minimax.chat/v1
```

```rust
use ai_agent::Agent;
let mut agent = Agent::new("MiniMaxAI/MiniMax-M2.5", 10);
agent.prompt("List 10 files").await?;
```

See [Usage Examples](#usage-examples) for more.

## Core Features

| Feature | Description |
|---------|-------------|
| **Agent** | Create agents with custom models, tools, and prompts |
| **Subagent** | Spawn subagents for parallel or specialized tasks |
| **Session** | Persist, resume, fork conversations on disk |
| **Context Compact** | Automatic conversation summarization when approaching context limits |
| **Skills** | Load external skills or use 15+ bundled skills |
| **Hooks** | 20+ lifecycle events (PreToolUse, PostToolUse, SessionStart, etc.) |
| **Tools** | 37 built-in tools across 10 categories (File Ops, Shell, Web, LSP, Multi-agent, Tasks, Planning, Scheduling, Git, MCP, etc.) |
| **Memory** | File-based persistent context via MEMORY.md |
| **Permissions** | Tool access control with allow/deny rules |
| **Plugins** | Load plugins with commands, skills, MCP servers |
| **MCP** | Connect to Model Context Protocol servers |
| **Cost Tracking** | Real-time token usage and cost estimation |

## Built-in Tools

The SDK ships with **37 built-in tools** organized into 10 categories. All tools are available out of the box with full parameter validation and type-safe schemas.

### File Operations
| Tool | Description |
|------|-------------|
| `Read` | Read files — text, images (PNG/JPG/GIF/WebP), PDFs, Jupyter notebooks |
| `Write` | Write content to files with exact path control |
| `Edit` | Perform exact string replacements in files (single or all occurrences) |
| `NotebookEdit` | Edit Jupyter notebook cells — replace, insert, or delete |

### File Discovery & Search
| Tool | Description |
|------|-------------|
| `Glob` | Find files by glob pattern (e.g. `**/*.ts`) |
| `Grep` | Search file contents with regex via ripgrep — supports context lines, line numbers, file filters |

### Shell & Command Execution
| Tool | Description |
|------|-------------|
| `Bash` | Execute shell commands with sandboxing, timeouts, and destructive command safety checks |
| `PowerShell` | Execute PowerShell commands (Windows, with git safety and security checks) |

### Web
| Tool | Description |
|------|-------------|
| `WebFetch` | Fetch and extract content from any URL (HTML → Markdown, JSON, plain text) |
| `WebSearch` | Search the web for up-to-date information |
| `WebBrowser` | Headless browser automation — navigate, screenshot, click, fill, evaluate JS, manage tabs |

### Code Intelligence
| Tool | Description |
|------|-------------|
| `LSP` | Language Server Protocol operations — go-to-definition, find references, hover, document/workspace symbols, call hierarchy, implementations |

### Multi-agent Orchestration
| Tool | Description |
|------|-------------|
| `Agent` | Launch subagents with specialized capabilities (Explore, Plan, code-review, verification, etc.) |
| `TeamCreate` / `TeamDelete` | Create and delete teams of parallel-working agents |
| `SendMessage` | Send messages between agents within a team |

### Task Management
| Tool | Description |
|------|-------------|
| `TaskCreate` | Create a new task with subject, description, and active form |
| `TaskList` | List all tasks with statuses and dependencies |
| `TaskUpdate` | Update task status, details, or dependencies (pending → in_progress → completed) |
| `TaskGet` | Get full details of a specific task |
| `TaskStop` | Stop a running background task by ID |
| `TaskOutput` | Retrieve output from a completed or running background task |

### Planning & User Interaction
| Tool | Description |
|------|-------------|
| `EnterPlanMode` | Switch to planning mode for multi-step implementation design |
| `ExitPlanMode` | Present the plan for user approval and begin execution |
| `AskUserQuestion` | Ask the user multi-choice questions with previews and multi-select support |

### Scheduling
| Tool | Description |
|------|-------------|
| `CronCreate` | Schedule recurring or one-shot tasks using cron expressions |
| `CronDelete` | Cancel a scheduled task |
| `CronList` | List all scheduled tasks |

### Git & Worktrees
| Tool | Description |
|------|-------------|
| `EnterWorktree` | Create an isolated git worktree for feature development |
| `ExitWorktree` | Exit and clean up a worktree |

### Skills & Configuration
| Tool | Description |
|------|-------------|
| `Skill` | Invoke skills by name (e.g. brainstorming, TDD, debugging, security-review) |
| `Config` | Read or update harness configuration (permissions, hooks, env vars) |

### System
| Tool | Description |
|------|-------------|
| `Monitor` | Monitor system resources and performance |
| `ToolSearch` | Fetch full schemas for deferred tools (lazy-loaded tool discovery) |

### MCP (Model Context Protocol)
| Tool | Description |
|------|-------------|
| `ListMcpResourcesTool` | List available resources from configured MCP servers |
| `ReadMcpResourceTool` | Read a specific resource from an MCP server by URI |

### Remote / Cloud
| Tool | Description |
|------|-------------|
| `RemoteTrigger` | Manage scheduled remote Claude Code agents via CCR API — list, create, update, run |

## Usage Examples

> The agent automatically uses 37+ built-in tools across 10 categories to accomplish tasks.

### Multi-turn Conversation
```rust
let mut agent = Agent::new("MiniMaxAI/MiniMax-M2.5", 5);
agent.prompt("Create /tmp/hello.txt with 'Hello'").await?;
agent.prompt("Read that file back").await?;
println!("Messages: {}", agent.get_messages().len());
```

### Custom Tools
```rust
let calculator = ai_agent::Tool {
    name: "Calculator".into(),
    description: "Evaluate math expressions".into(),
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

### MCP Servers
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

### Interrupting Agent Execution

Call `agent.interrupt()` from another task to cancel a running `prompt()` or `submit_message()`.
The operation returns `AgentError::UserAborted`.

```rust
use ai_agent::Agent;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

let agent = Arc::new(Mutex::new(Agent::new("MiniMaxAI/MiniMax-M2.5", 10)));
let interrupt_agent = Arc::clone(&agent);

// Spawn a task that interrupts after 5 seconds
let interrupt_task = tokio::spawn(async move {
    tokio::time::sleep(Duration::from_secs(5)).await;
    interrupt_agent.lock().await.interrupt();
});

// Run the prompt with exclusive access
let result = {
    let mut ag = agent.lock().await;
    ag.prompt("Process a large codebase").await
};

let _ = tokio::time::timeout(Duration::from_secs(10), interrupt_task).await;
```

See `examples/27_interrupt.rs` for a full runnable example.

## Configuration

### Agent Options
| Option | Default | Description |
|--------|---------|-------------|
| `model` | MiniMaxAI/MiniMax-M2.5 | LLM model ID |
| `max_turns` | 10 | Max agentic turns |
| `max_tokens` | 16384 | Max response tokens |
| `max_budget_usd` | — | Spending cap |
| `system_prompt` | — | Custom system prompt |
| `cwd` | process.cwd() | Working directory |
| `allowed_tools` | all | Tool allow-list |
| `disallowed_tools` | — | Tool deny-list |

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `AI_AUTH_TOKEN` | — | API key (required) |
| `AI_MODEL` | MiniMaxAI/MiniMax-M2.5 | Model name |
| `AI_BASE_URL` | — | Custom API endpoint |
| `AI_CONTEXT_WINDOW` | 200000 | Context window size |
| `AI_DISABLE_AUTO_MEMORY` | false | Disable auto memory |
| `AI_MEMORY_PATH_OVERRIDE` | ~/.ai | Memory directory |
| `AI_AUTO_COMPACT_WINDOW` | model-based | Compact trigger window |
| `AI_AUTOCOMPACT_PCT_OVERRIDE` | — | Threshold % (0-100) |
| `AI_DISABLE_COMPACT` | false | Disable compaction |
| `AI_CODE_DISABLE_BACKGROUND_TASKS` | false | Disable background tasks |

## API Compatibility

SDK uses OpenAI format, compatible with:

- [MiniMax](https://platform.minimax.chat)
- [Anthropic](https://www.anthropic.com) (via compatible endpoint)
- [OpenAI](https://openai.com) (compatible mode)
- Any provider with `/v1/chat/completions` endpoint

## Architecture

```
┌─────────────────────────────────────┐
│         Your Application             │
│   use ai_agent::Agent            │
└──────────────┬──────────────────────┘
               │
    ┌──────────▼──────────┐
    │       Agent         │  Session, tools, MCP
    │    prompt()         │
    └──────────┬──────────┘
               │
    ┌──────────▼──────────┐
    │    QueryEngine      │  Agent loop: API → tools → repeat
    └──────────┬──────────┘
               │
    ┌──────────┼──────────┐
    │          │          │
┌───▼───┐  ┌───▼────┐  ┌──▼────┐
│  LLM  │  │ 37+    │  │  MCP  │
│  API  │  │Tools   │  │Server │
└───────┘  └───────┘  └───────┘
```

## Examples

```bash
cargo run --example 01_simple_query
cargo run --example 18_plugin
cargo run --example 19_hooks
```

## License

MIT
