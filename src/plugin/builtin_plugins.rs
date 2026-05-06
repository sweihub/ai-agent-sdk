// Source: ~/claudecode/openclaudecode/src/plugins/builtinPlugins.ts
//! Built-in Plugin Registry
//!
//! Manages built-in plugins that ship with the CLI and can be enabled/disabled
//! by users via the /plugin UI.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use crate::plugin::types::{LoadedPlugin, PluginManifest};
use crate::types::plugin::BuiltinPluginDefinition;

const BUILTIN_MARKETPLACE_NAME: &str = "builtin";

static BUILTIN_PLUGINS: Mutex<Vec<BuiltinPluginDefinition>> = Mutex::new(Vec::new());

/// The marketplace name for built-in plugins.
pub const BUILTIN_MARKETPLACE_NAME_CONST: &str = BUILTIN_MARKETPLACE_NAME;

/// Register a built-in plugin. Call this from init or at startup.
pub fn register_builtin_plugin(definition: BuiltinPluginDefinition) {
    let mut plugins = BUILTIN_PLUGINS.lock().unwrap();
    plugins.push(definition);
}

/// Check if a plugin ID represents a built-in plugin (ends with @builtin).
pub fn is_builtin_plugin_id(plugin_id: &str) -> bool {
    plugin_id.ends_with(&format!("@{}", BUILTIN_MARKETPLACE_NAME))
}

/// Get a specific built-in plugin definition by name.
/// Returns `None` if not found. Since the definition contains closures,
/// we return a clone of the clonable fields instead.
pub fn get_builtin_plugin_definition(name: &str) -> Option<BuiltinPluginSummary> {
    let plugins = BUILTIN_PLUGINS.lock().unwrap();
    plugins
        .iter()
        .find(|p| p.name == name)
        .map(|d| BuiltinPluginSummary {
            name: d.name.clone(),
            description: d.description.clone(),
            version: d.version.clone(),
            has_skills: d.skills.is_some(),
            has_hooks: d.hooks.is_some(),
            has_mcp_servers: d.mcp_servers.is_some(),
            default_enabled: d.default_enabled,
        })
}

/// Summary of a built-in plugin definition (no closures).
#[derive(Debug, Clone)]
pub struct BuiltinPluginSummary {
    pub name: String,
    pub description: String,
    pub version: Option<String>,
    pub has_skills: bool,
    pub has_hooks: bool,
    pub has_mcp_servers: bool,
    pub default_enabled: Option<bool>,
}

/// Result of getting built-in plugins, split by enabled/disabled state.
#[derive(Debug, Default)]
pub struct BuiltinPluginResult {
    pub enabled: Vec<LoadedPlugin>,
    pub disabled: Vec<LoadedPlugin>,
}

/// Get all registered built-in plugins as LoadedPlugin objects, split into
/// enabled/disabled based on user settings (with defaultEnabled as fallback).
/// Plugins whose isAvailable() returns false are omitted entirely.
pub fn get_builtin_plugins() -> BuiltinPluginResult {
    let plugins = BUILTIN_PLUGINS.lock().unwrap();
    let mut enabled = Vec::new();
    let mut disabled = Vec::new();

    // Load user-scope enabled plugins settings
    let user_enabled_plugins = load_user_enabled_plugins();

    for definition in plugins.iter() {
        // Skip plugins that are not available
        if let Some(is_avail) = &definition.is_available {
            if !is_avail() {
                continue;
            }
        }

        let plugin_id = format!("{}@{}", definition.name, BUILTIN_MARKETPLACE_NAME);
        let user_setting = user_enabled_plugins.get(&plugin_id);

        // Enabled state: user preference > plugin default > true
        let is_enabled = match user_setting {
            Some(&true) => true,
            Some(&false) => false,
            None => definition.default_enabled.unwrap_or(true),
        };

        let plugin = LoadedPlugin {
            name: definition.name.clone(),
            manifest: PluginManifest {
                name: definition.name.clone(),
                version: definition.version.clone(),
                description: Some(definition.description.clone()),
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
            path: BUILTIN_MARKETPLACE_NAME.to_string(),
            source: plugin_id.clone(),
            repository: plugin_id,
            enabled: Some(is_enabled),
            is_builtin: Some(true),
            sha: None,
            commands_path: None,
            commands_paths: None,
            commands_metadata: None,
            agents_path: None,
            agents_paths: None,
            skills_path: None,
            skills_paths: None,
            output_styles_path: None,
            output_styles_paths: None,
            hooks_config: definition.hooks.clone(),
            mcp_servers: definition.mcp_servers.clone(),
            lsp_servers: None,
            settings: None,
        };

        if is_enabled {
            enabled.push(plugin);
        } else {
            disabled.push(plugin);
        }
    }

    BuiltinPluginResult { enabled, disabled }
}

/// Load the user's enabled plugins settings from the settings file.
/// Returns a map of plugin_id -> enabled_state.
fn load_user_enabled_plugins() -> HashMap<String, bool> {
    let settings_dir = match std::env::var("AI_CODE_CONFIG_HOME") {
        Ok(dir) => dir,
        Err(_) => {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            format!("{}/.ai", home)
        }
    };

    let settings_path = format!("{}/settings.json", settings_dir);
    let content = match std::fs::read_to_string(&settings_path) {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };

    let settings: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return HashMap::new(),
    };

    let mut result = HashMap::new();
    if let Some(enabled_plugins) = settings.get("enabledPlugins").and_then(|v| v.as_object()) {
        for (plugin_id, enabled) in enabled_plugins {
            if let Some(val) = enabled.as_bool() {
                result.insert(plugin_id.clone(), val);
            }
        }
    }

    result
}

/// Get skills from enabled built-in plugins as BundledSkillDefinitions.
/// Skills from disabled plugins are not returned.
/// Returns the names of enabled built-in plugins that have skills defined.
pub fn get_builtin_plugin_skill_definitions() -> Vec<String> {
    let BuiltinPluginResult { enabled, .. } = get_builtin_plugins();

    // Collect enabled plugin names that have skills defined
    let enabled_names: HashSet<&str> = enabled.iter().map(|p| p.name.as_str()).collect();

    let plugins = BUILTIN_PLUGINS.lock().unwrap();
    plugins
        .iter()
        .filter(|d| enabled_names.contains(d.name.as_str()) && d.skills.is_some())
        .map(|d| d.name.clone())
        .collect()
}

/// Clear built-in plugins registry (for testing).
pub fn clear_builtin_plugins() {
    let mut plugins = BUILTIN_PLUGINS.lock().unwrap();
    plugins.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[serial_test::serial]
    #[test]
    fn test_register_and_get_builtin_plugin() {
        clear_builtin_plugins();

        let definition = BuiltinPluginDefinition {
            name: "test-plugin".to_string(),
            description: "A test built-in plugin".to_string(),
            version: Some("1.0.0".to_string()),
            skills: None,
            hooks: None,
            mcp_servers: None,
            is_available: None,
            default_enabled: Some(true),
        };

        register_builtin_plugin(definition);

        let result = get_builtin_plugin_definition("test-plugin");
        assert!(result.is_some());
        assert_eq!(result.unwrap().description, "A test built-in plugin");

        clear_builtin_plugins();
    }

    #[test]
    fn test_is_builtin_plugin_id() {
        assert!(is_builtin_plugin_id("my-plugin@builtin"));
        assert!(!is_builtin_plugin_id("my-plugin@marketplace"));
        assert!(!is_builtin_plugin_id("my-plugin"));
    }

    #[serial_test::serial]
    #[test]
    fn test_get_builtin_plugins_enabled_disabled() {
        clear_builtin_plugins();

        let enabled_plugin = BuiltinPluginDefinition {
            name: "enabled-plugin".to_string(),
            description: "Should be enabled".to_string(),
            version: None,
            skills: None,
            hooks: None,
            mcp_servers: None,
            is_available: None,
            default_enabled: Some(true),
        };

        let disabled_plugin = BuiltinPluginDefinition {
            name: "disabled-plugin".to_string(),
            description: "Should be disabled".to_string(),
            version: None,
            skills: None,
            hooks: None,
            mcp_servers: None,
            is_available: None,
            default_enabled: Some(false),
        };

        register_builtin_plugin(enabled_plugin);
        register_builtin_plugin(disabled_plugin);

        let result = get_builtin_plugins();
        assert_eq!(result.enabled.len(), 1);
        assert_eq!(result.disabled.len(), 1);
        assert_eq!(result.enabled[0].name, "enabled-plugin");
        assert_eq!(result.disabled[0].name, "disabled-plugin");

        clear_builtin_plugins();
    }

    #[serial_test::serial]
    #[test]
    fn test_get_builtin_plugins_filters_unavailable() {
        clear_builtin_plugins();

        let unavailable = BuiltinPluginDefinition {
            name: "unavailable-plugin".to_string(),
            description: "Should be filtered".to_string(),
            version: None,
            skills: None,
            hooks: None,
            mcp_servers: None,
            is_available: Some(Box::new(|| false)),
            default_enabled: Some(true),
        };

        let available = BuiltinPluginDefinition {
            name: "available-plugin".to_string(),
            description: "Should be included".to_string(),
            version: None,
            skills: None,
            hooks: None,
            mcp_servers: None,
            is_available: Some(Box::new(|| true)),
            default_enabled: Some(true),
        };

        register_builtin_plugin(unavailable);
        register_builtin_plugin(available);

        let result = get_builtin_plugins();
        assert_eq!(result.enabled.len(), 1);
        assert_eq!(result.enabled[0].name, "available-plugin");

        clear_builtin_plugins();
    }

    #[serial_test::serial]
    #[test]
    fn test_clear_builtin_plugins() {
        clear_builtin_plugins();

        let definition = BuiltinPluginDefinition {
            name: "to-clear".to_string(),
            description: "Will be cleared".to_string(),
            version: None,
            skills: None,
            hooks: None,
            mcp_servers: None,
            is_available: None,
            default_enabled: None,
        };

        register_builtin_plugin(definition);
        assert!(get_builtin_plugin_definition("to-clear").is_some());

        clear_builtin_plugins();
        assert!(get_builtin_plugin_definition("to-clear").is_none());
    }
}
