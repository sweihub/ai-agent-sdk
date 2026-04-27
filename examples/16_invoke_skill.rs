/**
 * Example 15: Invoke Skill Demo
 *
 * Demonstrates how to invoke a custom skill.
 *
 * Run: cargo run --example 15_invoke_skill
 *
 * Environment variables from .env:
 * - AI_BASE_URL: LLM server URL
 * - AI_AUTH_TOKEN: API authentication token
 * - AI_MODEL: Model name
 */
use ai_agent::skills::{get_bundled_skills, init_bundled_skills, invoke_skill};

// Register the custom sing skill
mod sing {
    pub mod register_sing_skill {
        use ai_agent::AgentError;
        use ai_agent::skills::bundled_skills::{
            BundledSkillDefinition, ContentBlock, SkillContext, register_bundled_skill,
        };

        fn get_prompt_for_command(
            _args: &str,
            _context: &SkillContext,
        ) -> Result<Vec<ContentBlock>, AgentError> {
            Ok(vec![ContentBlock::Text {
                text: "Jingle Bells Jingle Bells Jingle All The Way ...".to_string(),
            }])
        }

        pub fn register() {
            let _ = register_bundled_skill(BundledSkillDefinition {
                name: "sing".to_string(),
                description: "Output a test song".to_string(),
                aliases: None,
                when_to_use: None,
                argument_hint: None,
                allowed_tools: None,
                model: None,
                disable_model_invocation: None,
                user_invocable: Some(true),
                is_enabled: None,
                hooks: None,
                context: None,
                agent: None,
                files: None,
                get_prompt_for_command: std::sync::Arc::new(get_prompt_for_command),
            });
        }
    }
}

fn main() {
    println!("--- Example 15: Invoke Skill Demo ---\n");

    // Initialize bundled skills
    init_bundled_skills();

    // Register custom sing skill
    sing::register_sing_skill::register();

    // List all registered skills
    let skills = get_bundled_skills();
    println!("Registered skills:");
    for skill in &skills {
        println!("  - {}: {}", skill.name, skill.description);
    }
    println!();

    // Invoke the sing skill
    println!("Invoking /sing skill...\n");

    match invoke_skill("sing", "") {
        Ok(blocks) => {
            for block in blocks {
                match block {
                    ai_agent::skills::bundled_skills::ContentBlock::Text { text } => {
                        println!("Output: {}", text);
                    }
                    _ => {}
                }
            }
        }
        Err(e) => {
            println!("Error: {}", e);
        }
    }

    println!("\n=== done ===");
}
