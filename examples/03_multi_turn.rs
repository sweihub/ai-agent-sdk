/**
 * Example 3: Multi-Turn Conversation
 *
 * Demonstrates session persistence across multiple turns.
 * The agent remembers context from previous interactions.
 *
 * Run: cargo run --example 03_multi_turn
 */
use ai_agent::Agent;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Example 3: Multi-Turn Conversation ---\n");

    let agent =
        Agent::new(&std::env::var("AI_MODEL").unwrap_or_else(|_| "claude-sonnet-4-6".to_string()))
            .max_turns(5);

    // Turn 1: Create a file
    println!("> Turn 1: Create a file");
    let r1 = agent
        .query(
            "Use Bash to run: echo \"Hello Open Agent SDK\" > /tmp/oas-test.txt. Confirm briefly.",
        )
        .await?;
    println!("  {}\n", r1.text);

    // Turn 2: Read back (should remember context)
    println!("> Turn 2: Read the file back");
    let r2 = agent
        .query("Read the file you just created and tell me its contents.")
        .await?;
    println!("  {}\n", r2.text);

    // Turn 3: Clean up
    println!("> Turn 3: Cleanup");
    let r3 = agent.query("Delete that file with Bash. Confirm.").await?;
    println!("  {}\n", r3.text);

    println!("Session history: {} messages", agent.get_messages().len());

    Ok(())
}
