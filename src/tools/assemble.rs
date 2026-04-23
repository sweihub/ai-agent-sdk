// Source: /data/home/swei/claudecode/openclaudecode/src/tools.ts (assembleToolPool)
/// Assemble tool pool with deduplication and alphabetical sorting.
///
/// Matches TypeScript behavior: built-in tools are sorted, MCP tools are sorted,
/// then concatenated with built-ins first. Deduplication by name gives built-ins
/// priority (critical for prompt cache stability).
use crate::types::ToolDefinition;

/// Assemble tool pool from built-in and MCP tools.
///
/// 1. Sorts built-in tools by name (alphabetically)
/// 2. Sorts MCP tools by name (alphabetically)
/// 3. Concatenates: built-ins first, then MCP tools
/// 4. Deduplicates by name (first occurrence wins, so built-ins take priority)
///
/// This ensures deterministic ordering for prompt cache stability.
pub fn assemble_tool_pool(
    built_in: &[ToolDefinition],
    mcp_tools: &[ToolDefinition],
) -> Vec<ToolDefinition> {
    let mut builtin_sorted = built_in.to_vec();
    builtin_sorted.sort_by(|a, b| a.name.cmp(&b.name));

    let mut mcp_sorted = mcp_tools.to_vec();
    mcp_sorted.sort_by(|a, b| a.name.cmp(&b.name));

    // Concatenate built-ins then MCP tools
    let mut combined: Vec<ToolDefinition> = builtin_sorted;
    combined.extend(mcp_sorted);

    // Deduplicate by name (first occurrence wins — built-ins take priority)
    let mut seen = std::collections::HashSet::new();
    combined.retain(|t| seen.insert(t.name.clone()));

    combined
}

/// Filter MCP tools by deny rules (server-prefix stripping).
///
/// Matches TypeScript behavior: a deny rule for `mcp__serverName` blocks
/// all tools from that server. A deny rule for `mcp__serverName_toolName`
/// blocks a specific tool from a server.
pub fn filter_tools_by_deny_rules(
    tools: &[ToolDefinition],
    deny_rules: &[String],
) -> Vec<ToolDefinition> {
    tools
        .iter()
        .filter(|tool| {
            !deny_rules.iter().any(|rule| {
                // Exact match
                if &tool.name == rule {
                    return true;
                }
                // MCP server-prefix match: deny rule `mcp__serverName` blocks all tools from that server
                if rule.ends_with("__") && tool.name.starts_with(rule.as_str()) {
                    return true;
                }
                // MCP tool prefix match: deny rule `mcp__serverName_` blocks all tools from that server
                if rule.ends_with('_') && tool.name.starts_with(rule.as_str()) {
                    return true;
                }
                // Wildcard: deny rule `*` blocks all
                if rule == "*" {
                    return true;
                }
                false
            })
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tool(name: &str) -> ToolDefinition {
        ToolDefinition {
            name: name.to_string(),
            description: format!("Tool {}", name),
            input_schema: ToolDefinition::default().input_schema,
            ..Default::default()
        }
    }

    #[test]
    fn test_assemble_tool_pool_sorts_builtins() {
        let built_in = vec![make_tool("Zebra"), make_tool("Alpha"), make_tool("Beta")];
        let result = assemble_tool_pool(&built_in, &[]);
        assert_eq!(result.iter().map(|t| &t.name).collect::<Vec<_>>(), &["Alpha", "Beta", "Zebra"]);
    }

    #[test]
    fn test_assemble_tool_pool_sorts_mcp_tools() {
        let mcp = vec![make_tool("mcp__server_Z"), make_tool("mcp__server_A")];
        let result = assemble_tool_pool(&[], &mcp);
        assert_eq!(
            result.iter().map(|t| &t.name).collect::<Vec<_>>(),
            &["mcp__server_A", "mcp__server_Z"]
        );
    }

    #[test]
    fn test_assemble_tool_pool_builtins_first() {
        let built_in = vec![make_tool("B")];
        let mcp = vec![make_tool("A")];
        let result = assemble_tool_pool(&built_in, &mcp);
        assert_eq!(result.iter().map(|t| &t.name).collect::<Vec<_>>(), &["B", "A"]);
    }

    #[test]
    fn test_assemble_tool_pool_dedup_builtins_win() {
        let built_in = vec![make_tool("Read")];
        let mcp = vec![make_tool("Read")];
        let result = assemble_tool_pool(&built_in, &mcp);
        assert_eq!(result.len(), 1);
        assert_eq!(&result[0].name, "Read");
    }

    #[test]
    fn test_filter_tools_by_deny_rules_exact() {
        let tools = vec![make_tool("Bash"), make_tool("Read")];
        let rules = vec!["Bash".to_string()];
        let result = filter_tools_by_deny_rules(&tools, &rules);
        assert_eq!(result.iter().map(|t| &t.name).collect::<Vec<_>>(), &["Read"]);
    }

    #[test]
    fn test_filter_tools_by_deny_rules_mcp_server_prefix() {
        let tools = vec![
            make_tool("mcp__fs_read"),
            make_tool("mcp__fs_write"),
            make_tool("mcp__git_status"),
        ];
        let rules = vec!["mcp__fs_".to_string()];
        let result = filter_tools_by_deny_rules(&tools, &rules);
        assert_eq!(result.iter().map(|t| &t.name).collect::<Vec<_>>(), &["mcp__git_status"]);
    }

    #[test]
    fn test_filter_tools_by_deny_rules_wildcard() {
        let tools = vec![make_tool("Bash"), make_tool("Read")];
        let rules = vec!["*".to_string()];
        let result = filter_tools_by_deny_rules(&tools, &rules);
        assert!(result.is_empty());
    }

    #[test]
    fn test_assemble_full_flow() {
        let built_in = vec![make_tool("Zebra"), make_tool("Alpha")];
        let mcp = vec![
            make_tool("mcp__z"),
            make_tool("Zebra"), // duplicate — should be deduped
            make_tool("mcp__a"),
        ];
        let result = assemble_tool_pool(&built_in, &mcp);
        let names: Vec<_> = result.iter().map(|t| &t.name).collect();
        // Built-ins sorted first: Alpha, Zebra. MCP sorted after: mcp__a, mcp__z (Zebra deduped)
        assert_eq!(names, &["Alpha", "Zebra", "mcp__a", "mcp__z"]);
    }
}
