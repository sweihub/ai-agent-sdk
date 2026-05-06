// Source: ~/claudecode/openclaudecode/src/utils/hooks/skillImprovement.ts
#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::types::Message;
use crate::utils::hooks::api_query_hook_helper::{
    ApiQueryHookConfig, ReplHookContext, create_api_query_hook,
};
use crate::utils::hooks::post_sampling_hooks::register_post_sampling_hook;

/// Number of user messages between each skill improvement analysis
const TURN_BATCH_SIZE: usize = 5;

/// A skill update suggestion
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SkillUpdate {
    pub section: String,
    pub change: String,
    pub reason: String,
}

/// Skill improvement suggestion
#[derive(Debug, Clone)]
pub struct SkillImprovementSuggestion {
    pub skill_name: String,
    pub updates: Vec<SkillUpdate>,
}

/// State for skill improvement tracking
struct SkillImprovementState {
    last_analyzed_count: usize,
    last_analyzed_index: usize,
}

lazy_static::lazy_static! {
    static ref SKILL_IMPROVEMENT_STATE: Arc<Mutex<SkillImprovementState>> = Arc::new(Mutex::new(
        SkillImprovementState {
            last_analyzed_count: 0,
            last_analyzed_index: 0,
        }
    ));
}

/// Find the project skill (simplified)
fn find_project_skill() -> Option<ProjectSkillInfo> {
    // In the TS version, this calls getInvokedSkillsForAgent
    // and looks for skills starting with "projectSettings:"
    None
}

/// Project skill information
struct ProjectSkillInfo {
    skill_name: String,
    skill_path: String,
    content: String,
}

/// Format recent messages for the skill improvement prompt
fn format_recent_messages(messages: &[Message]) -> String {
    messages
        .iter()
        .filter(|m| m.is_user() || m.is_assistant())
        .map(|m| {
            let role = if m.is_user() { "User" } else { "Assistant" };
            let content = m.content.chars().take(500).collect::<String>();
            format!("{}: {}", role, content)
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Count messages matching a predicate
fn count_messages<F>(messages: &[Message], predicate: F) -> usize
where
    F: Fn(&Message) -> bool,
{
    messages.iter().filter(|m| predicate(m)).count()
}

/// Create the skill improvement hook
fn create_skill_improvement_hook() -> Arc<
    dyn Fn(ReplHookContext) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
        + Send
        + Sync,
> {
    let config: ApiQueryHookConfig<Vec<SkillUpdate>> = ApiQueryHookConfig {
        name: "skill_improvement".to_string(),
        should_run: Box::new(|context| {
            let query_source = context.query_source.clone();
            let messages = context.messages.clone();
            Box::pin(async move {
                // Only run for main REPL thread
                if query_source
                    .as_ref()
                    .map(|s| s != "repl_main_thread")
                    .unwrap_or(true)
                {
                    return false;
                }

                // Only run if there's a project skill
                if find_project_skill().is_none() {
                    return false;
                }

                // Only run every TURN_BATCH_SIZE user messages
                let mut state = SKILL_IMPROVEMENT_STATE.lock().unwrap();
                let user_count = count_messages(&messages, |m| m.is_user());
                if user_count - state.last_analyzed_count < TURN_BATCH_SIZE {
                    return false;
                }

                state.last_analyzed_count = user_count;
                true
            })
        }),
        build_messages: Box::new(|context| {
            let project_skill = match find_project_skill() {
                Some(s) => s,
                None => return Vec::new(),
            };

            let mut state = SKILL_IMPROVEMENT_STATE.lock().unwrap();
            // Only analyze messages since the last check
            let new_messages = context.messages[state.last_analyzed_index..].to_vec();
            state.last_analyzed_index = context.messages.len();

            let formatted = format_recent_messages(&new_messages);

            let prompt = format!(
                r#"You are analyzing a conversation where a user is executing a skill (a repeatable process).
Your job: identify if the user's recent messages contain preferences, requests, or corrections that should be permanently added to the skill definition for future runs.

<skill_definition>
{}
</skill_definition>

<recent_messages>
{}
</recent_messages>

Look for:
- Requests to add, change, or remove steps: "can you also ask me X", "please do Y too", "don't do Z"
- Preferences about how steps should work: "ask me about energy levels", "note the time", "use a casual tone"
- Corrections: "no, do X instead", "always use Y", "make sure to..."

Ignore:
- Routine conversation that doesn't generalize (one-time answers, chitchat)
- Things the skill already does

Output a JSON array inside <updates> tags. Each item: {{"section": "which step/section to modify or 'new step'", "change": "what to add/modify", "reason": "which user message prompted this"}}.
Output <updates>[]</updates> if no updates are needed."#,
                project_skill.content, formatted
            );

            vec![Message {
                role: crate::types::api_types::MessageRole::User,
                content: prompt,
                attachments: None,
                tool_call_id: None,
                tool_calls: None,
                is_error: None,
                is_meta: None,
                is_api_error_message: None,
                error_details: None,
                uuid: None,
                timestamp: None,
            }]
        }),
        system_prompt: None,
        use_tools: Some(false),
        parse_response: Box::new(|content, _context| {
            // Extract content between <updates> tags
            if let Some(updates_str) = extract_tag(content, "updates") {
                match serde_json::from_str::<Vec<SkillUpdate>>(&updates_str) {
                    Ok(updates) => updates,
                    Err(_) => Vec::new(),
                }
            } else {
                Vec::new()
            }
        }),
        log_result: Box::new(|result, context| {
            if let crate::utils::hooks::api_query_hook_helper::ApiQueryResult::Success {
                result: updates,
                uuid,
                ..
            } = result
            {
                if !updates.is_empty() {
                    let project_skill = find_project_skill();
                    let skill_name = project_skill
                        .as_ref()
                        .map(|s| s.skill_name.clone())
                        .unwrap_or_else(|| "unknown".to_string());

                    log_event(
                        "tengu_skill_improvement_detected",
                        &serde_json::json!({
                            "updateCount": updates.len(),
                            "uuid": uuid,
                            "skill_name": skill_name,
                        }),
                    );

                    // Update app state with suggestion
                    // This would set context.tool_use_context.setAppState
                    log::debug!(
                        "Skill improvement detected for '{}': {} updates",
                        skill_name,
                        updates.len()
                    );
                }
            }
        }),
        get_model: Box::new(|_context| get_small_fast_model()),
    };

    let boxed_hook = create_api_query_hook(config);
    Arc::from(boxed_hook)
}

/// Initialize skill improvement hook
pub fn init_skill_improvement() {
    // Check feature flags (simplified - would use GrowthBook in production)
    let skill_improvement_enabled = true; // feature('SKILL_IMPROVEMENT')
    let copper_panda_enabled = false; // getFeatureValue_CACHED_MAY_BE_STALE('tengu_copper_panda', false)

    if skill_improvement_enabled && copper_panda_enabled {
        let hook = create_skill_improvement_hook();
        register_post_sampling_hook(hook);
    }
}

/// Apply skill improvements by calling a side-channel LLM to rewrite the skill file.
/// Fire-and-forget - does not block the main conversation.
pub async fn apply_skill_improvement(skill_name: &str, updates: &[SkillUpdate]) {
    if skill_name.is_empty() {
        return;
    }

    // Skills live at .claude/skills/<name>/SKILL.md relative to CWD
    let cwd = std::env::current_dir().unwrap_or_default();
    let file_path = cwd
        .join(".claude")
        .join("skills")
        .join(skill_name)
        .join("SKILL.md");

    let current_content = match tokio::fs::read_to_string(&file_path).await {
        Ok(content) => content,
        Err(_) => {
            log::error!("Failed to read skill file for improvement: {:?}", file_path);
            return;
        }
    };

    let update_list: String = updates
        .iter()
        .map(|u| format!("- {}: {}", u.section, u.change))
        .collect::<Vec<_>>()
        .join("\n");

    let prompt = format!(
        r#"You are editing a skill definition file. Apply the following improvements to the skill.

<current_skill_file>
{}
</current_skill_file>

<improvements>
{}
</improvements>

Rules:
- Integrate the improvements naturally into the existing structure
- Preserve frontmatter (--- block) exactly as-is
- Preserve the overall format and style
- Do not remove existing content unless an improvement explicitly replaces it
- Output the complete updated file inside <updated_file> tags"#,
        current_content, update_list
    );

    // This would call the LLM to apply the improvements
    // For now, just log
    log::debug!(
        "Would apply skill improvements for '{}': {}",
        skill_name,
        update_list
    );
}

/// Extract content between XML-style tags
fn extract_tag(content: &str, tag_name: &str) -> Option<String> {
    let open_tag = format!("<{}>", tag_name);
    let close_tag = format!("</{}>", tag_name);

    if let Some(start) = content.find(&open_tag) {
        let content_start = start + open_tag.len();
        if let Some(end) = content[content_start..].find(&close_tag) {
            return Some(content[content_start..content_start + end].to_string());
        }
    }
    None
}

/// Get the small fast model
fn get_small_fast_model() -> String {
    "claude-3-haiku-20240307".to_string()
}

/// Log event for analytics (simplified)
fn log_event(event_name: &str, metadata: &serde_json::Value) {
    log::debug!("Analytics event: {} - {:?}", event_name, metadata);
}

/// Message extension methods
impl Message {
    fn is_user(&self) -> bool {
        matches!(self.role, crate::types::api_types::MessageRole::User)
    }

    fn is_assistant(&self) -> bool {
        matches!(self.role, crate::types::api_types::MessageRole::Assistant)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_tag() {
        let content = "Some text <updates>[{\"section\": \"test\", \"change\": \"add\", \"reason\": \"because\"}]</updates> more text";
        let result = extract_tag(content, "updates");
        assert!(result.is_some());
        let updates = result.unwrap();
        assert!(updates.contains("section"));
    }

    #[test]
    fn test_extract_tag_empty() {
        let content = "<updates>[]</updates>";
        let result = extract_tag(content, "updates");
        assert_eq!(result, Some("[]".to_string()));
    }

    #[test]
    fn test_extract_tag_not_found() {
        let content = "No tags here";
        let result = extract_tag(content, "updates");
        assert!(result.is_none());
    }

    #[test]
    fn test_format_recent_messages() {
        let messages = vec![
            Message {
                content: "Hello".to_string(),
                ..Default::default()
            },
            Message {
                content: "Hi there".to_string(),
                ..Default::default()
            },
        ];
        let result = format_recent_messages(&messages);
        // Would contain "User: Hello" and "User: Hi there"
        assert!(result.contains("Hello"));
    }

    #[test]
    fn test_count_messages() {
        let messages = vec![
            Message {
                content: "msg1".to_string(),
                ..Default::default()
            },
            Message {
                content: "msg2".to_string(),
                ..Default::default()
            },
            Message {
                content: "msg3".to_string(),
                ..Default::default()
            },
        ];
        let count = count_messages(&messages, |_| true);
        assert_eq!(count, 3);
    }
}
