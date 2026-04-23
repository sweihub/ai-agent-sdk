// Source: /data/home/swei/claudecode/openclaudecode/src/services/tools/toolExecution.ts
//! Input validation for tool calls.
//!
//! Translated from TypeScript checkPermissionsAndCallTool validation step.

use crate::types::{ToolDefinition, ToolInputSchema};

/// Validate tool input against the tool's JSON Schema.
/// Returns Ok(()) if valid, or Err with a human-readable error message.
///
/// Matches TypeScript's Zod schema validation with structured error messages.
pub fn validate_tool_input(
    name: &str,
    input: &serde_json::Value,
    tools: &[ToolDefinition],
) -> Result<(), String> {
    let tool = tools
        .iter()
        .find(|t| t.name == name)
        .ok_or(format!("Tool '{}' not found", name))?;
    validate_against_schema(name, input, &tool.input_schema)
}

/// Validate input against a specific tool's schema.
fn validate_against_schema(
    tool_name: &str,
    input: &serde_json::Value,
    schema: &ToolInputSchema,
) -> Result<(), String> {
    let properties = schema
        .properties
        .as_object()
        .ok_or_else(|| format!("Invalid schema for tool '{}'", tool_name))?;
    let required = schema.required.as_ref();

    let mut errors: Vec<String> = Vec::new();

    // Check required fields
    if let Some(req) = required {
        for field in req {
            if !input.get(field).is_some() {
                errors.push(format!("The required parameter `{}` is missing", field));
            }
        }
    }

    // Check each provided field against schema
    if let Some(obj) = input.as_object() {
        for (key, value) in obj {
            if let Some(prop_schema) = properties.get(key.as_str()) {
                if let Some(prop_type) = prop_schema.get("type") {
                    if let Some(prop_type_str) = prop_type.as_str() {
                        if !check_type(value, prop_type_str) {
                            let received = json_type_name(value);
                            errors.push(format!(
                                "The parameter `{}` type is expected as `{}` but provided as `{}`",
                                key, prop_type_str, received
                            ));
                        }
                    }
                }
            }
        }
    }

    // Check for unexpected parameters (properties not in schema)
    if let Some(obj) = input.as_object() {
        for key in obj.keys() {
            if !properties.contains_key(key.as_str()) {
                errors.push(format!("An unexpected parameter `{}` was provided", key));
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        let issue_word = if errors.len() > 1 { "issues" } else { "issue" };
        Err(format!(
            "{} failed due to the following {}:\n{}",
            tool_name,
            issue_word,
            errors.join("\n")
        ))
    }
}

/// Check if a JSON value matches the expected schema type.
fn check_type(value: &serde_json::Value, expected_type: &str) -> bool {
    match expected_type {
        "string" => value.is_string(),
        "number" => value.is_number(),
        "integer" => value.is_number() && value.as_i64().is_some(),
        "boolean" => value.is_boolean(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        "null" => value.is_null(),
        _ => true, // Unknown types are permissive
    }
}

/// Get the JSON type name for error messages.
fn json_type_name(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

/// Tool definition lookup by name.
/// Matches TypeScript's findToolByName which also checks aliases.
pub fn find_tool_by_name<'a>(
    tools: &'a [ToolDefinition],
    name: &str,
) -> Option<&'a ToolDefinition> {
    tools.iter().find(|t| t.name == name).or_else(|| {
        // Fallback: check if it's a deprecated alias
        // Maps "Read" -> "FileRead", "Edit" -> "FileEdit", "Write" -> "FileWrite", "Glob" -> "Glob", etc.
        match name {
            "Read" => tools.iter().find(|t| t.name == "FileRead"),
            "Edit" => tools.iter().find(|t| t.name == "FileEdit"),
            "Write" => tools.iter().find(|t| t.name == "FileWrite"),
            "G" => tools.iter().find(|t| t.name == "Glob"),
            _ => None,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ToolInputSchema;

    fn make_tool(
        name: &str,
        properties: serde_json::Value,
        required: Option<Vec<String>>,
    ) -> ToolDefinition {
        ToolDefinition {
            name: name.to_string(),
            description: format!("Test tool {}", name),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties,
                required,
            },
            annotations: None,
            should_defer: None,
            always_load: None,
            is_mcp: None,
            search_hint: None,
            aliases: None,
            user_facing_name: None,
            interrupt_behavior: None,
        }
    }

    #[test]
    fn test_valid_input() {
        let tool = make_tool(
            "Bash",
            serde_json::json!({
                "command": { "type": "string" }
            }),
            Some(vec!["command".to_string()]),
        );
        let input = serde_json::json!({ "command": "ls -la" });
        assert!(validate_tool_input("Bash", &input, &[tool]).is_ok());
    }

    #[test]
    fn test_missing_required_field() {
        let tool = make_tool(
            "Bash",
            serde_json::json!({
                "command": { "type": "string" }
            }),
            Some(vec!["command".to_string()]),
        );
        let input = serde_json::json!({});
        let err = validate_tool_input("Bash", &input, &[tool]).unwrap_err();
        assert!(err.contains("The required parameter `command` is missing"));
    }

    #[test]
    fn test_type_mismatch() {
        let tool = make_tool(
            "Bash",
            serde_json::json!({
                "command": { "type": "string" }
            }),
            Some(vec!["command".to_string()]),
        );
        let input = serde_json::json!({ "command": 123 });
        let err = validate_tool_input("Bash", &input, &[tool]).unwrap_err();
        assert!(err.contains("type is expected as `string` but provided as `number`"));
    }

    #[test]
    fn test_unexpected_parameter() {
        let tool = make_tool(
            "Bash",
            serde_json::json!({
                "command": { "type": "string" }
            }),
            Some(vec!["command".to_string()]),
        );
        let input = serde_json::json!({ "command": "ls", "unknown_field": "val" });
        let err = validate_tool_input("Bash", &input, &[tool]).unwrap_err();
        assert!(err.contains("An unexpected parameter `unknown_field` was provided"));
    }

    #[test]
    fn test_alias_resolution() {
        let tool = make_tool("FileRead", serde_json::json!({}), None);
        let tools = vec![tool];
        assert!(find_tool_by_name(&tools, "FileRead").is_some());
        assert!(find_tool_by_name(&tools, "Read").is_some());
        assert!(find_tool_by_name(&tools, "NonExistent").is_none());
    }
}
