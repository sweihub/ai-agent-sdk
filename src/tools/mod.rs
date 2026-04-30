pub mod agent;
pub mod background_task_registry;
pub mod assemble;
pub mod ask;
pub mod brief;
pub mod bash;
pub mod config;
pub mod config_tools;
pub mod cron;
pub mod deferred_tools;
pub mod edit;
pub mod glob;
pub mod discover_skills;
pub mod grep;
pub mod lsp;
pub mod mcp;
pub mod mcp_resource_reader;
pub mod mcp_resources;
pub mod mcp_tool;
pub mod mcp_auth;
pub mod monitor;
pub mod notebook_edit;
pub mod orchestration;
pub mod placeholder;
pub mod plan;
pub mod powershell;
pub mod read;
pub mod remote_trigger;
pub mod repl;
pub mod search;
pub mod send_user_file;
pub mod skill;
pub mod sleep_tool;
pub mod snip;
pub mod task_output;
pub mod task_stop;
pub mod terminal_capture;
pub mod tasks;
pub mod team;
pub mod todo;
pub mod types;
pub mod synthetic_output;
pub mod web_browser;
pub mod web_fetch;
pub mod web_search;
pub mod worktree;
pub mod overflow_test;
pub mod review_artifact;
pub mod workflow;
pub mod write;

pub use types::{
    Tool, ToolDefinition, ToolFuture, ToolInputSchema, filter_tools, get_all_base_tools,
};
pub use assemble::{assemble_tool_pool, filter_tools_by_deny_rules};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_all_base_tools_returns_all_tools() {
        let tools = get_all_base_tools();
        // Should have 50 built-in tools (42 + OverflowTest + ReviewArtifact + Workflow + Snip + DiscoverSkills + TerminalCapture + MCPTool + McpAuth)
        assert_eq!(tools.len(), 50);
    }

    #[test]
    fn test_get_all_base_tools_contains_bash_tool() {
        let tools = get_all_base_tools();
        let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(tool_names.contains(&"Bash"));
    }

    #[test]
    fn test_get_all_base_tools_contains_file_read_tool() {
        let tools = get_all_base_tools();
        let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(tool_names.contains(&"Read"));
    }

    #[test]
    fn test_get_all_base_tools_contains_file_write_tool() {
        let tools = get_all_base_tools();
        let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(tool_names.contains(&"Write"));
    }

    #[test]
    fn test_get_all_base_tools_contains_glob_tool() {
        let tools = get_all_base_tools();
        let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(tool_names.contains(&"Glob"));
    }

    #[test]
    fn test_get_all_base_tools_contains_grep_tool() {
        let tools = get_all_base_tools();
        let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(tool_names.contains(&"Grep"));
    }

    #[test]
    fn test_get_all_base_tools_contains_file_edit_tool() {
        let tools = get_all_base_tools();
        let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(tool_names.contains(&"FileEdit"));
    }

    #[test]
    fn test_get_all_base_tools_contains_notebook_edit_tool() {
        let tools = get_all_base_tools();
        let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(tool_names.contains(&"NotebookEdit"));
    }

    #[test]
    fn test_get_all_base_tools_contains_web_fetch_tool() {
        let tools = get_all_base_tools();
        let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(tool_names.contains(&"WebFetch"));
    }

    #[test]
    fn test_get_all_base_tools_contains_web_search_tool() {
        let tools = get_all_base_tools();
        let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(tool_names.contains(&"WebSearch"));
    }

    #[test]
    fn test_get_all_base_tools_contains_agent_tool() {
        let tools = get_all_base_tools();
        let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(tool_names.contains(&"Agent"));
    }

    #[test]
    fn test_filter_tools_by_allowed() {
        let tools = vec![
            ToolDefinition {
                name: "Bash".to_string(),
                description: "Execute shell commands".to_string(),
                input_schema: ToolInputSchema {
                    schema_type: "object".to_string(),
                    properties: serde_json::json!({}),
                    required: None,
                },
                annotations: None,
                should_defer: None,
                always_load: None,
                is_mcp: None,
                search_hint: None,
                aliases: None,
                user_facing_name: None,
                interrupt_behavior: None,
            },
            ToolDefinition {
                name: "Read".to_string(),
                description: "Read files".to_string(),
                input_schema: ToolInputSchema {
                    schema_type: "object".to_string(),
                    properties: serde_json::json!({}),
                    required: None,
                },
                annotations: None,
                should_defer: None,
                always_load: None,
                is_mcp: None,
                search_hint: None,
                aliases: None,
                user_facing_name: None,
                interrupt_behavior: None,
            },
        ];
        let filtered = filter_tools(tools, Some(vec!["Bash".to_string()]), None);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "Bash");
    }

    #[test]
    fn test_filter_tools_by_disallowed() {
        let tools = vec![
            ToolDefinition {
                name: "Bash".to_string(),
                description: "Execute shell commands".to_string(),
                input_schema: ToolInputSchema {
                    schema_type: "object".to_string(),
                    properties: serde_json::json!({}),
                    required: None,
                },
                annotations: None,
                should_defer: None,
                always_load: None,
                is_mcp: None,
                search_hint: None,
                aliases: None,
                user_facing_name: None,
                interrupt_behavior: None,
            },
            ToolDefinition {
                name: "Read".to_string(),
                description: "Read files".to_string(),
                input_schema: ToolInputSchema {
                    schema_type: "object".to_string(),
                    properties: serde_json::json!({}),
                    required: None,
                },
                annotations: None,
                should_defer: None,
                always_load: None,
                is_mcp: None,
                search_hint: None,
                aliases: None,
                user_facing_name: None,
                interrupt_behavior: None,
            },
        ];
        let filtered = filter_tools(tools, None, Some(vec!["Bash".to_string()]));
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "Read");
    }
}
