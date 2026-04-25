/**
 * Example 26: Conversation Memory
 *
 * Demonstrates that the agent maintains context across multiple query() calls.
 * The agent remembers information from previous turns in the conversation.
 *
 * Run: cargo run --example 26_conversation_memory
 *
 * Environment variables from .env:
 * - AI_BASE_URL: LLM server URL
 * - AI_AUTH_TOKEN: API authentication token
 * - AI_MODEL: Model name
 */
use ai_agent::Agent;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Example 26: Conversation Memory ---\n");

    // Create agent with default configuration from .env
    let agent =
        Agent::new(&std::env::var("AI_MODEL").unwrap_or_else(|_| "claude-sonnet-4-6".to_string()))
            .max_turns(10);

    println!("=== Turn 1: Tell the agent my favorite color ===\n");
    let result1 = agent
        .query("My favorite color is blue. Remember this.")
        .await?;

    println!("Agent: {}\n", result1.text.trim());

    // Verify message history is accumulating
    let messages = agent.get_messages();
    println!("(Message history: {} messages so far)\n", messages.len());

    println!("=== Turn 2: Ask the agent what my favorite color is ===\n");
    let result2 = agent.query("What is my favorite color?").await?;

    println!("Agent: {}\n", result2.text.trim());

    // Final message count
    let final_messages = agent.get_messages();
    println!("(Final message count: {} messages)", final_messages.len());

    // Verify the response mentions "blue"
    if result2.text.to_lowercase().contains("blue") {
        println!("\n✓ SUCCESS: Agent remembered that your favorite color is blue!");
    } else {
        println!("\n✗ FAILURE: Agent did not remember the color blue.");
    }

    println!("\n=== done ===");
    Ok(())
}
