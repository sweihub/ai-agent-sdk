/**
 * Example 1: Simple Query
 *
 * Demonstrates the basic Agent::new() + prompt() flow with streaming events.
 * Tools are automatically available - the model will use them when needed.
 *
 * Run: cargo run --example 01_simple_query
 */
use ai_agent::{Agent, AgentEvent};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Example 1: Simple Query ---\n");

    // Track streaming output
    let needs_newline = Arc::new(AtomicBool::new(false));
    let _needs_newline_clone = needs_newline.clone();

    // Create agent with streaming event callback
    let mut agent = Agent::with_event_callback(
        &std::env::var("AI_MODEL").unwrap_or_else(|_| "claude-sonnet-4-6".to_string()),
        10,
        move |event| {
            match event {
                AgentEvent::Thinking { turn } => {
                    println!("\n=== Turn {} ===", turn);
                }
                AgentEvent::MessageStart { message_id } => {
                    println!("[Stream] Message started: {}", &message_id[..8]);
                }
                AgentEvent::ContentBlockStart { index, block_type } => {
                    print!("[Stream] Block #{} ({}) ", index, block_type);
                }
                AgentEvent::ContentBlockDelta { index: _, delta } => {
                    match delta {
                        ai_agent::ContentDelta::Text { text } => {
                            print!("{}", text);
                            _needs_newline_clone.store(true, Ordering::SeqCst);
                        }
                        ai_agent::ContentDelta::ToolUse { id, name, input, is_complete } => {
                            if is_complete {
                                println!("\n[Tool] '{}' ({}) - COMPLETE", name, &id[..8]);
                            } else {
                                // Show partial input
                                let input_str = serde_json::to_string(&input).unwrap_or_default();
                                println!("\n[Tool] '{}' ({}) - input: {}", name, &id[..8], input_str.chars().take(50).collect::<String>());
                            }
                        }
                        ai_agent::ContentDelta::Thinking { text } => {
                            // Show thinking (hidden by default, visible in verbose mode)
                            print!("[thinking: {}]", text.chars().take(50).collect::<String>());
                        }
                    }
                }
                AgentEvent::ContentBlockStop { index: _ } => {
                    println!(" - stopped");
                }
                AgentEvent::MessageStop => {
                    println!("[Stream] Message stopped");
                }
                AgentEvent::ToolStart { tool_name, .. } => {
                    println!("\n[Tool Execute] {}", tool_name);
                }
                AgentEvent::ToolComplete { tool_name, result, .. } => {
                    println!("[Tool Done] {} - {} chars", tool_name, result.content.len());
                }
                AgentEvent::ToolError { tool_name, tool_call_id: _, error } => {
                    println!("[Tool Error] {} - {}", tool_name, error);
                }
                AgentEvent::Done { result } => {
                    println!("\n=== Done ===");
                    println!("Turns: {}", result.num_turns);
                    println!("Tokens: {} in / {} out", result.usage.input_tokens, result.usage.output_tokens);
                    println!("Duration: {}ms", result.duration_ms);
                }
                AgentEvent::RequestStart => {
                    print!("[Request] ");
                }
                AgentEvent::MaxTurnsReached { max_turns, turn_count } => {
                    println!("\n[Max Turns] Reached {} of {} turns", turn_count, max_turns);
                }
                AgentEvent::Tombstone { message } => {
                    println!("[Tombstone] {}", message.chars().take(100).collect::<String>());
                }
                AgentEvent::StreamRequestEnd => {
                    println!("\n[Request] response received");
                }
                AgentEvent::RateLimitStatus { is_rate_limited, retry_after_secs: _ } => {
                    println!("\n[Rate Limit] {}", if is_rate_limited { "hit" } else { "cleared" });
                }
            }
        },
    );

    // Ask a question that explicitly requires tool use
    let result = agent.query(
        "Use the Glob tool to find all .rs files in the src directory, then tell me how many you found."
    ).await?;

    if needs_newline.load(Ordering::SeqCst) {
        println!();
    }

    println!("\n=== Final Answer ===");
    println!("{}", result.text);

    Ok(())
}