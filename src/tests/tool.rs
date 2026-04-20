use crate::tool::*;
use std::collections::HashMap;

#[test]
fn test_tool_matches_name_primary() {
    assert!(tool_matches_name("Bash", None, "Bash"));
}

#[test]
fn test_tool_matches_name_alias() {
    let aliases = vec!["sh".to_string(), "shell".to_string()];
    assert!(tool_matches_name("Bash", Some(&aliases), "sh"));
    assert!(tool_matches_name("Bash", Some(&aliases), "shell"));
    assert!(!tool_matches_name("Bash", Some(&aliases), "BashExtra"));
}

#[test]
fn test_tool_matches_name_no_alias_match() {
    assert!(!tool_matches_name("Bash", None, "Shell"));
}

#[test]
fn test_find_tool_by_name() {
    let tools = vec![
        ToolDefinition {
            name: "Bash".to_string(),
            description: "Run shell commands".to_string(),
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
        },
        ToolDefinition {
            name: "Read".to_string(),
            description: "Read a file".to_string(),
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
        },
    ];

    let found = find_tool_by_name(&tools, "Read");
    assert!(found.is_some());
    assert_eq!(found.unwrap().name, "Read");

    let not_found = find_tool_by_name(&tools, "Write");
    assert!(not_found.is_none());
}

#[test]
fn test_filter_tool_progress_messages() {
    use crate::types::message::ProgressMessage;
    use std::collections::HashMap;

    let hook_progress = ProgressMessage {
        base: crate::types::message::MessageBase {
            uuid: Some("1".to_string()),
            parent_uuid: None,
            timestamp: None,
            created_at: None,
            is_meta: None,
            is_virtual: None,
            is_compact_summary: None,
            tool_use_result: None,
            origin: None,
            extra: HashMap::new(),
        },
        message_type: "progress".to_string(),
        progress: Some(serde_json::json!({ "kind": "hook_progress" })),
    };

    let tool_progress = ProgressMessage {
        base: crate::types::message::MessageBase {
            uuid: Some("2".to_string()),
            parent_uuid: None,
            timestamp: None,
            created_at: None,
            is_meta: None,
            is_virtual: None,
            is_compact_summary: None,
            tool_use_result: None,
            origin: None,
            extra: HashMap::new(),
        },
        message_type: "progress".to_string(),
        progress: Some(serde_json::json!({ "kind": "bash_progress" })),
    };

    let messages = vec![hook_progress.clone(), tool_progress.clone()];
    let filtered = filter_tool_progress_messages(&messages);

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].base.uuid, Some("2".to_string()));
}

#[test]
fn test_get_empty_tool_permission_context() {
    let ctx = get_empty_tool_permission_context();
    assert_eq!(ctx.mode, "default");
    assert!(ctx.additional_working_directories.is_empty());
    assert!(!ctx.is_bypass_permissions_mode_available);
}

#[test]
fn test_tool_builder_defaults() {
    let tool = ToolBuilder::new("test_tool")
        .input_schema(ToolInputSchema {
            schema_type: "object".to_string(),
            properties: serde_json::json!({
                "arg": { "type": "string" }
            }),
            required: Some(vec!["arg".to_string()]),
        })
        .description_fn(|_input, _nis, _tpc, _tools| {
            Box::pin(async move { "A test tool".to_string() })
        })
        .prompt_fn(|_get_ctx, _tools, _agents, _allowed| {
            Box::pin(async move { "Use the test_tool.".to_string() })
        })
        .call_fn(|_args, _ctx, _can_use, _parent, _on_progress| {
            Box::pin(async move {
                Ok(ToolResult {
                    data: serde_json::json!({ "ok": true }),
                    new_messages: None,
                    context_modifier: None,
                    mcp_meta: None,
                })
            })
        })
        .map_tool_result_fn(|content, tool_use_id| {
            ToolResultBlockParam {
                block_type: "tool_result".to_string(),
                tool_use_id: tool_use_id.to_string(),
                content: vec![ContentBlockParam::Text {
                    text: content.to_string(),
                }],
                is_error: None,
            }
        })
        .render_tool_use_message_fn(|_input, _options| {
            "Test tool use".to_string()
        })
        .build();

    assert_eq!(tool.name(), "test_tool");
    assert!(tool.is_enabled());
    assert!(!tool.is_concurrency_safe(serde_json::Value::Null));
    assert!(!tool.is_read_only(serde_json::Value::Null));
    assert!(!tool.is_destructive(serde_json::Value::Null));
    assert!(!tool.should_defer());
    assert!(!tool.always_load());
    assert!(!tool.is_mcp());
    assert!(!tool.is_lsp());
    assert_eq!(tool.interrupt_behavior(), "block");
    assert_eq!(tool.max_result_size_chars(), usize::MAX);
    assert!(!tool.strict());
    assert!(!tool.is_transparent_wrapper());
    assert!(!tool.requires_user_interaction());
}

#[test]
fn test_permission_result_allow() {
    let mut map = HashMap::new();
    map.insert("key".to_string(), serde_json::json!("value"));
    let result = PermissionResult::allow(map.clone());

    match result {
        PermissionResult::Allow { updated_input, .. } => {
            assert_eq!(updated_input, Some(map));
        }
        _ => panic!("Expected Allow variant"),
    }
}

#[test]
fn test_validation_result_valid() {
    assert!(matches!(ValidationResult::Valid, ValidationResult::Valid));
}

#[test]
fn test_validation_result_invalid() {
    let result = ValidationResult::Invalid {
        message: "Bad input".to_string(),
        error_code: 400,
    };
    match result {
        ValidationResult::Invalid { message, error_code } => {
            assert_eq!(message, "Bad input");
            assert_eq!(error_code, 400);
        }
        _ => panic!("Expected Invalid variant"),
    }
}
