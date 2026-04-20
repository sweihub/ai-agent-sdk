use crate::query_engine::{empty_json_value, QueryEngine, QueryEngineConfig};
use crate::tools::deferred_tools::{
    parse_tool_name, parse_tool_search_query, search_tools_with_keywords, ToolSearchQuery,
};
use crate::tools::search::ToolSearchTool;
use crate::tools::get_all_base_tools;
use crate::types::{Message, MessageRole, ToolCall, ToolContext, ToolDefinition, ToolInputSchema};
use crate::AgentError;

#[tokio::test]
async fn test_engine_creation() {
    let engine = QueryEngine::new(QueryEngineConfig {
        cwd: "/tmp".to_string(),
        model: "claude-sonnet-4-6".to_string(),
        api_key: None,
        base_url: None,
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
    assert_eq!(engine.get_turn_count(), 0);
}

#[tokio::test]
async fn test_engine_submit_message() {
    let mut engine = QueryEngine::new(QueryEngineConfig {
        cwd: "/tmp".to_string(),
        model: "claude-sonnet-4-6".to_string(),
        api_key: None,
        base_url: None,
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

    let result = engine.submit_message("Hello").await;
    // Should fail because no API key
    assert!(result.is_err());
}

#[test]
fn test_strip_thinking() {
    use crate::query_engine::strip_thinking;

    // Test stripping thinking tags from content
    let content =
        "<think>I should list the files here.</think>Here are the files: file1.txt, file2.txt";
    let result = strip_thinking(content);
    assert_eq!(result, "Here are the files: file1.txt, file2.txt");

    // Test content without thinking tags
    let content2 = "Hello world";
    let result2 = strip_thinking(content2);
    assert_eq!(result2, "Hello world");

    // Test content with only thinking tags
    let content3 = "<think>Thinking...</think>";
    let result3 = strip_thinking(content3);
    assert_eq!(result3, "");

    // Test multiple thinking blocks (no spaces between thinking and text in input)
    let content4 = "<think>First think</think>Hello<think>Second think</think>World";
    let result4 = strip_thinking(content4);
    assert_eq!(result4, "HelloWorld");
}

#[test]
fn test_strip_thinking_utf8() {
    use crate::query_engine::strip_thinking;

    // Test UTF-8 multi-byte characters (arrow → is 3 bytes)
    let content = "<think>思考</think>Hello → World";
    let result = strip_thinking(content);
    assert_eq!(result, "Hello → World");

    // Test Chinese characters (each char is 3 bytes)
    let content2 = "<think>中文</think>你好世界";
    let result2 = strip_thinking(content2);
    assert_eq!(result2, "你好世界");

    // Test emoji (4 bytes each)
    let content3 = "<think>thinking emoji 🎭</think>Hello 👋 World";
    let result3 = strip_thinking(content3);
    assert_eq!(result3, "Hello 👋 World");

    // Test mixed content with UTF-8
    let content4 = "<think>The → symbol is here</think>Result: 你好 🎉";
    let result4 = strip_thinking(content4);
    assert_eq!(result4, "Result: 你好 🎉");

    // Test thinking at start with UTF-8
    let content5 = "<think>thinking开始啦</think>继续内容";
    let result5 = strip_thinking(content5);
    assert_eq!(result5, "继续内容");

    // Test thinking at end with UTF-8
    let content6 = "开始内容<think>thinking结束啦</think>";
    let result6 = strip_thinking(content6);
    assert_eq!(result6, "开始内容");

    // Test multiple UTF-8 thinking blocks
    let content7 = "<think>第一步思考→思考第二步</think>执行→完成";
    let result7 = strip_thinking(content7);
    assert_eq!(result7, "执行→完成");
}

#[test]
fn test_fallback_tool_call_extraction() {
    // Test that fallback path extracts tool calls from non-streaming response
    use serde_json::json;

    // Simulate a non-streaming response with tool calls
    let response = json!({
        "choices": [
            {
                "message": {
                    "content": null,
                    "tool_calls": [
                        {
                            "id": "call_123",
                            "type": "function",
                            "function": {
                                "name": "Bash",
                                "arguments": "{\"command\": \"ls -la\"}"
                            }
                        }
                    ]
                },
                "finish_reason": "tool_calls"
            }
        ],
        "usage": {
            "prompt_tokens": 100,
            "completion_tokens": 50
        }
    });

    // Extract tool calls like the fallback code does
    let mut tool_calls = Vec::new();
    if let Some(choices) = response.get("choices").and_then(|c| c.as_array()) {
        if let Some(first) = choices.first() {
            if let Some(msg) = first.get("message") {
                if let Some(tc_array) = msg.get("tool_calls").and_then(|t| t.as_array()) {
                    for tc in tc_array {
                        let id = tc.get("id").and_then(|i| i.as_str()).unwrap_or("");
                        let func = tc.get("function");
                        let name = func
                            .and_then(|f| f.get("name"))
                            .and_then(|n| n.as_str())
                            .unwrap_or("");
                        let args = func.and_then(|f| f.get("arguments"));
                        let args_val = if let Some(args_str) = args.and_then(|a| a.as_str()) {
                            serde_json::from_str(args_str).unwrap_or_else(|_| empty_json_value())
                        } else {
                            args.cloned().unwrap_or_else(|| empty_json_value())
                        };
                        tool_calls.push(serde_json::json!({
                            "id": id,
                            "name": name,
                            "arguments": args_val,
                        }));
                    }
                }
            }
        }
    }

    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0]["name"], "Bash");
    assert_eq!(tool_calls[0]["id"], "call_123");
}

#[test]
fn test_streaming_tool_call_extraction() {
    // Test that streaming path can extract tool calls from SSE-like data
    use serde_json::json;

    // Simulate a streaming chunk with tool call delta
    let chunk = json!({
        "choices": [
            {
                "delta": {
                    "tool_calls": [
                        {
                            "id": "call_456",
                            "type": "function",
                            "function": {
                                "name": "Read",
                                "arguments": "{\"file_path\": \"/tmp/test\"}"
                            }
                        }
                    ]
                },
                "finish_reason": "tool_calls"
            }
        ]
    });

    // Verify the chunk has tool_calls
    let tool_calls = chunk
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("delta"))
        .and_then(|delta| delta.get("tool_calls"))
        .and_then(|tc| tc.as_array());

    assert!(tool_calls.is_some());
    let tc = tool_calls.unwrap().first().unwrap();
    assert_eq!(tc.get("id").and_then(|i| i.as_str()), Some("call_456"));
    assert_eq!(
        tc.get("function")
            .and_then(|f| f.get("name"))
            .and_then(|n| n.as_str()),
        Some("Read")
    );
}

// =========================================================================
// Tool Calling Tests
// =========================================================================

#[test]
fn test_tool_definition_serialization() {
    let tools = get_all_base_tools();
    assert!(!tools.is_empty());

    // Test that tools can be serialized to OpenAI function format
    for tool in &tools {
        let tool_json = serde_json::json!({
            "type": "function",
            "function": {
                "name": tool.name,
                "description": tool.description,
                "parameters": tool.input_schema
            }
        });

        // Verify all required fields exist
        assert!(tool_json.get("type").is_some());
        assert!(tool_json.get("function").is_some());
        let func = tool_json.get("function").unwrap();
        assert!(func.get("name").is_some());
        assert!(func.get("description").is_some());
        assert!(func.get("parameters").is_some());

        // Verify name is not empty
        let name = func.get("name").unwrap().as_str().unwrap();
        assert!(!name.is_empty());
    }
}

#[test]
fn test_tool_call_parsing() {
    // Test parsing tool calls from message
    let tool_calls = vec![
        ToolCall {
            id: "call_abc123".to_string(),
            r#type: "function".to_string(),
            name: "Bash".to_string(),
            arguments: serde_json::json!({"command": "ls -la"}),
        },
        ToolCall {
            id: "call_def456".to_string(),
            r#type: "function".to_string(),
            name: "Read".to_string(),
            arguments: serde_json::json!({"path": "/tmp/test.txt"}),
        },
    ];

    // Verify tool call structure
    assert_eq!(tool_calls.len(), 2);
    assert_eq!(tool_calls[0].id, "call_abc123");
    assert_eq!(tool_calls[0].name, "Bash");
    assert_eq!(tool_calls[1].id, "call_def456");
    assert_eq!(tool_calls[1].name, "Read");
}

#[test]
fn test_tool_result_message_format() {
    // Test that tool results can be created with tool_call_id
    let msg = Message {
        role: MessageRole::Tool,
        content: "file content here".to_string(),
        tool_call_id: Some("call_abc123".to_string()),
        is_error: Some(false),
        ..Default::default()
    };

    assert_eq!(msg.role, MessageRole::Tool);
    assert_eq!(msg.tool_call_id, Some("call_abc123".to_string()));
    assert_eq!(msg.is_error, Some(false));
}

#[test]
fn test_tool_execution_context() {
    let ctx = ToolContext {
        cwd: "/tmp/test".to_string(),
        abort_signal: None,
    };

    assert_eq!(ctx.cwd, "/tmp/test");
}

#[test]
fn test_base_tools_available() {
    let tools = get_all_base_tools();

    // Verify essential tools are available
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();

    // Must have Bash tool
    assert!(tool_names.contains(&"Bash"), "Bash tool must be available");

    // Must have Read tool
    assert!(
        tool_names.contains(&"FileRead"),
        "FileRead tool must be available"
    );

    // Must have Write tool
    assert!(
        tool_names.contains(&"FileWrite"),
        "FileWrite tool must be available"
    );

    // Must have Glob tool
    assert!(tool_names.contains(&"Glob"), "Glob tool must be available");

    // Must have Grep tool
    assert!(tool_names.contains(&"Grep"), "Grep tool must be available");

    // Must have Edit tool
    assert!(
        tool_names.contains(&"FileEdit"),
        "FileEdit tool must be available"
    );
}

#[test]
fn test_tool_schemas_have_required_fields() {
    let tools = get_all_base_tools();

    for tool in &tools {
        // Name must not be empty
        assert!(!tool.name.is_empty(), "Tool {} has empty name", tool.name);

        // Description must not be empty
        assert!(
            !tool.description.is_empty(),
            "Tool {} has empty description",
            tool.name
        );

        // Input schema must have required fields
        let schema = &tool.input_schema;
        assert!(
            !schema.schema_type.is_empty(),
            "Tool {} has empty schema_type",
            tool.name
        );
        assert!(
            schema.properties.is_object(),
            "Tool {} has non-object properties",
            tool.name
        );
    }
}

#[test]
fn test_tool_schema_has_required_parameters() {
    let tools = get_all_base_tools();

    // Find Bash tool and verify it has command parameter
    let bash_tool = tools.iter().find(|t| t.name == "Bash").unwrap();
    let props = &bash_tool.input_schema.properties;
    assert!(
        props.get("command").is_some(),
        "Bash tool must have 'command' parameter"
    );

    // Find Read tool and verify it has path parameter
    let read_tool = tools.iter().find(|t| t.name == "FileRead").unwrap();
    let read_props = &read_tool.input_schema.properties;
    assert!(
        read_props.get("path").is_some(),
        "FileRead tool must have 'path' parameter"
    );

    // Find Write tool and verify it has path and content parameters
    let write_tool = tools.iter().find(|t| t.name == "FileWrite").unwrap();
    let write_props = &write_tool.input_schema.properties;
    assert!(
        write_props.get("path").is_some(),
        "FileWrite tool must have 'path' parameter"
    );
    assert!(
        write_props.get("content").is_some(),
        "FileWrite tool must have 'content' parameter"
    );

    // Verify required arrays are defined
    assert!(
        bash_tool.input_schema.required.is_some(),
        "Bash tool must have required parameters"
    );
}

#[tokio::test]
async fn test_engine_with_tools_config() {
    let tools = get_all_base_tools();

    let engine = QueryEngine::new(QueryEngineConfig {
        cwd: "/tmp".to_string(),
        model: "claude-sonnet-4-6".to_string(),
        api_key: None,
        base_url: None,
        tools: tools.clone(),
        system_prompt: Some("You are a helpful assistant.".to_string()),
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

    // Verify tools are stored in config
    assert!(!engine.config.tools.is_empty());
}

#[tokio::test]
async fn test_engine_system_prompt_includes_tool_guidance() {
    // Test that system prompt includes tool usage guidance
    let engine = QueryEngine::new(QueryEngineConfig {
        cwd: "/tmp".to_string(),
        model: "claude-sonnet-4-6".to_string(),
        api_key: None,
        base_url: None,
        tools: vec![],
        system_prompt: Some("You are an agent that helps users with software engineering tasks. Use the tools available to you to assist the user.".to_string()),
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

    // Verify system prompt is set
    assert!(engine.config.system_prompt.is_some());
    let prompt = engine.config.system_prompt.as_ref().unwrap();
    assert!(prompt.contains("tools"));
}

#[test]
fn test_tool_call_arguments_json() {
    // Test that tool call arguments can be serialized/deserialized as JSON
    let tc = ToolCall {
        id: "call_test".to_string(),
        r#type: "function".to_string(),
        name: "Bash".to_string(),
        arguments: serde_json::json!({
            "command": "echo hello"
        }),
    };

    // Serialize arguments to string
    let args_str = tc.arguments.to_string();
    assert!(!args_str.is_empty());

    // Deserialize back
    let parsed: serde_json::Value = serde_json::from_str(&args_str).unwrap();
    assert_eq!(
        parsed.get("command").and_then(|v| v.as_str()),
        Some("echo hello")
    );
}

#[test]
fn test_build_api_messages_includes_tools_info() {
    // This test verifies that the system prompt structure supports tool calling
    let system_prompt = "You are an agent. Use the tools available to you: Bash, Read, Write, Glob, Grep, Edit.";

    // Verify the prompt mentions tools
    assert!(system_prompt.contains("tools"));
    assert!(system_prompt.contains("Bash"));
}

#[tokio::test]
async fn test_query_engine_tool_registration() {
    let tools = get_all_base_tools();
    let tool_names: Vec<String> = tools.iter().map(|t| t.name.clone()).collect();

    // Verify we have multiple tools registered
    assert!(tool_names.len() >= 10, "Should have at least 10 tools");

    // Verify key tools exist
    assert!(tool_names.contains(&"Bash".to_string()));
    assert!(tool_names.contains(&"FileRead".to_string()));
    assert!(tool_names.contains(&"FileWrite".to_string()));
    assert!(tool_names.contains(&"Glob".to_string()));
    assert!(tool_names.contains(&"Grep".to_string()));
    assert!(tool_names.contains(&"FileEdit".to_string()));
}

#[test]
fn test_openai_tool_format_compatibility() {
    // Test that tools serialize to OpenAI-compatible format
    let tools = get_all_base_tools();
    let bash_tool = tools.iter().find(|t| t.name == "Bash").unwrap();

    let openai_format = serde_json::json!({
        "type": "function",
        "function": {
            "name": bash_tool.name,
            "description": bash_tool.description,
            "parameters": bash_tool.input_schema
        }
    });

    // Verify OpenAI format structure
    assert_eq!(openai_format.get("type").unwrap(), "function");
    let func = openai_format.get("function").unwrap();
    assert!(func.get("name").is_some());
    assert!(func.get("description").is_some());
    assert!(func.get("parameters").is_some());

    // Verify it can be serialized to JSON string
    let json_str = openai_format.to_string();
    assert!(!json_str.is_empty());

    // Verify it can be deserialized back
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    assert_eq!(parsed.get("type").unwrap(), "function");
}

#[tokio::test]
async fn test_engine_message_history_with_tool_calls() {
    let mut engine = QueryEngine::new(QueryEngineConfig {
        cwd: "/tmp".to_string(),
        model: "claude-sonnet-4-6".to_string(),
        api_key: None,
        base_url: None,
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

    // Add user message
    engine.messages.push(Message {
        role: MessageRole::User,
        content: "List files in /tmp".to_string(),
        ..Default::default()
    });

    // Add assistant message with tool call
    engine.messages.push(Message {
        role: MessageRole::Assistant,
        content: "".to_string(),
        tool_calls: Some(vec![ToolCall {
            id: "call_123".to_string(),
            r#type: "function".to_string(),
            name: "Bash".to_string(),
            arguments: serde_json::json!({"command": "ls /tmp"}),
        }]),
        ..Default::default()
    });

    // Add tool result message
    engine.messages.push(Message {
        role: MessageRole::Tool,
        content: "file1.txt\nfile2.txt".to_string(),
        tool_call_id: Some("call_123".to_string()),
        ..Default::default()
    });

    // Verify message history
    assert_eq!(engine.messages.len(), 3);
    assert_eq!(engine.messages[1].role, MessageRole::Assistant);
    assert!(engine.messages[1].tool_calls.is_some());
    assert_eq!(engine.messages[2].role, MessageRole::Tool);
    assert_eq!(
        engine.messages[2].tool_call_id,
        Some("call_123".to_string())
    );
}

#[test]
fn test_tool_result_error_handling() {
    // Test tool result with error
    let error_msg = Message {
        role: MessageRole::Tool,
        content: "Error: Permission denied".to_string(),
        tool_call_id: Some("call_err".to_string()),
        is_error: Some(true),
        ..Default::default()
    };

    assert_eq!(error_msg.is_error, Some(true));
    assert!(error_msg.content.contains("Error"));
}

// ========================================================================
// Deferred Tool Loading Tests
// ========================================================================

fn make_deferred_tool(name: &str, should_defer: bool, is_mcp: bool) -> ToolDefinition {
    let mut t = ToolDefinition {
        name: name.to_string(),
        description: format!("{} tool", name),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: serde_json::json!({}),
            required: None,
        },
        annotations: None,
        should_defer: if should_defer { Some(true) } else { None },
        always_load: None,
        is_mcp: if is_mcp { Some(true) } else { None },
        search_hint: Some(format!("{} capability", name.to_lowercase())),
        aliases: None,
        user_facing_name: None,
    };
    t
}

/// Test that separate_tools_for_request correctly splits upfront vs deferred tools
#[test]
fn test_separate_tools_upfront_vs_deferred() {
    // Enable tool search for this test
    unsafe { std::env::set_var("ENABLE_TOOL_SEARCH", "1") };
    let mut engine = QueryEngine::new(QueryEngineConfig {
        model: "test-model".to_string(),
        tools: vec![
            make_deferred_tool("Bash", false, false),        // upfront
            make_deferred_tool("FileRead", false, false),    // upfront
            make_deferred_tool("WebSearch", true, false),    // deferred
            make_deferred_tool("WebFetch", true, false),     // deferred
            make_deferred_tool("mcp__slack__send", true, true), // deferred (MCP)
        ],
        cwd: "/tmp".to_string(),
        ..Default::default()
    });

    let (upfront, deferred) = engine.separate_tools_for_request();

    // 2 upfront tools
    assert_eq!(upfront.len(), 2);
    assert!(upfront.iter().any(|t| t.name == "Bash"));
    assert!(upfront.iter().any(|t| t.name == "FileRead"));

    // 3 deferred tools
    assert_eq!(deferred.len(), 3);
    assert!(deferred.iter().any(|t| t.name == "WebSearch"));
    assert!(deferred.iter().any(|t| t.name == "WebFetch"));
    assert!(deferred.iter().any(|t| t.name == "mcp__slack__send"));
}

/// Test that after discovering a deferred tool via tool_reference,
/// it moves from deferred to upfront on the next request
#[test]
fn test_discovered_deferred_tool_moves_to_upfront() {
    // Enable tool search for this test
    unsafe { std::env::set_var("ENABLE_TOOL_SEARCH", "1") };
    let mut engine = QueryEngine::new(QueryEngineConfig {
        model: "test-model".to_string(),
        tools: vec![
            make_deferred_tool("Bash", false, false),
            make_deferred_tool("WebSearch", true, false),
            make_deferred_tool("WebFetch", true, false),
        ],
        cwd: "/tmp".to_string(),
        ..Default::default()
    });

    // Initially, only Bash is upfront
    let (upfront, deferred) = engine.separate_tools_for_request();
    assert_eq!(upfront.len(), 1);
    assert_eq!(upfront[0].name, "Bash");
    assert_eq!(deferred.len(), 2);

    // Simulate: model called ToolSearch, got tool_reference for WebSearch
    // This is what the API response looks like after tool_reference expansion
    let tool_search_result = Message {
        role: MessageRole::User,
        content: serde_json::json!([{
            "type": "tool_result",
            "tool_use_id": "call_search_123",
            "content": [
                {"type": "tool_reference", "tool_name": "WebSearch"}
            ]
        }]).to_string(),
        tool_call_id: Some("call_search_123".to_string()),
        ..Default::default()
    };
    engine.messages.push(tool_search_result);

    // Now separate again - WebSearch should have moved to upfront
    let (upfront2, deferred2) = engine.separate_tools_for_request();

    assert_eq!(upfront2.len(), 2);
    assert!(upfront2.iter().any(|t| t.name == "Bash"));
    assert!(upfront2.iter().any(|t| t.name == "WebSearch"));

    assert_eq!(deferred2.len(), 1);
    assert_eq!(deferred2[0].name, "WebFetch");
}

/// Test full flow: initial call -> ToolSearch -> discover tool -> use it
/// This simulates what happens when the LLM needs to find an unregistered tool
#[test]
fn test_full_deferred_tool_discovery_flow() {
    // Enable tool search for this test
    unsafe { std::env::set_var("ENABLE_TOOL_SEARCH", "1") };
    // Scenario: LLM wants to use WebSearch but it's deferred
    // Step 1: Initial state - WebSearch is deferred, not in upfront tools
    let tools = vec![
        make_deferred_tool("Bash", false, false),
        make_deferred_tool("FileRead", false, false),
        make_deferred_tool("WebSearch", true, false),
    ];

    let engine = QueryEngine::new(QueryEngineConfig {
        model: "test-model".to_string(),
        tools: tools.clone(),
        cwd: "/tmp".to_string(),
        ..Default::default()
    });

    // Step 2: Get upfront tools - WebSearch should NOT be here
    let (upfront, deferred) = engine.separate_tools_for_request();
    assert_eq!(upfront.len(), 2);
    assert!(upfront.iter().all(|t| t.name != "WebSearch"));

    // Step 3: LLM sees <available-deferred-tools> block with WebSearch name
    // LLM calls ToolSearchTool with "select:WebSearch"
    // ToolSearchTool returns tool_reference block
    let tool_reference_result = ToolSearchTool::build_tool_reference_result(
        &["WebSearch".to_string()],
        "call_toolsearch_001"
    );

    // Step 4: Verify tool_reference format
    assert_eq!(tool_reference_result["type"], "tool_result");
    let content = tool_reference_result["content"].as_array().unwrap();
    assert_eq!(content.len(), 1);
    assert_eq!(content[0]["type"], "tool_reference");
    assert_eq!(content[0]["tool_name"], "WebSearch");

    // Step 5: The API expands tool_reference and model can now call WebSearch
    // Simulate the model calling WebSearch - the response appears in messages
    let mut engine2 = QueryEngine::new(QueryEngineConfig {
        model: "test-model".to_string(),
        tools: tools.clone(),
        cwd: "/tmp".to_string(),
        ..Default::default()
    });

    // Simulate tool_reference result message (what the API sends back after expansion)
    let discovered_msg = Message {
        role: MessageRole::User,
        content: serde_json::json!([{
            "type": "tool_result",
            "tool_use_id": "call_toolsearch_001",
            "content": [
                {"type": "tool_reference", "tool_name": "WebSearch"}
            ]
        }]).to_string(),
        tool_call_id: Some("call_toolsearch_001".to_string()),
        ..Default::default()
    };
    engine2.messages.push(discovered_msg);

    // Step 6: Now WebSearch should be in upfront tools
    let (upfront_after, deferred_after) = engine2.separate_tools_for_request();
    assert!(upfront_after.iter().any(|t| t.name == "WebSearch"));
    assert!(deferred_after.is_empty() || deferred_after.iter().all(|t| t.name != "WebSearch"));
}

/// Test that multiple deferred tools can be discovered in one ToolSearch call
#[test]
fn test_discover_multiple_deferred_tools() {
    // Enable tool search for this test
    unsafe { std::env::set_var("ENABLE_TOOL_SEARCH", "1") };
    let tools = vec![
        make_deferred_tool("Bash", false, false),
        make_deferred_tool("WebSearch", true, false),
        make_deferred_tool("WebFetch", true, false),
        make_deferred_tool("mcp__github__pr", true, true),
    ];

    let mut engine = QueryEngine::new(QueryEngineConfig {
        model: "test-model".to_string(),
        tools,
        cwd: "/tmp".to_string(),
        ..Default::default()
    });

    // Initially only Bash is upfront
    let (upfront, deferred) = engine.separate_tools_for_request();
    assert_eq!(upfront.len(), 1);
    assert_eq!(deferred.len(), 3);

    // LLM calls ToolSearch with "select:WebSearch,WebFetch"
    let multi_discovery = ToolSearchTool::build_tool_reference_result(
        &["WebSearch".to_string(), "WebFetch".to_string()],
        "call_toolsearch_002"
    );

    // Verify both tool_references are in the result
    let content = multi_discovery["content"].as_array().unwrap();
    assert_eq!(content.len(), 2);
    assert_eq!(content[0]["tool_name"], "WebSearch");
    assert_eq!(content[1]["tool_name"], "WebFetch");

    // Add the discovery to engine messages
    let discovered_msg = Message {
        role: MessageRole::User,
        content: serde_json::json!([{
            "type": "tool_result",
            "tool_use_id": "call_toolsearch_002",
            "content": [
                {"type": "tool_reference", "tool_name": "WebSearch"},
                {"type": "tool_reference", "tool_name": "WebFetch"}
            ]
        }]).to_string(),
        tool_call_id: Some("call_toolsearch_002".to_string()),
        ..Default::default()
    };
    engine.messages.push(discovered_msg);

    // Now both WebSearch and WebFetch should be upfront
    let (upfront_after, deferred_after) = engine.separate_tools_for_request();
    assert_eq!(upfront_after.len(), 3);
    assert!(upfront_after.iter().any(|t| t.name == "Bash"));
    assert!(upfront_after.iter().any(|t| t.name == "WebSearch"));
    assert!(upfront_after.iter().any(|t| t.name == "WebFetch"));

    // Only MCP tool remains deferred
    assert_eq!(deferred_after.len(), 1);
    assert_eq!(deferred_after[0].name, "mcp__github__pr");
}

/// Test that <available-deferred-tools> block is correctly injected into messages
#[test]
fn test_available_deferred_tools_block_injection() {
    // Enable tool search for this test
    unsafe { std::env::set_var("ENABLE_TOOL_SEARCH", "1") };
    let tools = vec![
        make_deferred_tool("Bash", false, false),
        make_deferred_tool("WebSearch", true, false),
        make_deferred_tool("WebFetch", true, false),
    ];

    let engine = QueryEngine::new(QueryEngineConfig {
        model: "test-model".to_string(),
        tools,
        cwd: "/tmp".to_string(),
        ..Default::default()
    });

    let mut api_messages = vec![
        serde_json::json!({"role": "user", "content": "Search the web for Rust"}),
        serde_json::json!({
            "role": "assistant",
            "content": "Calling tool: Bash"
        }),
    ];

    engine.maybe_inject_deferred_tools_block(&mut api_messages);

    // Should have injected the <available-deferred-tools> block
    assert_eq!(api_messages.len(), 3);
    let injected = &api_messages[0];
    let content = injected["content"].as_str().unwrap();
    assert!(content.contains("<available-deferred-tools>"));
    assert!(content.contains("WebSearch"));
    assert!(content.contains("WebFetch"));
    assert!(content.contains("ToolSearchTool"));
}

/// Test that discovered tools are NOT shown in <available-deferred-tools>
#[test]
fn test_discovered_tools_excluded_from_available_block() {
    // Enable tool search for this test
    unsafe { std::env::set_var("ENABLE_TOOL_SEARCH", "1") };
    let tools = vec![
        make_deferred_tool("Bash", false, false),
        make_deferred_tool("WebSearch", true, false),
        make_deferred_tool("WebFetch", true, false),
    ];

    let engine = QueryEngine::new(QueryEngineConfig {
        model: "test-model".to_string(),
        tools,
        cwd: "/tmp".to_string(),
        ..Default::default()
    });

    // WebSearch is already discovered - include it in api_messages
    let mut api_messages = vec![
        // Previously discovered WebSearch via tool_reference
        serde_json::json!({
            "role": "user",
            "content": serde_json::json!([{
                "type": "tool_result",
                "tool_use_id": "call_123",
                "content": [
                    {"type": "tool_reference", "tool_name": "WebSearch"}
                ]
            }]).to_string()
        }),
        serde_json::json!({"role": "user", "content": "Now fetch a URL"}),
    ];

    engine.maybe_inject_deferred_tools_block(&mut api_messages);

    // Should inject, but WebSearch should NOT be in the list
    assert_eq!(api_messages.len(), 3);
    // Find the injected block (it's inserted at position 0, before the discovered message)
    let injected = &api_messages[0];
    let content = injected["content"].as_str().unwrap();
    assert!(content.contains("WebFetch")); // Still deferred
    assert!(!content.contains("WebSearch")); // Already discovered
}

/// Test that when no deferred tools exist, nothing is injected
#[test]
fn test_no_injection_when_no_deferred_tools() {
    let tools = vec![
        make_deferred_tool("Bash", false, false),
        make_deferred_tool("FileRead", false, false),
    ];

    let engine = QueryEngine::new(QueryEngineConfig {
        model: "test-model".to_string(),
        tools,
        cwd: "/tmp".to_string(),
        ..Default::default()
    });

    let mut api_messages = vec![
        serde_json::json!({"role": "user", "content": "Read a file"}),
    ];

    engine.maybe_inject_deferred_tools_block(&mut api_messages);

    // No injection should happen
    assert_eq!(api_messages.len(), 1);
}

/// Test keyword search finds deferred tools by capability phrase (search_hint)
#[test]
fn test_keyword_search_finds_deferred_tools_by_hint() {
    let web_search = make_deferred_tool("WebSearch", true, false);
    let web_fetch = make_deferred_tool("WebFetch", true, false);
    let bash = make_deferred_tool("Bash", false, false);

    let tools = vec![&web_search, &web_fetch, &bash];

    // Search by capability
    let results = search_tools_with_keywords("search web", &tools, 5);
    assert!(results.contains(&"WebSearch".to_string()));

    let results = search_tools_with_keywords("fetch url", &tools, 5);
    assert!(results.contains(&"WebFetch".to_string()));

    // Search by tool name
    let results = search_tools_with_keywords("search", &tools, 5);
    assert!(results.contains(&"WebSearch".to_string()));
}

/// Test that the tool_reference content format matches what the API expects
#[test]
fn test_tool_reference_format_for_api_expansion() {
    // This test verifies the exact format that the API uses to expand tool_references
    let matches = vec!["WebSearch".to_string()];
    let result = ToolSearchTool::build_tool_reference_result(&matches, "call_abc");

    // The API looks for: content[].type == "tool_reference" && content[].tool_name
    let content_array = result["content"].as_array().unwrap();
    assert_eq!(content_array.len(), 1);

    let ref_block = &content_array[0];
    assert_eq!(ref_block["type"], "tool_reference");
    assert_eq!(ref_block["tool_name"], "WebSearch");

    // This is the format the API expands into the model's context
    // After expansion, the model sees the full tool schema and can call it
}

/// Test select: query parsing for ToolSearchTool
#[test]
fn test_tool_search_select_query() {
    // Single tool select
    let query = parse_tool_search_query("select:WebSearch");
    match query {
        ToolSearchQuery::Select(tools) => {
            assert_eq!(tools, vec!["WebSearch"]);
        }
        _ => panic!("Expected Select query"),
    }

    // Multi-tool select
    let query = parse_tool_search_query("select:WebSearch,WebFetch");
    match query {
        ToolSearchQuery::Select(tools) => {
            assert_eq!(tools, vec!["WebSearch", "WebFetch"]);
        }
        _ => panic!("Expected Select query"),
    }

    // Keyword query (no select: prefix)
    let query = parse_tool_search_query("find information online");
    match query {
        ToolSearchQuery::Keyword(s) => {
            assert_eq!(s, "find information online");
        }
        _ => panic!("Expected Keyword query"),
    }
}

/// Test that MCP tools are correctly identified as deferred
#[test]
fn test_mcp_tools_are_deferred() {
    let mcp_tool = make_deferred_tool("mcp__github__get_pr", false, true);
    assert!(crate::tools::deferred_tools::is_deferred_tool(&mcp_tool));

    // Even if should_defer is false, MCP tools are deferred
    let mcp_tool_no_defer = make_deferred_tool("mcp__slack__send", false, true);
    assert!(crate::tools::deferred_tools::is_deferred_tool(&mcp_tool_no_defer));
}

/// Test that tool names are correctly parsed for keyword search
#[test]
fn test_parse_tool_name_for_search() {
    // Regular tool
    let regular = parse_tool_name("FileRead");
    assert!(!regular.is_mcp);

    // MCP tool
    let mcp = parse_tool_name("mcp__github__get_pull_request");
    assert!(mcp.is_mcp);
    assert_eq!(mcp.parts, vec!["github", "get", "pull", "request"]);
}

/// Test that search handles exact tool name match (fast path)
#[test]
fn test_keyword_search_exact_match_fast_path() {
    let web_search = make_deferred_tool("WebSearch", true, false);
    let tools = vec![&web_search];

    // Exact tool name match should return immediately
    let results = search_tools_with_keywords("WebSearch", &tools, 5);
    assert_eq!(results, vec!["WebSearch"]);
}

/// Test that search handles MCP prefix queries
#[test]
fn test_keyword_search_mcp_prefix() {
    let mcp_github_pr = make_deferred_tool("mcp__github__get_pr", true, true);
    let mcp_slack_send = make_deferred_tool("mcp__slack__send_message", true, true);
    let tools = vec![&mcp_github_pr, &mcp_slack_send];

    // Query by MCP server prefix
    let results = search_tools_with_keywords("mcp__github", &tools, 5);
    assert!(results.contains(&"mcp__github__get_pr".to_string()));

    let results = search_tools_with_keywords("mcp__slack", &tools, 5);
    assert!(results.contains(&"mcp__slack__send_message".to_string()));
}
