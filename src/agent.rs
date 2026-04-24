// Source: /data/home/swei/claudecode/openclaudecode/src/utils/model/agent.ts
use crate::env::EnvConfig;
use crate::error::AgentError;
use crate::query_engine::{QueryEngine, QueryEngineConfig};
use crate::stream::{CancelGuard, EventSubscriber};
use crate::tools::ask::AskUserQuestionTool;
use crate::tools::bash::BashTool;
use crate::tools::brief::BriefTool;
use crate::tools::synthetic_output::SyntheticOutputTool;
use crate::tools::config::ConfigTool;
use crate::tools::cron::{CronCreateTool, CronDeleteTool, CronListTool};
use crate::tools::edit::FileEditTool;
use crate::tools::glob::GlobTool;
use crate::tools::grep::GrepTool;
use crate::tools::lsp::LSPTool;
use crate::tools::mcp_resource_reader::ReadMcpResourceTool;
use crate::tools::mcp_resources::ListMcpResourcesTool;
use crate::tools::monitor::MonitorTool;
use crate::tools::notebook_edit::NotebookEditTool;
use crate::tools::plan::{EnterPlanModeTool, ExitPlanModeTool};
use crate::tools::read::FileReadTool as ReadTool;
use crate::tools::remote_trigger::RemoteTriggerTool;
use crate::tools::search::ToolSearchTool;
use crate::tools::send_user_file::SendUserFileTool;
use crate::tools::skill::SkillTool;
use crate::tools::skill::register_skills_from_dir;
use crate::skills::loader::load_all_skills;
use crate::utils::hooks::register_hooks_from_skills;
use crate::tools::sleep_tool::SleepTool;
use crate::tools::task_output::TaskOutputTool;
use crate::tools::tasks::{TaskCreateTool, TaskGetTool, TaskListTool, TaskUpdateTool};
use crate::tools::team::{SendMessageTool, TeamCreateTool, TeamDeleteTool};
use crate::tools::todo::TodoWriteTool;
use crate::tools::web_browser::WebBrowserTool;
use crate::tools::web_fetch::WebFetchTool;
use crate::tools::web_search::WebSearchTool;
use crate::tools::worktree::{EnterWorktreeTool, ExitWorktreeTool};
use crate::tools::write::FileWriteTool as WriteTool;
use crate::permission::{PermissionResult, PermissionAllowDecision, PermissionDenyDecision, PermissionDecisionReason};
use crate::types::AgentEvent;
use crate::types::*;
use std::sync::Arc;
use tokio::sync::{Mutex as TokioMutex, mpsc};

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

    // FileRead tool - register backfill function (expand file_path for observers)
    engine.register_tool_backfill(
        "FileRead".to_string(),
        |input: &mut serde_json::Value| {
            if let Some(fp) = input.get("file_path").and_then(|v| v.as_str()) {
                let expanded = crate::utils::path::expand_path(fp);
                if let Some(obj) = input.as_object_mut() {
                    obj.insert("file_path".to_string(), serde_json::json!(expanded));
                }
            }
        },
    );

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
    // FileWrite tool - register backfill function (expand file_path for observers)
    engine.register_tool_backfill(
        "FileWrite".to_string(),
        |input: &mut serde_json::Value| {
            if let Some(fp) = input.get("file_path").and_then(|v| v.as_str()) {
                let expanded = crate::utils::path::expand_path(fp);
                if let Some(obj) = input.as_object_mut() {
                    obj.insert("file_path".to_string(), serde_json::json!(expanded));
                }
            }
        },
    );

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
        get_tool_use_summary: Some(Arc::new(move |input| {
            fns_summary.get_tool_use_summary(input)
        })),
        get_activity_description: None,
        render_tool_result_message: Some(Arc::new(move |content, _progress, _options| {
            fns_render.render_tool_result_message(content)
        })),
    };
    engine.register_tool_with_render("FileEdit".to_string(), edit_executor, render_fns);
    // FileEdit tool - register backfill function (expand file_path for observers)
    engine.register_tool_backfill(
        "FileEdit".to_string(),
        |input: &mut serde_json::Value| {
            if let Some(fp) = input.get("file_path").and_then(|v| v.as_str()) {
                let expanded = crate::utils::path::expand_path(fp);
                if let Some(obj) = input.as_object_mut() {
                    obj.insert("file_path".to_string(), serde_json::json!(expanded));
                }
            }
        },
    );

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

    // TaskOutput tool
    let task_output_executor = move |input: serde_json::Value,
                                     ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = TaskOutputTool::new();
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
    engine.register_tool("TaskOutput".to_string(), task_output_executor);

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
    engine.register_tool(
        "ListMcpResourcesTool".to_string(),
        list_mcp_resources_executor,
    );

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
    engine.register_tool(
        "ReadMcpResourceTool".to_string(),
        read_mcp_resource_executor,
    );

    // BriefTool (SendUserMessage) — primary visible output channel
    let brief_executor = move |input: serde_json::Value,
                               ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = BriefTool::new();
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
    engine.register_tool("SendUserMessage".to_string(), brief_executor);

    // SyntheticOutputTool (StructuredOutput) — structured output enforcement
    let synthetic_output_executor =
        move |input: serde_json::Value, ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
            let tool_clone = SyntheticOutputTool::new();
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
    engine.register_tool("StructuredOutput".to_string(), synthetic_output_executor);
}

/// Subscriber info for fan-out event delivery
///
/// Thread-safe, Clone-able agent handle for tokio async usage.
/// All internal state is held behind a single `Arc<Mutex<>>` — cloning Agent
/// just increments the reference count. All public methods take `&self`.
///
/// # Sharing across tasks
///
/// ```rust,ignore
/// let agent = Agent::new("claude-sonnet-4-6");
///
/// // Clone into another task
/// let agent2 = agent.clone();
/// let handle = tokio::spawn(async move {
///     agent2.query("do work").await
/// });
///
/// // Subscribe from the original
/// let (mut sub, _guard) = agent.subscribe();
/// ```
#[derive(Clone)]
pub struct Agent {
    pub(crate) inner: std::sync::Arc<std::sync::Mutex<AgentInner>>,
}

#[cfg(test)]
impl Agent {
    /// Test-only accessor for the inner agent state.
    pub(crate) fn inner_for_test(&self) -> &std::sync::Arc<std::sync::Mutex<AgentInner>> {
        &self.inner
    }
}

pub(crate) struct AgentInner {
    model: String,
    api_key: Option<String>,
    base_url: Option<String>,
    cwd: String,
    system_prompt: Option<String>,
    max_turns: u32,
    max_budget_usd: Option<f64>,
    max_tokens: u32,
    fallback_model: Option<String>,
    pub(crate) thinking: Option<ThinkingConfig>,
    mcp_servers: Option<std::collections::HashMap<String, crate::mcp::McpServerConfig>>,
    tool_pool: Vec<ToolDefinition>,
    pub(crate) allowed_tools: Vec<String>,
    pub(crate) disallowed_tools: Vec<String>,
    #[cfg(test)]
    pub on_event: Option<std::sync::Arc<dyn Fn(AgentEvent) + Send + Sync>>,
    #[cfg(not(test))]
    pub(crate) on_event: Option<std::sync::Arc<dyn Fn(AgentEvent) + Send + Sync>>,
    session_id: String,
    abort_controller: std::sync::Arc<crate::utils::AbortController>,
    /// Persisted QueryEngine for multi-turn reuse (matches TypeScript pattern).
    /// Shared via Arc<TokioMutex> so spawned tasks from query() can access
    /// the same conversation state (messages, usage, turns).
    /// `None` until first query — lazily initialized.
    engine: Option<Arc<TokioMutex<QueryEngine>>>,
}

impl Agent {
    /// Create a new agent with the given model name.
    ///
    /// The model defaults to the `AI_MODEL` environment variable, then
    /// `"claude-sonnet-4-6"`. All other config (API key, base URL, max turns,
    /// thinking) also defaults from environment.
    ///
    /// Chain builder methods to customize:
    /// ```ignore
    /// let agent = Agent::new("claude-sonnet-4-6")
    ///     .max_turns(20)
    ///     .system_prompt("You are a code reviewer.")
    ///     .thinking(ThinkingConfig::Enabled { budget_tokens: 4096 });
    /// ```
    ///
    /// Returns a [`Clone`]able handle — all internal state uses interior
    /// mutability, so [`Agent::query`] takes `&self` and the agent can be
    /// shared across async tasks via `Arc<Agent>`.
    pub fn new(model: &str) -> Self {
        let env_config = EnvConfig::load();
        let model = env_config.model.unwrap_or_else(|| model.to_string());
        let api_key = env_config.auth_token.clone();
        let base_url = env_config.base_url.clone();
        let cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| ".".to_string());

        Self {
            inner: std::sync::Arc::new(std::sync::Mutex::new(AgentInner {
                model,
                api_key,
                base_url,
                cwd,
                system_prompt: None,
                max_turns: 10,
                max_budget_usd: None,
                max_tokens: 16384,
                fallback_model: None,
                thinking: None,
                mcp_servers: None,
                tool_pool: vec![],
                allowed_tools: vec![],
                disallowed_tools: vec![],
                on_event: None,
                session_id: uuid::Uuid::new_v4().to_string(),
                abort_controller: std::sync::Arc::new(
                    crate::utils::create_abort_controller_default(),
                ),
                engine: None,
            })),
        }
    }

    /// Configure the model name.
    ///
    /// Triggers engine recreation if the engine is already initialized.
    pub fn model(mut self, model: &str) -> Self {
        self.inner.lock().unwrap().model = model.to_string();
        self
    }

    /// Set the API key.
    ///
    /// Triggers engine recreation if the engine is already initialized.
    pub fn api_key(mut self, api_key: &str) -> Self {
        self.inner.lock().unwrap().api_key = Some(api_key.to_string());
        self
    }

    /// Set a custom base URL for the API.
    ///
    /// Triggers engine recreation if the engine is already initialized.
    pub fn base_url(mut self, base_url: &str) -> Self {
        self.inner.lock().unwrap().base_url = Some(base_url.to_string());
        self
    }

    /// Set the working directory.
    ///
    /// Triggers engine recreation if the engine is already initialized.
    pub fn cwd(mut self, cwd: &str) -> Self {
        self.inner.lock().unwrap().cwd = cwd.to_string();
        self
    }

    /// Set a custom system prompt.
    pub fn system_prompt(mut self, prompt: &str) -> Self {
        self.inner.lock().unwrap().system_prompt = Some(prompt.to_string());
        self
    }

    /// Set the maximum number of turns.
    ///
    /// Triggers engine recreation if the engine is already initialized.
    pub fn max_turns(mut self, max_turns: u32) -> Self {
        self.inner.lock().unwrap().max_turns = max_turns;
        self
    }

    /// Set the maximum budget in USD.
    pub fn max_budget_usd(mut self, budget: f64) -> Self {
        self.inner.lock().unwrap().max_budget_usd = Some(budget);
        self
    }

    /// Set the maximum tokens for a single response.
    pub fn max_tokens(mut self, max_tokens: u32) -> Self {
        self.inner.lock().unwrap().max_tokens = max_tokens;
        self
    }

    /// Set the fallback model.
    pub fn fallback_model(mut self, model: &str) -> Self {
        self.inner.lock().unwrap().fallback_model = Some(model.to_string());
        self
    }

    /// Set thinking configuration for extended thinking.
    pub fn thinking(mut self, thinking: ThinkingConfig) -> Self {
        self.inner.lock().unwrap().thinking = Some(thinking);
        self
    }

    /// Set tool definitions.
    pub fn tools(mut self, tools: Vec<ToolDefinition>) -> Self {
        self.inner.lock().unwrap().tool_pool = tools;
        self
    }

    /// Only allow specific tools by name.
    pub fn allowed_tools(mut self, tools: Vec<String>) -> Self {
        self.inner.lock().unwrap().allowed_tools = tools;
        self
    }

    /// Explicitly disallow specific tools by name.
    pub fn disallowed_tools(mut self, tools: Vec<String>) -> Self {
        self.inner.lock().unwrap().disallowed_tools = tools;
        self
    }

    /// Set MCP server configurations.
    pub fn mcp_servers(
        mut self,
        servers: std::collections::HashMap<String, crate::mcp::McpServerConfig>,
    ) -> Self {
        self.inner.lock().unwrap().mcp_servers = Some(servers);
        self
    }

    /// Set an event callback for streaming agent events.
    ///
    /// Can be called at any time (before or after `query()`). Takes `&self`
    /// so it can be updated between queries for dynamic event handling.
    pub fn on_event<F>(mut self, callback: F) -> Self
    where
        F: Fn(AgentEvent) + Send + Sync + 'static,
    {
        self.inner.lock().unwrap().on_event = Some(std::sync::Arc::new(callback));
        self
    }

    /// Uses interior mutability — takes `&self` so the agent can be shared across tasks.
    /// Lazily creates the QueryEngine on first query.
    fn init_engine(&self) {
        let mut inner = self.inner.lock().unwrap();
        if inner.engine.is_none() {
            let cwd = inner.cwd.clone();
            let allowed_tools = inner.allowed_tools.clone();
            let disallowed_tools = inner.disallowed_tools.clone();
            let tool_pool = inner.tool_pool.clone();
            let can_use_tool: Option<
                std::sync::Arc<dyn Fn(ToolDefinition, serde_json::Value) -> PermissionResult + Send + Sync>,
            > = if !allowed_tools.is_empty() || !disallowed_tools.is_empty() {
                Some(std::sync::Arc::new(
                    move |tool_def: ToolDefinition, _input: serde_json::Value| {
                        if !allowed_tools.is_empty() && !allowed_tools.contains(&tool_def.name) {
                            return PermissionResult::Deny(PermissionDenyDecision::new(
                                &format!("Tool '{}' is not in the allowed tools list", tool_def.name),
                                PermissionDecisionReason::Other { reason: "allowed tools filter".to_string() },
                            ));
                        }
                        if disallowed_tools.contains(&tool_def.name) {
                            return PermissionResult::Deny(PermissionDenyDecision::new(
                                &format!("Tool '{}' is in the disallowed tools list", tool_def.name),
                                PermissionDecisionReason::Other { reason: "disallowed tools filter".to_string() },
                            ));
                        }
                        PermissionResult::Allow(PermissionAllowDecision::default())
                    },
                ))
            } else {
                None
            };
            let config = QueryEngineConfig {
                cwd: cwd.clone(),
                model: inner.model.clone(),
                api_key: inner.api_key.clone(),
                base_url: inner.base_url.clone(),
                tools: tool_pool,
                system_prompt: None,
                max_turns: inner.max_turns,
                max_budget_usd: inner.max_budget_usd,
                max_tokens: inner.max_tokens,
                fallback_model: inner.fallback_model.clone(),
                user_context: std::collections::HashMap::new(),
                system_context: std::collections::HashMap::new(),
                can_use_tool,
                on_event: inner.on_event.clone(),
                thinking: inner.thinking.clone(),
                abort_controller: Some(inner.abort_controller.clone()),
                token_budget: None,
                agent_id: None,
                loaded_nested_memory_paths: std::collections::HashSet::new(),
            };
            let mut engine = QueryEngine::new(config);
            register_all_tool_executors(&mut engine);

            // Register hooks from loaded skills
            let session_id = inner.session_id.clone();
            if let Ok(skills) = load_all_skills(&cwd) {
                let _set_app_state = |_: &dyn Fn(&mut serde_json::Value)| {};
                register_hooks_from_skills(&_set_app_state, &session_id, &skills);
            }

            inner.engine = Some(Arc::new(TokioMutex::new(engine)));
        }
    }

    /// One-shot query — creates an agent, sends the prompt, and returns the text.
    ///
    /// Use this for single-turn interactions where you don't need conversation history
    /// or multi-turn reuse. For persistent agents, use `Agent::new()` + `.query()`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let answer = Agent::prompt("claude-sonnet-4-6", "Explain quantum computing")
    ///     .await?;
    /// println!("{answer}");
    /// ```
    pub async fn prompt(model: &str, prompt: &str) -> Result<String, AgentError> {
        let agent = Self::new(model);
        let result = agent.query(prompt).await?;
        Ok(result.text)
    }

    pub fn get_model(&self) -> String {
        self.inner.lock().unwrap().model.clone()
    }

    pub fn get_session_id(&self) -> String {
        self.inner.lock().unwrap().session_id.clone()
    }

    /// Get all messages in the conversation history.
    /// Delegates to the persisted QueryEngine which owns the message state
    /// (matches TypeScript: engine.mutableMessages).
    pub fn get_messages(&self) -> Vec<Message> {
        let inner = self.inner.lock().unwrap();
        inner
            .engine
            .as_ref()
            .and_then(|e| e.try_lock().ok().map(|g| g.get_messages()))
            .unwrap_or_default()
    }

    /// Get all tools available to the agent
    pub fn get_tools(&self) -> Vec<ToolDefinition> {
        self.inner.lock().unwrap().tool_pool.clone()
    }

    /// Set system prompt for the agent (interior mutability).
    pub fn set_system_prompt(&self, prompt: &str) {
        self.inner.lock().unwrap().system_prompt = Some(prompt.to_string());
    }

    /// Set the working directory for the agent (interior mutability).
    pub fn set_cwd(&self, cwd: &str) {
        self.inner.lock().unwrap().cwd = cwd.to_string();
    }

    /// Set the event callback for agent events.
    ///
    /// Can be called at any time. Takes effect on the next `query()` call,
    /// where `on_event` is set on the engine before submitting the message.
    /// Takes `&self` (interior mutability).
    pub fn set_event_callback<F>(&self, callback: F)
    where
        F: Fn(AgentEvent) + Send + Sync + 'static,
    {
        self.inner.lock().unwrap().on_event = Some(std::sync::Arc::new(callback));
    }

    /// Set thinking configuration for the agent (interior mutability).
    pub fn set_thinking(&self, thinking: Option<ThinkingConfig>) {
        self.inner.lock().unwrap().thinking = thinking;
    }

    /// Execute a tool directly (for testing/demo purposes).
    /// Takes `&self` (interior mutability).
    pub async fn execute_tool(
        &self,
        name: &str,
        input: serde_json::Value,
    ) -> Result<ToolResult, AgentError> {
        let inner = &*self.inner.lock().unwrap();
        let cwd = inner.cwd.clone();
        let model = inner.model.clone();
        let api_key = inner.api_key.clone();
        let base_url = inner.base_url.clone();

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
            abort_controller: Some(inner.abort_controller.clone()),
            token_budget: None,
            agent_id: None,
            loaded_nested_memory_paths: std::collections::HashSet::new(),
        });

        // Snapshot parent context fields to thread into subagent closures
        let parent_can_use_tool: Option<
            std::sync::Arc<dyn Fn(ToolDefinition, serde_json::Value) -> PermissionResult + Send + Sync>,
        > = {
            let allowed_tools = inner.allowed_tools.clone();
            let disallowed_tools = inner.disallowed_tools.clone();
            if !allowed_tools.is_empty() || !disallowed_tools.is_empty() {
                Some(std::sync::Arc::new(
                    move |tool_def: ToolDefinition, _input: serde_json::Value| {
                        if !allowed_tools.is_empty() && !allowed_tools.contains(&tool_def.name) {
                            return PermissionResult::Deny(PermissionDenyDecision::new(
                                &format!("Tool '{}' is not in the allowed tools list", tool_def.name),
                                PermissionDecisionReason::Other { reason: "allowed tools filter".to_string() },
                            ));
                        }
                        if disallowed_tools.contains(&tool_def.name) {
                            return PermissionResult::Deny(PermissionDenyDecision::new(
                                &format!("Tool '{}' is in the disallowed tools list", tool_def.name),
                                PermissionDecisionReason::Other { reason: "disallowed tools filter".to_string() },
                            ));
                        }
                        PermissionResult::Allow(PermissionAllowDecision::default())
                    },
                ))
            } else {
                None
            }
        };
        let parent_on_event = inner.on_event.clone();
        let parent_thinking = inner.thinking.clone();

        // Register all tool executors (including Bash, Read, Write, etc.)
        register_all_tool_executors(&mut engine);

        // Register Agent tool executor with full parameter support
        let subagent_abort = inner.abort_controller.clone();
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
            let parent_can_use_tool = parent_can_use_tool.clone();
            let parent_on_event = parent_on_event.clone();
            let parent_thinking = parent_thinking.clone();

            Box::pin(async move {
                // Extract ALL parameters from input
                let description = input["description"].as_str().unwrap_or("subagent").to_string();
                let subagent_prompt = input["prompt"].as_str().unwrap_or("").to_string();
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

                // Extract run_in_background — spawns the subagent in a tokio task
                let run_in_background = input["run_in_background"]
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
                let system_prompt =
                    build_agent_system_prompt(&description, subagent_type.as_deref());

                // Create sub-agent engine with proper system prompt
                let mut sub_engine = QueryEngine::new(QueryEngineConfig {
                    cwd: subagent_cwd,
                    model: subagent_model.to_string(),
                    api_key,
                    base_url,
                    tools: crate::tools::get_all_base_tools(),
                    system_prompt: Some(system_prompt),
                    max_turns,
                    max_budget_usd: None,
                    max_tokens: 16384,
                    fallback_model: None,
                    user_context: std::collections::HashMap::new(),
                    system_context: std::collections::HashMap::new(),
                    can_use_tool: parent_can_use_tool,
                    on_event: parent_on_event,
                    thinking: parent_thinking,
                    abort_controller: Some(subagent_abort.clone()),
                    token_budget: None,
                    agent_id: agent_name.clone().or_else(|| Some(description.to_string())),
                    loaded_nested_memory_paths: std::collections::HashSet::new(),
                });
                register_all_tool_executors(&mut sub_engine);

                // Initialize agent MCP servers — merges MCP tools into subagent engine
                let mcp_result = {
                    let mcp_servers =
                        crate::services::mcp::agent_mcp::parse_agent_mcp_servers(&input);
                    if !mcp_servers.is_empty() {
                        let result =
                            crate::services::mcp::agent_mcp::initialize_agent_mcp_servers(
                                &mcp_servers, None,
                            )
                            .await;

                        // Merge MCP tool definitions into sub_engine's tool list
                        let mcp_tool_count = result.tools.len();
                        let mcp_conn_count = result.connections.len();
                        if mcp_tool_count > 0 {
                            for mcp_tool in &result.tools {
                                sub_engine
                                    .config
                                    .tools
                                    .push(mcp_tool.clone());

                                // Register MCP tool executor via registry
                                let mcp_registry =
                                    crate::services::mcp::tool_executor::McpToolRegistry::new();
                                let executor = crate::services::mcp::tool_executor::
                                    create_named_mcp_executor(
                                    mcp_registry,
                                    &mcp_tool.name,
                                );
                                sub_engine.register_tool(mcp_tool.name.clone(), executor);
                            }

                            log::info!(
                                "[Subagent: {}] Added {} MCP tools from {} server(s)",
                                description,
                                mcp_tool_count,
                                mcp_conn_count
                            );
                        }

                        Some(result)
                    } else {
                        None
                    }
                };

                // Execute subagent task
                let result: Result<ToolResult, AgentError> = if run_in_background {
                    // Spawn subagent in a tokio task and return immediately with a task ID
                    let task_id = uuid::Uuid::new_v4().to_string();
                    let task_id_clone = task_id.clone();
                    let prompt = subagent_prompt.clone();
                    let desc = description.clone();
                    tokio::spawn(async move {
                        match sub_engine.submit_message(&prompt).await {
                            Ok((result_text, _)) => {
                                log::info!("[BackgroundAgent:{task_id}] {desc}: {result_text}");
                            }
                            Err(e) => {
                                log::error!("[BackgroundAgent:{task_id}] {desc}: {e}");
                            }
                        }
                    });
                    Ok(ToolResult {
                        result_type: "text".to_string(),
                        tool_use_id: "agent_tool".to_string(),
                        content: format!(
                            "[Background subagent '{}'] Task {} started. Use TaskOutput(task_id=\"{}\") to retrieve results.",
                            description, task_id_clone, task_id_clone
                        ),
                        is_error: Some(false),
                        was_persisted: None,
                    })
                } else {
                    match sub_engine.submit_message(&subagent_prompt).await {
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
                };

                // Cleanup MCP connections after subagent completion
                if let Some(mcp_result) = mcp_result {
                    (mcp_result.cleanup)();
                }

                result
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

        let inner = &*self.inner.lock().unwrap();
        let ai_md_prompt = load_ai_md(cwd).ok().flatten();
        let memory_mechanics_prompt =
            if inner.system_prompt.is_some() && crate::memdir::has_auto_mem_path_override() {
                load_memory_prompt_sync()
            } else {
                None
            };
        let base_system_prompt = base_build_system_prompt();

        let system_prompt = match (
            &ai_md_prompt,
            &memory_mechanics_prompt,
            &inner.system_prompt,
        ) {
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
    /// Tools are sorted by name for prompt cache stability (matches TypeScript assembleToolPool).
    /// Applies deny rules to filter out disallowed MCP tools.
    fn select_tools(&self) -> Vec<ToolDefinition> {
        let inner = &*self.inner.lock().unwrap();
        let tools = if inner.tool_pool.is_empty() {
            crate::tools::get_all_base_tools()
        } else {
            inner.tool_pool.clone()
        };
        let disallowed_tools = inner.disallowed_tools.clone();

        // Sort by name for prompt cache stability (built-in tools are already sorted)
        let mut sorted = tools;
        sorted.sort_by(|a, b| a.name.cmp(&b.name));
        // Deduplicate by name (first occurrence wins)
        let mut seen = std::collections::HashSet::new();
        sorted.retain(|t| seen.insert(t.name.clone()));

        // Apply deny rules (MCP server-prefix, wildcard, exact match)
        if !disallowed_tools.is_empty() {
            sorted = crate::tools::filter_tools_by_deny_rules(&sorted, &disallowed_tools);
        }
        sorted
    }

    /// Main query method - handles the full agent loop including tool use,
    /// streaming responses, and multi-turn interaction with the LLM.
    ///
    /// Reuses a persisted QueryEngine across calls so that conversation history,
    /// usage tracking, and tool state accumulate naturally (matches TypeScript pattern).
    ///
    /// Takes `&self` (interior mutability) — the agent can be shared across tasks.
    pub async fn query(&self, prompt: &str) -> Result<QueryResult, AgentError> {
        self.init_engine();

        // Clone all data from AgentInner before any .await (MutexGuard is not Send).
        // Single lock acquisition.
        let (cwd, on_event, thinking, abort_controller, engine) = {
            let inner = self.inner.lock().unwrap();
            (
                inner.cwd.clone(),
                inner.on_event.clone(),
                inner.thinking.clone(),
                inner.abort_controller.clone(),
                Arc::clone(inner.engine.as_ref().unwrap()),
            )
        };
        let cwd_path = std::path::Path::new(&cwd);

        let system_prompt = self.build_system_prompt(cwd_path);
        let tools = self.select_tools();

        let start = std::time::Instant::now();
        let (response_text, exit_reason, current_model, usage, turns) = {
            let mut eng = engine.lock().await;

            // Update per-query config
            eng.config.system_prompt = system_prompt;
            eng.config.tools = tools;
            eng.config.on_event = on_event;
            eng.config.thinking = thinking;

            // Snapshot engine config values needed by the subagent closure
            let engine_tools = eng.config.tools.clone();
            let engine_model = eng.config.model.clone();
            let engine_api_key = eng.config.api_key.clone();
            let engine_base_url = eng.config.base_url.clone();
            let engine_cwd = eng.config.cwd.clone();
            let subagent_abort = abort_controller.clone();
            // Additional values for fork subagent path
            let engine_messages = eng.messages.clone();
            let engine_user_context = eng.config.user_context.clone();
            let engine_system_context = eng.config.system_context.clone();
            let engine_thinking = eng.config.thinking.clone();
            let engine_can_use_tool = eng.config.can_use_tool.clone();
            let engine_on_event = eng.config.on_event.clone();

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
                let parent_messages = engine_messages.clone();
                let parent_user_context = engine_user_context.clone();
                let parent_system_context = engine_system_context.clone();
                let parent_thinking = engine_thinking.clone();
                let parent_can_use_tool = engine_can_use_tool.clone();
                let parent_on_event = engine_on_event.clone();

                Box::pin(async move {
                    let description = input["description"].as_str().unwrap_or("subagent").to_string();
                    let subagent_prompt = input["prompt"].as_str().unwrap_or("").to_string();
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

                    let run_in_background = input["run_in_background"]
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

                    // Fork subagent detection: when no subagent_type is specified AND fork is enabled
                    let is_fork = subagent_type.is_none()
                        && crate::tools::agent::prompt::is_fork_subagent_enabled()
                        && !parent_messages.iter().any(|m| {
                            m.role == crate::types::MessageRole::User
                                && m.content.contains(crate::tools::agent::constants::FORK_BOILERPLATE_TAG)
                        });

                    let system_prompt =
                        build_agent_system_prompt(&description, subagent_type.as_deref());

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
                        can_use_tool: parent_can_use_tool,
                        on_event: parent_on_event,
                        thinking: parent_thinking.clone(),
                        abort_controller: Some(subagent_abort.clone()),
                        token_budget: None,
                        agent_id: agent_name.clone().or_else(|| Some(description.to_string())),
                        loaded_nested_memory_paths: std::collections::HashSet::new(),
                    });
                    register_all_tool_executors(&mut sub_engine);

                    // Fork subagent: configure cache-safe params and forked messages
                    if is_fork {
                        // Empty system prompt for prompt cache sharing with parent
                        sub_engine.config.system_prompt = Some(String::new());
                        // Inherit parent's context for cache sharing
                        sub_engine.config.user_context = parent_user_context.clone();
                        sub_engine.config.system_context = parent_system_context.clone();
                        sub_engine.config.thinking = parent_thinking.clone();
                        // Use fork agent definition's max_turns
                        let fork_agent = crate::tools::agent::fork_subagent::fork_agent();
                        sub_engine.config.max_turns = fork_agent.max_turns.unwrap_or(200) as u32;
                        // Build forked messages for prompt cache sharing
                        let forked_messages = crate::tools::agent::fork_subagent::build_forked_messages_from_sdk(
                            &parent_messages,
                            &subagent_prompt,
                        );
                        sub_engine.set_messages(forked_messages);
                    }

                    if run_in_background {
                        let task_id = uuid::Uuid::new_v4().to_string();
                        let task_id_display = task_id.clone();
                        let prompt = subagent_prompt.clone();
                        let desc = description.clone();
                        tokio::spawn(async move {
                            match sub_engine.submit_message(&prompt).await {
                                Ok((result_text, _)) => {
                                    log::info!("[BackgroundAgent:{task_id}] {desc}: {result_text}");
                                }
                                Err(e) => {
                                    log::error!("[BackgroundAgent:{task_id}] {desc}: {e}");
                                }
                            }
                        });
                        Ok(ToolResult {
                            result_type: "text".to_string(),
                            tool_use_id: "agent_tool".to_string(),
                            content: format!(
                                "[Background subagent '{}'] Task {} started. Use TaskOutput(task_id=\"{}\") to retrieve results.",
                                description, task_id_display, task_id_display
                            ),
                            is_error: Some(false),
                            was_persisted: None,
                        })
                    } else {
                        match sub_engine.submit_message(&subagent_prompt).await {
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
        if current_model != self.get_model() {
            self.inner.lock().unwrap().model = current_model;
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
    /// Clears all messages, usage tracking, and turn count. This starts a fresh
    /// conversation while preserving model, API key, tools, and other settings.
    /// Takes `&self` (interior mutability).
    pub fn reset(&self) {
        {
            let inner = &*self.inner.lock().unwrap();
            if let Some(engine) = &inner.engine {
                if let Ok(mut eng) = engine.try_lock() {
                    eng.reset();
                }
            }
        }
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

    /// Interrupt the agent loop. This aborts the current `query()` call,
    /// cancelling any in-flight API requests and tool execution.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let agent = Agent::new("claude-sonnet-4-6");
    ///
    /// tokio::spawn(async move {
    ///     agent.query("Do a lot of work").await.unwrap();
    /// });
    ///
    /// tokio::time::sleep(Duration::from_secs(5)).await;
    /// agent.interrupt(); // Cancel the running prompt
    /// ```
    pub fn interrupt(&self) {
        {
            let inner = &*self.inner.lock().unwrap();
            inner.abort_controller.abort(None);
            if let Some(ref engine) = inner.engine {
                if let Ok(mut eng) = engine.try_lock() {
                    eng.interrupt();
                }
            }
        }
    }
}

/// Build system prompt for subagent based on agent type
pub(super) fn build_agent_system_prompt(
    agent_description: &str,
    agent_type: Option<&str>,
) -> String {
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
            format!("{}\n\nTask description: {}", base_prompt, agent_description)
        }
    }
}
