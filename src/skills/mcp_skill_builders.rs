// Source: ~/claudecode/openclaudecode/src/skills/mcpSkillBuilders.ts
//! Write-once registry for the two loadSkillsDir functions that MCP skill
//! discovery needs. This module is a dependency-graph leaf: it imports nothing
//! but types, so both MCP skill discovery and the skill loader can depend on
//! it without forming a cycle.
//!
//! Registration happens at skill loader module init — long before any
//! MCP server connects.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Function type for creating a skill command from loaded skill data.
pub type CreateSkillCommandFn = dyn Fn(&LoadedSkillCommandParams) -> crate::skills::bundled_skills::BundledSkillDefinition
    + Send
    + Sync;

/// Function type for parsing skill frontmatter fields from markdown content.
pub type ParseSkillFrontmatterFieldsFn = dyn Fn(&str) -> SkillFrontmatterFields
    + Send
    + Sync;

/// Parameters for creating a skill command.
#[derive(Debug, Clone)]
pub struct LoadedSkillCommandParams {
    /// Skill name (used for the /slash command)
    pub skill_name: String,
    /// Display name shown in the skill listing
    pub display_name: Option<String>,
    /// Description shown to the model
    pub description: String,
    /// Whether the description was explicitly set by the user
    pub has_user_specified_description: bool,
    /// Markdown content of the skill (instructions)
    pub markdown_content: String,
    /// Tools allowed when running this skill
    pub allowed_tools: Option<Vec<String>>,
    /// Short argument hint for the skill
    pub argument_hint: Option<String>,
    /// Argument names expected by the skill
    pub argument_names: Option<Vec<String>>,
    /// When this skill is most useful
    pub when_to_use: Option<String>,
    /// Skill version
    pub version: Option<u32>,
    /// Target model for the skill
    pub model: Option<String>,
    /// Whether the LLM should invoke this skill
    pub disable_model_invocation: bool,
    /// Whether the user can invoke this skill directly
    pub user_invocable: bool,
    /// Source location (e.g., "bundled", "user", "project")
    pub source: String,
    /// Base directory of the skill
    pub base_dir: String,
    /// File name of the skill
    pub file_name: String,
}

/// Parsed frontmatter fields from a skill markdown file.
#[derive(Debug, Clone, Default)]
pub struct SkillFrontmatterFields {
    /// Skill name
    pub name: Option<String>,
    /// Description of what the skill does
    pub description: Option<String>,
    /// Tools the skill is allowed to use
    pub allowed_tools: Option<Vec<String>>,
    /// Short hint for what arguments to provide
    pub argument_hint: Option<String>,
    /// Expected argument names
    pub argument_names: Option<Vec<String>>,
    /// When this skill is most useful
    pub when_to_use: Option<String>,
    /// Skill version
    pub version: Option<u32>,
    /// Target model
    pub model: Option<String>,
    /// Whether the LLM should invoke this skill
    pub disable_model_invocation: bool,
    /// Whether the user can invoke this skill directly
    pub user_invocable: bool,
    /// Hooks provided by the skill
    pub hooks: Option<serde_json::Value>,
    /// Agent configuration for the skill
    pub agent: Option<serde_json::Value>,
    /// Context injection for the skill
    pub context: Option<serde_json::Value>,
}

/// Builders registry — set once at module init.
pub struct MCPSkillBuilders {
    /// Function to create a skill command from loaded params
    create_skill_command: Box<CreateSkillCommandFn>,
    /// Function to parse skill frontmatter fields from markdown
    parse_skill_frontmatter_fields: Box<ParseSkillFrontmatterFieldsFn>,
}

/// Singleton state: None until registered.
/// Uses a Mutex-wrapped Option so we can clear it for tests.
static BUILDERS: std::sync::Mutex<Option<Arc<MCPSkillBuilders>>> = std::sync::Mutex::new(None);

/// Whether the builders have been registered.
static BUILDERS_REGISTERED: AtomicBool = AtomicBool::new(false);

/// Register the MCP skill builders. Call this from skill loader init at startup.
///
/// This is a write-once registry — calling it more than once is a no-op.
pub fn register_mcp_skill_builders(
    create_skill_command: Box<CreateSkillCommandFn>,
    parse_skill_frontmatter_fields: Box<ParseSkillFrontmatterFieldsFn>,
) {
    if BUILDERS_REGISTERED.load(Ordering::SeqCst) {
        return;
    }

    let mut builders = BUILDERS.lock().unwrap();
    if builders.is_some() {
        return;
    }

    *builders = Some(Arc::new(MCPSkillBuilders {
        create_skill_command,
        parse_skill_frontmatter_fields,
    }));
    BUILDERS_REGISTERED.store(true, Ordering::SeqCst);
}

/// Get the registered MCP skill builders.
///
/// # Panics
/// Panics if the builders have not been registered yet (i.e., skill loader
/// has not been evaluated).
pub fn get_mcp_skill_builders() -> Arc<MCPSkillBuilders> {
    if !BUILDERS_REGISTERED.load(Ordering::SeqCst) {
        panic!(
            "MCP skill builders not registered — skill loader has not been initialized yet"
        );
    }
    BUILDERS.lock().unwrap().as_ref().cloned().expect("builders should be Some")
}

/// Check if MCP skill builders have been registered.
pub fn are_mcp_skill_builders_registered() -> bool {
    BUILDERS_REGISTERED.load(Ordering::SeqCst)
}

/// Clear the MCP skill builders registry (for testing).
pub fn clear_mcp_skill_builders() {
    *BUILDERS.lock().unwrap() = None;
    BUILDERS_REGISTERED.store(false, Ordering::SeqCst);
}

/// Convert a serde_yaml::Value to serde_json::Value.
fn yaml_to_json_value(v: &serde_yaml::Value) -> serde_json::Value {
    match v {
        serde_yaml::Value::Null => serde_json::Value::Null,
        serde_yaml::Value::Bool(b) => serde_json::Value::Bool(*b),
        serde_yaml::Value::Number(n) => {
            if let Some(u) = n.as_u64() {
                serde_json::Value::Number(serde_json::Number::from(u))
            } else if let Some(i) = n.as_i64() {
                serde_json::Value::Number(serde_json::Number::from(i))
            } else if let Some(f) = n.as_f64() {
                serde_json::Value::Number(
                    serde_json::Number::from_f64(f).unwrap_or(serde_json::Number::from(0)),
                )
            } else {
                serde_json::Value::Null
            }
        }
        serde_yaml::Value::String(s) => serde_json::Value::String(s.clone()),
        serde_yaml::Value::Sequence(seq) => {
            serde_json::Value::Array(seq.iter().map(yaml_to_json_value).collect())
        }
        serde_yaml::Value::Mapping(map) => {
            let mut m = serde_json::Map::new();
            for (k, val) in map {
                if let Some(ks) = k.as_str() {
                    m.insert(ks.to_string(), yaml_to_json_value(val));
                }
            }
            serde_json::Value::Object(m)
        }
        serde_yaml::Value::Tagged(tagged) => yaml_to_json_value(&tagged.value),
        _ => serde_json::Value::Null,
    }
}

/// Default implementation of parse_skill_frontmatter_fields.
/// Parses YAML frontmatter from a skill markdown file.
pub fn default_parse_skill_frontmatter_fields(content: &str) -> SkillFrontmatterFields {
    let mut fields = SkillFrontmatterFields::default();

    // Extract YAML frontmatter between --- markers
    let mut lines = content.lines();
    let first_line = lines.next();
    if first_line != Some("---") {
        return fields;
    }

    let mut yaml_content = String::new();
    for line in lines {
        if line == "---" {
            break;
        }
        yaml_content.push_str(line);
        yaml_content.push('\n');
    }

    // Parse YAML frontmatter
    if let Ok(doc) = serde_yaml::from_str::<serde_yaml::Value>(&yaml_content) {
        if let Some(map) = doc.as_mapping() {
            if let Some(name) = map.get(&serde_yaml::Value::String("name".into())) {
                fields.name = name.as_str().map(|s| s.to_string());
            }
            if let Some(desc) = map.get(&serde_yaml::Value::String("description".into())) {
                fields.description = desc.as_str().map(|s| s.to_string());
            }
            if let Some(at) = map.get(&serde_yaml::Value::String("allowedTools".into())) {
                fields.allowed_tools = at.as_sequence().map(|seq| {
                    seq.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect()
                });
            }
            if let Some(ah) = map.get(&serde_yaml::Value::String("argumentHint".into())) {
                fields.argument_hint = ah.as_str().map(|s| s.to_string());
            }
            if let Some(an) = map.get(&serde_yaml::Value::String("argumentNames".into())) {
                fields.argument_names = an.as_sequence().map(|seq| {
                    seq.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect()
                });
            }
            if let Some(wu) = map.get(&serde_yaml::Value::String("whenToUse".into())) {
                fields.when_to_use = wu.as_str().map(|s| s.to_string());
            }
            if let Some(ver) = map.get(&serde_yaml::Value::String("version".into())) {
                fields.version = ver.as_u64().map(|v| v as u32);
            }
            if let Some(model) = map.get(&serde_yaml::Value::String("model".into())) {
                fields.model = model.as_str().map(|s| s.to_string());
            }
            if let Some(dmi) = map.get(&serde_yaml::Value::String("disableModelInvocation".into())) {
                fields.disable_model_invocation = dmi.as_bool().unwrap_or(false);
            }
            if let Some(ui) = map.get(&serde_yaml::Value::String("userInvocable".into())) {
                fields.user_invocable = ui.as_bool().unwrap_or(true);
            }
            if let Some(hooks) = map.get(&serde_yaml::Value::String("hooks".into())) {
                fields.hooks = Some(yaml_to_json_value(hooks));
            }
            if let Some(agent) = map.get(&serde_yaml::Value::String("agent".into())) {
                fields.agent = Some(yaml_to_json_value(agent));
            }
            if let Some(ctx) = map.get(&serde_yaml::Value::String("context".into())) {
                fields.context = Some(yaml_to_json_value(ctx));
            }
        }
    }

    fields
}

/// Helper to create a minimal BundledSkillDefinition for tests.
#[cfg(test)]
fn make_bundled_def(name: &str, description: &str) -> crate::skills::bundled_skills::BundledSkillDefinition {
    use crate::AgentError;
    crate::skills::bundled_skills::BundledSkillDefinition {
        name: name.to_string(),
        description: description.to_string(),
        aliases: None,
        when_to_use: None,
        argument_hint: None,
        allowed_tools: None,
        model: None,
        disable_model_invocation: None,
        user_invocable: None,
        is_enabled: None,
        hooks: None,
        context: None,
        agent: None,
        files: None,
        get_prompt_for_command: std::sync::Arc::new(|_args, _ctx| Err(AgentError::Internal("test stub".into()))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[serial_test::serial]
    fn test_clear_and_register() {
        clear_mcp_skill_builders();
        assert!(!are_mcp_skill_builders_registered());

        let create_fn = Box::new(move |_params: &LoadedSkillCommandParams| {
            make_bundled_def("test", "test skill")
        });
        let parse_fn = Box::new(|_content: &str| SkillFrontmatterFields::default());

        register_mcp_skill_builders(create_fn, parse_fn);
        assert!(are_mcp_skill_builders_registered());

        clear_mcp_skill_builders();
        assert!(!are_mcp_skill_builders_registered());
    }

    #[test]
    #[serial_test::serial]
    fn test_register_once() {
        clear_mcp_skill_builders();

        let create_fn = Box::new(move |_params: &LoadedSkillCommandParams| {
            make_bundled_def("first", "first registration")
        });
        let parse_fn = Box::new(|_content: &str| SkillFrontmatterFields::default());
        register_mcp_skill_builders(create_fn, parse_fn);

        // Second registration should be a no-op
        let create_fn2 = Box::new(move |_params: &LoadedSkillCommandParams| {
            make_bundled_def("second", "second registration")
        });
        let parse_fn2 = Box::new(|_content: &str| SkillFrontmatterFields::default());
        register_mcp_skill_builders(create_fn2, parse_fn2);

        // Should still be registered (first one)
        let builders = get_mcp_skill_builders();
        let _ = builders;

        clear_mcp_skill_builders();
    }

    #[test]
    #[serial_test::serial]
    fn test_panic_on_unregistered() {
        clear_mcp_skill_builders();

        // Should panic when not registered
        let result = std::panic::catch_unwind(|| {
            let _ = get_mcp_skill_builders();
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_default_parse_frontmatter() {
        let content = "---\nname: my-skill\ndescription: A test skill\nallowedTools:\n  - Bash\n  - Read\nargumentHint: query\nversion: 2\nuserInvocable: true\ndisableModelInvocation: false\n---\n\nSkill instructions here\n";
        let fields = default_parse_skill_frontmatter_fields(content);
        assert_eq!(fields.name, Some("my-skill".to_string()));
        assert_eq!(fields.description, Some("A test skill".to_string()));
        assert_eq!(
            fields.allowed_tools,
            Some(vec!["Bash".to_string(), "Read".to_string()])
        );
        assert_eq!(fields.argument_hint, Some("query".to_string()));
        assert_eq!(fields.version, Some(2));
        assert!(fields.user_invocable);
        assert!(!fields.disable_model_invocation);
    }

    #[test]
    fn test_parse_no_frontmatter() {
        let content = "Just plain markdown content";
        let fields = default_parse_skill_frontmatter_fields(content);
        assert!(fields.name.is_none());
        assert!(fields.description.is_none());
    }

    #[test]
    fn test_parse_partial_frontmatter() {
        let content = "---\nname: partial\n---\nContent";
        let fields = default_parse_skill_frontmatter_fields(content);
        assert_eq!(fields.name, Some("partial".to_string()));
        assert!(fields.description.is_none());
    }
}
