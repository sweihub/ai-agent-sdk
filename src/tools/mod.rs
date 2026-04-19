pub mod agent;
pub mod ask;
pub mod bash;
pub mod config;
pub mod config_tools;
pub mod cron;
pub mod deferred_tools;
pub mod edit;
pub mod glob;
pub mod grep;
pub mod lsp;
pub mod mcp_resources;
pub mod mcp_resource_reader;
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
pub mod task_stop;
pub mod tasks;
pub mod team;
pub mod todo;
pub mod types;
pub mod web_browser;
pub mod web_fetch;
pub mod web_search;
pub mod worktree;
pub mod write;

pub use types::{
    filter_tools, get_all_base_tools, Tool, ToolDefinition, ToolFuture, ToolInputSchema,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_all_base_tools_returns_all_tools() {
        let tools = get_all_base_tools();
        // Should have 37 built-in tools (33 original + LSP, RemoteTrigger, ListMcpResourcesTool, ReadMcpResourceTool)
        assert_eq!(tools.len(), 37);
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
        assert!(tool_names.contains(&"FileRead"));
    }

    #[test]
    fn test_get_all_base_tools_contains_file_write_tool() {
        let tools = get_all_base_tools();
        let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(tool_names.contains(&"FileWrite"));
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
            },
            ToolDefinition {
                name: "FileRead".to_string(),
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
            },
            ToolDefinition {
                name: "FileRead".to_string(),
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
            },
        ];
        let filtered = filter_tools(tools, None, Some(vec!["Bash".to_string()]));
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "FileRead");
    }
}
