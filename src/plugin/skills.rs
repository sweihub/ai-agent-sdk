// Source: /data/home/swei/claudecode/openclaudecode/src/commands/skills/skills.tsx
//! Plugin skills - loads skills from plugins
//!
//! Ported from ~/claudecode/openclaudecode/src/utils/plugins/loadPluginCommands.ts
//!
//! This module provides functionality to load skills from plugin directories.
//! Skills can be defined either as:
//! - A direct SKILL.md file in the plugin's skills directory
//! - Subdirectories containing SKILL.md files (skill-name/SKILL.md format)

use crate::AgentError;
use crate::plugin::types::{LoadedPlugin, PluginManifest};
use crate::skills::loader::LoadedSkill;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Skill metadata parsed from plugin SKILL.md frontmatter
#[derive(Debug, Clone)]
pub struct PluginSkillMetadata {
    /// Full skill name including plugin prefix (e.g., "my-plugin:skill-name")
    pub full_name: String,
    /// Plugin name (e.g., "my-plugin")
    pub plugin_name: String,
    /// Skill name without plugin prefix (e.g., "skill-name")
    pub skill_name: String,
    /// Human-readable description
    pub description: Option<String>,
    /// Allowed tools for this skill
    pub allowed_tools: Option<Vec<String>>,
    /// Argument hint
    pub argument_hint: Option<String>,
    /// When to use this skill
    pub when_to_use: Option<String>,
    /// Whether the skill is user-invocable
    pub user_invocable: Option<bool>,
}

/// A skill loaded from a plugin
#[derive(Debug, Clone)]
pub struct PluginSkill {
    pub metadata: PluginSkillMetadata,
    /// The skill content (markdown)
    pub content: String,
    /// The base directory for this skill
    pub base_dir: String,
    /// Source plugin name
    pub source: String,
    /// Path to the SKILL.md file
    pub file_path: String,
}

/// Loaded skills grouped by plugin
#[derive(Debug, Clone, Default)]
pub struct PluginSkills {
    pub skills: HashMap<String, PluginSkill>,
}

impl PluginSkills {
    /// Create a new empty PluginSkills
    pub fn new() -> Self {
        Self {
            skills: HashMap::new(),
        }
    }

    /// Add a skill to the collection
    pub fn insert(&mut self, skill: PluginSkill) {
        self.skills.insert(skill.metadata.full_name.clone(), skill);
    }

    /// Get a skill by full name
    pub fn get(&self, name: &str) -> Option<&PluginSkill> {
        self.skills.get(name)
    }

    /// Get all skill names
    pub fn names(&self) -> Vec<String> {
        self.skills.keys().cloned().collect()
    }

    /// Get skills count
    pub fn len(&self) -> usize {
        self.skills.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    /// Convert to LoadedSkill for integration with the skill system
    pub fn to_loaded_skills(&self) -> Vec<LoadedSkill> {
        self.skills
            .values()
            .map(|plugin_skill| {
                let metadata = crate::skills::loader::SkillMetadata {
                    name: plugin_skill.metadata.full_name.clone(),
                    description: plugin_skill
                        .metadata
                        .description
                        .clone()
                        .unwrap_or_default(),
                    allowed_tools: plugin_skill.metadata.allowed_tools.clone(),
                    argument_hint: plugin_skill.metadata.argument_hint.clone(),
                    arg_names: None,
                    when_to_use: plugin_skill.metadata.when_to_use.clone(),
                    user_invocable: plugin_skill.metadata.user_invocable,
                    paths: None,
                    hooks: None,
                    effort: None,
                    model: None,
                    context: None,
                    agent: None,
                    shell: None,
                };
                LoadedSkill {
                    metadata,
                    content: plugin_skill.content.clone(),
                    base_dir: plugin_skill.base_dir.clone(),
                }
            })
            .collect()
    }
}

/// Parse frontmatter from SKILL.md content
fn parse_frontmatter(content: &str) -> (HashMap<String, String>, String) {
    let mut fields = HashMap::new();
    let trimmed = content.trim();

    if !trimmed.starts_with("---") {
        return (fields, content.to_string());
    }

    if let Some(end_pos) = trimmed[3..].find("---") {
        let frontmatter = &trimmed[3..end_pos + 3];
        for line in frontmatter.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(colon_pos) = line.find(':') {
                let key = line[..colon_pos].trim().to_string();
                let value = line[colon_pos + 1..].trim().to_string();
                fields.insert(key, value);
            }
        }
        let body = trimmed[end_pos + 6..].trim_start().to_string();
        return (fields, body);
    }

    (fields, content.to_string())
}

/// Load a single skill from a SKILL.md file in a plugin
fn load_skill_from_file(
    skill_file_path: &Path,
    plugin_name: &str,
    skill_name: &str,
    source: &str,
) -> Result<PluginSkill, AgentError> {
    let content = fs::read_to_string(skill_file_path).map_err(|e| AgentError::Io(e))?;

    let (fields, body) = parse_frontmatter(&content);

    let full_name = format!("{}:{}", plugin_name, skill_name);

    let description = fields.get("description").cloned();
    let allowed_tools = fields
        .get("allowed-tools")
        .map(|s| s.split(',').map(|x| x.trim().to_string()).collect());
    let argument_hint = fields.get("argument-hint").cloned();
    let when_to_use = fields.get("when_to_use").cloned();
    let user_invocable = fields.get("user-invocable").and_then(|v| match v.as_str() {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    });

    let metadata = PluginSkillMetadata {
        full_name: full_name.clone(),
        plugin_name: plugin_name.to_string(),
        skill_name: skill_name.to_string(),
        description,
        allowed_tools,
        argument_hint,
        when_to_use,
        user_invocable,
    };

    let base_dir = skill_file_path
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    Ok(PluginSkill {
        metadata,
        content: body,
        base_dir,
        source: source.to_string(),
        file_path: skill_file_path.to_string_lossy().to_string(),
    })
}

/// Load skills from a plugin skills directory
///
/// Supports two formats:
/// 1. Direct SKILL.md in the skills directory (skills_path/SKILL.md)
/// 2. Subdirectories with SKILL.md (skills_path/skill-name/SKILL.md)
fn load_skills_from_plugin_dir(
    skills_path: &Path,
    plugin_name: &str,
    source: &str,
    _manifest: &PluginManifest,
    loaded_paths: &mut std::collections::HashSet<String>,
) -> Vec<PluginSkill> {
    let mut skills = Vec::new();

    // Check for direct SKILL.md in the skills directory
    let direct_skill_path = skills_path.join("SKILL.md");
    if direct_skill_path.exists() {
        let path_str = direct_skill_path.to_string_lossy().to_string();
        if !loaded_paths.contains(&path_str) {
            loaded_paths.insert(path_str);

            // Skill name is the directory name
            let skill_name = skills_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");

            match load_skill_from_file(&direct_skill_path, plugin_name, skill_name, source) {
                Ok(skill) => skills.push(skill),
                Err(e) => {
                    log::warn!(
                        "Failed to load skill from {}: {}",
                        direct_skill_path.display(),
                        e
                    );
                }
            }
            return skills;
        }
    }

    // Otherwise, scan subdirectories for SKILL.md files
    if !skills_path.is_dir() {
        return skills;
    }

    if let Ok(entries) = fs::read_dir(skills_path) {
        for entry in entries.flatten() {
            let entry_path = entry.path();

            // Accept both directories and symlinks
            if !entry_path.is_dir() && !entry_path.is_symlink() {
                continue;
            }

            let skill_file_path = entry_path.join("SKILL.md");
            if !skill_file_path.exists() {
                continue;
            }

            let path_str = skill_file_path.to_string_lossy().to_string();
            if loaded_paths.contains(&path_str) {
                continue;
            }
            loaded_paths.insert(path_str);

            let skill_name = entry_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");

            match load_skill_from_file(&skill_file_path, plugin_name, skill_name, source) {
                Ok(skill) => skills.push(skill),
                Err(e) => {
                    log::warn!(
                        "Failed to load skill from {}: {}",
                        skill_file_path.display(),
                        e
                    );
                }
            }
        }
    }

    skills
}

/// Load skills from a plugin
///
/// Loads skills from:
/// 1. Default skills directory (plugin_path/skills)
/// 2. Additional paths specified in manifest.skills
pub fn load_plugin_skills(plugin: &LoadedPlugin) -> PluginSkills {
    let mut skills = PluginSkills::new();
    let mut loaded_paths = std::collections::HashSet::new();

    // Load from default skills directory
    if let Some(ref skills_path) = plugin.skills_path {
        let path = PathBuf::from(skills_path);
        if path.exists() {
            log::debug!(
                "Loading skills from plugin {} default path: {}",
                plugin.name,
                skills_path
            );
            let loaded = load_skills_from_plugin_dir(
                &path,
                &plugin.name,
                &plugin.source,
                &plugin.manifest,
                &mut loaded_paths,
            );
            for skill in loaded {
                skills.insert(skill);
            }
        }
    }

    // Load from additional paths specified in manifest
    if let Some(ref skills_paths) = plugin.skills_paths {
        for skill_path in skills_paths {
            let path = PathBuf::from(skill_path);
            if path.exists() {
                log::debug!(
                    "Loading skills from plugin {} custom path: {}",
                    plugin.name,
                    skill_path
                );
                let loaded = load_skills_from_plugin_dir(
                    &path,
                    &plugin.name,
                    &plugin.source,
                    &plugin.manifest,
                    &mut loaded_paths,
                );
                for skill in loaded {
                    skills.insert(skill);
                }
            }
        }
    }

    skills
}

/// Load skills from multiple plugins
pub fn load_skills_from_plugins(plugins: &[LoadedPlugin]) -> PluginSkills {
    let mut all_skills = PluginSkills::new();

    for plugin in plugins {
        let plugin_skills = load_plugin_skills(plugin);
        for skill in plugin_skills.skills.into_values() {
            all_skills.insert(skill);
        }
    }

    all_skills
}

/// Register plugin skills with the global skill registry
pub fn register_plugin_skills(plugins: &[LoadedPlugin]) {
    let plugin_skills = load_skills_from_plugins(plugins);
    if !plugin_skills.is_empty() {
        let loaded_skills = plugin_skills.to_loaded_skills();
        crate::tools::skill::register_skills(loaded_skills.clone());
        log::info!(
            "Registered {} plugin skills: {:?}",
            loaded_skills.len(),
            loaded_skills
                .iter()
                .map(|s| s.metadata.name.clone())
                .collect::<Vec<_>>()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_skill(dir: &Path, name: &str, content: &str) {
        let skill_dir = dir.join(name);
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), content).unwrap();
    }

    #[test]
    fn test_parse_frontmatter() {
        let content = r#"---
description: A test skill
allowed-tools: tool1,tool2
---

This is the skill content.
"#;
        let (fields, body) = parse_frontmatter(content);
        assert_eq!(fields.get("description"), Some(&"A test skill".to_string()));
        assert_eq!(
            fields.get("allowed-tools"),
            Some(&"tool1,tool2".to_string())
        );
        assert_eq!(body, "This is the skill content.");
    }

    #[test]
    fn test_parse_frontmatter_no_frontmatter() {
        let content = "Just plain content without frontmatter";
        let (fields, body) = parse_frontmatter(content);
        assert!(fields.is_empty());
        assert_eq!(body, content);
    }

    #[test]
    fn test_load_skills_from_plugin_dir() {
        let temp_dir = TempDir::new().unwrap();

        // Create skills directory structure
        let skills_dir = temp_dir.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();

        // Create a skill subdirectory
        create_test_skill(
            &skills_dir,
            "test-skill",
            r#"---
description: A test skill
---

Test skill content here.
"#,
        );

        // Load skills
        let mut loaded_paths = std::collections::HashSet::new();
        let skills = load_skills_from_plugin_dir(
            &skills_dir,
            "test-plugin",
            "test-source",
            &PluginManifest {
                name: "test-plugin".to_string(),
                version: None,
                description: None,
                author: None,
                homepage: None,
                repository: None,
                license: None,
                keywords: None,
                dependencies: None,
                commands: None,
                agents: None,
                skills: None,
                hooks: None,
                output_styles: None,
                channels: None,
                mcp_servers: None,
                lsp_servers: None,
                settings: None,
                user_config: None,
            },
            &mut loaded_paths,
        );

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].metadata.full_name, "test-plugin:test-skill");
        assert_eq!(skills[0].metadata.plugin_name, "test-plugin");
        assert_eq!(skills[0].metadata.skill_name, "test-skill");
        assert_eq!(skills[0].content, "Test skill content here.");
    }

    #[test]
    fn test_plugin_skill_to_loaded_skill() {
        let plugin_skill = PluginSkill {
            metadata: PluginSkillMetadata {
                full_name: "my-plugin:my-skill".to_string(),
                plugin_name: "my-plugin".to_string(),
                skill_name: "my-skill".to_string(),
                description: Some("A plugin skill".to_string()),
                allowed_tools: Some(vec!["tool1".to_string()]),
                argument_hint: None,
                when_to_use: None,
                user_invocable: Some(true),
            },
            content: "Skill content".to_string(),
            base_dir: "/path/to/skill".to_string(),
            source: "my-plugin".to_string(),
            file_path: "/path/to/skill/SKILL.md".to_string(),
        };

        let loaded_skills = PluginSkills {
            skills: [("my-plugin:my-skill".to_string(), plugin_skill)]
                .into_iter()
                .collect(),
        };

        let converted = loaded_skills.to_loaded_skills();
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].metadata.name, "my-plugin:my-skill");
        assert_eq!(converted[0].content, "Skill content");
    }

    #[test]
    fn test_plugin_skills_collection() {
        let mut skills = PluginSkills::new();
        assert!(skills.is_empty());

        let skill = PluginSkill {
            metadata: PluginSkillMetadata {
                full_name: "test:skill".to_string(),
                plugin_name: "test".to_string(),
                skill_name: "skill".to_string(),
                description: None,
                allowed_tools: None,
                argument_hint: None,
                when_to_use: None,
                user_invocable: None,
            },
            content: "content".to_string(),
            base_dir: "/base".to_string(),
            source: "test".to_string(),
            file_path: "/base/SKILL.md".to_string(),
        };

        skills.insert(skill);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills.get("test:skill").unwrap().content, "content");
        assert_eq!(skills.names(), vec!["test:skill"]);
    }
}
