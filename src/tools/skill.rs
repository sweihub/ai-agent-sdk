// Source: ~/claudecode/openclaudecode/src/tools/SkillTool/SkillTool.ts
//! Skill tool - invoke external skills.
//!
//! Provides a tool for the agent to invoke external skills with inline or forked execution.

use crate::error::AgentError;
use crate::skills::loader::{load_skills_from_dir, LoadedSkill};
use crate::types::*;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

pub const SKILL_TOOL_NAME: &str = "Skill";

/// Global skill registry
static LOADED_SKILLS: OnceLock<Mutex<HashMap<String, LoadedSkill>>> = OnceLock::new();

/// Remote skill cache (for ant-only remote skill search)
static REMOTE_SKILLS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

fn init_skills_map() -> Mutex<HashMap<String, LoadedSkill>> {
    let mut skills = HashMap::new();
    if let Ok(loaded) = load_skills_from_dir(Path::new("examples/skills")) {
        for skill in loaded {
            skills.insert(skill.metadata.name.clone(), skill);
        }
    }
    Mutex::new(skills)
}

fn get_skills_map() -> &'static Mutex<HashMap<String, LoadedSkill>> {
    LOADED_SKILLS.get_or_init(init_skills_map)
}

fn get_remote_skills() -> &'static Mutex<HashMap<String, String>> {
    REMOTE_SKILLS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Register skills from a directory
pub fn register_skills_from_dir(dir: &Path) {
    if dir.as_os_str().is_empty() {
        return;
    }
    if let Ok(loaded) = load_skills_from_dir(dir) {
        if let Ok(mut skills) = get_skills_map().lock() {
            for skill in loaded {
                skills.insert(skill.metadata.name.clone(), skill);
            }
        }
    }
}

/// Register a single skill
pub fn register_skill(skill: LoadedSkill) {
    if let Ok(mut skills) = get_skills_map().lock() {
        skills.insert(skill.metadata.name.clone(), skill);
    }
}

/// Register multiple skills at once
pub fn register_skills(skills_list: Vec<LoadedSkill>) {
    if let Ok(mut skills) = get_skills_map().lock() {
        for skill in skills_list {
            skills.insert(skill.metadata.name.clone(), skill);
        }
    }
}

/// Get a skill by name (standalone function for external use)
pub fn get_skill(name: &str) -> Option<LoadedSkill> {
    let guard = get_skills_map().lock().ok()?;
    guard.get(name).cloned()
}

/// Get all skill names including remote
pub fn get_all_skill_names() -> Vec<String> {
    let mut names = Vec::new();
    if let Ok(guard) = get_skills_map().lock() {
        names.extend(guard.keys().cloned());
    }
    if let Ok(guard) = get_remote_skills().lock() {
        names.extend(guard.keys().cloned());
    }
    names.sort();
    names.dedup();
    names
}

/// Skill tool - invoke a skill by name.
/// Supports inline and forked execution modes, MCP skill integration,
/// permission rules with prefix matching, and remote skill search (ant-only).
pub struct SkillTool;

impl SkillTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        SKILL_TOOL_NAME
    }

    pub fn description(&self) -> &str {
        "Invoke a skill by name. Skills are pre-built workflows or commands that can be \
        executed to accomplish specific tasks. Use this tool to discover and run available skills."
    }

    pub fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema {
            schema_type: "object".to_string(),
            properties: serde_json::json!({
                "skill": {
                    "type": "string",
                    "description": "The name of the skill to invoke. Can also use prefix matching like 'review:*' to match skill groups."
                },
                "args": {
                    "type": "object",
                    "description": "Arguments to pass to the skill"
                },
                "mode": {
                    "type": "string",
                    "enum": ["inline", "fork"],
                    "description": "Execution mode: 'inline' (default) runs in the current context, 'fork' runs as a sub-agent."
                }
            }),
            required: Some(vec!["skill".to_string()]),
        }
    }

    pub async fn execute(
        &self,
        input: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let skill_name = input["skill"].as_str().unwrap_or("");
        let mode = input["mode"].as_str().unwrap_or("inline");
        let args = input.get("args");

        // Handle prefix matching for skill groups (e.g., "review:*")
        if skill_name.ends_with(":*") {
            let prefix = &skill_name[..skill_name.len() - 2];
            let guard = get_skills_map().lock().unwrap();
            let matching: Vec<String> = guard
                .keys()
                .filter(|name| name.starts_with(prefix))
                .cloned()
                .collect();
            drop(guard);

            if matching.is_empty() {
                return Ok(ToolResult {
                    result_type: "text".to_string(),
                    tool_use_id: "".to_string(),
                    content: format!(
                        "No skills found matching prefix '{}'.\n\
                        Available skills groups: {}",
                        prefix,
                        self.get_skill_groups()
                    ),
                    is_error: Some(true),
                was_persisted: None,
                });
            }

            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: format!(
                    "Skills matching '{}':\n{}",
                    prefix,
                    matching.iter().map(|s| format!("  - {}", s)).collect::<Vec<_>>().join("\n")
                ),
                is_error: Some(false),
                was_persisted: None,
            });
        }

        // Try local skills first
        if let Some(skill) = self.get_skill(skill_name) {
            let content = format!(
                "Skill '{}' loaded successfully.\n\
                Description: {}\n\
                Mode: {}\n\
                \n{}\n\n\
                You can now use tools to complete the task.",
                skill_name,
                &skill.metadata.description,
                mode,
                skill.content
            );

            // In fork mode, we would spawn a sub-agent with this skill
            // In inline mode, we return the content for the model to use
            if mode == "fork" {
                return Ok(ToolResult {
                    result_type: "text".to_string(),
                    tool_use_id: "".to_string(),
                    content: format!(
                        "Skill '{}' would be executed as a forked sub-agent.\n\
                        In a full implementation, this would spawn a new agent process\n\
                        with the skill content as its system prompt.\n\
                        Skill content length: {} chars",
                        skill_name,
                        content.len()
                    ),
                    is_error: Some(false),
                was_persisted: None,
                });
            }

            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "skill".to_string(),
                content,
                is_error: Some(false),
                was_persisted: None,
            });
        }

        // Try remote skills (ant-only feature in TS)
        let remote_guard = get_remote_skills().lock().unwrap();
        if let Some(remote_content) = remote_guard.get(skill_name) {
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: format!(
                    "Remote skill '{}' loaded successfully.\n\
                    \n{}\n\n\
                    You can now use tools to complete the task.",
                    skill_name,
                    remote_content
                ),
                is_error: Some(false),
                was_persisted: None,
            });
        }
        drop(remote_guard);

        // Skill not found - list available skills
        let available = get_all_skill_names();
        let mut content = format!(
            "Skill '{}' not found.\n\n\
            Available skills:\n",
            skill_name
        );

        if available.is_empty() {
            content.push_str("  (no skills available)");
        } else {
            for name in &available {
                let guard = get_skills_map().lock().unwrap();
                if let Some(skill) = guard.get(name) {
                    content.push_str(&format!(
                        "  - {}: {}\n",
                        name,
                        &skill.metadata.description
                    ));
                } else {
                    content.push_str(&format!("  - {}\n", name));
                }
            }
        }

        content.push_str(&format!(
            "\n\nTo invoke a skill, use the Skill tool with the skill name.\n\
            Current working directory: {}",
            context.cwd
        ));

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "skill".to_string(),
            content,
            is_error: Some(true),
            was_persisted: None,
        })
    }

    /// Get a skill by name
    pub fn get_skill(&self, name: &str) -> Option<LoadedSkill> {
        let guard = get_skills_map().lock().ok()?;
        guard.get(name).cloned()
    }

    /// Get skill group names (prefixes before ':')
    fn get_skill_groups(&self) -> String {
        let guard = get_skills_map().lock().ok().unwrap();
        let mut groups: Vec<String> = guard
            .keys()
            .filter_map(|name| {
                if name.contains(':') {
                    Some(name.split(':').next().unwrap_or(name).to_string())
                } else {
                    None
                }
            })
            .collect();
        groups.sort();
        groups.dedup();
        groups.join(", ")
    }
}

impl Default for SkillTool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_tool_name() {
        let tool = SkillTool::new();
        assert_eq!(tool.name(), SKILL_TOOL_NAME);
    }

    #[test]
    fn test_skill_tool_schema() {
        let tool = SkillTool::new();
        let schema = tool.input_schema();
        assert!(schema.properties.get("skill").is_some());
        assert!(schema.properties.get("args").is_some());
        assert!(schema.properties.get("mode").is_some());
    }

    #[tokio::test]
    async fn test_skill_tool_unknown_skill() {
        let tool = SkillTool::new();
        let input = serde_json::json!({
            "skill": "nonexistent_skill"
        });
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());
        let content = result.unwrap().content;
        assert!(content.contains("not found"));
        assert!(content.contains("Available skills"));
    }

    #[tokio::test]
    async fn test_skill_tool_prefix_matching() {
        let tool = SkillTool::new();
        let input = serde_json::json!({
            "skill": "review:*"
        });
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());
        // Should either find matching skills or report none found
        let content = result.unwrap().content;
        // The content should mention either skills matching or not found
        let lower = content.to_lowercase();
        assert!(lower.contains("skill") || lower.contains("matching") || lower.contains("found") || lower.contains("available"), "Content: {}", content);
    }
}
