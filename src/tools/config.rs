// Source: ~/claudecode/openclaudecode/src/tools/ConfigTool/ConfigTool.ts
//! Config tool - dynamic configuration.
//!
//! Provides tool for reading and updating configuration settings.

use crate::error::AgentError;
use crate::types::*;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

pub const CONFIG_TOOL_NAME: &str = "Config";

/// Global config store
static CONFIG: OnceLock<Mutex<HashMap<String, serde_json::Value>>> = OnceLock::new();

fn get_config_map() -> &'static Mutex<HashMap<String, serde_json::Value>> {
    CONFIG.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Nested config key resolution
fn get_nested_config(
    config: &HashMap<String, serde_json::Value>,
    key: &str,
) -> Option<serde_json::Value> {
    // Simple key lookup (supports dot-notation like "settings.theme")
    if key.contains('.') {
        let parts: Vec<&str> = key.split('.').collect();
        let mut current: Option<&serde_json::Value> = None;
        for part in &parts {
            if current.is_none() {
                current = config.get(*part);
            } else {
                current = current
                    .and_then(|v| v.as_object())
                    .and_then(|obj| obj.get(*part));
            }
        }
        current.cloned()
    } else {
        config.get(key).cloned()
    }
}

/// Config tool - read and update dynamic configuration
pub struct ConfigTool;

impl ConfigTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        CONFIG_TOOL_NAME
    }

    pub fn description(&self) -> &str {
        "Read or update dynamic configuration settings. Use 'get' to read a setting, 'set' to update a setting, or 'list' to see all settings."
    }

    pub fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema {
            schema_type: "object".to_string(),
            properties: serde_json::json!({
                "action": {
                    "type": "string",
                    "enum": ["get", "set", "list"],
                    "description": "Action to perform: get (read a setting), set (update a setting), or list (show all settings)"
                },
                "key": {
                    "type": "string",
                    "description": "Configuration key (for get/set actions). Supports dot notation for nested keys (e.g., 'settings.theme')"
                },
                "value": {
                    "type": "string",
                    "description": "Configuration value (for set action). Will be parsed as JSON if possible, otherwise treated as string"
                }
            }),
            required: Some(vec!["action".to_string()]),
        }
    }

    pub async fn execute(
        &self,
        input: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let action = input["action"].as_str().unwrap_or("list");
        let key = input["key"].as_str().unwrap_or("");
        let value_str = input["value"].as_str().unwrap_or("");

        match action {
            "get" => {
                if key.is_empty() {
                    return Ok(ToolResult {
                        result_type: "text".to_string(),
                        tool_use_id: "".to_string(),
                        content: "Error: key is required for 'get' action".to_string(),
                        is_error: Some(true),
                was_persisted: None,
                    });
                }
                let guard = get_config_map().lock().unwrap();
                if let Some(val) = get_nested_config(&guard, key) {
                    Ok(ToolResult {
                        result_type: "text".to_string(),
                        tool_use_id: "".to_string(),
                        content: format!("Config '{}': {}", key, val),
                        is_error: Some(false),
                was_persisted: None,
                    })
                } else {
                    Ok(ToolResult {
                        result_type: "text".to_string(),
                        tool_use_id: "".to_string(),
                        content: format!("Config '{}' is not set", key),
                        is_error: None,
                was_persisted: None,
                    })
                }
            }
            "set" => {
                if key.is_empty() {
                    return Ok(ToolResult {
                        result_type: "text".to_string(),
                        tool_use_id: "".to_string(),
                        content: "Error: key is required for 'set' action".to_string(),
                        is_error: Some(true),
                was_persisted: None,
                    });
                }
                if value_str.is_empty() {
                    return Ok(ToolResult {
                        result_type: "text".to_string(),
                        tool_use_id: "".to_string(),
                        content: "Error: value is required for 'set' action".to_string(),
                        is_error: Some(true),
                was_persisted: None,
                    });
                }
                // Parse value as JSON if possible, otherwise treat as string
                let value: serde_json::Value =
                    serde_json::from_str(value_str).unwrap_or(serde_json::json!(value_str));

                let mut guard = get_config_map().lock().unwrap();
                guard.insert(key.to_string(), value);
                drop(guard);

                Ok(ToolResult {
                    result_type: "text".to_string(),
                    tool_use_id: "".to_string(),
                    content: format!("Config '{}' has been updated", key),
                    is_error: Some(false),
                was_persisted: None,
                })
            }
            "list" => {
                let guard = get_config_map().lock().unwrap();
                if guard.is_empty() {
                    Ok(ToolResult {
                        result_type: "text".to_string(),
                        tool_use_id: "".to_string(),
                        content: "No configuration settings set.".to_string(),
                        is_error: None,
                was_persisted: None,
                    })
                } else {
                    let items: Vec<String> = guard
                        .iter()
                        .map(|(k, v)| format!("  {}: {}", k, v))
                        .collect();
                    Ok(ToolResult {
                        result_type: "text".to_string(),
                        tool_use_id: "".to_string(),
                        content: format!("Configuration settings:\n{}", items.join("\n")),
                        is_error: Some(false),
                was_persisted: None,
                    })
                }
            }
            _ => Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: format!("Invalid action: '{}'. Must be 'get', 'set', or 'list'.", action),
                is_error: Some(true),
                was_persisted: None,
            }),
        }
    }
}

impl Default for ConfigTool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_tool_name() {
        let tool = ConfigTool::new();
        assert_eq!(tool.name(), CONFIG_TOOL_NAME);
    }

    #[test]
    fn test_config_tool_schema() {
        let tool = ConfigTool::new();
        let schema = tool.input_schema();
        assert_eq!(schema.schema_type, "object");
        assert!(schema.properties.get("action").is_some());
    }

    #[tokio::test]
    async fn test_config_tool_list_empty() {
        let tool = ConfigTool::new();
        let input = serde_json::json!({ "action": "list" });
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_config_tool_set_and_get() {
        let tool = ConfigTool::new();
        let context = ToolContext::default();

        // Set a value
        let set_result = tool
            .execute(
                serde_json::json!({ "action": "set", "key": "test_key", "value": "\"hello\"" }),
                &context,
            )
            .await;
        assert!(set_result.is_ok());

        // Get the value
        let get_result = tool
            .execute(serde_json::json!({ "action": "get", "key": "test_key" }), &context)
            .await;
        assert!(get_result.is_ok());
        assert!(get_result.unwrap().content.contains("hello"));
    }
}
