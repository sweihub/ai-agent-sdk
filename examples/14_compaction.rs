/**
 * Example 14: Memory Compaction Demo
 *
 * Demonstrates automatic context compaction when conversation gets too long.
 * The agent summarizes old messages to free up context space.
 *
 * Run: cargo run --example 14_compaction
 *
 * Environment variables from .env:
 * - AI_BASE_URL: LLM server URL
 * - AI_AUTH_TOKEN: API authentication token
 * - AI_MODEL: Model name
 */
use ai_agent::{Agent, EnvConfig, get_auto_compact_threshold};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Example 14: Memory Compaction Demo ---\n");

    // Load config from .env
    let config = EnvConfig::load();
    let model = config
        .model
        .unwrap_or_else(|| "claude-sonnet-4-6".to_string());

    println!("Using model: {}", model);
    println!(
        "Auto-compact threshold: {} tokens\n",
        get_auto_compact_threshold(&model)
    );

    let agent = Agent::new(&model).max_turns(10);

    // A paragraph to repeat - about 100 tokens each time
    let paragraph = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. \
Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. \
Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris. \
Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore. \
Eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident. \
Sunt in culpa qui officia deserunt mollit anim id est laborum.";

    // Send multiple prompts to build up context
    for i in 1..=5 {
        println!("--- Turn {} ---", i);

        let prompt = format!(
            "Turn {}: Remember this: {}. Just say 'OK turn {}' if you understand.",
            i, paragraph, i
        );

        let result = agent.query(&prompt).await?;

        // Show message count
        println!("Messages: {}", agent.get_messages().len());
        println!(
            "Response: {}\n",
            result.text.trim().lines().next().unwrap_or("")
        );
    }

    // Now ask about early content to trigger recall
    println!("--- Final Query ---");
    let result = agent
        .query("What did I ask you to remember in turn 1?")
        .await?;
    println!("Messages: {}", agent.get_messages().len());
    println!(
        "Response: {}\n",
        result.text.trim().lines().next().unwrap_or("")
    );

    println!("=== done ===");

    Ok(())
}
