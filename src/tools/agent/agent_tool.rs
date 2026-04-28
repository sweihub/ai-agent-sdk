// Source: ~/claudecode/openclaudecode/src/tools/AgentTool/

//! AgentTool struct that implements the Tool trait from tools/types.rs.
//! Replaces the inline closures used in agent.rs for the "Agent" tool.

use std::collections::HashMap;
use std::sync::Arc;

use crate::error::AgentError;
use crate::query_engine::{QueryEngine, QueryEngineConfig};
use crate::tools::types::{Tool, ToolInputSchema};
use crate::types::ToolResult;
use crate::types::{Message, ToolContext};
use super::agent_tool_utils::extract_partial_result_from_engine;

/// Configuration for the AgentTool, held behind an Arc for cloning into closures.
#[derive(Clone)]
pub struct AgentToolConfig {
    pub cwd: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub model: String,
    pub tool_pool: Vec<crate::types::ToolDefinition>,
    pub abort_controller: Arc<crate::utils::AbortController>,
    pub can_use_tool: Option<
        Arc<dyn Fn(crate::types::ToolDefinition, serde_json::Value) -> crate::permission::PermissionResult + Send + Sync>,
    >,
    pub on_event: Option<Arc<dyn Fn(crate::types::AgentEvent) + Send + Sync>>,
    pub thinking: Option<crate::types::ThinkingConfig>,
    /// Parent messages for fork subagent logic. Used in the query() path.
    pub parent_messages: Vec<Message>,
    /// Parent user context for fork subagent. Used in the query() path.
    pub parent_user_context: HashMap<String, String>,
    /// Parent system context for fork subagent. Used in the query() path.
    pub parent_system_context: HashMap<String, String>,
    /// Parent session ID for sidechain transcript recording.
    pub parent_session_id: Option<String>,
}

/// A tool that spawns subagents to handle complex, multi-step tasks autonomously.
///
/// Implements the `Tool` trait from `tools/types.rs` and mirrors the executor
/// logic previously implemented as inline closures in `agent.rs`.
pub struct AgentTool {
    config: AgentToolConfig,
}

impl AgentTool {
    /// Create a new AgentTool with the given configuration.
    pub fn new(config: AgentToolConfig) -> Self {
        Self { config }
    }

    /// Get a reference to the tool's configuration.
    pub fn config(&self) -> &AgentToolConfig {
        &self.config
    }
}

impl Tool for AgentTool {
    fn name(&self) -> &str {
        "Agent"
    }

    fn description(&self) -> &str {
        "Launch a new agent to handle complex, multi-step tasks autonomously. Use this tool to spawn specialized subagents."
    }

    fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema {
            schema_type: "object".to_string(),
            properties: serde_json::json!({
                "description": {
                    "type": "string",
                    "description": "A short description (3-5 words) summarizing what the agent will do"
                },
                "subagent_type": {
                    "type": "string",
                    "description": "The type of subagent to use. If omitted, uses the general-purpose agent."
                },
                "prompt": {
                    "type": "string",
                    "description": "The task prompt for the subagent to execute"
                },
                "model": {
                    "type": "string",
                    "description": "Optional model override for this subagent"
                },
                "max_turns": {
                    "type": "number",
                    "description": "Maximum number of turns for this subagent (default: 10)"
                },
                "run_in_background": {
                    "type": "boolean",
                    "description": "Whether to run the agent in the background (default: false)"
                },
                "name": {
                    "type": "string",
                    "description": "Optional name for the subagent"
                },
                "team_name": {
                    "type": "string",
                    "description": "Optional team name for the subagent"
                },
                "mode": {
                    "type": "string",
                    "description": "Optional permission mode for the subagent"
                },
                "cwd": {
                    "type": "string",
                    "description": "Optional working directory for the subagent"
                },
                "isolation": {
                    "type": "string",
                    "enum": ["worktree", "remote"],
                    "description": "Isolation mode: 'worktree' for git worktree, 'remote' for remote CCR"
                }
            }),
            required: Some(vec!["description".to_string(), "prompt".to_string()]),
        }
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let config = self.config.clone();

        // Extract ALL parameters from input
        let description = input["description"].as_str().unwrap_or("subagent").to_string();
        let subagent_prompt = input["prompt"].as_str().unwrap_or("").to_string();
        let subagent_model = input["model"]
            .as_str()
            .map(|s| s.to_string())
            .unwrap_or_else(|| config.model.clone());
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
            .unwrap_or_else(|| config.cwd.clone());

        let _isolation = input["isolation"].as_str().map(|s| s.to_string());

        // Build system prompt for subagent
        let system_prompt =
            crate::agent::build_agent_system_prompt(&description, subagent_type.as_deref());

        // Create sub-agent engine with proper system prompt
        let mut sub_engine = QueryEngine::new(QueryEngineConfig {
            cwd: subagent_cwd,
            model: subagent_model.to_string(),
            api_key: config.api_key.clone(),
            base_url: config.base_url.clone(),
            tools: config.tool_pool.clone(),
            system_prompt: Some(system_prompt),
            max_turns,
            max_budget_usd: None,
            max_tokens: crate::utils::context::get_max_output_tokens_for_model(&subagent_model) as u32,
            fallback_model: None,
            user_context: HashMap::new(),
            system_context: HashMap::new(),
            can_use_tool: config.can_use_tool.clone(),
            on_event: config.on_event.clone(),
            thinking: config.thinking.clone(),
            abort_controller: Some(config.abort_controller.clone()),
            token_budget: None,
            agent_id: agent_name.clone().or_else(|| Some(description.to_string())),
            session_state: None,
            loaded_nested_memory_paths: std::collections::HashSet::new(),
            task_budget: None,
            orphaned_permission: None,
        });

        // Register all tool executors on the sub-engine
        crate::agent::register_all_tool_executors(&mut sub_engine);

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

        // Fork subagent detection: when no subagent_type is specified AND fork is enabled
        let is_fork = subagent_type.is_none()
            && crate::tools::agent::prompt::is_fork_subagent_enabled()
            && !config.parent_messages.iter().any(|m| {
                m.role == crate::types::MessageRole::User
                    && m.content.contains(crate::tools::agent::constants::FORK_BOILERPLATE_TAG)
            });

        // Fork subagent: configure cache-safe params and forked messages
        if is_fork {
            // Empty system prompt for prompt cache sharing with parent
            sub_engine.config.system_prompt = Some(String::new());
            // Inherit parent's context for cache sharing
            sub_engine.config.user_context = config.parent_user_context.clone();
            sub_engine.config.system_context = config.parent_system_context.clone();
            // Use fork agent definition's max_turns
            let fork_agent = crate::tools::agent::fork_subagent::fork_agent();
            sub_engine.config.max_turns = fork_agent.max_turns.unwrap_or(200) as u32;
            // Build forked messages for prompt cache sharing
            let forked_messages = crate::tools::agent::fork_subagent::build_forked_messages_from_sdk(
                &config.parent_messages,
                &subagent_prompt,
            );
            sub_engine.set_messages(forked_messages);
        }

        // Execute subagent task
        let result: Result<ToolResult, AgentError> = if run_in_background {
            // Spawn subagent in a tokio task and return immediately with a task ID
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
                        // Distinguish killed vs failed for background agents too
                        let is_killed = matches!(e, AgentError::UserAborted);
                        if is_killed {
                            let partial = super::agent_tool_utils::extract_partial_result_from_engine(&sub_engine.messages)
                                .unwrap_or_else(|| "No output produced".to_string());
                            log::info!(
                                "[BackgroundAgent:{task_id}] {desc}: Killed - partial: {}",
                                partial
                            );
                        } else {
                            log::error!("[BackgroundAgent:{task_id}] {desc}: Failed - {e}");
                        }
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
                Err(e) => {
                    // Distinguish abort/kill from other errors.
                    // Matches TypeScript agentToolUtils.ts:638-681:
                    // AbortError -> status 'killed' with finalMessage from extractPartialResult
                    // Other errors -> status 'failed' with error message
                    let is_killed = matches!(e, AgentError::UserAborted)
                        || config.abort_controller.is_aborted();

                    if is_killed {
                        let partial = extract_partial_result_from_engine(&sub_engine.messages)
                            .unwrap_or_else(|| "No output produced".to_string());
                        log::info!(
                            "[Subagent: {}] Killed - partial result: {}",
                            description, partial
                        );
                        Ok(ToolResult {
                            result_type: "text".to_string(),
                            tool_use_id: "agent_tool".to_string(),
                            content: format!(
                                "[Subagent: {}] Status: killed\nFinal output: {}",
                                description, partial
                            ),
                            is_error: Some(true),
                            was_persisted: None,
                        })
                    } else {
                        log::error!("[Subagent: {}] Failed: {}", description, e);
                        Ok(ToolResult {
                            result_type: "text".to_string(),
                            tool_use_id: "agent_tool".to_string(),
                            content: format!(
                                "[Subagent: {}] Status: failed\nError: {}",
                                description, e
                            ),
                            is_error: Some(true),
                            was_persisted: None,
                        })
                    }
                }
            }
        };

        // Cleanup MCP connections after subagent completion
        if let Some(mcp_result) = mcp_result {
            (mcp_result.cleanup)();
        }

        result
    }
}

/// Create a tool executor closure from an AgentTool for use with
/// `QueryEngine::register_tool()`.
///
/// The executor clones the AgentTool (via Arc) and calls its `execute` method.
pub fn create_agent_tool_executor(
    tool: Arc<AgentTool>,
) -> impl Fn(serde_json::Value, &ToolContext) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<ToolResult, AgentError>> + Send>> + Send + Sync + 'static {
    move |input: serde_json::Value,
          ctx: &ToolContext|
     -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<ToolResult, AgentError>> + Send>> {
        let tool_clone = Arc::clone(&tool);
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    }
}
