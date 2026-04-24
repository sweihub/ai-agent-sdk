// Source: ~/claudecode/openclaudecode/src/tools/SkillTool/SkillTool.ts
//! Skill tool - invoke external skills.
//!
//! Provides a tool for the agent to invoke external skills with inline or forked execution.

use crate::error::AgentError;
use crate::skills::loader::{LoadedSkill, load_skills_from_dir};
use crate::types::*;
use crate::utils::cwd::get_cwd;
use regex::Regex;
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
    if let Ok(loaded) = load_skills_from_dir(Path::new("examples/skills"), &get_cwd()) {
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

/// Regex for triple-brace argument placeholders: `{{{name}}}`
fn argument_pattern() -> &'static Regex {
    lazy_static::lazy_static! {
        static ref ARG_PATTERN: Regex = Regex::new(r"\{\{\{(\w+)\}\}\}").unwrap();
    }
    &ARG_PATTERN
}

/// Parse argument names from skill content.
///
/// Finds all `{{{name}}}` patterns and returns a deduplicated list
/// in first-occurrence order.
pub fn parse_argument_names(skill_content: &str) -> Vec<String> {
    let mut seen = HashMap::new();
    let mut names = Vec::new();
    for cap in argument_pattern().captures_iter(skill_content) {
        if let Some(name) = cap.get(1).map(|m| m.as_str().to_string()) {
            if seen.insert(name.clone(), ()).is_none() {
                names.push(name);
            }
        }
    }
    names
}

/// Substitute `{{{arg_name}}}` placeholders in content with values from the args map.
///
/// Placeholders whose key is not present in the map are left unchanged.
pub fn substitute_arguments(content: &str, args: &HashMap<String, String>) -> String {
    if args.is_empty() {
        return content.to_string();
    }
    argument_pattern()
        .replace_all(content, |cap: &regex::Captures| {
            let key = cap[1].to_string();
            if let Some(val) = args.get(&key) {
                val.clone()
            } else {
                // Leave unchanged if not in map
                cap[0].to_string()
            }
        })
        .to_string()
}

/// Register skills from a directory
pub fn register_skills_from_dir(dir: &Path) {
    if dir.as_os_str().is_empty() {
        return;
    }
    if let Ok(loaded) = load_skills_from_dir(dir, &get_cwd()) {
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

        // Parse args into a HashMap for argument substitution
        let args_map: HashMap<String, String> = if let Some(args_obj) = input.get("args") {
            if let Some(obj) = args_obj.as_object() {
                obj.iter()
                    .filter_map(|(k, v)| {
                        let val = v.as_str().unwrap_or("").to_string();
                        Some((k.clone(), val))
                    })
                    .collect()
            } else {
                HashMap::new()
            }
        } else {
            HashMap::new()
        };

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
                    matching
                        .iter()
                        .map(|s| format!("  - {}", s))
                        .collect::<Vec<_>>()
                        .join("\n")
                ),
                is_error: Some(false),
                was_persisted: None,
            });
        }

        // Try local skills first
        if let Some(skill) = self.get_skill(skill_name) {
            let substituted_content = substitute_arguments(&skill.content, &args_map);
            let content = format!(
                "Skill '{}' loaded successfully.\n\
                Description: {}\n\
                Mode: {}\n\
                \n{}\n\n\
                You can now use tools to complete the task.",
                skill_name, &skill.metadata.description, mode, substituted_content
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
            let substituted_remote = substitute_arguments(remote_content, &args_map);
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: format!(
                    "Remote skill '{}' loaded successfully.\n\
                    \n{}\n\n\
                    You can now use tools to complete the task.",
                    skill_name, substituted_remote
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
                    content.push_str(&format!("  - {}: {}\n", name, &skill.metadata.description));
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

/// Reset the global skill registries for test isolation.
pub fn reset_skills_for_testing() {
    {
        let guard = get_skills_map().lock().unwrap();
        drop(guard);
    }
    // LOADED_SKILLS is initialized once and can't be reset without unsafe
    // Just clear any accumulated skills by reinitializing the inner map
    if let Ok(mut skills) = get_skills_map().lock() {
        // Re-init from directory
        skills.clear();
        if let Ok(loaded) = crate::skills::loader::load_skills_from_dir(std::path::Path::new("examples/skills"), &std::env::current_dir().unwrap_or_default()) {
            for skill in loaded {
                skills.insert(skill.metadata.name.clone(), skill);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::skills::loader::SkillMetadata;
    use crate::tests::common::clear_all_test_state;

    #[test]
    fn test_skill_tool_name() {
        clear_all_test_state();
        let tool = SkillTool::new();
        assert_eq!(tool.name(), SKILL_TOOL_NAME);
    }

    #[test]
    fn test_skill_tool_schema() {
        clear_all_test_state();
        let tool = SkillTool::new();
        let schema = tool.input_schema();
        assert!(schema.properties.get("skill").is_some());
        assert!(schema.properties.get("args").is_some());
        assert!(schema.properties.get("mode").is_some());
    }

    #[tokio::test]
    async fn test_skill_tool_unknown_skill() {
        clear_all_test_state();
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
        clear_all_test_state();
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
        assert!(
            lower.contains("skill")
                || lower.contains("matching")
                || lower.contains("found")
                || lower.contains("available"),
            "Content: {}",
            content
        );
    }

    // --- Argument substitution tests ---

    #[test]
    fn test_parse_argument_names_single() {
        let names = parse_argument_names("Review the file {{{filename}}} for issues");
        assert_eq!(names, vec!["filename"]);
    }

    #[test]
    fn test_parse_argument_names_multiple() {
        let content = "Review {{{file}}} using {{{language}}}. Output to {{{report}}}.";
        let names = parse_argument_names(content);
        assert_eq!(names, vec!["file", "language", "report"]);
    }

    #[test]
    fn test_parse_argument_names_deduplicates() {
        let content = "{{{file}}} is the input. Process {{{file}}} and return result.";
        let names = parse_argument_names(content);
        assert_eq!(names, vec!["file"]);
    }

    #[test]
    fn test_parse_argument_names_preserves_order() {
        let content = "Do X with {{{first}}}, then Y with {{{second}}}, then Z with {{{first}}} again.";
        let names = parse_argument_names(content);
        assert_eq!(names, vec!["first", "second"]);
    }

    #[test]
    fn test_parse_argument_names_empty() {
        let names = parse_argument_names("No placeholders here");
        assert!(names.is_empty());
    }

    #[test]
    fn test_parse_argument_names_with_underscores_and_digits() {
        let content = "Use {{{my_file_2}}} and {{{report_v3}}}.";
        let names = parse_argument_names(content);
        assert_eq!(names, vec!["my_file_2", "report_v3"]);
    }

    #[test]
    fn test_substitute_arguments_complete_map() {
        let content = "Review {{{file}}} in {{{language}}}.";
        let mut args = HashMap::new();
        args.insert("file".to_string(), "main.rs".to_string());
        args.insert("language".to_string(), "Rust".to_string());
        let result = substitute_arguments(content, &args);
        assert_eq!(result, "Review main.rs in Rust.");
    }

    #[test]
    fn test_substitute_arguments_partial_map() {
        let content = "Review {{{file}}} in {{{language}}}.";
        let mut args = HashMap::new();
        args.insert("file".to_string(), "main.rs".to_string());
        // language is missing
        let result = substitute_arguments(content, &args);
        assert_eq!(result, "Review main.rs in {{{language}}}.");
    }

    #[test]
    fn test_substitute_arguments_empty_map() {
        let content = "Review {{{file}}} in {{{language}}}.";
        let args = HashMap::new();
        let result = substitute_arguments(content, &args);
        assert_eq!(result, "Review {{{file}}} in {{{language}}}.");
    }

    #[test]
    fn test_substitute_arguments_value_with_special_regex_chars() {
        let content = "Process {{{file}}}.";
        let mut args = HashMap::new();
        // Value contains regex special characters
        args.insert("file".to_string(), "src/main.rs (v1.0).txt".to_string());
        let result = substitute_arguments(content, &args);
        assert_eq!(result, "Process src/main.rs (v1.0).txt.");
    }

    #[test]
    fn test_substitute_arguments_value_with_braces() {
        let content = "Config: {{{template}}}.";
        let mut args = HashMap::new();
        args.insert("template".to_string(), "{{json: true}}".to_string());
        let result = substitute_arguments(content, &args);
        assert_eq!(result, "Config: {{json: true}}.");
    }

    #[test]
    fn test_substitute_arguments_no_placeholders() {
        let content = "This is plain text with no arguments.";
        let mut args = HashMap::new();
        args.insert("foo".to_string(), "bar".to_string());
        let result = substitute_arguments(content, &args);
        assert_eq!(result, "This is plain text with no arguments.");
    }

    #[test]
    fn test_substitute_arguments_repeated_placeholder() {
        let content = "File: {{{name}}}. Again: {{{name}}}.";
        let mut args = HashMap::new();
        args.insert("name".to_string(), "test.txt".to_string());
        let result = substitute_arguments(content, &args);
        assert_eq!(result, "File: test.txt. Again: test.txt.");
    }

    #[tokio::test]
    async fn test_skill_tool_execute_with_args() {
        clear_all_test_state();

        // Register a skill with argument placeholders
        let skill = LoadedSkill {
            metadata: SkillMetadata {
                name: "test_arg_skill".to_string(),
                description: "A skill with args".to_string(),
                allowed_tools: None,
                argument_hint: None,
                arg_names: None,
                when_to_use: None,
                user_invocable: None,
                paths: None,
                hooks: None,
                effort: None,
                model: None,
                context: None,
                agent: None,
            },
            content: "Process the file {{{filename}}} using {{{method}}}.".to_string(),
            base_dir: "".to_string(),
        };
        register_skill(skill);

        let tool = SkillTool::new();
        let input = serde_json::json!({
            "skill": "test_arg_skill",
            "args": {
                "filename": "main.rs",
                "method": "static analysis"
            }
        });
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());
        let content = result.unwrap().content;
        assert!(content.contains("main.rs"), "Content should contain substituted filename: {}", content);
        assert!(content.contains("static analysis"), "Content should contain substituted method: {}", content);
        // The original placeholders should be gone
        assert!(!content.contains("{{{filename}}}"), "Placeholder should be substituted: {}", content);
        assert!(!content.contains("{{{method}}}"), "Placeholder should be substituted: {}", content);
    }

    #[tokio::test]
    async fn test_skill_tool_execute_with_partial_args() {
        clear_all_test_state();

        let skill = LoadedSkill {
            metadata: SkillMetadata {
                name: "test_partial_args".to_string(),
                description: "Partial args test".to_string(),
                allowed_tools: None,
                argument_hint: None,
                arg_names: None,
                when_to_use: None,
                user_invocable: None,
                paths: None,
                hooks: None,
                effort: None,
                model: None,
                context: None,
                agent: None,
            },
            content: "Target: {{{target}}}, Mode: {{{mode}}}.".to_string(),
            base_dir: "".to_string(),
        };
        register_skill(skill);

        let tool = SkillTool::new();
        let input = serde_json::json!({
            "skill": "test_partial_args",
            "args": {
                "target": "production"
            }
        });
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());
        let content = result.unwrap().content;
        assert!(content.contains("production"), "Substituted value should appear: {}", content);
        // Unsubstituted placeholder should remain
        assert!(content.contains("{{{mode}}}"), "Unsubstituted placeholder should remain: {}", content);
    }
}
