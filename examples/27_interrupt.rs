/// Example 27: Interrupting Agent Execution
///
/// Demonstrates how to cancel a running agent loop from another task using
/// `agent.interrupt()`. This is useful for building responsive UIs, implementing
/// timeouts, or allowing users to stop long-running agent operations.
///
/// Run: cargo run --example 27_interrupt

use ai_agent::Agent;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Example 27: Interrupting Agent Execution ---\n");

    let model = std::env::var("AI_MODEL").unwrap_or_else(|_| "claude-sonnet-4-6".to_string());

    // Wrap agent in Arc<Mutex<>> for shared ownership across async tasks.
    // tokio::sync::Mutex allows .await locking for &mut self methods like prompt().
    let agent = Arc::new(Mutex::new(Agent::new(&model, 10)));

    // Spawn a background task that sends the interrupt signal after 3 seconds.
    // interrupt() takes &self, so Arc::clone is sufficient.
    let interrupt_agent = Arc::clone(&agent);
    let interrupt_task = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(3)).await;
        println!("\n[Task] Calling interrupt()\n");
        interrupt_agent.lock().await.interrupt();
    });

    // Run the agent prompt with exclusive mutable access.
    let result = {
        let mut ag = agent.lock().await;
        ag.prompt("List 10 files in the current directory, then read each of them").await
    };

    // Wait for the interrupt task to complete (it's a no-op after interrupt())
    let _ = tokio::time::timeout(Duration::from_secs(5), interrupt_task).await;

    match result {
        Ok(resp) => {
            println!("[Agent] Prompt completed: {} turns, {} output tokens",
                resp.num_turns, resp.usage.output_tokens);
        }
        Err(ai_agent::error::AgentError::UserAborted) => {
            println!("[Agent] Prompt was interrupted (UserAborted)!");
        }
        Err(e) => {
            println!("[Agent] Prompt errored: {:?}", e);
        }
    }

    println!("\n--- Example complete ---");
    Ok(())
}
