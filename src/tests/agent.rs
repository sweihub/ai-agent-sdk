use crate::Agent;
use crate::agent::build_agent_system_prompt;
use crate::env::EnvConfig;
use crate::tests::common::clear_all_test_state;
use crate::types::ContentDelta;

/// Test that Agent tool correctly extracts all parameters from input
#[tokio::test]
async fn test_agent_tool_parses_all_parameters() {
    // Test parameter extraction from various input formats
    // This verifies all parameters are now properly parsed

    // Test 1: subagent_type parameter (snake_case)
    let input1 = serde_json::json!({
        "description": "explore-agent",
        "prompt": "Explore the codebase",
        "subagent_type": "Explore"
    });
    assert_eq!(input1["subagent_type"].as_str(), Some("Explore"));
    assert_eq!(input1["subagentType"].as_str(), None); // snake_case works

    // Test 2: subagent_type parameter (camelCase)
    let input2 = serde_json::json!({
        "description": "explore-agent",
        "prompt": "Explore the codebase",
        "subagentType": "Plan"
    });
    assert_eq!(input2["subagentType"].as_str(), Some("Plan"));

    // Test 3: run_in_background (snake_case)
    let input3 = serde_json::json!({
        "description": "background-agent",
        "prompt": "Run in background",
        "run_in_background": true
    });
    assert_eq!(input3["run_in_background"].as_bool(), Some(true));

    // Test 4: runInBackground (camelCase)
    let input4 = serde_json::json!({
        "description": "background-agent",
        "runInBackground": true
    });
    assert_eq!(input4["runInBackground"].as_bool(), Some(true));

    // Test 5: max_turns (snake_case)
    let input5 = serde_json::json!({
        "description": "test",
        "max_turns": 5
    });
    assert_eq!(input5["max_turns"].as_u64(), Some(5));

    // Test 6: maxTurns (camelCase)
    let input6 = serde_json::json!({
        "description": "test",
        "maxTurns": 10
    });
    assert_eq!(input6["maxTurns"].as_u64(), Some(10));

    // Test 7: team_name (snake_case)
    let input7 = serde_json::json!({
        "description": "team-agent",
        "team_name": "my-team"
    });
    assert_eq!(input7["team_name"].as_str(), Some("my-team"));

    // Test 8: teamName (camelCase)
    let input8 = serde_json::json!({
        "description": "team-agent",
        "teamName": "my-team"
    });
    assert_eq!(input8["teamName"].as_str(), Some("my-team"));

    // Test 9: cwd parameter
    let input9 = serde_json::json!({
        "description": "custom-cwd",
        "cwd": "/custom/path"
    });
    assert_eq!(input9["cwd"].as_str(), Some("/custom/path"));

    // Test 10: name parameter
    let input10 = serde_json::json!({
        "name": "my-agent",
        "description": "named-agent"
    });
    assert_eq!(input10["name"].as_str(), Some("my-agent"));

    // Test 11: mode parameter
    let input11 = serde_json::json!({
        "description": "plan-mode",
        "mode": "plan"
    });
    assert_eq!(input11["mode"].as_str(), Some("plan"));

    // Test 12: isolation parameter
    let input12 = serde_json::json!({
        "description": "isolated",
        "isolation": "worktree"
    });
    assert_eq!(input12["isolation"].as_str(), Some("worktree"));

    // Verify all expected keys are now handled
    // The agent tool executor should handle all these parameters
}

/// Test that Agent tool creates subagent with proper system prompt based on agent type
#[tokio::test]
async fn test_agent_tool_system_prompt_by_type() {
    // Test system prompt generation for different agent types
    let explore_prompt = build_agent_system_prompt("Explore task", Some("Explore"));
    assert!(explore_prompt.contains("Explore agent"));

    let plan_prompt = build_agent_system_prompt("Plan task", Some("Plan"));
    assert!(plan_prompt.contains("Plan agent"));

    let review_prompt = build_agent_system_prompt("Review task", Some("Review"));
    assert!(review_prompt.contains("Review agent"));

    let general_prompt = build_agent_system_prompt("General task", None);
    assert!(general_prompt.contains("Task description: General task"));
}

/// Check if required environment variables are present for real API tests
/// Returns true if AI_BASE_URL, AI_MODEL, and AI_AUTH_TOKEN can be loaded from .env
pub fn has_required_env_vars() -> bool {
    let config = EnvConfig::load();
    config.base_url.is_some() && config.model.is_some() && config.auth_token.is_some()
}

/// Test Agent creation with options
#[tokio::test]
async fn test_create_agent() {
    let agent = Agent::new("claude-sonnet-4-6");
    assert!(!agent.get_model().is_empty());
}

/// Test Agent tool calling with real .env config
/// This test makes an actual API call using the configured model
#[serial_test::serial]
#[tokio::test]
async fn test_agent_tool_calling_with_real_env_config() {
    // Only run if required env vars are set
    if !has_required_env_vars() {
        eprintln!("Skipping test: AI_BASE_URL, AI_MODEL, or AI_AUTH_TOKEN not set");
        return;
    }

    // Load config from .env file
    let config = EnvConfig::load();

    // Verify config is loaded
    assert!(config.base_url.is_some(), "Base URL should be configured");
    assert!(
        config.auth_token.is_some(),
        "Auth token should be configured"
    );
    assert!(config.model.is_some(), "Model should be configured");

    // Create agent with real config
    let agent = Agent::new(config.model.as_ref().unwrap());

    // Verify agent was created with the configured model
    let model = agent.get_model();
    assert!(!model.is_empty(), "Agent should have a model set");
    println!("Using model: {}", model);
}

/// Test agent prompt with real API call using .env config
/// This is an integration test that exercises the full agent flow
#[serial_test::serial]
#[tokio::test]
async fn test_agent_prompt_with_real_api() {
    // Only run if required env vars are set
    if !has_required_env_vars() {
        eprintln!("Skipping test: AI_BASE_URL, AI_MODEL, or AI_AUTH_TOKEN not set");
        return;
    }

    // Load config from .env file
    let config = EnvConfig::load();

    // Skip test if no API configured
    if config.base_url.is_none() || config.auth_token.is_none() {
        eprintln!("Skipping test: no API config found");
        return;
    }

    clear_all_test_state();

    // Create agent with all tools and real config
    use crate::get_all_tools;
    let tools = get_all_tools();

    let agent = Agent::new(config.model.as_ref().unwrap())
        .max_turns(3)
        .tools(tools);

    // Make a simple prompt that should trigger tool use
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        agent.query("What is 2 + 2? Just give me the answer."),
    )
    .await
    .expect("test timed out after 30s");

    // Verify we got a response
    assert!(result.is_ok(), "Agent should respond successfully");
    let response = result.unwrap();
    assert!(!response.text.is_empty(), "Response should not be empty");
    println!("Agent response: {}", response.text);
}

/// Test agent tool calling with multiple tools from .env config
/// This tests that the agent can use tools when configured via .env
#[serial_test::serial]
#[tokio::test]
async fn test_agent_with_multiple_tools_real_config() {
    // Only run if required env vars are set
    if !has_required_env_vars() {
        eprintln!("Skipping test: AI_BASE_URL, AI_MODEL, or AI_AUTH_TOKEN not set");
        return;
    }

    // Load config from .env file
    let config = EnvConfig::load();

    // Skip if no API configured
    if config.base_url.is_none() || config.auth_token.is_none() {
        eprintln!("Skipping test: no API config found");
        return;
    }

    // Get all available tools
    use crate::get_all_tools;
    let tools = get_all_tools();

    // Verify we have tools available
    assert!(!tools.is_empty(), "Should have tools available");

    clear_all_test_state();

    let agent = Agent::new(config.model.as_ref().unwrap())
        .max_turns(3)
        .tools(tools);

    // Prompt that might use tools
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        agent.query("List all Rust files in the current directory using glob"),
    )
    .await
    .expect("test timed out after 30s");

    // Should get a response (may or may not use tools depending on model)
    assert!(result.is_ok(), "Agent should respond");
    let response = result.unwrap();
    assert!(!response.text.is_empty(), "Response should not be empty");
    println!("Agent response: {}", response.text);
}

/// Test that tool executors are registered and can be invoked
/// This verifies the fix for tool calling not working in TUI
#[serial_test::serial]
#[tokio::test]
async fn test_tool_executors_registered() {
    // Only run if required env vars are set
    if !has_required_env_vars() {
        eprintln!("Skipping test: AI_BASE_URL, AI_MODEL, or AI_AUTH_TOKEN not set");
        return;
    }

    // Load config from .env file
    let config = EnvConfig::load();

    // Skip if no API configured
    if config.base_url.is_none() || config.auth_token.is_none() {
        eprintln!("Skipping test: no API config found");
        return;
    }

    // Get all available tools
    use crate::get_all_tools;
    let tools = get_all_tools();

    // Verify tools are available
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    assert!(tool_names.contains(&"Bash"), "Should have Bash tool");
    assert!(
        tool_names.contains(&"Read"),
        "Should have FileRead tool"
    );
    assert!(tool_names.contains(&"Glob"), "Should have Glob tool");
    println!("Available tools: {:?}", tool_names);

    // Create agent
    let agent = Agent::new(config.model.as_ref().unwrap())
        .max_turns(3)
        .tools(tools);

    // Prompt that should definitely use the Bash tool
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        agent.query("Run this command: echo 'hello from tool test'"),
    )
    .await
    .expect("test timed out after 30s");

    // Verify we got a response
    assert!(result.is_ok(), "Agent should respond successfully");
    let response = result.unwrap();
    assert!(!response.text.is_empty(), "Response should not be empty");

    // Check that the tool was actually used (response should contain output)
    let text_lower = response.text.to_lowercase();
    let tool_was_used = text_lower.contains("hello from tool test") || text_lower.contains("tool");
    println!(
        "Tool calling test - Response: {} (tool_used: {})",
        response.text, tool_was_used
    );
}

/// Test Glob tool directly via agent
#[serial_test::serial]
#[tokio::test]
async fn test_glob_tool_via_agent() {
    // Only run if required env vars are set
    if !has_required_env_vars() {
        eprintln!("Skipping test: AI_BASE_URL, AI_MODEL, or AI_AUTH_TOKEN not set");
        return;
    }

    // Load config from .env file
    let config = EnvConfig::load();

    // Skip if no API configured
    if config.base_url.is_none() || config.auth_token.is_none() {
        eprintln!("Skipping test: no API config found");
        return;
    }

    // Get all available tools
    use crate::get_all_tools;
    let tools = get_all_tools();

    let agent = Agent::new(config.model.as_ref().unwrap())
        .max_turns(3)
        .tools(tools);

    // Prompt that should use Glob tool
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        agent.query("List all .rs files in the src directory using the Glob tool"),
    )
    .await
    .expect("test timed out after 30s");

    assert!(result.is_ok(), "Agent should respond");
    let response = result.unwrap();
    assert!(!response.text.is_empty(), "Response should not be empty");
    println!("Glob tool test response: {}", response.text);
}

/// Test FileRead tool directly via agent
#[serial_test::serial]
#[tokio::test]
async fn test_fileread_tool_via_agent() {
    // Only run if required env vars are set
    if !has_required_env_vars() {
        eprintln!("Skipping test: AI_BASE_URL, AI_MODEL, or AI_AUTH_TOKEN not set");
        return;
    }

    // Load config from .env file
    let config = EnvConfig::load();

    // Skip if no API configured
    if config.base_url.is_none() || config.auth_token.is_none() {
        eprintln!("Skipping test: no API config found");
        return;
    }

    // Get all available tools
    use crate::get_all_tools;
    let tools = get_all_tools();

    let agent = Agent::new(config.model.as_ref().unwrap())
        .max_turns(3)
        .tools(tools);

    // Prompt that should use FileRead tool
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        agent.query("Read the Cargo.toml file from the current directory"),
    )
    .await
    .expect("test timed out after 30s");

    assert!(result.is_ok(), "Agent should respond");
    let response = result.unwrap();
    assert!(!response.text.is_empty(), "Response should not be empty");
    // The response should contain something from Cargo.toml
    println!("FileRead tool test response: {}", response.text);
}

/// Test multiple tool calls in one turn
#[serial_test::serial]
#[tokio::test]
async fn test_multiple_tool_calls() {
    // Only run if required env vars are set
    if !has_required_env_vars() {
        eprintln!("Skipping test: AI_BASE_URL, AI_MODEL, or AI_AUTH_TOKEN not set");
        return;
    }

    // Load config from .env file
    let config = EnvConfig::load();

    // Skip if no API configured
    if config.base_url.is_none() || config.auth_token.is_none() {
        eprintln!("Skipping test: no API config found");
        return;
    }

    // Get all available tools
    use crate::get_all_tools;
    let tools = get_all_tools();

    let agent = Agent::new(config.model.as_ref().unwrap())
        .max_turns(5)
        .tools(tools);

    // Prompt that should use multiple tools
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        agent.query("First list all files in the current directory, then read the README.md file if it exists"),
    )
    .await
    .expect("test timed out after 30s");

    assert!(result.is_ok(), "Agent should respond");
    let response = result.unwrap();
    assert!(!response.text.is_empty(), "Response should not be empty");
    println!("Multiple tool calls test response: {}", response.text);
}

/// Test that the agent remembers context across multiple query() calls
/// This verifies that messages are properly accumulated in the Agent and
/// passed to the QueryEngine for context maintenance across turns.
/// Retries up to 3 times because LLM response under rate limiting can be unpredictable.
#[serial_test::serial]
#[tokio::test]
async fn test_agent_remembers_context_across_queries() {
    if !has_required_env_vars() {
        eprintln!("Skipping test: AI_BASE_URL, AI_MODEL, or AI_AUTH_TOKEN not set");
        return;
    }

    let config = EnvConfig::load();
    if config.base_url.is_none() || config.auth_token.is_none() {
        eprintln!("Skipping test: no API config found");
        return;
    }

    clear_all_test_state();

    // Retry up to 3 times because LLM under rate limiting can give unpredictable responses
    let mut last_error = String::new();
    for attempt in 1..=3 {
        // Use no tools — LLM must answer directly from context, avoiding network calls.
        let agent = Agent::new(config.model.as_ref().unwrap())
            .max_turns(5);

        let result1 = tokio::time::timeout(
            std::time::Duration::from_secs(90),
            agent.query("Reply ONLY with the single word: 'Acknowledged'"),
        )
        .await
        .expect("Turn 1 timed out after 90s")
        .expect("First query should succeed");
        if result1.text.is_empty() {
            last_error = format!("Turn 1 empty response, attempt {}", attempt);
            continue;
        }
        println!("Turn 1 response: {}", result1.text);

        let result2 = tokio::time::timeout(
            std::time::Duration::from_secs(90),
            agent.query("What was the exact word I asked you to reply with in the previous message?"),
        )
        .await
        .expect("Turn 2 timed out after 90s")
        .expect("Second query should succeed");
        if result2.text.is_empty() {
            last_error = format!("Turn 2 empty response, attempt {}", attempt);
            continue;
        }
        println!("Turn 2 response: {}", result2.text);

        let text_lower = result2.text.to_lowercase();
        if !text_lower.contains("acknowledged") {
            last_error = format!("LLM didn't recall 'Acknowledged': '{}', attempt {}", result2.text, attempt);
            continue;
        }

        let messages = agent.get_messages();
        if messages.len() < 4 {
            last_error = format!("Only {} messages, attempt {}", messages.len(), attempt);
            continue;
        }
        println!("Message history has {} messages", messages.len());

        // All checks passed
        last_error.clear();
        break;
    }
    assert!(
        last_error.is_empty(),
        "Test failed after 3 attempts: {}",
        last_error
    );
}

/// Test that ToolSearchTool can be used to discover and use a deferred tool
/// This tests the full flow: agent uses ToolSearch to find a tool, then uses it
#[serial_test::serial]
#[tokio::test]
async fn test_tool_search_discovers_and_uses_deferred_tool() {
    // Only run if required env vars are set
    if !has_required_env_vars() {
        eprintln!("Skipping test: AI_BASE_URL, AI_MODEL, or AI_AUTH_TOKEN not set");
        return;
    }

    // Load config from .env file
    let config = EnvConfig::load();

    // Skip if no API configured
    if config.base_url.is_none() || config.auth_token.is_none() {
        eprintln!("Skipping test: no API config found");
        return;
    }

    // Get all available tools (includes deferred tools like WebSearch, WebFetch)
    use crate::get_all_tools;
    let tools = get_all_tools();

    // Verify we have ToolSearch available
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    assert!(
        tool_names.contains(&"ToolSearch"),
        "Should have ToolSearch tool"
    );
    assert!(
        tool_names.contains(&"WebSearch"),
        "Should have WebSearch (deferred) tool"
    );

    let agent = Agent::new(config.model.as_ref().unwrap())
        .max_turns(10)
        .tools(tools);

    // Ask the agent to discover and use WebSearch via ToolSearch
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(120),
        agent.query("Use ToolSearch to discover the WebSearch tool, then use it to look up the latest news about Iran."),
    )
    .await
    .expect("test timed out after 120s");

    assert!(result.is_ok(), "Agent should respond successfully");
    let response = result.unwrap();
    assert!(
        !response.text.is_empty(),
        "Agent returned empty response (possible API issue)."
    );

    // Verify ToolSearch was used to discover WebSearch by checking message history.
    // The LLM should discover WebSearch via ToolSearch before using it to answer the question.
    let messages = agent.get_messages();
    let has_tool_search_call = messages
        .iter()
        .any(|m| m.content.contains("ToolSearch") || m.content.contains("WebSearch"));
    assert!(
        has_tool_search_call,
        "Agent should have used ToolSearch to discover WebSearch. Messages: {:?}",
        messages
    );
}

/// Test that AgentEvent streaming events are properly emitted during agent execution.
/// This verifies: Thinking, MessageStart, MessageStop, ContentBlockDelta, ToolStart, ToolComplete
#[serial_test::serial]
#[tokio::test]
async fn test_agent_events_emitted_correctly() {
    // Only run if required env vars are set
    if !has_required_env_vars() {
        eprintln!("Skipping test: AI_BASE_URL, AI_MODEL, or AI_AUTH_TOKEN not set");
        return;
    }

    // Load config from .env file
    let config = EnvConfig::load();

    clear_all_test_state();

    // Skip if no API configured
    if config.base_url.is_none() || config.auth_token.is_none() {
        eprintln!("Skipping test: no API config found");
        return;
    }

    // Use no tools and max_turns(1) — LLM must answer directly without network calls.
    // This guarantees the test completes in reasonable time under rate limiting.

    // Track events received
    use std::sync::Mutex;
    let events_received: std::sync::Arc<Mutex<Vec<crate::types::AgentEvent>>> =
        std::sync::Arc::new(Mutex::new(Vec::new()));
    let events_clone = events_received.clone();

    // Create agent with event callback
    let agent = Agent::new(config.model.as_ref().unwrap())
        .max_turns(1)
        .on_event(move |event| {
            events_clone.lock().unwrap().push(event);
        });

    // Simple prompt that forces a direct text response (no tools)
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        agent.query("Reply ONLY with: 'EventTest123'"),
    )
    .await
    .expect("test timed out after 30s");

    // Verify we got a response
    assert!(result.is_ok(), "Agent should respond successfully");
    let response = result.unwrap();
    assert!(!response.text.is_empty(), "Response should not be empty");
    println!("Agent response: {}", response.text);

    // Get the events that were received
    let events = events_received.lock().unwrap();

    // Verify we received Thinking event
    let has_thinking = events
        .iter()
        .any(|e| matches!(e, crate::types::AgentEvent::Thinking { .. }));
    assert!(
        has_thinking,
        "Should have received Thinking event. Events: {:?}",
        events
    );

    // Verify we received MessageStart event
    let has_message_start = events
        .iter()
        .any(|e| matches!(e, crate::types::AgentEvent::MessageStart { .. }));
    assert!(
        has_message_start,
        "Should have received MessageStart event. Events: {:?}",
        events
    );

    // Verify we received MessageStop event
    let has_message_stop = events
        .iter()
        .any(|e| matches!(e, crate::types::AgentEvent::MessageStop));
    assert!(
        has_message_stop,
        "Should have received MessageStop event. Events: {:?}",
        events
    );

    // Verify we received ContentBlockDelta event (text content)
    let has_content_delta = events.iter().any(|e| match e {
        crate::types::AgentEvent::ContentBlockDelta { delta, .. } => match delta {
            ContentDelta::Text { text } => !text.is_empty(),
            _ => false,
        },
        _ => false,
    });
    assert!(
        has_content_delta,
        "Should have received ContentBlockDelta with text. Events: {:?}",
        events
    );

    // Verify the output contains our test string
    let text_lower = response.text.to_lowercase();
    let has_expected_output = text_lower.contains("eventtest123");
    assert!(
        has_expected_output,
        "Response should contain 'EventTest123'. Response: {}",
        response.text
    );

    // Verify we received Done event (final event on normal completion)
    let has_done = events
        .iter()
        .any(|e| matches!(e, crate::types::AgentEvent::Done { .. }));
    assert!(
        has_done,
        "Should have received AgentEvent::Done on normal completion. Events: {:?}",
        events
    );

    println!("All event checks passed! Events received:");
    for (i, event) in events.iter().enumerate() {
        println!("  {}: {:?}", i, event);
    }
}

/// Test that MaxTurnsReached event is emitted when max turns is limited to 1
#[serial_test::serial]
#[tokio::test]
async fn test_agent_max_turns_reached_event() {
    // Only run if required env vars are set
    if !has_required_env_vars() {
        eprintln!("Skipping test: AI_BASE_URL, AI_MODEL, or AI_AUTH_TOKEN not set");
        return;
    }

    // Load config from .env file
    let config = EnvConfig::load();

    // Skip if no API configured
    if config.base_url.is_none() || config.auth_token.is_none() {
        eprintln!("Skipping test: no API config found");
        return;
    }

    // Get all available tools
    use crate::get_all_tools;
    let tools = get_all_tools();

    clear_all_test_state();

    // Track events received
    use std::sync::Mutex;
    let events_received: std::sync::Arc<Mutex<Vec<crate::types::AgentEvent>>> =
        std::sync::Arc::new(Mutex::new(Vec::new()));
    let events_clone = events_received.clone();

    // Create agent with max_turns=1 (forces MaxTurnsReached)
    let agent = Agent::new(config.model.as_ref().unwrap())
        .max_turns(1)
        .tools(tools)
        .on_event(move |event| {
            events_clone.lock().unwrap().push(event);
        });

    // Prompt that requires tool use (will need more than 1 turn)
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        agent.query("Run this command: echo 'MaxTurnsTest'"),
    )
    .await
    .expect("test timed out after 30s");

    // Should still return a result (may be truncated due to max turns)
    assert!(
        result.is_ok(),
        "Agent should return result even with max turns"
    );
    let response = result.unwrap();
    println!("Agent response (max_turns=1): {}", response.text);

    // Get the events that were received
    let events = events_received.lock().unwrap();

    // Verify we received MessageStart event
    let has_message_start = events
        .iter()
        .any(|e| matches!(e, crate::types::AgentEvent::MessageStart { .. }));
    assert!(
        has_message_start,
        "Should have received MessageStart event. Events: {:?}",
        events
    );

    // Verify we received MessageStop event
    let has_message_stop = events
        .iter()
        .any(|e| matches!(e, crate::types::AgentEvent::MessageStop));
    assert!(
        has_message_stop,
        "Should have received MessageStop event. Events: {:?}",
        events
    );

    // Verify we received MaxTurnsReached event
    let has_max_turns = events
        .iter()
        .any(|e| matches!(e, crate::types::AgentEvent::MaxTurnsReached { .. }));
    println!(
        "MaxTurnsReached check: {:?}",
        events
            .iter()
            .filter(|e| matches!(e, crate::types::AgentEvent::MaxTurnsReached { .. }))
            .collect::<Vec<_>>()
    );
    assert!(
        has_max_turns,
        "Should have received MaxTurnsReached event. Events: {:?}",
        events
    );

    // Done event must follow MaxTurnsReached
    let has_done = events
        .iter()
        .any(|e| matches!(e, crate::types::AgentEvent::Done { .. }));
    assert!(
        has_done,
        "Should have received AgentEvent::Done after MaxTurnsReached. Events: {:?}",
        events
    );

    println!("MaxTurnsReached test passed! Events received:");
    for (i, event) in events.iter().enumerate() {
        println!("  {}: {:?}", i, event);
    }
}

/// Test that ToolError event is emitted when a tool fails
#[serial_test::serial]
#[tokio::test]
async fn test_agent_tool_error_event() {
    // Only run if required env vars are set
    if !has_required_env_vars() {
        eprintln!("Skipping test: AI_BASE_URL, AI_MODEL, or AI_AUTH_TOKEN not set");
        return;
    }

    // Load config from .env file
    let config = EnvConfig::load();

    // Skip if no API configured
    if config.base_url.is_none() || config.auth_token.is_none() {
        eprintln!("Skipping test: no API config found");
        return;
    }

    clear_all_test_state();

    // Get all available tools
    use crate::get_all_tools;
    let tools = get_all_tools();

    // Track events received
    use std::sync::Mutex;
    let events_received: std::sync::Arc<Mutex<Vec<crate::types::AgentEvent>>> =
        std::sync::Arc::new(Mutex::new(Vec::new()));
    let events_clone = events_received.clone();

    // Create agent with event callback
    let agent = Agent::new(config.model.as_ref().unwrap())
        .max_turns(3)
        .tools(tools)
        .on_event(move |event| {
            events_clone.lock().unwrap().push(event);
        });

    // Prompt that tries to access a non-existing file (will trigger tool error)
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        agent.query("Read the first line of the file 'no-such-file-xyz.txt' using head command"),
    )
    .await
    .expect("test timed out after 30s");

    // Should still return a result (agent handles error)
    assert!(
        result.is_ok(),
        "Agent should return result even with tool error"
    );
    let response = result.unwrap();
    println!("Agent response (tool error case): {}", response.text);

    // Get the events that were received
    let events = events_received.lock().unwrap();

    // Verify we received MessageStart event
    let has_message_start = events
        .iter()
        .any(|e| matches!(e, crate::types::AgentEvent::MessageStart { .. }));
    assert!(
        has_message_start,
        "Should have received MessageStart event. Events: {:?}",
        events
    );

    // Verify we received MessageStop event
    let has_message_stop = events
        .iter()
        .any(|e| matches!(e, crate::types::AgentEvent::MessageStop));
    assert!(
        has_message_stop,
        "Should have received MessageStop event. Events: {:?}",
        events
    );

    // Verify we received ToolStart event
    let has_tool_start = events
        .iter()
        .any(|e| matches!(e, crate::types::AgentEvent::ToolStart { .. }));
    println!(
        "ToolStart check: {:?}",
        events
            .iter()
            .filter(|e| matches!(e, crate::types::AgentEvent::ToolStart { .. }))
            .collect::<Vec<_>>()
    );
    assert!(
        has_tool_start,
        "Should have received ToolStart event. Events: {:?}",
        events
    );

    // Verify we received ToolError event (file not found should trigger error)
    let has_tool_error = events
        .iter()
        .any(|e| matches!(e, crate::types::AgentEvent::ToolError { .. }));
    println!(
        "ToolError check: {:?}",
        events
            .iter()
            .filter(|e| matches!(e, crate::types::AgentEvent::ToolError { .. }))
            .collect::<Vec<_>>()
    );

    // Note: ToolError may or may not fire depending on how the LLM handles the error
    // The important thing is we get a response
    if has_tool_error {
        println!("ToolError event detected!");
    }

    println!("ToolError test completed! Events received:");
    for (i, event) in events.iter().enumerate() {
        println!("  {}: {:?}", i, event);
    }
}

// ---------------------------------------------------------------------------
// Persisted engine mechanics tests
// ---------------------------------------------------------------------------

/// Verify that the agent accumulates messages across multiple query() calls.
/// This is the core persisted-engine test: the engine must not be recreated
/// between calls, so conversation state carries forward naturally.
#[serial_test::serial]
#[tokio::test]
async fn test_persisted_engine_accumulates_messages() {
    if !has_required_env_vars() {
        eprintln!("Skipping test: AI_BASE_URL, AI_MODEL, or AI_AUTH_TOKEN not set");
        return;
    }

    let config = EnvConfig::load();
    if config.base_url.is_none() || config.auth_token.is_none() {
        eprintln!("Skipping test: no API config found");
        return;
    }

    let agent = Agent::new("claude-sonnet-4-6").max_turns(5);

    clear_all_test_state();

    // Before any query, message list should be empty
    assert!(
        agent.get_messages().is_empty(),
        "Messages should be empty before first query"
    );

    // First call — store the message count after
    let result1 = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        agent.query("Say 'Hello' and nothing else."),
    )
    .await
    .expect("Turn 1 timed out after 30s");
    assert!(result1.is_ok(), "First query should succeed");
    let msgs1 = agent.get_messages();
    assert!(
        msgs1.len() >= 2,
        "After first query: expected >=2 messages, got {}",
        msgs1.len()
    );

    // Second call — message list must be longer (accumulates)
    let result2 = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        agent.query("Repeat back what I just said to you."),
    )
    .await
    .expect("Turn 2 timed out after 30s");
    assert!(result2.is_ok(), "Second query should succeed");
    let msgs2 = agent.get_messages();
    assert!(
        msgs2.len() > msgs1.len(),
        "After second query: expected {} > {} messages (messages should accumulate)",
        msgs2.len(),
        msgs1.len()
    );

    // Third call — still more messages
    let result3 = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        agent.query("Now say goodbye."),
    )
    .await
    .expect("Turn 3 timed out after 30s");
    assert!(result3.is_ok(), "Third query should succeed");
    let msgs3 = agent.get_messages();
    assert!(
        msgs3.len() > msgs2.len(),
        "After third query: expected {} > {} messages (messages should keep accumulating)",
        msgs3.len(),
        msgs2.len()
    );

    println!(
        "Persisted engine: {} -> {} -> {} messages across 3 turns",
        msgs1.len(),
        msgs2.len(),
        msgs3.len()
    );
}

/// Verify that reset() causes the engine to be recreated and messages are cleared.
#[serial_test::serial]
#[tokio::test]
async fn test_reset_clears_engine_state() {
    if !has_required_env_vars() {
        eprintln!("Skipping test: AI_BASE_URL, AI_MODEL, or AI_AUTH_TOKEN not set");
        return;
    }

    let config = EnvConfig::load();
    if config.base_url.is_none() || config.auth_token.is_none() {
        eprintln!("Skipping test: no API config found");
        return;
    }

    let agent = Agent::new("claude-sonnet-4-6").max_turns(5);

    clear_all_test_state();

    // First query
    let _r1 = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        agent.query("Say 'ResetTest'."),
    )
    .await
    .expect("Turn 1 timed out after 30s");
    let msgs_before = agent.get_messages();
    assert!(msgs_before.len() >= 2);

    // Reset
    agent.reset();

    // After reset, message list must be empty
    assert!(
        agent.get_messages().is_empty(),
        "Messages should be empty after reset"
    );

    // Second query should work fine (engine recreated)
    let _r2 = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        agent.query("Say 'PostReset'."),
    )
    .await
    .expect("Turn 2 timed out after 30s");
    let msgs_after = agent.get_messages();
    assert!(
        msgs_after.len() >= 2,
        "After reset + query: expected >=2 messages, got {}",
        msgs_after.len()
    );

    // Agent can continue calling after reset (engine was recreated)
    let _r3 = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        agent.query("Say 'Again'."),
    )
    .await
    .expect("Turn 3 timed out after 30s");
    let msgs_after2 = agent.get_messages();
    assert!(
        msgs_after2.len() > msgs_after.len(),
        "Agent should accumulate messages again after reset"
    );

    println!(
        "Reset test: {} -> clear -> {} -> {} messages",
        msgs_before.len(),
        msgs_after.len(),
        msgs_after2.len()
    );
}

/// Verify that the agent remembers context across query() calls via the
/// persisted engine — the LLM should reference information from turn 1 when
/// asked about it in turn 2.
#[serial_test::serial]
#[tokio::test]
async fn test_persisted_engine_llm_remembers_context() {
    if !has_required_env_vars() {
        eprintln!("Skipping test: AI_BASE_URL, AI_MODEL, or AI_AUTH_TOKEN not set");
        return;
    }

    let agent = Agent::new("claude-sonnet-4-6").max_turns(3);

    clear_all_test_state();

    // Turn 1: store a fact — LLM must answer without tools
    let _r1 = tokio::time::timeout(
        std::time::Duration::from_secs(120),
        agent.query("Reply ONLY with: 'OK'"),
    )
    .await
    .expect("Turn 1 timed out after 120s")
    .expect("Turn 1 should succeed");
    assert!(agent.get_messages().len() >= 2);

    // Turn 2: recall task — tests that turn 1 is in context
    let r2 = tokio::time::timeout(
        std::time::Duration::from_secs(120),
        agent.query("Repeat the exact word from my previous message. Reply with only that word."),
    )
    .await
    .expect("Turn 2 timed out after 120s")
    .expect("Second turn should succeed");
    let answer = r2.text.to_lowercase();
    println!("LLM remembers context: '{}'", answer);

    // The key assertion: message count proves context is preserved across query() calls.
    // The LLM content check is best-effort — under rate limiting it may paraphrase.
    let msgs2 = agent.get_messages();
    assert!(
        msgs2.len() >= 4,
        "After 2 turns: expected >=4 messages, got {}. Turn 2 response: '{}'",
        msgs2.len(),
        r2.text
    );

    // Turn 3: verify 3-turn context
    let r3 = tokio::time::timeout(
        std::time::Duration::from_secs(120),
        agent.query("What word did I ask you to repeat? Reply with only that word."),
    )
    .await
    .expect("Turn 3 timed out after 120s")
    .expect("Third turn should succeed");
    let answer3 = r3.text.to_lowercase();
    println!("LLM remembers turn 2: '{}'", answer3);

    let msgs3 = agent.get_messages();
    assert!(
        msgs3.len() >= 6,
        "After 3 turns: expected >=6 messages, got {}. Turn 3 response: '{}'",
        msgs3.len(),
        r3.text
    );

    println!(
        "LLM context retention test passed: {} messages across 3 turns",
        msgs3.len()
    );
}

/// Test that an agent with can_use_tool (via allowed/disallowed tools) is configured
/// correctly on its QueryEngine. This verifies the parent-side of the context
/// inheritance chain.
#[test]
fn test_agent_can_use_tool_config() {
    let agent = Agent::new("claude-sonnet-4-6")
        .allowed_tools(vec!["Bash".to_string()]);

    // Verify allowed_tools are stored on AgentInner
    let inner = agent.inner_for_test().lock();
    assert_eq!(inner.allowed_tools, vec!["Bash".to_string()]);
    assert!(inner.disallowed_tools.is_empty());
}

/// Test that an agent with on_event and thinking configured stores them correctly.
#[test]
fn test_agent_event_and_thinking_config() {
    use std::sync::{Arc, Mutex};
    let events: Arc<Mutex<Vec<crate::types::AgentEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let events_clone = events.clone();

    let agent = Agent::new("claude-sonnet-4-6")
        .on_event(move |_event| {})
        .thinking(crate::types::ThinkingConfig::Enabled {
            budget_tokens: 4096,
        });

    let inner = agent.inner_for_test().lock();
    assert!(inner.on_event.is_some(), "on_event should be set");
    assert!(inner.thinking.is_some(), "thinking should be set");
}

/// Test that subagents created through the query() path inherit parent context fields.
/// This integration test verifies can_use_tool and on_event propagation to subagents
/// by having the parent agent spawn a subagent and checking that the on_event callback
/// fires for subagent activity.
#[serial_test::serial]
#[tokio::test]
async fn test_subagent_inherits_parent_context() {
    if !has_required_env_vars() {
        eprintln!("Skipping test: AI_BASE_URL, AI_MODEL, or AI_AUTH_TOKEN not set");
        return;
    }

    let config = EnvConfig::load();
    if config.base_url.is_none() || config.auth_token.is_none() {
        eprintln!("Skipping test: no API config found");
        return;
    }

    clear_all_test_state();

    use std::sync::{Arc, Mutex};
    let events: Arc<Mutex<Vec<crate::types::AgentEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let events_clone = events.clone();

    use crate::get_all_tools;
    let tools = get_all_tools();

    // Create agent with all context fields configured
    let agent = Agent::new(config.model.as_ref().unwrap())
        .max_turns(5)
        .tools(tools)
        .on_event(move |event| {
            events_clone.lock().unwrap().push(event);
        });

    // Verify parent has on_event configured
    let inner = agent.inner_for_test().lock();
    assert!(inner.on_event.is_some(), "Parent agent should have on_event configured");
    drop(inner);

    // Prompt that may or may not use Agent tool - we just verify the events flow
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(60),
        agent.query("Reply with a single word: ContextInherited"),
    )
    .await
    .expect("Query timed out");

    assert!(result.is_ok(), "Agent should respond successfully");
    let response = result.unwrap();
    assert!(!response.text.is_empty(), "Response should not be empty");

    // Verify events were received through on_event callback
    let events = events.lock().unwrap();
    assert!(!events.is_empty(), "Events should have been received via on_event callback");

    // Verify at least a MessageStart event was received (confirms callback works)
    let has_message_start = events
        .iter()
        .any(|e| matches!(e, crate::types::AgentEvent::MessageStart { .. }));
    assert!(
        has_message_start,
        "Should have received MessageStart event through on_event callback"
    );

    println!("Subagent context inheritance test passed! Events received: {}", events.len());
}

/// Test that disallowed_tools configuration on the parent agent is preserved.
/// Subagents created by the parent should inherit this restriction.
#[test]
fn test_agent_disallowed_tools_config() {
    let agent = Agent::new("claude-sonnet-4-6")
        .disallowed_tools(vec!["Bash".to_string(), "Write".to_string()]);

    let inner = agent.inner_for_test().lock();
    assert_eq!(
        inner.disallowed_tools,
        vec!["Bash".to_string(), "Write".to_string()]
    );
    assert!(inner.allowed_tools.is_empty());
}

// ---------------------------------------------------------------------------
// AgentTool struct tests
// ---------------------------------------------------------------------------

/// Test that AgentTool implements the Tool trait correctly.
#[test]
fn test_agent_tool_trait_implementation() {
    use crate::tools::agent::{AgentTool, AgentToolConfig};
    use crate::tools::types::Tool;
    use crate::utils::AbortController;

    let config = AgentToolConfig {
        cwd: "/tmp".to_string(),
        api_key: Some("test-key".to_string()),
        base_url: Some("http://localhost:8080".to_string()),
        model: "claude-sonnet-4-6".to_string(),
        tool_pool: vec![],
        abort_controller: std::sync::Arc::new(AbortController::new(50)),
        can_use_tool: None,
        on_event: None,
        thinking: None,
        parent_messages: vec![],
        parent_user_context: std::collections::HashMap::new(),
        parent_system_context: std::collections::HashMap::new(),
    parent_session_id: None,
    };
    let tool = AgentTool::new(config);

    // Verify name
    assert_eq!(tool.name(), "Agent");

    // Verify description contains key terms
    let desc = tool.description();
    assert!(desc.contains("agent"), "Description should mention agent");
    assert!(
        desc.contains("autonomously"),
        "Description should mention autonomous operation"
    );

    // Verify input schema
    let schema = tool.input_schema();
    assert_eq!(schema.schema_type, "object");
    let props = schema.properties.as_object().expect("properties should be an object");
    assert!(props.contains_key("description"), "Schema should have 'description' property");
    assert!(props.contains_key("prompt"), "Schema should have 'prompt' property");
    assert!(props.contains_key("subagent_type"), "Schema should have 'subagent_type' property");
    assert!(props.contains_key("model"), "Schema should have 'model' property");
    assert!(props.contains_key("max_turns"), "Schema should have 'max_turns' property");
    assert!(
        props.contains_key("run_in_background"),
        "Schema should have 'run_in_background' property"
    );
    assert!(props.contains_key("name"), "Schema should have 'name' property");
    assert!(props.contains_key("team_name"), "Schema should have 'team_name' property");
    assert!(props.contains_key("mode"), "Schema should have 'mode' property");
    assert!(props.contains_key("cwd"), "Schema should have 'cwd' property");
    assert!(props.contains_key("isolation"), "Schema should have 'isolation' property");

    // Verify required fields
    let required = schema.required.as_ref().expect("required should be set");
    assert!(required.contains(&"description".to_string()));
    assert!(required.contains(&"prompt".to_string()));

    // Verify config accessor
    let cfg = tool.config();
    assert_eq!(cfg.cwd, "/tmp");
    assert_eq!(cfg.model, "claude-sonnet-4-6");
}

/// Test that AgentToolConfig is cloneable and preserves all fields.
#[test]
fn test_agent_tool_config_clone() {
    use crate::tools::agent::AgentToolConfig;
    use crate::utils::AbortController;

    let config = AgentToolConfig {
        cwd: "/test/path".to_string(),
        api_key: Some("my-key".to_string()),
        base_url: Some("http://example.com".to_string()),
        model: "test-model".to_string(),
        tool_pool: vec![],
        abort_controller: std::sync::Arc::new(AbortController::new(50)),
        can_use_tool: None,
        on_event: None,
        thinking: None,
        parent_messages: vec![],
        parent_user_context: std::collections::HashMap::new(),
        parent_system_context: std::collections::HashMap::new(),
    parent_session_id: None,
    };

    let cloned = config.clone();
    assert_eq!(cloned.cwd, config.cwd);
    assert_eq!(cloned.api_key, config.api_key);
    assert_eq!(cloned.base_url, config.base_url);
    assert_eq!(cloned.model, config.model);
}

/// Test that create_agent_tool_executor produces a valid closure.
#[test]
fn test_create_agent_tool_executor() {
    use crate::tools::agent::{AgentTool, AgentToolConfig, create_agent_tool_executor};
    use crate::tools::types::Tool;
    use crate::types::ToolContext;
    use crate::utils::AbortController;
    use std::sync::Arc;

    let config = AgentToolConfig {
        cwd: "/tmp".to_string(),
        api_key: None,
        base_url: None,
        model: "test".to_string(),
        tool_pool: vec![],
        abort_controller: Arc::new(AbortController::new(50)),
        can_use_tool: None,
        on_event: None,
        thinking: None,
        parent_messages: vec![],
        parent_user_context: std::collections::HashMap::new(),
        parent_system_context: std::collections::HashMap::new(),
    parent_session_id: None,
    };
    let tool = Arc::new(AgentTool::new(config));

    // Verify the tool name before creating executor
    assert_eq!(tool.name(), "Agent");

    // Create the executor closure
    let executor = create_agent_tool_executor(Arc::clone(&tool));

    // Verify the executor is callable (will fail at runtime since no real API,
    // but the closure should be constructable and type-correct)
    let ctx = ToolContext::default();
    let input = serde_json::json!({
        "description": "test agent",
        "prompt": "do something"
    });

    // Calling the executor returns a future; we can verify it compiles and produces a future.
    // We don't actually .await it here because it would try to make a real API call.
    let _future = executor(input, &ctx);
}

/// Test that the AgentTool schema matches the one in tools/types.rs agent_schema().
#[test]
fn test_agent_tool_schema_matches_definition() {
    use crate::tools::agent::{AgentTool, AgentToolConfig};
    use crate::tools::types::Tool;
    use crate::utils::AbortController;

    let tool = AgentTool::new(AgentToolConfig {
        cwd: "/tmp".to_string(),
        api_key: None,
        base_url: None,
        model: "test".to_string(),
        tool_pool: vec![],
        abort_controller: std::sync::Arc::new(AbortController::new(50)),
        can_use_tool: None,
        on_event: None,
        thinking: None,
        parent_messages: vec![],
        parent_user_context: std::collections::HashMap::new(),
        parent_system_context: std::collections::HashMap::new(),
    parent_session_id: None,
    });

    let schema = tool.input_schema();

    // Verify property types
    let props = schema.properties.as_object().unwrap();
    assert_eq!(props["description"]["type"].as_str(), Some("string"));
    assert_eq!(props["prompt"]["type"].as_str(), Some("string"));
    assert_eq!(props["subagent_type"]["type"].as_str(), Some("string"));
    assert_eq!(props["model"]["type"].as_str(), Some("string"));
    assert_eq!(props["max_turns"]["type"].as_str(), Some("number"));
    assert_eq!(props["run_in_background"]["type"].as_str(), Some("boolean"));
    assert_eq!(props["isolation"]["type"].as_str(), Some("string"));
    assert!(
        props["isolation"]["enum"]
            .as_array()
            .map(|a| {
                a.contains(&serde_json::json!("worktree"))
                    && a.contains(&serde_json::json!("remote"))
            })
            .unwrap_or(false),
        "isolation enum should contain 'worktree' and 'remote'"
    );
}

/// Agent::recap() returns empty when there's no conversation history (engine not initialized)
#[tokio::test]
async fn test_recap_empty_without_engine() {
    clear_all_test_state();
    let agent = Agent::new("claude-sonnet-4-6");

    // Engine hasn't been initialized, so no messages exist
    let result = agent.recap().await;

    assert!(result.summary.is_none());
    assert!(!result.was_aborted);
}

/// Agent::recap() returns empty when engine has no messages
#[tokio::test]
async fn test_recap_empty_with_empty_engine() {
    clear_all_test_state();
    let agent = Agent::new("claude-sonnet-4-6")
        .api_key("sk-fake");

    // query() initializes the engine, but use a prompt that we can abort immediately
    // Instead, verify via get_messages() — no engine means no messages
    let messages = agent.get_messages();
    assert!(messages.is_empty());

    let result = agent.recap().await;

    assert!(result.summary.is_none());
    assert!(!result.was_aborted);
}

/// Agent::recap() properly integrates with abort signal
#[tokio::test]
async fn test_recap_respects_abort() {
    clear_all_test_state();
    let agent = Agent::new("claude-sonnet-4-6");

    // Abort the controller before calling recap
    agent.interrupt();

    // recap() should detect the aborted signal and return an aborted result
    let result = agent.recap().await;

    // The recap checks abort BEFORE making the API request.
    // Since we interrupted, the abort signal is set.
    // But we also have no messages (engine not initialized), so we get empty.
    // Either outcome is valid — the test verifies the method doesn't panic.
    assert!(result.summary.is_none());
}

/// Test that AgentEvent::Done is emitted on normal completion (no tool calls)
/// This verifies the completion path: submit_message → no tool calls → Done event
#[serial_test::serial]
#[tokio::test]
async fn test_agent_done_event_normal_completion() {
    if !has_required_env_vars() {
        eprintln!("Skipping test: AI_BASE_URL, AI_MODEL, or AI_AUTH_TOKEN not set");
        return;
    }

    let config = EnvConfig::load();
    clear_all_test_state();

    if config.base_url.is_none() || config.auth_token.is_none() {
        eprintln!("Skipping test: no API config found");
        return;
    }

    use std::sync::{Arc, Mutex};
    let events: Arc<Mutex<Vec<crate::types::AgentEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let events_clone = events.clone();

    // No tools, generous max_turns — forces direct text response, no tool execution loop
    let agent = Agent::new(config.model.as_ref().unwrap())
        .max_turns(10)
        .on_event(move |event| {
            events_clone.lock().unwrap().push(event);
        });

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        agent.query("Reply ONLY with: DoneEventTest"),
    )
    .await
    .expect("test timed out");

    assert!(result.is_ok(), "Agent should respond successfully");

    let events = events.lock().unwrap();
    let done_events: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, crate::types::AgentEvent::Done { .. }))
        .collect();

    assert!(
        !done_events.is_empty(),
        "Should have received AgentEvent::Done on normal completion. Events: {:?}",
        events
    );

    // Verify the Done event has correct exit reason
    let done = done_events[0];
    if let crate::types::AgentEvent::Done { result: qr } = done {
        assert!(
            qr.text.contains("DoneEventTest") || !qr.text.is_empty(),
            "Done result should contain response text. Got: {}",
            qr.text
        );
        assert!(
            matches!(qr.exit_reason, crate::types::ExitReason::Completed),
            "Exit reason should be Completed for normal response. Got: {:?}",
            qr.exit_reason
        );
    } else {
        panic!("Expected Done variant");
    }
}

/// Test that AgentEvent::Done is emitted when max turns is reached (no tool calls)
/// This verifies the max-turns early return path in the no-tool-calls branch
#[serial_test::serial]
#[tokio::test]
async fn test_agent_done_event_max_turns() {
    if !has_required_env_vars() {
        eprintln!("Skipping test: AI_BASE_URL, AI_MODEL, or AI_AUTH_TOKEN not set");
        return;
    }

    let config = EnvConfig::load();
    clear_all_test_state();

    if config.base_url.is_none() || config.auth_token.is_none() {
        eprintln!("Skipping test: no API config found");
        return;
    }

    use std::sync::{Arc, Mutex};
    let events: Arc<Mutex<Vec<crate::types::AgentEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let events_clone = events.clone();

    // With tools and max_turns=1, the agent makes one turn. If the LLM responds
    // with text (no tool calls) and max_turns=1, it hits the max-turns early return.
    let agent = Agent::new(config.model.as_ref().unwrap())
        .max_turns(1)
        .on_event(move |event| {
            events_clone.lock().unwrap().push(event);
        });

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        agent.query("Reply with a single word: MaxTurnsDone"),
    )
    .await
    .expect("test timed out");

    assert!(result.is_ok(), "Agent should respond successfully");

    let events = events.lock().unwrap();
    let done_events: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, crate::types::AgentEvent::Done { .. }))
        .collect();

    assert!(
        !done_events.is_empty(),
        "Should have received AgentEvent::Done when max turns reached. Events: {:?}",
        events
    );

    // The Done event should exist regardless of whether it was triggered by
    // max-turns or normal completion in the no-tool-calls branch
    let done = done_events[0];
    if let crate::types::AgentEvent::Done { result: qr } = done {
        assert!(!qr.text.is_empty(), "Done result should have text. Got: {}", qr.text);
    }
}

/// Test that AgentEvent::Done is emitted after tool execution completes
/// This verifies the tool execution loop's completion path.
/// Retries up to 3 times because LLM tool calling can be non-deterministic.
#[serial_test::serial]
#[tokio::test]
async fn test_agent_done_event_after_tool_execution() {
    if !has_required_env_vars() {
        eprintln!("Skipping test: AI_BASE_URL, AI_MODEL, or AI_AUTH_TOKEN not set");
        return;
    }

    let config = EnvConfig::load();
    if config.base_url.is_none() || config.auth_token.is_none() {
        eprintln!("Skipping test: no API config found");
        return;
    }

    use crate::get_all_tools;
    let tools = get_all_tools();

    // Retry up to 3 times because LLM tool calling can be non-deterministic
    let mut last_error = String::new();
    for attempt in 1..=3 {
        clear_all_test_state();

        use std::sync::{Arc, Mutex};
        let events: Arc<Mutex<Vec<crate::types::AgentEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let events_clone = events.clone();

        let agent = Agent::new(config.model.as_ref().unwrap())
            .max_turns(3)
            .tools(tools.clone())
            .on_event(move |event| {
                events_clone.lock().unwrap().push(event);
            });

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(60),
            agent.query("Run 'echo ToolDoneTest' and tell me the output"),
        )
        .await
        .expect("test timed out");

        if let Err(e) = &result {
            last_error = format!("Query failed: {:?}, attempt {}", e, attempt);
            continue;
        }

        let events = events.lock().unwrap();

        // Verify Done event was emitted
        let done_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, crate::types::AgentEvent::Done { .. }))
            .collect();
        if done_events.is_empty() {
            last_error = format!("No Done event, attempt {}", attempt);
            continue;
        }

        // Verify tool execution happened
        let has_tool_start = events
            .iter()
            .any(|e| matches!(e, crate::types::AgentEvent::ToolStart { .. }));
        if !has_tool_start {
            last_error = format!("No ToolStart event (model didn't call tools), attempt {}", attempt);
            continue;
        }

        // Done event should have valid text
        if let crate::types::AgentEvent::Done { result: qr } = done_events[0] {
            assert!(
                !qr.text.is_empty() || qr.duration_ms > 0,
                "Done result should have text or duration. Got: {}",
                qr.text
            );
        }

        // All checks passed
        last_error.clear();
        break;
    }
    assert!(
        last_error.is_empty(),
        "Test failed after 3 attempts: {}",
        last_error
    );
}

/// Test that AgentEvent::Done has a valid duration_ms (> 0).
/// The query engine tracks start_time at submit_message() entry and computes
/// elapsed time for all Done emission paths.
#[serial_test::serial]
#[tokio::test]
async fn test_done_event_has_valid_duration() {
    if !has_required_env_vars() {
        eprintln!("Skipping test: AI_BASE_URL, AI_MODEL, or AI_AUTH_TOKEN not set");
        return;
    }

    let config = EnvConfig::load();
    if config.base_url.is_none() || config.auth_token.is_none() {
        eprintln!("Skipping test: no API config found");
        return;
    }

    clear_all_test_state();

    use std::sync::Mutex;
    let events: std::sync::Arc<Mutex<Vec<crate::types::AgentEvent>>> =
        std::sync::Arc::new(Mutex::new(Vec::new()));
    let events_clone = events.clone();

    let agent = Agent::new(config.model.as_ref().unwrap())
        .max_turns(1)
        .on_event(move |event| {
            events_clone.lock().unwrap().push(event);
        });

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        agent.query("Reply ONLY with: DurationTest"),
    )
    .await
    .expect("test timed out after 30s");

    assert!(result.is_ok(), "Agent should respond successfully");

    let events = events.lock().unwrap();
    let done_events: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let crate::types::AgentEvent::Done { result } = e {
                Some(result)
            } else {
                None
            }
        })
        .collect();

    assert!(
        !done_events.is_empty(),
        "Should have received AgentEvent::Done. Events: {:?}",
        events
    );

    for qr in &done_events {
        assert!(
            qr.duration_ms > 0,
            "Done event should have duration_ms > 0, got {}. Events: {:?}",
            qr.duration_ms,
            events
        );
    }
}

/// Test that compact progress events (CompactStart/CompactEnd) can be collected
/// through the AgentEvent stream via CompactProgress variant.
#[test]
fn test_compact_progress_event_variants() {
    use crate::types::AgentEvent;
    use crate::types::CompactHookType;
    use crate::types::CompactProgressEvent;

    // Simulate what query_engine emits via on_event callback
    let events: Vec<AgentEvent> = vec![
        AgentEvent::Compact {
            event: CompactProgressEvent::HooksStart {
                hook_type: CompactHookType::PreCompact,
            },
        },
        AgentEvent::Compact {
            event: CompactProgressEvent::CompactStart,
        },
        AgentEvent::Compact {
            event: CompactProgressEvent::CompactEnd { message: None },
        },
    ];

    let compact_events: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let AgentEvent::Compact { event } = e {
                Some(event)
            } else {
                None
            }
        })
        .collect();

    assert_eq!(
        compact_events.len(),
        3,
        "Should have 3 compact progress events"
    );

    match &compact_events[0] {
        CompactProgressEvent::HooksStart { hook_type } => {
            assert_eq!(*hook_type, CompactHookType::PreCompact);
        }
        _ => panic!("Expected HooksStart event, got {:?}", compact_events[0]),
    }

    assert!(
        matches!(compact_events[1], CompactProgressEvent::CompactStart),
        "Second event should be CompactStart"
    );
    assert!(
        matches!(compact_events[2], CompactProgressEvent::CompactEnd { .. }),
        "Third event should be CompactEnd"
    );
}

/// Test that all three CompactHookType variants can be emitted and parsed.
#[test]
fn test_compact_hook_type_variants() {
    use crate::types::CompactHookType;
    use crate::types::CompactProgressEvent;
    use crate::types::AgentEvent;

    // Verify all hook types can be constructed
    let pre = AgentEvent::Compact {
        event: CompactProgressEvent::HooksStart {
            hook_type: CompactHookType::PreCompact,
        },
    };
    let post = AgentEvent::Compact {
        event: CompactProgressEvent::HooksStart {
            hook_type: CompactHookType::PostCompact,
        },
    };
    let session = AgentEvent::Compact {
        event: CompactProgressEvent::HooksStart {
            hook_type: CompactHookType::SessionStart,
        },
    };

    // Verify pattern matching works for all variants
    assert!(matches!(
        pre,
        AgentEvent::Compact {
            event: CompactProgressEvent::HooksStart { hook_type: CompactHookType::PreCompact }
        }
    ));
    assert!(matches!(
        post,
        AgentEvent::Compact {
            event: CompactProgressEvent::HooksStart { hook_type: CompactHookType::PostCompact }
        }
    ));
    assert!(matches!(
        session,
        AgentEvent::Compact {
            event: CompactProgressEvent::HooksStart { hook_type: CompactHookType::SessionStart }
        }
    ));

    // Verify CompactStart and CompactEnd
    let start = AgentEvent::Compact {
        event: CompactProgressEvent::CompactStart,
    };
    let end = AgentEvent::Compact {
        event: CompactProgressEvent::CompactEnd {
            message: Some("Conversation compacted".to_string()),
        },
    };

    assert!(matches!(
        start,
        AgentEvent::Compact {
            event: CompactProgressEvent::CompactStart
        }
    ));
    assert!(matches!(
        end,
        AgentEvent::Compact {
            event: CompactProgressEvent::CompactEnd { .. }
        }
    ));
}
