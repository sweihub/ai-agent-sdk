// Source: /data/home/swei/claudecode/openclaudecode/src/utils/model/agent.ts
use crate::query_engine::{QueryEngine, QueryEngineConfig};
use crate::env::EnvConfig;
use crate::error::AgentError;
use crate::stream::{CancelGuard, EventSubscriber, QueryStream};
use crate::types::AgentEvent;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex as TokioMutex};
use crate::tools::bash::BashTool;
use crate::tools::edit::FileEditTool;
use crate::tools::glob::GlobTool;
use crate::tools::grep::GrepTool;
use crate::tools::read::FileReadTool as ReadTool;
use crate::tools::write::FileWriteTool as WriteTool;
use crate::tools::web_fetch::WebFetchTool;
use crate::tools::web_search::WebSearchTool;
use crate::tools::notebook_edit::NotebookEditTool;
use crate::tools::tasks::{TaskCreateTool, TaskListTool, TaskUpdateTool, TaskGetTool};
use crate::tools::todo::TodoWriteTool;
use crate::tools::cron::{CronCreateTool, CronDeleteTool, CronListTool};
use crate::tools::config::ConfigTool;
use crate::tools::worktree::{EnterWorktreeTool, ExitWorktreeTool};
use crate::tools::plan::{EnterPlanModeTool, ExitPlanModeTool};
use crate::tools::ask::AskUserQuestionTool;
use crate::tools::team::{TeamCreateTool, TeamDeleteTool, SendMessageTool};
use crate::tools::skill::SkillTool;
use crate::tools::skill::register_skills_from_dir;
use crate::tools::search::ToolSearchTool;
use crate::tools::monitor::MonitorTool;
use crate::tools::send_user_file::SendUserFileTool;
use crate::tools::web_browser::WebBrowserTool;
use crate::tools::sleep_tool::SleepTool;
use crate::tools::lsp::LSPTool;
use crate::tools::remote_trigger::RemoteTriggerTool;
use crate::tools::mcp_resources::ListMcpResourcesTool;
use crate::tools::mcp_resource_reader::ReadMcpResourceTool;
use crate::types::*;

/// Tracks engine-critical configuration to detect when the QueryEngine must be recreated.
#[derive(Debug, Clone, Default)]
struct EngineConfig {
    model: String,
    api_key: Option<String>,
    base_url: Option<String>,
    cwd: String,
    max_turns: u32,
}

impl EngineConfig {
    fn eq_ignore_tools(&self, other: &Self) -> bool {
        self.model == other.model
            && self.api_key == other.api_key
            && self.base_url == other.base_url
            && self.cwd == other.cwd
            && self.max_turns == other.max_turns
    }
}

/// Register all built-in tool executors
fn register_all_tool_executors(engine: &mut QueryEngine) {
    type BoxFuture<T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send>>;

    // Bash tool - clone tool and ctx into async block
    let bash_executor = move |input: serde_json::Value,
                              ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = BashTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    engine.register_tool("Bash".to_string(), bash_executor);

    // FileRead tool
    let read_executor = move |input: serde_json::Value,
                              ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = ReadTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    engine.register_tool("FileRead".to_string(), read_executor);

    // FileWrite tool
    let write_executor = move |input: serde_json::Value,
                               ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = WriteTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    engine.register_tool("FileWrite".to_string(), write_executor);

    // Glob tool
    let glob_executor = move |input: serde_json::Value,
                              ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = GlobTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    engine.register_tool("Glob".to_string(), glob_executor);

    // Grep tool
    let grep_executor = move |input: serde_json::Value,
                              ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = GrepTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    engine.register_tool("Grep".to_string(), grep_executor);

    // FileEdit tool - with rendering metadata for TUI display
    let file_edit_tool = FileEditTool::new();
    let edit_executor = move |input: serde_json::Value,
                              ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = FileEditTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    // Clone Arc for each closure capture
    let fns_user_facing = Arc::new(file_edit_tool);
    let fns_summary = Arc::clone(&fns_user_facing);
    let fns_render = Arc::clone(&fns_user_facing);
    let render_fns = crate::query_engine::ToolRenderFns {
        user_facing_name: Arc::new(move |input| fns_user_facing.user_facing_name(input)),
        get_tool_use_summary: Some(Arc::new(move |input| fns_summary.get_tool_use_summary(input))),
        get_activity_description: None,
        render_tool_result_message: Some(Arc::new(move |content, _progress, _options| {
            fns_render.render_tool_result_message(content)
        })),
    };
    engine.register_tool_with_render("FileEdit".to_string(), edit_executor, render_fns);

    // Skill tool - register skills from examples/skills directory
    use std::path::Path;
    register_skills_from_dir(Path::new("examples/skills"));

    let skill_executor = move |input: serde_json::Value,
                               ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = SkillTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    engine.register_tool("Skill".to_string(), skill_executor);

    // Monitor tool
    let monitor_executor = move |input: serde_json::Value,
                                   ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = MonitorTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    engine.register_tool("Monitor".to_string(), monitor_executor);

    // SendUserFile tool
    let send_user_file_executor = move |input: serde_json::Value,
                                              ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = SendUserFileTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    engine.register_tool("send_user_file".to_string(), send_user_file_executor);

    // WebBrowser tool
    let web_browser_executor = move |input: serde_json::Value,
                                           ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = WebBrowserTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    engine.register_tool("WebBrowser".to_string(), web_browser_executor);

    // WebFetch tool
    let web_fetch_executor = move |input: serde_json::Value,
                                    ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = WebFetchTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    engine.register_tool("WebFetch".to_string(), web_fetch_executor);

    // WebSearch tool
    let web_search_executor = move |input: serde_json::Value,
                                     ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = WebSearchTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    engine.register_tool("WebSearch".to_string(), web_search_executor);

    // NotebookEdit tool
    let notebook_edit_executor = move |input: serde_json::Value,
                                        ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = NotebookEditTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    engine.register_tool("NotebookEdit".to_string(), notebook_edit_executor);

    // TaskCreate tool
    let task_create_executor = move |input: serde_json::Value,
                                      ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = TaskCreateTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    engine.register_tool("TaskCreate".to_string(), task_create_executor);

    // TaskList tool
    let task_list_executor = move |input: serde_json::Value,
                                    ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = TaskListTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    engine.register_tool("TaskList".to_string(), task_list_executor);

    // TaskUpdate tool
    let task_update_executor = move |input: serde_json::Value,
                                      ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = TaskUpdateTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    engine.register_tool("TaskUpdate".to_string(), task_update_executor);

    // TaskGet tool
    let task_get_executor = move |input: serde_json::Value,
                                   ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = TaskGetTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    engine.register_tool("TaskGet".to_string(), task_get_executor);

    // TodoWrite tool
    let todo_write_executor = move |input: serde_json::Value,
                                     ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = TodoWriteTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    engine.register_tool("TodoWrite".to_string(), todo_write_executor);

    // CronCreate tool
    let cron_create_executor = move |input: serde_json::Value,
                                      ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = CronCreateTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    engine.register_tool("CronCreate".to_string(), cron_create_executor);

    // CronDelete tool
    let cron_delete_executor = move |input: serde_json::Value,
                                      ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = CronDeleteTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    engine.register_tool("CronDelete".to_string(), cron_delete_executor);

    // CronList tool
    let cron_list_executor = move |input: serde_json::Value,
                                    ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = CronListTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    engine.register_tool("CronList".to_string(), cron_list_executor);

    // Config tool
    let config_executor = move |input: serde_json::Value,
                                ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = ConfigTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    engine.register_tool("Config".to_string(), config_executor);

    // EnterWorktree tool
    let enter_worktree_executor = move |input: serde_json::Value,
                                         ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = EnterWorktreeTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    engine.register_tool("EnterWorktree".to_string(), enter_worktree_executor);

    // ExitWorktree tool
    let exit_worktree_executor = move |input: serde_json::Value,
                                        ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = ExitWorktreeTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    engine.register_tool("ExitWorktree".to_string(), exit_worktree_executor);

    // EnterPlanMode tool
    let enter_plan_mode_executor = move |input: serde_json::Value,
                                           ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = EnterPlanModeTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    engine.register_tool("EnterPlanMode".to_string(), enter_plan_mode_executor);

    // ExitPlanMode tool
    let exit_plan_mode_executor = move |input: serde_json::Value,
                                          ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = ExitPlanModeTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    engine.register_tool("ExitPlanMode".to_string(), exit_plan_mode_executor);

    // AskUserQuestion tool
    let ask_user_question_executor = move |input: serde_json::Value,
                                             ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = AskUserQuestionTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    engine.register_tool("AskUserQuestion".to_string(), ask_user_question_executor);

    // ToolSearch tool
    let tool_search_executor = move |input: serde_json::Value,
                                        ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = ToolSearchTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    engine.register_tool("ToolSearch".to_string(), tool_search_executor);

    // TeamCreate tool
    let team_create_executor = move |input: serde_json::Value,
                                      ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = TeamCreateTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    engine.register_tool("TeamCreate".to_string(), team_create_executor);

    // TeamDelete tool
    let team_delete_executor = move |input: serde_json::Value,
                                      ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = TeamDeleteTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    engine.register_tool("TeamDelete".to_string(), team_delete_executor);

    // SendMessage tool
    let send_message_executor = move |input: serde_json::Value,
                                       ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = SendMessageTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    engine.register_tool("SendMessage".to_string(), send_message_executor);

    // Sleep tool - wait for a duration without holding a shell process
    let sleep_executor = move |input: serde_json::Value,
                                 ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = SleepTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    engine.register_tool("Sleep".to_string(), sleep_executor);

    // LSP tool - code intelligence via Language Server Protocol
    let lsp_executor = move |input: serde_json::Value,
                               ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = LSPTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    engine.register_tool("LSP".to_string(), lsp_executor);

    // RemoteTrigger tool - manage remote agent triggers
    let remote_trigger_executor = move |input: serde_json::Value,
                                          ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = RemoteTriggerTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    engine.register_tool("RemoteTrigger".to_string(), remote_trigger_executor);

    // ListMcpResourcesTool - list MCP server resources
    let list_mcp_resources_executor = move |input: serde_json::Value,
                                              ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = ListMcpResourcesTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    engine.register_tool("ListMcpResourcesTool".to_string(), list_mcp_resources_executor);

    // ReadMcpResourceTool - read MCP resources
    let read_mcp_resource_executor = move |input: serde_json::Value,
                                             ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = ReadMcpResourceTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    engine.register_tool("ReadMcpResourceTool".to_string(), read_mcp_resource_executor);
}

/// Subscriber info for fan-out event delivery
pub struct Agent {
    config: AgentOptions,
    model: String,
    api_key: Option<String>,
    base_url: Option<String>,
    tool_pool: Vec<ToolDefinition>,
    session_id: String,
    abort_controller: std::sync::Arc<crate::utils::AbortController>,
    /// Persisted QueryEngine for multi-turn reuse (matches TypeScript pattern).
    /// Shared via Arc<TokioMutex> so query_stream() spawned task and query()
    /// can access the same conversation state (messages, usage, turns).
    persist_engine: Option<Arc<TokioMutex<QueryEngine>>>,
    engine_config: Option<EngineConfig>,
}

impl From<AgentOptions> for Agent {
    fn from(options: AgentOptions) -> Self {
        Agent::create(options)
    }
}

impl Agent {
    /// Create a new agent with model name and max turns
    pub fn new(model: &str, max_turns: u32) -> Self {
        Self::create(AgentOptions {
            model: Some(model.to_string()),
            max_turns: Some(max_turns),
            ..Default::default()
        })
    }

    /// Create a new agent with model, max turns, and event callback for streaming
    pub fn with_event_callback<F>(model: &str, max_turns: u32, on_event: F) -> Self
    where
        F: Fn(AgentEvent) + Send + Sync + 'static,
    {
        let mut agent = Self::new(model, max_turns);
        agent.config.on_event = Some(std::sync::Arc::new(on_event));
        agent
    }

    /// Create agent from AgentOptions
    pub fn create(options: AgentOptions) -> Self {
        // Load env config for defaults
        let env_config = EnvConfig::load();

        // Use env value, then options value, then default
        let model = env_config
            .model
            .clone()
            .or_else(|| options.model.clone())
            .unwrap_or_else(|| "claude-sonnet-4-6".to_string());

        let api_key = env_config
            .auth_token
            .clone()
            .or_else(|| options.api_key.clone());

        let base_url = env_config
            .base_url
            .clone()
            .or_else(|| options.base_url.clone());

        let session_id = uuid::Uuid::new_v4().to_string();

        Self {
            config: options.clone(),
            model,
            api_key,
            base_url,
            tool_pool: options.tools.clone(),
            session_id,
            abort_controller: std::sync::Arc::new(crate::utils::create_abort_controller_default()),
            persist_engine: None,
            engine_config: None,
        }
    }

    /// Lazily create or return the persisted QueryEngine.
    /// Recreates the engine when engine-critical config has changed.
    fn get_or_create_engine(&mut self) -> Arc<TokioMutex<QueryEngine>> {
        let needs_recreate = self.persist_engine.is_none()
            || match &self.engine_config {
                Some(ec) => {
                    let cwd = self.config.cwd.clone().unwrap_or_else(|| {
                        std::env::current_dir()
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_else(|_| ".".to_string())
                    });
                    let current = EngineConfig {
                        model: self.model.clone(),
                        api_key: self.api_key.clone(),
                        base_url: self.base_url.clone(),
                        cwd,
                        max_turns: self.config.max_turns.unwrap_or(10),
                    };
                    !ec.eq_ignore_tools(&current)
                }
                None => true,
            };

        if needs_recreate {
            let cwd = self.config.cwd.clone().unwrap_or_else(|| {
                std::env::current_dir()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| ".".to_string())
            });
            let config = QueryEngineConfig {
                cwd: cwd.clone(),
                model: self.model.clone(),
                api_key: self.api_key.clone(),
                base_url: self.base_url.clone(),
                tools: vec![],
                system_prompt: None,
                max_turns: self.config.max_turns.unwrap_or(10),
                max_budget_usd: self.config.max_budget_usd,
                max_tokens: self.config.max_tokens.unwrap_or(16384),
                fallback_model: self.config.fallback_model.clone(),
                user_context: std::collections::HashMap::new(),
                system_context: std::collections::HashMap::new(),
                can_use_tool: None,
                on_event: self.config.on_event.clone(),
                thinking: self.config.thinking.clone(),
                abort_controller: Some(self.abort_controller.clone()),
            };
            let mut engine = QueryEngine::new(config);
            register_all_tool_executors(&mut engine);
            self.persist_engine = Some(Arc::new(TokioMutex::new(engine)));
            self.engine_config = Some(EngineConfig {
                model: self.model.clone(),
                api_key: self.api_key.clone(),
                base_url: self.base_url.clone(),
                cwd,
                max_turns: self.config.max_turns.unwrap_or(10),
            });
        }

        Arc::clone(self.persist_engine.as_ref().unwrap())
    }

    pub fn get_model(&self) -> &str {
        &self.model
    }

    pub fn get_session_id(&self) -> &str {
        &self.session_id
    }

    /// Get all messages in the conversation history.
    /// Delegates to the persisted QueryEngine which owns the message state
    /// (matches TypeScript: engine.mutableMessages).
    /// Uses try_lock() — returns messages if no async operation holds the lock,
    /// otherwise returns an empty vec (the engine is busy in a query_stream).
    pub fn get_messages(&self) -> Vec<Message> {
        self.persist_engine
            .as_ref()
            .and_then(|e| e.try_lock().ok().map(|guard| guard.get_messages()))
            .unwrap_or_default()
    }

    /// Get all tools available to the agent
    pub fn get_tools(&self) -> &[ToolDefinition] {
        &self.tool_pool
    }

    /// Set system prompt for the agent
    pub fn set_system_prompt(&mut self, prompt: &str) {
        self.config.system_prompt = Some(prompt.to_string());
    }

    /// Set the working directory for the agent
    pub fn set_cwd(&mut self, cwd: &str) {
        self.config.cwd = Some(cwd.to_string());
    }

    /// Set the event callback for agent events (tool start/complete/error, thinking, done)
    /// Note: This must be called BEFORE query() - it sets the callback on the engine
    pub fn set_event_callback<F>(&mut self, callback: F)
    where
        F: Fn(AgentEvent) + Send + Sync + 'static,
    {
        self.config.on_event = Some(std::sync::Arc::new(callback));
    }

    /// Set thinking configuration for the agent
    pub fn set_thinking(&mut self, thinking: Option<ThinkingConfig>) {
        self.config.thinking = thinking;
    }

    /// Execute a tool directly (for testing/demo purposes)
    pub async fn execute_tool(
        &mut self,
        name: &str,
        input: serde_json::Value,
    ) -> Result<ToolResult, AgentError> {
        // Create a temporary engine to execute the tool
        let cwd = self.config.cwd.clone().unwrap_or_else(|| {
            std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| ".".to_string())
        });
        let model = self.model.clone();
        let api_key = self.api_key.clone();
        let base_url = self.base_url.clone();

        let mut engine = QueryEngine::new(QueryEngineConfig {
            cwd: cwd.clone(),
            model: model.clone(),
            api_key: api_key.clone(),
            base_url: base_url.clone(),
            tools: vec![],
            system_prompt: None,
            max_turns: 10,
            max_budget_usd: None,
            max_tokens: 16384,
            fallback_model: None,
            user_context: std::collections::HashMap::new(),
            system_context: std::collections::HashMap::new(),
            can_use_tool: None,
            on_event: None,
            thinking: None,
            abort_controller: Some(self.abort_controller.clone()),
        });

        // Register all tool executors (including Bash, Read, Write, etc.)
        register_all_tool_executors(&mut engine);

        // Register Agent tool executor with full parameter support
        let subagent_abort = std::sync::Arc::clone(&self.abort_controller);
        let agent_tool_executor = move |input: serde_json::Value,
                                        _ctx: &ToolContext|
              -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<ToolResult, AgentError>> + Send>,
        > {
            let cwd = cwd.clone();
            let api_key = api_key.clone();
            let base_url = base_url.clone();
            let model = model.clone();
            let subagent_abort = subagent_abort.clone();

            Box::pin(async move {
                // Extract ALL parameters from input
                let description = input["description"].as_str().unwrap_or("subagent");
                let subagent_prompt = input["prompt"].as_str().unwrap_or("");
                let subagent_model = input["model"]
                    .as_str()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| model.clone());
                let max_turns = input["max_turns"]
                    .as_u64()
                    .or_else(|| input["maxTurns"].as_u64()) // Support camelCase too
                    .unwrap_or(10) as u32;

                // NEW: Extract subagent_type
                let subagent_type = input["subagent_type"]
                    .as_str()
                    .or_else(|| input["subagentType"].as_str())
                    .map(|s| s.to_string());

                // NEW: Extract run_in_background (ignored for now, async not supported)
                let _run_in_background = input["run_in_background"]
                    .as_bool()
                    .or_else(|| input["runInBackground"].as_bool())
                    .unwrap_or(false);

                // NEW: Extract name
                let agent_name = input["name"].as_str().map(|s| s.to_string());

                // NEW: Extract team_name
                let _team_name = input["team_name"]
                    .as_str()
                    .or_else(|| input["teamName"].as_str())
                    .map(|s| s.to_string());

                // NEW: Extract mode (permission mode - ignored for now)
                let _mode = input["mode"].as_str().map(|s| s.to_string());

                // NEW: Extract cwd (working directory override)
                let subagent_cwd = input["cwd"]
                    .as_str()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| cwd.clone());

                // NEW: Extract isolation
                let _isolation = input["isolation"].as_str().map(|s| s.to_string());

                // Build system prompt for subagent
                let system_prompt = build_agent_system_prompt(description, subagent_type.as_deref());

                // Create sub-agent engine with proper system prompt
                let mut sub_engine = QueryEngine::new(QueryEngineConfig {
                    cwd: subagent_cwd,
                    model: subagent_model.to_string(),
                    api_key,
                    base_url,
                    tools: vec![],
                    system_prompt: Some(system_prompt),
                    max_turns,
                    max_budget_usd: None,
                    max_tokens: 16384,
                    fallback_model: None,
                    user_context: std::collections::HashMap::new(),
                    system_context: std::collections::HashMap::new(),
                    can_use_tool: None,
                    on_event: None,
                    thinking: None,
                    abort_controller: Some(subagent_abort.clone()),
                });

                match sub_engine.submit_message(subagent_prompt).await {
                    Ok((result_text, _)) => {
                        let mut content = format!("[Subagent: {}]", description);
                        if let Some(ref name) = agent_name {
                            content = format!("[Subagent: {} ({})]", description, name);
                        }
                        content = format!("{}\n\n{}", content, result_text);
                        Ok(ToolResult {
                            result_type: "text".to_string(),
                            tool_use_id: "agent_tool".to_string(),
                            content,
                            is_error: Some(false),
                was_persisted: None,
                        })
                    }
                    Err(e) => Ok(ToolResult {
                        result_type: "text".to_string(),
                        tool_use_id: "agent_tool".to_string(),
                        content: format!("[Subagent: {}] Error: {}", description, e),
                        is_error: Some(true),
                was_persisted: None,
                    }),
                }
            })
        };

        engine.register_tool("Agent".to_string(), agent_tool_executor);
        let tool_call_id = uuid::Uuid::new_v4().to_string();
        engine.execute_tool(name, input, tool_call_id).await
    }

    /// Build the system prompt by combining AI.md, memory mechanics,
    /// base system prompt, and custom system prompt.
    fn build_system_prompt(&self, cwd: &std::path::Path) -> Option<String> {
        use crate::ai_md::load_ai_md;
        use crate::memdir::load_memory_prompt_sync;
        use crate::prompts::build_system_prompt as base_build_system_prompt;

        let ai_md_prompt = load_ai_md(cwd).ok().flatten();
        let memory_mechanics_prompt = if self.config.system_prompt.is_some()
            && crate::memdir::has_auto_mem_path_override()
        {
            load_memory_prompt_sync()
        } else {
            None
        };
        let base_system_prompt = base_build_system_prompt();

        let system_prompt = match (&ai_md_prompt, &memory_mechanics_prompt, &self.config.system_prompt) {
            (Some(ai_md), Some(mem), Some(custom)) => Some(format!(
                "{}\n\n{}\n\n{}\n\n{}",
                ai_md, mem, base_system_prompt, custom
            )),
            (Some(ai_md), Some(mem), None) => {
                Some(format!("{}\n\n{}\n\n{}", ai_md, mem, base_system_prompt))
            }
            (Some(ai_md), None, Some(custom)) => {
                Some(format!("{}\n\n{}\n\n{}", ai_md, base_system_prompt, custom))
            }
            (Some(ai_md), None, None) => Some(format!("{}\n\n{}", ai_md, base_system_prompt)),
            (None, Some(mem), Some(custom)) => {
                Some(format!("{}\n\n{}\n\n{}", mem, base_system_prompt, custom))
            }
            (None, Some(mem), None) => Some(format!("{}\n\n{}", mem, base_system_prompt)),
            (None, None, Some(custom)) => Some(format!("{}\n\n{}", base_system_prompt, custom)),
            (None, None, None) => Some(base_system_prompt),
        };
        system_prompt
    }

    /// Select the tools to use: all base tools if tool_pool is empty, otherwise the tool pool.
    fn select_tools(&self) -> Vec<ToolDefinition> {
        if self.tool_pool.is_empty() {
            crate::tools::get_all_base_tools()
        } else {
            self.tool_pool.clone()
        }
    }

    /// Main query method - handles the full agent loop including tool use,
    /// streaming responses, and multi-turn interaction with the LLM.
    ///
    /// Reuses a persisted QueryEngine across calls so that conversation history,
    /// usage tracking, and tool state accumulate naturally (matches TypeScript pattern).
    pub async fn query(&mut self, prompt: &str) -> Result<QueryResult, AgentError> {
        let cwd = self.config.cwd.clone().unwrap_or_else(|| {
            std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| ".".to_string())
        });
        let cwd_path = std::path::Path::new(&cwd);

        let system_prompt = self.build_system_prompt(cwd_path);
        let tools = self.select_tools();

        // Get the shared persisted engine
        let engine = self.get_or_create_engine();

        let start = std::time::Instant::now();
        let (response_text, exit_reason, current_model, usage, turns) = {
            let mut eng = engine.lock().await;

            // Update per-query config
            eng.config.system_prompt = system_prompt;
            eng.config.tools = tools;
            eng.config.on_event = self.config.on_event.clone();
            eng.config.thinking = self.config.thinking.clone();

            // Snapshot engine config values needed by the subagent closure
            let engine_tools = eng.config.tools.clone();
            let engine_model = eng.config.model.clone();
            let engine_api_key = eng.config.api_key.clone();
            let engine_base_url = eng.config.base_url.clone();
            let engine_cwd = eng.config.cwd.clone();
            let subagent_abort = std::sync::Arc::clone(&self.abort_controller);

            let agent_tool_executor = move |input: serde_json::Value,
                                            _ctx: &ToolContext|
                  -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<ToolResult, AgentError>> + Send>,
            > {
                let cwd = engine_cwd.clone();
                let api_key = engine_api_key.clone();
                let base_url = engine_base_url.clone();
                let model = engine_model.clone();
                let tool_pool = engine_tools.clone();
                let subagent_abort = subagent_abort.clone();

                Box::pin(async move {
                    let description = input["description"].as_str().unwrap_or("subagent");
                    let subagent_prompt = input["prompt"].as_str().unwrap_or("");
                    let subagent_model = input["model"]
                        .as_str()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| model.clone());
                    let max_turns = input["max_turns"]
                        .as_u64()
                        .or_else(|| input["maxTurns"].as_u64())
                        .unwrap_or(10) as u32;

                    let subagent_type = input["subagent_type"]
                        .as_str()
                        .or_else(|| input["subagentType"].as_str())
                        .map(|s| s.to_string());

                    let _run_in_background = input["run_in_background"]
                        .as_bool()
                        .or_else(|| input["runInBackground"].as_bool())
                        .unwrap_or(false);

                    let agent_name = input["name"].as_str().map(|s| s.to_string());

                    let _team_name = input["team_name"]
                        .as_str()
                        .or_else(|| input["teamName"].as_str())
                        .map(|s| s.to_string());

                    let _mode = input["mode"].as_str().map(|s| s.to_string());

                    let subagent_cwd = input["cwd"]
                        .as_str()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| cwd.clone());

                    let _isolation = input["isolation"].as_str().map(|s| s.to_string());

                    let system_prompt = build_agent_system_prompt(description, subagent_type.as_deref());

                    let mut sub_engine = QueryEngine::new(QueryEngineConfig {
                        cwd: subagent_cwd,
                        model: subagent_model.to_string(),
                        api_key,
                        base_url,
                        tools: tool_pool,
                        system_prompt: Some(system_prompt),
                        max_turns,
                        max_budget_usd: None,
                        max_tokens: 16384,
                        fallback_model: None,
                        user_context: std::collections::HashMap::new(),
                        system_context: std::collections::HashMap::new(),
                        can_use_tool: None,
                        on_event: None,
                        thinking: None,
                        abort_controller: Some(subagent_abort.clone()),
                    });

                    match sub_engine.submit_message(subagent_prompt).await {
                        Ok((result_text, _)) => {
                            let mut content = format!("[Subagent: {}]", description);
                            if let Some(ref name) = agent_name {
                                content = format!("[Subagent: {} ({})]", description, name);
                            }
                            content = format!("{}\n\n{}", content, result_text);
                            Ok(ToolResult {
                                result_type: "text".to_string(),
                                tool_use_id: "agent_tool".to_string(),
                                content,
                                is_error: Some(false),
                                was_persisted: None,
                            })
                        }
                        Err(e) => Ok(ToolResult {
                            result_type: "text".to_string(),
                            tool_use_id: "agent_tool".to_string(),
                            content: format!("[Subagent: {}] Error: {}", description, e),
                            is_error: Some(true),
                            was_persisted: None,
                        }),
                    }
                })
            };

            eng.register_tool("Agent".to_string(), agent_tool_executor);

            // Run the query — collect results before dropping the lock
            let result = eng.submit_message(prompt).await?;
            let response_text = result.0;
            let exit_reason = result.1;
            let current_model = eng.config.model.clone();
            let usage = eng.get_usage();
            let turns = eng.get_turn_count();
            (response_text, exit_reason, current_model, usage, turns)
        }; // Lock released here

        // Track model in case it changed (for recreation detection)
        if current_model != self.model {
            self.model = current_model;
        }

        Ok(QueryResult {
            text: response_text,
            usage: TokenUsage {
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                cache_creation_input_tokens: usage.cache_creation_input_tokens,
                cache_read_input_tokens: usage.cache_read_input_tokens,
            },
            num_turns: turns,
            duration_ms: start.elapsed().as_millis() as u64,
            exit_reason,
        })
    }

    /// Reset the agent's conversation history, keeping configuration intact.
    ///
    /// Forces the QueryEngine to be recreated on the next query call, clearing
    /// all messages, usage tracking, and turn count. This starts a fresh
    /// conversation while preserving model, API key, tools, and other settings.
    pub fn reset(&mut self) {
        self.persist_engine = None;
        self.engine_config = None;
    }

    /// Execute a query with incremental event streaming.
    ///
    /// Returns a [`QueryStream`] that implements [`futures_util::Stream`], yielding
    /// [`AgentEvent`] instances as they occur during the agent loop. The engine
    /// runs on a spawned tokio task.
    ///
    /// Events always conclude with [`AgentEvent::Done`](types::AgentEvent::Done),
    /// whether the query completes normally, hits an error, or is interrupted.
    /// Drop the stream to abort the spawned task.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let mut stream = agent.query_stream("write hello world").await?;
    /// tokio::pin!(stream);
    ///
    /// loop {
    ///     tokio::select! {
    ///         Some(ev) = stream.next() => match ev {
    ///             AgentEvent::ContentBlockDelta {
    ///                 delta: AgentEvent::ContentDelta::Text { text },
    ///                 ..
    ///             } => print!("{}", text),
    ///             AgentEvent::Done { result } => {
    ///                 println!("\nDone! Turns: {}", result.num_turns);
    ///                 break;
    ///             }
    ///             _ => {}
    ///         },
    ///         None => break,
    ///     }
    /// }
    /// ```
    pub async fn query_stream(&mut self, prompt: &str) -> Result<QueryStream, AgentError> {
        let cwd = self.config.cwd.clone().unwrap_or_else(|| {
            std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| ".".to_string())
        });
        let cwd_path = std::path::Path::new(&cwd);

        let system_prompt = self.build_system_prompt(cwd_path);
        let tools = self.select_tools();

        // Clone prompt for spawned task
        let prompt_owned = prompt.to_string();

        // Get the shared persisted engine — the spawned task locks and uses it directly.
        // This ensures messages, turn_count, and usage accumulate across all calls.
        let engine = self.get_or_create_engine();

        // Create event channel
        let (stream_tx, stream_rx) = mpsc::channel(256);
        let stream_tx_clone = stream_tx.clone();
        let result_storage: Arc<std::sync::OnceLock<QueryResult>> = Arc::new(std::sync::OnceLock::new());
        let result_storage_for_task = Arc::clone(&result_storage);

        // Spawn engine loop on tokio task — uses the shared persisted engine via Arc
        let task = tokio::spawn(Self::run_stream_task(
            engine,
            system_prompt,
            tools,
            prompt_owned,
            stream_tx_clone,
            stream_tx,
            result_storage_for_task,
            self.config.thinking.clone(),
        ));

        Ok(QueryStream::new(stream_rx, task, result_storage))
    }

    /// Background task: lock shared engine, configure, run submit_message, emit events.
    async fn run_stream_task(
        engine: Arc<TokioMutex<QueryEngine>>,
        system_prompt: Option<String>,
        tools: Vec<ToolDefinition>,
        prompt_owned: String,
        stream_tx: mpsc::Sender<AgentEvent>,
        stream_tx_for_done: mpsc::Sender<AgentEvent>,
        result_storage: Arc<std::sync::OnceLock<QueryResult>>,
        thinking: Option<crate::types::ThinkingConfig>,
    ) {
        // Wrap the on_event callback to push events to stream_tx_for_done
        let on_event_tx = stream_tx_for_done.clone();
        let on_event = std::sync::Arc::new(
            move |event: AgentEvent| {
                let _ = on_event_tx.send(event);
            },
        );

        // Lock, configure, and run in a single critical section.
        // Engine state (messages, turn_count, usage) is modified in-place and
        // persists in the Arc for future query()/query_stream() calls.
        let (text, exit_reason, usage, num_turns) = {
            let mut engine = engine.lock().await;

            // Update per-query config
            engine.config.system_prompt = system_prompt;
            engine.config.tools = tools;
            engine.config.on_event = Some(on_event);
            engine.config.thinking = thinking;

            // Register the Agent tool executor for sub-agent spawning.
            let engine_tools = engine.config.tools.clone();
            let engine_model = engine.config.model.clone();
            let engine_api_key = engine.config.api_key.clone();
            let engine_base_url = engine.config.base_url.clone();
            let engine_cwd = engine.config.cwd.clone();

            let agent_tool_executor = move |input: serde_json::Value,
                                            _ctx: &ToolContext|
                  -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<ToolResult, AgentError>> + Send>,
            > {
                let cwd = engine_cwd.clone();
                let api_key = engine_api_key.clone();
                let base_url = engine_base_url.clone();
                let model = engine_model.clone();
                let tool_pool = engine_tools.clone();

                Box::pin(async move {
                    let description = input["description"].as_str().unwrap_or("subagent");
                    let subagent_prompt = input["prompt"].as_str().unwrap_or("");
                    let subagent_model = input["model"]
                        .as_str()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| model.clone());
                    let subagent_max_turns = input["max_turns"]
                        .as_u64()
                        .or_else(|| input["maxTurns"].as_u64())
                        .unwrap_or(10) as u32;

                    let subagent_type = input["subagent_type"]
                        .as_str()
                        .or_else(|| input["subagentType"].as_str())
                        .map(|s| s.to_string());

                    let agent_name = input["name"].as_str().map(|s| s.to_string());

                    let system_prompt =
                        build_agent_system_prompt(description, subagent_type.as_deref());

                    let mut sub_engine = QueryEngine::new(QueryEngineConfig {
                        cwd: cwd.clone(),
                        model: subagent_model,
                        api_key,
                        base_url,
                        tools: tool_pool,
                        system_prompt: Some(system_prompt),
                        max_turns: subagent_max_turns,
                        max_budget_usd: None,
                        max_tokens: 16384,
                        fallback_model: None,
                        user_context: std::collections::HashMap::new(),
                        system_context: std::collections::HashMap::new(),
                        can_use_tool: None,
                        on_event: None,
                        thinking: None,
                        abort_controller: Some(std::sync::Arc::new(
                            crate::utils::create_abort_controller_default(),
                        )),
                    });

                    match sub_engine.submit_message(subagent_prompt).await {
                        Ok((result_text, _)) => {
                            let mut content = format!("[Subagent: {}]", description);
                            if let Some(ref name) = agent_name {
                                content = format!("[Subagent: {} ({})]", description, name);
                            }
                            content = format!("{}\n\n{}", content, result_text);
                            Ok(ToolResult {
                                result_type: "text".to_string(),
                                tool_use_id: "agent_tool".to_string(),
                                content,
                                is_error: Some(false),
                                was_persisted: None,
                            })
                        }
                        Err(e) => Ok(ToolResult {
                            result_type: "text".to_string(),
                            tool_use_id: "agent_tool".to_string(),
                            content: format!("[Subagent: {}] Error: {}", description, e),
                            is_error: Some(true),
                            was_persisted: None,
                        }),
                    }
                })
            };

            engine.register_tool("Agent".to_string(), agent_tool_executor);

            // Run the query loop — messages/turns/usage accumulate in this engine
            let result = engine.submit_message(&prompt_owned).await;

            let (exit_reason, text, usage, num_turns) = match &result {
                Ok((text, reason)) => {
                    (reason.clone(), text.clone(), engine.get_usage(), engine.get_turn_count())
                }
                Err(e) => (
                    crate::types::ExitReason::ModelError {
                        error: e.to_string(),
                    },
                    format!("Error: {}", e),
                    engine.get_usage(),
                    engine.get_turn_count(),
                ),
            };

            (text, exit_reason, usage, num_turns)
        }; // Lock released here — engine persists in Arc for future calls

        let query_result = QueryResult {
            text: text.clone(),
            usage: usage.clone(),
            num_turns,
            duration_ms: 0,
            exit_reason: exit_reason.clone(),
        };

        // Store the result so QueryStream::result() can return it after stream ends
        let _ = result_storage.set(query_result.clone());

        // Dispatch Done event (always fires, even on abort/error)
        let _ = stream_tx_for_done.send(AgentEvent::Done { result: query_result });

        // Signal completion by dropping the channel sender
        drop(stream_tx);
    }

    /// Subscribe to agent events for the current and subsequent queries.
    ///
    /// Returns an [`EventSubscriber`] (implements [`futures_util::Stream`]) and a
    /// [`CancelGuard`]. Events flow to the subscriber until the guard is dropped.
    ///
    /// Takes `&self` (not `&mut self`) — you can subscribe from a shared reference,
    /// enabling decoupled TUI architectures where the subscriber is owned by a
    /// separate task from the agent.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let (mut sub, _guard) = agent.subscribe();
    /// tokio::pin!(sub);
    ///
    /// // Run query asynchronously
    /// tokio::spawn(async move {
    ///     agent.query("hello").await;
    /// });
    ///
    /// // Consume events
    /// while let Some(ev) = sub.next().await {
    ///     // render in TUI
    /// }
    /// ```
    pub fn subscribe(&self) -> (EventSubscriber, CancelGuard) {
        let (tx, rx) = mpsc::channel(256);
        let guard = CancelGuard::new(tx);
        (EventSubscriber::new(rx), guard)
    }

    /// Interrupt the agent loop. This aborts the current `prompt()` or `query()`
    /// call, cancelling any in-flight API requests and tool execution.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let mut agent = Agent::new("claude-sonnet-4-6", 10);
    ///
    /// tokio::spawn(async move {
    ///     agent.query("Do a lot of work").await.unwrap();
    /// });
    ///
    /// tokio::time::sleep(Duration::from_secs(5)).await;
    /// agent.interrupt(); // Cancel the running prompt
    /// ```
    pub fn interrupt(&self) {
        self.abort_controller.abort(None);
        // Also interrupt the persisted engine if it exists
        if let Some(ref engine) = self.persist_engine {
            if let Ok(mut eng) = engine.try_lock() {
                eng.interrupt();
            }
        }
    }
}

/// Build system prompt for subagent based on agent type
pub(super) fn build_agent_system_prompt(agent_description: &str, agent_type: Option<&str>) -> String {
    let base_prompt = "You are an agent that helps users with software engineering tasks. Use the tools available to you to assist the user.\n\nComplete the task fully—don't gold-plate, but don't leave it half-done. When you complete the task, respond with a concise report covering what was done and any key findings.";

    match agent_type {
        Some("Explore") => {
            format!(
                "{}\n\nYou are an Explore agent. Your goal is to explore and understand the codebase thoroughly. Use search and read tools to investigate. Report your findings in detail.",
                base_prompt
            )
        }
        Some("Plan") => {
            format!(
                "{}\n\nYou are a Plan agent. Your goal is to plan and analyze tasks before execution. Break down complex tasks into steps. Provide a detailed plan.",
                base_prompt
            )
        }
        Some("Review") => {
            format!(
                "{}\n\nYou are a Review agent. Your goal is to review code and provide constructive feedback. Be thorough and focus on best practices.",
                base_prompt
            )
        }
        _ => {
            // General purpose agent
            format!(
                "{}\n\nTask description: {}",
                base_prompt, agent_description
            )
        }
    }
}
