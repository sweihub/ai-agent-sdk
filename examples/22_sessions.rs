/**
 * Example 22: Session Management
 *
 * Demonstrates session persistence features:
 * - Save session to disk
 * - Load session from disk
 * - List all sessions
 * - Fork (copy) a session
 *
 * Run: cargo run --example 22_sessions
 *
 * Environment variables from .env:
 * - AI_BASE_URL: LLM server URL
 * - AI_AUTH_TOKEN: API authentication token
 * - AI_MODEL: Model name (defaults to claude-sonnet-4-6)
 */
use ai_agent::{Agent, EnvConfig, session};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Example 22: Session Management ---\n");

    let config = EnvConfig::load();
    let model = config
        .model
        .clone()
        .unwrap_or_else(|| "claude-sonnet-4-6".to_string());

    println!("Using model: {}\n", model);

    // Create agent
    let agent = Agent::new(&model).max_turns(5);

    // First turn: ask a question
    println!("=== Turn 1: Ask a question ===");
    let result = agent
        .query("What is the capital of France? Answer in one line.")
        .await?;
    println!("{}", result.text);
    println!();

    // Second turn: follow up
    println!("=== Turn 2: Follow up ===");
    let result = agent
        .query("Now name three tourist attractions there.")
        .await?;
    println!("{}", result.text);
    println!();

    // Save session to disk
    let session_id = "demo-session-001";
    println!("=== Saving session as '{}' ===", session_id);

    let metadata = session::SessionMetadata {
        id: session_id.to_string(),
        cwd: std::env::current_dir()?.to_string_lossy().to_string(),
        model: model.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
        updated_at: chrono::Utc::now().to_rfc3339(),
        message_count: agent.get_messages().len() as u32,
        summary: Some("Demo session about France".to_string()),
        tag: Some("demo".to_string()),
    };

    session::save_session(session_id, agent.get_messages().to_vec(), Some(metadata)).await?;
    println!("Session saved successfully!");
    println!();

    // List all sessions
    println!("=== Listing all sessions ===");
    let sessions = session::list_sessions().await?;
    for s in &sessions {
        println!(
            "- {} [{}] - {} messages",
            s.id,
            s.tag.as_deref().unwrap_or("no tag"),
            s.message_count
        );
    }
    println!();

    // Fork the session
    let fork_id = "demo-session-fork";
    println!("=== Forking session as '{}' ===", fork_id);
    session::fork_session(session_id, Some(fork_id.to_string())).await?;
    println!("Session forked successfully!");
    println!();

    // Load the forked session
    println!("=== Loading forked session ===");
    let loaded = session::load_session(fork_id).await?;
    if let Some(data) = loaded {
        println!("Loaded session: {} messages", data.messages.len());
        println!("Summary: {:?}", data.metadata.summary);
        // Show the conversation
        for (i, msg) in data.messages.iter().enumerate() {
            println!(
                "[{}] {:?}: {}",
                i,
                msg.role,
                &msg.content[..msg.content.len().min(80)]
            );
        }
    }
    println!();

    // Load original session into new agent
    println!("=== Loading original session ===");
    let original = session::load_session(session_id).await?;
    if let Some(data) = original {
        println!("Original session: {} messages", data.messages.len());
        // Resume with a follow-up question
        let agent2 = Agent::new(&model).max_turns(3);

        // Note: To truly resume, you'd need to inject messages into the engine
        // For now, we demonstrate the session API works
        let result = agent2.query("What currency does France use?").await?;
        println!("{}", result.text);
    }
    println!();

    println!("=== done ===");
    Ok(())
}
