// Source: /data/home/swei/claudecode/openclaudecode/src/utils/model/agent.ts
use crate::query_engine::{QueryEngine, QueryEngineConfig};
use crate::env::EnvConfig;
use crate::error::AgentError;
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

/// Register all built-in tool executors
fn register_all_tool_executors(engine: &mut QueryEngine) {
    type BoxFuture<T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send>>;

    // Bash tool - clone tool and ctx into async block
    let bash_executor = move |input: serde_json::Value,
                              ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = BashTool::new();
        let cwd = ctx.cwd.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: None,
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
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: None,
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
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: None,
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
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: None,
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
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: None,
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    engine.register_tool("Grep".to_string(), grep_executor);

    // FileEdit tool
    let edit_executor = move |input: serde_json::Value,
                              ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = FileEditTool::new();
        let cwd = ctx.cwd.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: None,
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    engine.register_tool("FileEdit".to_string(), edit_executor);

    // Skill tool - register skills from examples/skills directory
    use std::path::Path;
    register_skills_from_dir(Path::new("examples/skills"));

    let skill_executor = move |input: serde_json::Value,
                               ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = SkillTool::new();
        let cwd = ctx.cwd.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: None,
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
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: None,
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
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: None,
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
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: None,
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
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: None,
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
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: None,
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
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: None,
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
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: None,
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
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: None,
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
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: None,
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
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: None,
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
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: None,
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
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: None,
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
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: None,
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
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: None,
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
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: None,
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
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: None,
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
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: None,
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
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: None,
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
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: None,
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
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: None,
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
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: None,
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
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: None,
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
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: None,
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
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: None,
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
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: None,
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
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: None,
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
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: None,
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
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: None,
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
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: None,
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    engine.register_tool("ReadMcpResourceTool".to_string(), read_mcp_resource_executor);
}

pub struct Agent {
    config: AgentOptions,
    model: String,
    api_key: Option<String>,
    base_url: Option<String>,
    tool_pool: Vec<ToolDefinition>,
    messages: Vec<Message>,
    session_id: String,
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
            messages: vec![],
            session_id,
        }
    }

    pub fn get_model(&self) -> &str {
        &self.model
    }

    pub fn get_session_id(&self) -> &str {
        &self.session_id
    }

    /// Get all messages in the conversation history
    pub fn get_messages(&self) -> &[Message] {
        &self.messages
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
        });

        // Register all tool executors (including Bash, Read, Write, etc.)
        register_all_tool_executors(&mut engine);

        // Register Agent tool executor with full parameter support
        let agent_tool_executor = move |input: serde_json::Value,
                                        _ctx: &ToolContext|
              -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<ToolResult, AgentError>> + Send>,
        > {
            let cwd = cwd.clone();
            let api_key = api_key.clone();
            let base_url = base_url.clone();
            let model = model.clone();

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

    /// Simple blocking prompt method - sends a prompt and returns the result.
    /// This matches the TypeScript SDK's agent.prompt() API.
    pub async fn prompt(&mut self, prompt: &str) -> Result<QueryResult, AgentError> {
        self.query(prompt).await
    }

    pub async fn query(&mut self, prompt: &str) -> Result<QueryResult, AgentError> {
        use crate::ai_md::load_ai_md;
        use crate::memdir::load_memory_prompt_sync;
        use crate::prompts::build_system_prompt;
        use crate::tools::get_all_base_tools;

        let cwd = self.config.cwd.clone().unwrap_or_else(|| {
            std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| ".".to_string())
        });
        let cwd_path = std::path::Path::new(&cwd);
        let model = self.model.clone();
        let api_key = self.api_key.clone();
        let base_url = self.base_url.clone();

        // Build system prompt: AI.md + memory mechanics + custom system prompt
        // This matches TypeScript's memoryMechanicsPrompt logic:
        // customPrompt !== undefined && hasAutoMemPathOverride() ? loadMemoryPrompt() : null
        let ai_md_prompt = load_ai_md(cwd_path).ok().flatten();

        // Memory mechanics prompt is ONLY loaded when:
        // 1. custom system prompt exists (customPrompt !== undefined)
        // 2. AI_COWORK_MEMORY_PATH_OVERRIDE is set (hasAutoMemPathOverride())
        let memory_mechanics_prompt = if self.config.system_prompt.is_some()
            && crate::memdir::has_auto_mem_path_override() {
            load_memory_prompt_sync()
        } else {
            None
        };

        // Use the full system prompt from prompts module (matches TypeScript)
        let base_system_prompt = build_system_prompt();

        // Combine system prompt matching TypeScript's order:
        // [...(customPrompt !== undefined ? [customPrompt] : defaultSystemPrompt),
        //  ...(memoryMechanicsPrompt ? [memoryMechanicsPrompt] : []),
        //  ...(appendSystemPrompt ? [appendSystemPrompt] : [])]
        // Note: appendSystemPrompt is not exposed in current SDK but we reserve the slot
        let system_prompt = match (&ai_md_prompt, &memory_mechanics_prompt, &self.config.system_prompt) {
            // AI.md + memory mechanics + base + custom (custom always present in this branch)
            (Some(ai_md), Some(mem), Some(custom)) => Some(format!(
                "{}\n\n{}\n\n{}\n\n{}",
                ai_md, mem, base_system_prompt, custom
            )),
            // AI.md + memory mechanics + base (no custom)
            (Some(ai_md), Some(mem), None) => {
                Some(format!("{}\n\n{}\n\n{}", ai_md, mem, base_system_prompt))
            }
            // AI.md + base + custom (memory mechanics null)
            (Some(ai_md), None, Some(custom)) => {
                Some(format!("{}\n\n{}\n\n{}", ai_md, base_system_prompt, custom))
            }
            // AI.md + base (no custom, no memory)
            (Some(ai_md), None, None) => Some(format!("{}\n\n{}", ai_md, base_system_prompt)),
            // No AI.md, memory mechanics + base + custom
            (None, Some(mem), Some(custom)) => {
                Some(format!("{}\n\n{}\n\n{}", mem, base_system_prompt, custom))
            }
            // No AI.md, memory mechanics + base (no custom)
            (None, Some(mem), None) => Some(format!("{}\n\n{}", mem, base_system_prompt)),
            // No AI.md, no memory, base + custom
            (None, None, Some(custom)) => Some(format!("{}\n\n{}", base_system_prompt, custom)),
            // Base only
            (None, None, None) => Some(base_system_prompt),
        };

        // Use base tools if tool_pool is empty
        let tools = if self.tool_pool.is_empty() {
            get_all_base_tools()
        } else {
            self.tool_pool.clone()
        };

        let on_event = self.config.on_event.clone();
        let thinking = self.config.thinking.clone();
        let mut engine = QueryEngine::new(QueryEngineConfig {
            cwd: cwd.clone(),
            model: model.clone(),
            api_key: api_key.clone(),
            base_url: base_url.clone(),
            tools,
            system_prompt,
            max_turns: self.config.max_turns.unwrap_or(10),
            max_budget_usd: self.config.max_budget_usd,
            max_tokens: self.config.max_tokens.unwrap_or(16384),
            fallback_model: self.config.fallback_model.clone(),
            user_context: std::collections::HashMap::new(),
            system_context: std::collections::HashMap::new(),
            can_use_tool: None,
            on_event,
            thinking,
        });

        // Register all tool executors on the engine so they can be called
        register_all_tool_executors(&mut engine);

        // Clone tool_pool before the closure to avoid capturing self
        let tool_pool = self.tool_pool.clone();

        // Register the Agent tool executor to spawn sub-agents with full parameter support
        let agent_tool_executor = move |input: serde_json::Value,
                                        _ctx: &ToolContext|
              -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<ToolResult, AgentError>> + Send>,
        > {
            let cwd = cwd.clone();
            let api_key = api_key.clone();
            let base_url = base_url.clone();
            let model = model.clone();
            let tool_pool = tool_pool.clone();

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

                // NEW: Extract run_in_background (ignored for now)
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

                // NEW: Extract mode
                let _mode = input["mode"].as_str().map(|s| s.to_string());

                // NEW: Extract cwd (working directory override)
                let subagent_cwd = input["cwd"]
                    .as_str()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| cwd.clone());

                // NEW: Extract isolation
                let _isolation = input["isolation"].as_str().map(|s| s.to_string());

                // Build system prompt for subagent based on agent type
                let system_prompt = build_agent_system_prompt(description, subagent_type.as_deref());

                // Use parent agent's tool pool for the subagent
                let parent_tools = tool_pool;

                // Create a new engine for the subagent
                let mut sub_engine = QueryEngine::new(QueryEngineConfig {
                    cwd: subagent_cwd,
                    model: subagent_model.to_string(),
                    api_key,
                    base_url,
                    tools: parent_tools,
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
                });

                // Run the subagent
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

        // Pass existing messages to engine for continuing conversation
        engine.set_messages(self.messages.clone());

        let start = std::time::Instant::now();
        let (response_text, exit_reason) = engine.submit_message(prompt).await?;
        let messages = engine.get_messages();

        // Get actual usage from engine
        let engine_usage = engine.get_usage();
        let usage = TokenUsage {
            input_tokens: engine_usage.input_tokens,
            output_tokens: engine_usage.output_tokens,
            cache_creation_input_tokens: engine_usage.cache_creation_input_tokens,
            cache_read_input_tokens: engine_usage.cache_read_input_tokens,
        };

        // Store messages in agent
        self.messages = messages;

        Ok(QueryResult {
            text: response_text,
            usage,
            num_turns: engine.get_turn_count(),
            duration_ms: start.elapsed().as_millis() as u64,
            exit_reason,
        })
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
