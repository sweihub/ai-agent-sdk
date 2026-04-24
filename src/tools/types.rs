// Source: /data/home/swei/claudecode/openclaudecode/src/utils/filePersistence/types.ts
use crate::types::*;
use std::future::Future;

pub use crate::types::{ToolDefinition, ToolInputSchema};

// Schema functions for all tools
fn monitor_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({}),
        required: None,
    }
}

fn send_user_file_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({}),
        required: None,
    }
}

fn web_browser_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({}),
        required: None,
    }
}

fn brief_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({
            "message": {
                "type": "string",
                "description": "The message for the user. Supports markdown formatting."
            },
            "attachments": {
                "type": "array",
                "items": {"type": "string"},
                "description": "Optional file paths to attach"
            },
            "status": {
                "type": "string",
                "enum": ["normal", "proactive"],
                "description": "Use 'proactive' when surfacing something the user hasn't asked for"
            }
        }),
        required: Some(vec!["message".to_string()]),
    }
}

fn synthetic_output_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({}),
        required: None,
    }
}

/// A boxed async future that returns a ToolResult or AgentError.
/// This is the standard return type for tool executor functions.
pub type ToolFuture =
    std::pin::Pin<Box<dyn Future<Output = Result<ToolResult, crate::error::AgentError>> + Send>>;

pub trait Tool {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> ToolInputSchema;
    fn execute(
        &self,
        input: serde_json::Value,
        context: &ToolContext,
    ) -> impl Future<Output = Result<ToolResult, crate::error::AgentError>> + Send;
    /// Backfill observable input before observers see it (SDK stream, transcript, hooks).
    /// Mutates in place to add legacy/derived fields. Must be idempotent.
    /// The original API-bound input is never mutated (preserves prompt cache).
    fn backfill_observable_input(&self, _input: &mut serde_json::Value) {}
}

fn bash_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({
            "command": {
                "type": "string",
                "description": "The shell command to execute"
            }
        }),
        required: Some(vec!["command".to_string()]),
    }
}

fn file_read_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({
            "path": {
                "type": "string",
                "description": "The file path to read"
            }
        }),
        required: Some(vec!["path".to_string()]),
    }
}

fn file_write_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({
            "path": {
                "type": "string",
                "description": "The file path to write to"
            },
            "content": {
                "type": "string",
                "description": "The content to write"
            }
        }),
        required: Some(vec!["path".to_string(), "content".to_string()]),
    }
}

fn glob_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({
            "pattern": {
                "type": "string",
                "description": "The glob pattern to match files against"
            },
            "path": {
                "type": "string",
                "description": "The directory to search in. If not specified, the current working directory will be used."
            }
        }),
        required: Some(vec!["pattern".to_string()]),
    }
}

fn grep_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({
            "pattern": {
                "type": "string",
                "description": "The regex pattern to search for"
            },
            "path": {
                "type": "string",
                "description": "The file or directory to search in"
            }
        }),
        required: Some(vec!["pattern".to_string()]),
    }
}

fn file_edit_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({
            "file_path": {
                "type": "string",
                "description": "The absolute path to the file to modify"
            },
            "old_string": {
                "type": "string",
                "description": "The exact text to find and replace"
            },
            "new_string": {
                "type": "string",
                "description": "The replacement text"
            },
            "replace_all": {
                "type": "boolean",
                "description": "Replace all occurrences (default false)"
            }
        }),
        required: Some(vec![
            "file_path".to_string(),
            "old_string".to_string(),
            "new_string".to_string(),
        ]),
    }
}

fn notebook_edit_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({
            "notebook_path": {
                "type": "string",
                "description": "The absolute path to the Jupyter notebook file to edit (must be absolute, not relative)"
            },
            "cell_id": {
                "type": "string",
                "description": "The ID of the cell to edit. When inserting a new cell, the new cell will be inserted after the cell with this ID."
            },
            "new_source": {
                "type": "string",
                "description": "The new source for the cell"
            },
            "cell_type": {
                "type": "string",
                "enum": ["code", "markdown"],
                "description": "The type of the cell (code or markdown). If not specified, defaults to the current cell type."
            },
            "edit_mode": {
                "type": "string",
                "enum": ["replace", "insert", "delete"],
                "description": "The type of edit to make (replace, insert, delete). Defaults to replace."
            }
        }),
        required: Some(vec!["notebook_path".to_string(), "new_source".to_string()]),
    }
}

fn web_fetch_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({
            "url": {
                "type": "string",
                "description": "The URL to fetch content from"
            },
            "headers": {
                "type": "object",
                "description": "Optional HTTP headers",
                "additionalProperties": {
                    "type": "string"
                }
            },
            "prompt": {
                "type": "string",
                "description": "Optional prompt for LLM-based content extraction"
            }
        }),
        required: Some(vec!["url".to_string()]),
    }
}

fn web_search_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({
            "query": {
                "type": "string",
                "description": "The search query"
            },
            "num_results": {
                "type": "number",
                "description": "Number of results to return (default: 5)"
            }
        }),
        required: Some(vec!["query".to_string()]),
    }
}

const ALL_TOOLS: &[(&str, &str, fn() -> ToolDefinition)] = &[
    ("Bash", "Execute shell commands", || ToolDefinition {
        name: "Bash".to_string(),
        description: "Execute shell commands".to_string(),
        input_schema: bash_schema(),
        annotations: None,
        should_defer: None,
        always_load: None,
        is_mcp: None,
        search_hint: None,
        aliases: None,
        user_facing_name: None,
        interrupt_behavior: None,
    }),
    ("FileRead", "Read files, images, PDFs, notebooks", || {
        ToolDefinition {
            name: "FileRead".to_string(),
            description: "Read files from filesystem. Supports text files, images (PNG, JPG, GIF, WebP), PDFs, and Jupyter notebooks. Use offset and limit for large files.".to_string(),
            input_schema: file_read_schema(),
            annotations: Some(ToolAnnotations::concurrency_safe()),
            should_defer: Some(false),
            always_load: Some(true),
            is_mcp: None,
            search_hint: Some("read files, images, PDFs, notebooks".to_string()),
        aliases: None,
    user_facing_name: None,
            interrupt_behavior: None,
        }
    }),
    ("FileWrite", "Write content to files", || ToolDefinition {
        name: "FileWrite".to_string(),
        description: "Write content to files".to_string(),
        input_schema: file_write_schema(),
        annotations: None,
        should_defer: None,
        always_load: None,
        is_mcp: None,
        search_hint: None,
        aliases: None,
        user_facing_name: None,
        interrupt_behavior: None,
    }),
    ("Glob", "Find files by name pattern or wildcard", || {
        ToolDefinition {
            name: "Glob".to_string(),
            description: "Find files by glob pattern (glob pattern matching for file discovery)"
                .to_string(),
            input_schema: glob_schema(),
            annotations: Some(ToolAnnotations::concurrency_safe()),
            should_defer: Some(false),
            always_load: Some(true),
            is_mcp: None,
            search_hint: Some("find files by name pattern or wildcard".to_string()),
            aliases: None,
            user_facing_name: None,
            interrupt_behavior: None,
        }
    }),
    ("Grep", "Search file contents using regex", || {
        ToolDefinition {
        name: "Grep".to_string(),
        description: "Search file contents using regex patterns. Uses ripgrep (rg) if available, falls back to grep.".to_string(),
        input_schema: grep_schema(),
        annotations: Some(ToolAnnotations::concurrency_safe()),
        should_defer: Some(false),
        always_load: Some(true),
        is_mcp: None,
        search_hint: Some("search file contents using regex".to_string()),
    aliases: None,
    user_facing_name: None,
        interrupt_behavior: None,
    }
    }),
    (
        "FileEdit",
        "Edit files by performing exact string replacements",
        || ToolDefinition {
            name: "FileEdit".to_string(),
            description: "Edit files by performing exact string replacements".to_string(),
            input_schema: file_edit_schema(),
            annotations: None,
            should_defer: None,
            always_load: None,
            is_mcp: None,
            search_hint: None,
            aliases: None,
            user_facing_name: None,
            interrupt_behavior: None,
        },
    ),
    (
        "NotebookEdit",
        "Edit Jupyter notebook cells (.ipynb)",
        || ToolDefinition {
            name: "NotebookEdit".to_string(),
            description:
                "Edit Jupyter notebook (.ipynb) cells: replace, insert, or delete cell content"
                    .to_string(),
            input_schema: notebook_edit_schema(),
            annotations: None,
            should_defer: Some(true),
            always_load: None,
            is_mcp: None,
            search_hint: Some("edit Jupyter notebook cells (.ipynb)".to_string()),
            aliases: None,
            user_facing_name: None,
            interrupt_behavior: None,
        },
    ),
    (
        "WebFetch",
        "Fetch content from a URL and return it as text",
        || {
            ToolDefinition {
        name: "WebFetch".to_string(),
        description: "Fetch content from a URL and return it as text. Supports HTML pages, JSON APIs, and plain text. Strips HTML tags for readability. Preapproved hosts can be fetched without additional permission.".to_string(),
        input_schema: web_fetch_schema(),
        annotations: None,
        should_defer: Some(true),
        always_load: None,
        is_mcp: None,
        search_hint: Some("fetch web pages and URLs".to_string()),
    aliases: None,
    user_facing_name: None,
        interrupt_behavior: None,
    }
        },
    ),
    ("WebSearch", "Search the web for information", || {
        ToolDefinition {
        name: "WebSearch".to_string(),
        description: "Search the web for information. Returns search results with titles, URLs, and snippets.".to_string(),
        input_schema: web_search_schema(),
        annotations: Some(ToolAnnotations::concurrency_safe()),
        should_defer: Some(true),
        always_load: None,
        is_mcp: None,
        search_hint: Some("web search for information".to_string()),
    aliases: None,
    user_facing_name: None,
        interrupt_behavior: None,
    }
    }),
    (
        "Agent",
        "Launch a new agent to handle complex multi-step tasks",
        || {
            ToolDefinition {
        name: "Agent".to_string(),
        description: "Launch a new agent to handle complex, multi-step tasks autonomously. Use this tool to spawn specialized subagents.".to_string(),
        input_schema: agent_schema(),
        annotations: None,
        should_defer: None,
        always_load: None,
        is_mcp: None,
        search_hint: None,
    aliases: None,
    user_facing_name: None,
        interrupt_behavior: None,
    }
        },
    ),
    ("TaskCreate", "Create a new task in the task list", || {
        ToolDefinition {
            name: "TaskCreate".to_string(),
            description: "Create a new task in the task list".to_string(),
            input_schema: task_create_schema(),
            annotations: None,
            should_defer: None,
            always_load: None,
            is_mcp: None,
            search_hint: None,
            aliases: None,
            user_facing_name: None,
            interrupt_behavior: None,
        }
    }),
    ("TaskList", "List all tasks in the task list", || {
        ToolDefinition {
            name: "TaskList".to_string(),
            description: "List all tasks in the task list".to_string(),
            input_schema: task_list_schema(),
            annotations: Some(ToolAnnotations::concurrency_safe()),
            should_defer: None,
            always_load: None,
            is_mcp: None,
            search_hint: None,
            aliases: None,
            user_facing_name: None,
            interrupt_behavior: None,
        }
    }),
    ("TaskUpdate", "Update an existing task", || ToolDefinition {
        name: "TaskUpdate".to_string(),
        description: "Update an existing task's status or details".to_string(),
        input_schema: task_update_schema(),
        annotations: None,
        should_defer: None,
        always_load: None,
        is_mcp: None,
        search_hint: None,
        aliases: None,
        user_facing_name: None,
        interrupt_behavior: None,
    }),
    ("TaskGet", "Get details of a specific task", || {
        ToolDefinition {
            name: "TaskGet".to_string(),
            description: "Get details of a specific task by ID".to_string(),
            input_schema: task_get_schema(),
            annotations: Some(ToolAnnotations::concurrency_safe()),
            should_defer: None,
            always_load: None,
            is_mcp: None,
            search_hint: None,
            aliases: None,
            user_facing_name: None,
            interrupt_behavior: None,
        }
    }),
    (
        "TeamCreate",
        "Create a team of agents for parallel work",
        || ToolDefinition {
            name: "TeamCreate".to_string(),
            description: "Create a team of agents that can work in parallel".to_string(),
            input_schema: team_create_schema(),
            annotations: None,
            should_defer: None,
            always_load: None,
            is_mcp: None,
            search_hint: None,
            aliases: None,
            user_facing_name: None,
            interrupt_behavior: None,
        },
    ),
    ("TeamDelete", "Delete a team of agents", || ToolDefinition {
        name: "TeamDelete".to_string(),
        description: "Delete a previously created team".to_string(),
        input_schema: team_delete_schema(),
        annotations: None,
        should_defer: None,
        always_load: None,
        is_mcp: None,
        search_hint: None,
        aliases: None,
        user_facing_name: None,
        interrupt_behavior: None,
    }),
    ("SendMessage", "Send a message to another agent", || {
        ToolDefinition {
            name: "SendMessage".to_string(),
            description: "Send a message to another agent".to_string(),
            input_schema: send_message_schema(),
            annotations: None,
            should_defer: None,
            always_load: None,
            is_mcp: None,
            search_hint: None,
            aliases: None,
            user_facing_name: None,
            interrupt_behavior: None,
        }
    }),
    ("EnterWorktree", "Create and enter a git worktree", || {
        ToolDefinition {
            name: "EnterWorktree".to_string(),
            description: "Create and enter a git worktree for isolated work".to_string(),
            input_schema: enter_worktree_schema(),
            annotations: None,
            should_defer: None,
            always_load: None,
            is_mcp: None,
            search_hint: None,
            aliases: None,
            user_facing_name: None,
            interrupt_behavior: None,
        }
    }),
    (
        "ExitWorktree",
        "Exit a worktree and return to original directory",
        || ToolDefinition {
            name: "ExitWorktree".to_string(),
            description: "Exit a worktree and return to the original working directory".to_string(),
            input_schema: exit_worktree_schema(),
            annotations: None,
            should_defer: None,
            always_load: None,
            is_mcp: None,
            search_hint: None,
            aliases: None,
            user_facing_name: None,
            interrupt_behavior: None,
        },
    ),
    ("EnterPlanMode", "Enter structured planning mode", || {
        ToolDefinition {
            name: "EnterPlanMode".to_string(),
            description: "Enter structured planning mode to explore and design implementation"
                .to_string(),
            input_schema: enter_plan_mode_schema(),
            annotations: None,
            should_defer: None,
            always_load: None,
            is_mcp: None,
            search_hint: None,
            aliases: None,
            user_facing_name: None,
            interrupt_behavior: None,
        }
    }),
    ("ExitPlanMode", "Exit planning mode", || ToolDefinition {
        name: "ExitPlanMode".to_string(),
        description: "Exit planning mode and present the plan for approval".to_string(),
        input_schema: exit_plan_mode_schema(),
        annotations: None,
        should_defer: None,
        always_load: None,
        is_mcp: None,
        search_hint: None,
        aliases: None,
        user_facing_name: None,
        interrupt_behavior: None,
    }),
    (
        "AskUserQuestion",
        "Ask the user a question with multiple choice options",
        || ToolDefinition {
            name: "AskUserQuestion".to_string(),
            description: "Ask the user a question with multiple choice options".to_string(),
            input_schema: ask_user_question_schema(),
            annotations: Some(ToolAnnotations::concurrency_safe()),
            should_defer: None,
            always_load: None,
            is_mcp: None,
            search_hint: None,
            aliases: None,
            user_facing_name: None,
            interrupt_behavior: None,
        },
    ),
    ("ToolSearch", "Search for available tools", || {
        ToolDefinition {
            name: "ToolSearch".to_string(),
            description: "Search for available tools by name or description".to_string(),
            input_schema: tool_search_schema(),
            annotations: Some(ToolAnnotations::concurrency_safe()),
            should_defer: None,
            always_load: None,
            is_mcp: None,
            search_hint: None,
            aliases: None,
            user_facing_name: None,
            interrupt_behavior: None,
        }
    }),
    ("CronCreate", "Create a scheduled task", || ToolDefinition {
        name: "CronCreate".to_string(),
        description: "Create a scheduled task that runs on a cron schedule".to_string(),
        input_schema: cron_create_schema(),
        annotations: None,
        should_defer: None,
        always_load: None,
        is_mcp: None,
        search_hint: None,
        aliases: None,
        user_facing_name: None,
        interrupt_behavior: None,
    }),
    ("CronDelete", "Delete a scheduled task", || ToolDefinition {
        name: "CronDelete".to_string(),
        description: "Delete a previously created scheduled task".to_string(),
        input_schema: cron_delete_schema(),
        annotations: Some(ToolAnnotations::concurrency_safe()),
        should_defer: None,
        always_load: None,
        is_mcp: None,
        search_hint: None,
        aliases: None,
        user_facing_name: None,
        interrupt_behavior: None,
    }),
    ("CronList", "List all scheduled tasks", || ToolDefinition {
        name: "CronList".to_string(),
        description: "List all scheduled tasks".to_string(),
        input_schema: cron_list_schema(),
        annotations: Some(ToolAnnotations::concurrency_safe()),
        should_defer: None,
        always_load: None,
        is_mcp: None,
        search_hint: None,
        aliases: None,
        user_facing_name: None,
        interrupt_behavior: None,
    }),
    ("Config", "Read or update configuration", || {
        ToolDefinition {
            name: "Config".to_string(),
            description: "Read or update dynamic configuration".to_string(),
            input_schema: config_schema(),
            annotations: Some(ToolAnnotations::concurrency_safe()),
            should_defer: None,
            always_load: None,
            is_mcp: None,
            search_hint: None,
            aliases: None,
            user_facing_name: None,
            interrupt_behavior: None,
        }
    }),
    ("TodoWrite", "Manage the session task checklist", || {
        ToolDefinition {
            name: "TodoWrite".to_string(),
            description:
                "Update the todo list for this session. Provide the complete updated list of todos."
                    .to_string(),
            input_schema: todo_write_schema(),
            annotations: Some(ToolAnnotations::concurrency_safe()),
            should_defer: Some(true),
            always_load: None,
            is_mcp: None,
            search_hint: Some("manage the session task checklist".to_string()),
            aliases: None,
            user_facing_name: None,
            interrupt_behavior: None,
        }
    }),
    ("Skill", "Invoke a skill by name", || {
        ToolDefinition {
        name: "Skill".to_string(),
        description: "Invoke a skill by name. Skills are pre-built workflows or commands that can be executed to accomplish specific tasks.".to_string(),
        input_schema: skill_schema(),
        annotations: Some(ToolAnnotations::concurrency_safe()),
        should_defer: None,
        always_load: None,
        is_mcp: None,
        search_hint: Some("invoke skills and workflows".to_string()),
    aliases: None,
    user_facing_name: None,
        interrupt_behavior: None,
    }
    }),
    ("TaskStop", "Stop a running background task", || {
        ToolDefinition {
        name: "TaskStop".to_string(),
        description: "Stop a running background task by ID. Also accepts shell_id for backward compatibility with the deprecated KillShell tool.".to_string(),
        input_schema: task_stop_schema(),
        annotations: Some(ToolAnnotations::concurrency_safe()),
        should_defer: Some(true),
        always_load: None,
        is_mcp: None,
        search_hint: Some("kill a running background task".to_string()),
    aliases: None,
    user_facing_name: None,
        interrupt_behavior: None,
    }
    }),
    ("TaskOutput", "Retrieve output from background tasks", || {
        ToolDefinition {
        name: "TaskOutput".to_string(),
        description: "Retrieve output from a running or completed background task (bash command, agent, etc.). Supports blocking wait for completion with configurable timeout.".to_string(),
        input_schema: task_output_schema(),
        annotations: Some(ToolAnnotations::concurrency_safe()),
        should_defer: None,
        always_load: None,
        is_mcp: None,
        search_hint: Some("get task output and results".to_string()),
    aliases: None,
    user_facing_name: None,
        interrupt_behavior: None,
    }
    }),
    ("Monitor", "Monitor system resources", || ToolDefinition {
        name: "Monitor".to_string(),
        description: "Monitor system resources and performance".to_string(),
        input_schema: monitor_schema(),
        annotations: None,
        should_defer: None,
        always_load: None,
        is_mcp: None,
        search_hint: None,
        aliases: None,
        user_facing_name: None,
        interrupt_behavior: None,
    }),
    ("send_user_file", "Send a file from user to agent", || {
        ToolDefinition {
            name: "send_user_file".to_string(),
            description: "Send a file from the user to the agent".to_string(),
            input_schema: send_user_file_schema(),
            annotations: None,
            should_defer: None,
            always_load: None,
            is_mcp: None,
            search_hint: None,
            aliases: None,
            user_facing_name: None,
            interrupt_behavior: None,
        }
    }),
    ("WebBrowser", "Control a web browser", || ToolDefinition {
        name: "WebBrowser".to_string(),
        description: "Control a web browser for automation".to_string(),
        input_schema: web_browser_schema(),
        annotations: None,
        should_defer: None,
        always_load: None,
        is_mcp: None,
        search_hint: None,
        aliases: None,
        user_facing_name: None,
        interrupt_behavior: None,
    }),
    (
        "LSP",
        "Code intelligence via Language Server Protocol",
        || {
            ToolDefinition {
        name: "LSP".to_string(),
        description: "Interact with Language Server Protocol servers for code intelligence (definitions, references, symbols, hover, call hierarchy)".to_string(),
        input_schema: lsp_schema(),
        annotations: Some(ToolAnnotations::concurrency_safe()),
        should_defer: None,
        always_load: None,
        is_mcp: None,
        search_hint: None,
    aliases: None,
    user_facing_name: None,
        interrupt_behavior: None,
    }
        },
    ),
    (
        "RemoteTrigger",
        "Manage remote Claude Code agents via CCR API",
        || ToolDefinition {
            name: "RemoteTrigger".to_string(),
            description:
                "Manage scheduled remote Claude Code agents (triggers) via the claude.ai CCR API"
                    .to_string(),
            input_schema: remote_trigger_schema(),
            annotations: None,
            should_defer: None,
            always_load: None,
            is_mcp: None,
            search_hint: None,
            aliases: None,
            user_facing_name: None,
            interrupt_behavior: None,
        },
    ),
    ("ListMcpResourcesTool", "List MCP server resources", || {
        ToolDefinition {
            name: "ListMcpResourcesTool".to_string(),
            description: "List available resources from configured MCP servers".to_string(),
            input_schema: list_mcp_resources_schema(),
            annotations: Some(ToolAnnotations::concurrency_safe()),
            should_defer: None,
            always_load: None,
            is_mcp: None,
            search_hint: None,
            aliases: None,
            user_facing_name: None,
            interrupt_behavior: None,
        }
    }),
    (
        "ReadMcpResourceTool",
        "Read MCP server resources by URI",
        || ToolDefinition {
            name: "ReadMcpResourceTool".to_string(),
            description: "Read a specific resource from an MCP server by URI".to_string(),
            input_schema: read_mcp_resource_schema(),
            annotations: Some(ToolAnnotations::concurrency_safe()),
            should_defer: None,
            always_load: None,
            is_mcp: None,
            search_hint: None,
            aliases: None,
            user_facing_name: None,
            interrupt_behavior: None,
        },
    ),
    (
        "SendUserMessage",
        "Send a message to the user",
        || ToolDefinition {
            name: "SendUserMessage".to_string(),
            description: "Send a message to the user that they will actually read. Text outside this tool is visible in the detail view, but most won't open it -- the answer lives here.".to_string(),
            input_schema: brief_schema(),
            annotations: Some(ToolAnnotations::concurrency_safe()),
            should_defer: None,
            always_load: None,
            is_mcp: None,
            search_hint: Some("send message to user".to_string()),
            aliases: None,
            user_facing_name: None,
            interrupt_behavior: None,
        },
    ),
    (
        "StructuredOutput",
        "Return structured output in the requested format",
        || ToolDefinition {
            name: "StructuredOutput".to_string(),
            description: "Return structured output in the requested format. You MUST call this tool exactly once at the end of your response to provide the structured output.".to_string(),
            input_schema: synthetic_output_schema(),
            annotations: Some(ToolAnnotations::concurrency_safe()),
            should_defer: None,
            always_load: None,
            is_mcp: None,
            search_hint: Some("return the final response as structured JSON".to_string()),
            aliases: None,
            user_facing_name: None,
            interrupt_behavior: None,
        },
    ),
];

fn agent_schema() -> ToolInputSchema {
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
            "isolation": {
                "type": "string",
                "enum": ["worktree", "remote"],
                "description": "Isolation mode: 'worktree' for git worktree, 'remote' for remote CCR"
            }
        }),
        required: Some(vec!["description".to_string(), "prompt".to_string()]),
    }
}

// Task tool schemas
fn task_create_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({
            "subject": { "type": "string", "description": "A brief title for the task" },
            "description": { "type": "string", "description": "What needs to be done" },
            "activeForm": { "type": "string", "description": "Spinner text when in_progress" }
        }),
        required: Some(vec!["subject".to_string(), "description".to_string()]),
    }
}

fn task_list_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({}),
        required: None,
    }
}

fn task_update_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({
            "taskId": { "type": "string", "description": "The ID of the task to update" },
            "subject": { "type": "string", "description": "New subject for the task" },
            "description": { "type": "string", "description": "New description" },
            "status": { "type": "string", "enum": ["pending", "in_progress", "completed", "deleted"], "description": "New status" },
            "activeForm": { "type": "string", "description": "New spinner text" }
        }),
        required: Some(vec!["taskId".to_string()]),
    }
}

fn task_get_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({
            "taskId": { "type": "string", "description": "The ID of the task to retrieve" }
        }),
        required: Some(vec!["taskId".to_string()]),
    }
}

// Team tool schemas
fn team_create_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({
            "name": { "type": "string", "description": "Name of the team" },
            "description": { "type": "string", "description": "Description of what the team does" },
            "agents": { "type": "array", "items": serde_json::json!({}), "description": "List of agents in the team" }
        }),
        required: Some(vec!["name".to_string()]),
    }
}

fn team_delete_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({
            "name": { "type": "string", "description": "Name of the team to delete" }
        }),
        required: Some(vec!["name".to_string()]),
    }
}

fn send_message_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({
            "to": { "type": "string", "description": "Agent name to send message to" },
            "message": { "type": "string", "description": "Message content" }
        }),
        required: Some(vec!["to".to_string(), "message".to_string()]),
    }
}

// Worktree tool schemas
fn enter_worktree_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({
            "name": { "type": "string", "description": "Optional name for the worktree" }
        }),
        required: None,
    }
}

fn exit_worktree_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({
            "action": { "type": "string", "enum": ["keep", "remove"], "description": "What to do with the worktree" },
            "discardChanges": { "type": "boolean", "description": "Discard uncommitted changes before removing" }
        }),
        required: None,
    }
}

// Plan mode tool schemas
fn enter_plan_mode_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({
            "allowedPrompts": { "type": "array", "items": { "type": "string" }, "description": "Prompt-based permissions" }
        }),
        required: None,
    }
}

fn exit_plan_mode_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({}),
        required: None,
    }
}

// Ask user question schema
fn ask_user_question_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({
            "question": { "type": "string", "description": "The complete question to ask the user" },
            "header": { "type": "string", "description": "Very short label displayed as a chip/tag (max 12 chars)" },
            "options": { "type": "array", "items": serde_json::json!({}), "description": "Available choices for this question. Must have 2-4 options." },
            "multiSelect": { "type": "boolean", "description": "Set to true to allow the user to select multiple options instead of just one" },
            "preview": { "type": "object", "properties": { "type": { "type": "string", "enum": ["html", "markdown"] }, "content": { "type": "string" } }, "description": "Optional HTML or Markdown preview to show the user alongside the question" }
        }),
        required: Some(vec![
            "question".to_string(),
            "header".to_string(),
            "options".to_string(),
        ]),
    }
}

// ToolSearch schema
fn tool_search_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({
            "query": { "type": "string", "description": "Query to find deferred tools. Use \"select:<tool_name>\" for direct selection, or keywords to search." },
            "max_results": { "type": "number", "description": "Maximum number of results to return (default: 5)" }
        }),
        required: Some(vec!["query".to_string()]),
    }
}

// TaskStop schema
fn task_stop_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({
            "task_id": { "type": "string", "description": "The ID of the background task to stop" },
            "shell_id": { "type": "string", "description": "Deprecated: use task_id instead" }
        }),
        required: None,
    }
}

// TaskOutput schema
fn task_output_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({
            "task_id": { "type": "string", "description": "The task ID to get output from" },
            "block": { "type": "boolean", "description": "Whether to wait for completion. Default: true" },
            "timeout": { "type": "number", "description": "Max wait time in ms. Default: 30000, max: 600000" }
        }),
        required: Some(vec!["task_id".to_string()]),
    }
}

// Cron tool schemas
fn cron_create_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({
            "cron": { "type": "string", "description": "5-field cron expression" },
            "prompt": { "type": "string", "description": "The prompt to execute" },
            "recurring": { "type": "boolean", "description": "true = repeat, false = one-shot" },
            "durable": { "type": "boolean", "description": "true = persist across restarts" }
        }),
        required: Some(vec!["cron".to_string(), "prompt".to_string()]),
    }
}

fn cron_delete_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({
            "id": { "type": "string", "description": "Job ID returned by CronCreate" }
        }),
        required: Some(vec!["id".to_string()]),
    }
}

fn cron_list_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({}),
        required: None,
    }
}

// Config schema
fn config_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({
            "action": { "type": "string", "enum": ["get", "set", "list"], "description": "Action to perform" },
            "key": { "type": "string", "description": "Configuration key" },
            "value": { "type": "string", "description": "Configuration value" }
        }),
        required: Some(vec!["action".to_string()]),
    }
}

// TodoWrite schema
fn todo_write_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({
            "todos": { "type": "array", "items": serde_json::json!({}), "description": "List of todo items" }
        }),
        required: Some(vec!["todos".to_string()]),
    }
}

// Skill schema
fn skill_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({
            "skill": { "type": "string", "description": "The name of the skill to invoke" }
        }),
        required: Some(vec!["skill".to_string()]),
    }
}

// LSP tool schema
fn lsp_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({
            "operation": {
                "type": "string",
                "enum": ["goToDefinition", "findReferences", "hover", "documentSymbol", "workspaceSymbol", "goToImplementation", "prepareCallHierarchy", "incomingCalls", "outgoingCalls"],
                "description": "The LSP operation to perform"
            },
            "filePath": { "type": "string", "description": "The file to operate on" },
            "line": { "type": "integer", "description": "Line number (1-based)" },
            "character": { "type": "integer", "description": "Character offset (1-based)" }
        }),
        required: Some(vec!["operation".to_string(), "filePath".to_string()]),
    }
}

// RemoteTrigger tool schema
fn remote_trigger_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({
            "action": { "type": "string", "enum": ["list", "get", "create", "update", "run"], "description": "The action to perform" },
            "trigger_id": { "type": "string", "description": "Required for get, update, and run" },
            "body": { "type": "object", "description": "JSON body for create and update" }
        }),
        required: Some(vec!["action".to_string()]),
    }
}

// ListMcpResourcesTool schema
fn list_mcp_resources_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({
            "server": { "type": "string", "description": "Optional server name to filter resources by" }
        }),
        required: None,
    }
}

// ReadMcpResourceTool schema
fn read_mcp_resource_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: serde_json::json!({
            "server": { "type": "string", "description": "The MCP server name" },
            "uri": { "type": "string", "description": "The resource URI to read" }
        }),
        required: Some(vec!["server".to_string(), "uri".to_string()]),
    }
}

pub fn get_all_base_tools() -> Vec<ToolDefinition> {
    ALL_TOOLS.iter().map(|f| f.2()).collect()
}

pub fn filter_tools(
    tools: Vec<ToolDefinition>,
    allowed: Option<Vec<String>>,
    disallowed: Option<Vec<String>>,
) -> Vec<ToolDefinition> {
    let mut result = tools;
    if let Some(allowed) = allowed {
        let allowed_set: std::collections::HashSet<_> = allowed.into_iter().collect();
        result.retain(|t| allowed_set.contains(&t.name));
    }
    if let Some(disallowed) = disallowed {
        let disallowed_set: std::collections::HashSet<_> = disallowed.into_iter().collect();
        result.retain(|t| !disallowed_set.contains(&t.name));
    }
    result
}

// --------------------------------------------------------------------------
// Tool Helper Functions (translated from Tool.ts)
// --------------------------------------------------------------------------

/// Tool with metadata for matching
#[derive(Debug, Clone)]
pub struct ToolWithMetadata {
    pub name: String,
    pub aliases: Option<Vec<String>>,
}

/// Checks if a tool matches the given name (primary name or alias)
pub fn tool_matches_name(tool: &ToolWithMetadata, name: &str) -> bool {
    tool.name == name
        || tool
            .aliases
            .as_ref()
            .map_or(false, |a| a.contains(&name.to_string()))
}

/// Finds a tool by name or alias from a list of tools
pub fn find_tool_by_name<'a>(
    tools: &'a [ToolDefinition],
    name: &str,
) -> Option<&'a ToolDefinition> {
    tools.iter().find(|t| t.name == name)
}

/// Tool definition with optional fields (like ToolDef in TypeScript)
pub struct PartialToolDefinition {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Option<ToolInputSchema>,
    pub aliases: Option<Vec<String>>,
    pub search_hint: Option<String>,
    pub max_result_size_chars: Option<usize>,
    pub should_defer: Option<bool>,
    pub always_load: Option<bool>,
    pub is_enabled: Option<Box<dyn Fn() -> bool + Send + Sync>>,
    pub is_concurrency_safe: Option<Box<dyn Fn(&serde_json::Value) -> bool + Send + Sync>>,
    pub is_read_only: Option<Box<dyn Fn(&serde_json::Value) -> bool + Send + Sync>>,
    pub is_destructive: Option<Box<dyn Fn(&serde_json::Value) -> bool + Send + Sync>>,
    pub interrupt_behavior: Option<Box<dyn Fn() -> InterruptBehavior + Send + Sync>>,
    pub is_search_or_read_command:
        Option<Box<dyn Fn(&serde_json::Value) -> SearchOrReadCommand + Send + Sync>>,
    pub is_open_world: Option<Box<dyn Fn(&serde_json::Value) -> bool + Send + Sync>>,
    pub requires_user_interaction: Option<Box<dyn Fn() -> bool + Send + Sync>>,
    pub is_mcp: Option<bool>,
    pub is_lsp: Option<bool>,
    pub user_facing_name: Option<Box<dyn Fn(Option<&serde_json::Value>) -> String + Send + Sync>>,
}

impl Default for PartialToolDefinition {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: None,
            input_schema: None,
            aliases: None,
            user_facing_name: None,
            search_hint: None,
            max_result_size_chars: None,
            should_defer: None,
            always_load: None,
            is_enabled: None,
            is_concurrency_safe: None,
            is_read_only: None,
            is_destructive: None,
            interrupt_behavior: None,
            is_search_or_read_command: None,
            is_open_world: None,
            requires_user_interaction: None,
            is_mcp: None,
            is_lsp: None,
        }
    }
}

/// Interrupt behavior when user submits new message while tool is running
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InterruptBehavior {
    /// Stop the tool and discard its result
    Cancel,
    /// Keep running; the new message waits
    Block,
}

impl Default for InterruptBehavior {
    fn default() -> Self {
        InterruptBehavior::Block
    }
}

/// Search or read command result
#[derive(Debug, Clone, Default)]
pub struct SearchOrReadCommand {
    pub is_search: bool,
    pub is_read: bool,
    pub is_list: Option<bool>,
}

/// Build a complete `ToolDefinition` from a partial definition
pub fn build_tool(def: PartialToolDefinition) -> ToolDefinition {
    ToolDefinition {
        name: def.name.clone(),
        description: def.description.unwrap_or_default(),
        input_schema: def.input_schema.unwrap_or_default(),
        annotations: Some(ToolAnnotations {
            read_only: Some(
                def.is_read_only
                    .map_or(false, |f| f(&serde_json::json!({}))),
            ),
            destructive: Some(
                def.is_destructive
                    .as_ref()
                    .map_or(false, |f| f(&serde_json::json!({}))),
            ),
            concurrency_safe: Some(
                def.is_concurrency_safe
                    .as_ref()
                    .map_or(false, |f| f(&serde_json::json!({}))),
            ),
            open_world: None,
            idempotent: None,
        }),
        should_defer: None,
        always_load: None,
        is_mcp: None,
        search_hint: def.search_hint,
        aliases: def.aliases,
        user_facing_name: def.user_facing_name.map(|f| f(None)),
        interrupt_behavior: None,
    }
}
