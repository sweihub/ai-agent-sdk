use crate::types::ToolDefinition;
use serde::{Deserialize, Serialize};

/// Tool annotations (MCP standard).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolAnnotations {
    #[serde(rename = "readOnlyHint", skip_serializing_if = "Option::is_none")]
    pub read_only_hint: Option<bool>,
    #[serde(rename = "destructiveHint", skip_serializing_if = "Option::is_none")]
    pub destructive_hint: Option<bool>,
    #[serde(rename = "idempotentHint", skip_serializing_if = "Option::is_none")]
    pub idempotent_hint: Option<bool>,
    #[serde(rename = "openWorldHint", skip_serializing_if = "Option::is_none")]
    pub open_world_hint: Option<bool>,
}

/// Tool call result (MCP-compatible).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallToolResult {
    pub content: Vec<ContentBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ContentBlock {
    Text {
        #[serde(rename = "type")]
        content_type: String,
        text: String,
    },
    Image {
        #[serde(rename = "type")]
        content_type: String,
        data: String,
        mime_type: String,
    },
    Resource {
        #[serde(rename = "type")]
        content_type: String,
        resource: ResourceContent,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceContent {
    pub uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob: Option<String>,
}

/// SDK tool definition - stores tool metadata for later conversion to ToolDefinition.
/// The handler is not stored directly - it should be registered separately.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub annotations: Option<ToolAnnotations>,
}

/// Create a tool using JSON Schema.
///
/// This creates the metadata definition. The handler should be registered separately
/// with the tool system.
///
/// Usage:
/// ```ignore
/// let tool = create_tool(
///     "get_weather",
///     "Get weather for a city",
///     serde_json::json!({
///         "type": "object",
///         "properties": {
///             "city": { "type": "string", "description": "City name" }
///         },
///         "required": ["city"]
///     })
/// );
/// ```
pub fn create_tool(
    name: &str,
    description: &str,
    input_schema: serde_json::Value,
) -> SdkToolDefinition {
    SdkToolDefinition {
        name: name.to_string(),
        description: description.to_string(),
        input_schema,
        annotations: None,
    }
}

/// Create a tool with annotations.
pub fn create_tool_with_annotations(
    name: &str,
    description: &str,
    input_schema: serde_json::Value,
    annotations: ToolAnnotations,
) -> SdkToolDefinition {
    SdkToolDefinition {
        name: name.to_string(),
        description: description.to_string(),
        input_schema,
        annotations: Some(annotations),
    }
}

/// Convert an SdkToolDefinition to a ToolDefinition for the engine.
pub fn sdk_tool_to_tool_definition(sdk_tool: SdkToolDefinition) -> ToolDefinition {
    let tool_name = sdk_tool.name.clone();
    let tool_description = sdk_tool.description.clone();
    let input_schema = sdk_tool.input_schema.clone();

    // Extract properties and required from the JSON schema
    let (schema_type, properties, required) = extract_schema_parts(&input_schema);

    crate::types::ToolDefinition {
        name: tool_name,
        description: tool_description,
        input_schema: crate::types::ToolInputSchema {
            schema_type,
            properties,
            required,
        },
        annotations: None,
        should_defer: None,
        always_load: None,
        is_mcp: None,
        search_hint: None,
        aliases: None,
    }
}

/// Extract schema parts from a JSON schema.
fn extract_schema_parts(
    schema: &serde_json::Value,
) -> (String, serde_json::Value, Option<Vec<String>>) {
    let schema_type = schema
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("object")
        .to_string();

    let properties = schema
        .get("properties")
        .cloned()
        .unwrap_or(serde_json::json!({}));

    let required = schema
        .get("required")
        .and_then(|r| r.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|s| s.as_str().map(String::from))
                .collect()
        });

    (schema_type, properties, required)
}

/// Helper to create a text content block.
pub fn text_content(text: &str) -> ContentBlock {
    ContentBlock::Text {
        content_type: "text".to_string(),
        text: text.to_string(),
    }
}

/// Helper to create an image content block.
pub fn image_content(data: &str, mime_type: &str) -> ContentBlock {
    ContentBlock::Image {
        content_type: "image".to_string(),
        data: data.to_string(),
        mime_type: mime_type.to_string(),
    }
}

/// Helper to create a resource content block.
pub fn resource_content(uri: &str, text: Option<&str>, blob: Option<&str>) -> ContentBlock {
    ContentBlock::Resource {
        content_type: "resource".to_string(),
        resource: ResourceContent {
            uri: uri.to_string(),
            text: text.map(String::from),
            blob: blob.map(String::from),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_annotations_default() {
        let annotations = ToolAnnotations::default();
        assert!(annotations.read_only_hint.is_none());
    }

    #[test]
    fn test_tool_annotations_with_values() {
        let annotations = ToolAnnotations {
            read_only_hint: Some(true),
            destructive_hint: Some(false),
            idempotent_hint: Some(true),
            open_world_hint: None,
        };

        assert_eq!(annotations.read_only_hint, Some(true));
        assert_eq!(annotations.destructive_hint, Some(false));
    }

    #[test]
    fn test_call_tool_result_text() {
        let result = CallToolResult {
            content: vec![text_content("Hello world")],
            is_error: Some(false),
        };

        assert!(!result.is_error.unwrap());
        if let ContentBlock::Text { text, .. } = &result.content[0] {
            assert_eq!(text, "Hello world");
        } else {
            panic!("Expected Text content block");
        }
    }

    #[test]
    fn test_create_tool() {
        let tool = create_tool(
            "test_tool",
            "A test tool",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "arg": { "type": "string" }
                }
            }),
        );

        assert_eq!(tool.name, "test_tool");
        assert_eq!(tool.description, "A test tool");
    }

    #[test]
    fn test_create_tool_with_annotations() {
        let tool = create_tool_with_annotations(
            "readonly_tool",
            "A read-only tool",
            serde_json::json!({
                "type": "object",
                "properties": {}
            }),
            ToolAnnotations {
                read_only_hint: Some(true),
                ..Default::default()
            },
        );

        assert!(tool.annotations.is_some());
        assert_eq!(tool.annotations.unwrap().read_only_hint, Some(true));
    }

    #[test]
    fn test_sdk_tool_to_tool_definition() {
        let sdk_tool = create_tool(
            "weather",
            "Get weather info",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "city": { "type": "string", "description": "City name" }
                },
                "required": ["city"]
            }),
        );

        let tool_def = sdk_tool_to_tool_definition(sdk_tool);
        assert_eq!(tool_def.name, "weather");
        assert_eq!(tool_def.description, "Get weather info");
    }

    #[test]
    fn test_extract_schema_parts() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "age": { "type": "number" }
            },
            "required": ["name"]
        });

        let (schema_type, properties, required) = extract_schema_parts(&schema);

        assert_eq!(schema_type, "object");
        assert!(properties.get("name").is_some());
        assert_eq!(required, Some(vec!["name".to_string()]));
    }

    #[test]
    fn test_text_content_helper() {
        let content = text_content("test");
        match content {
            ContentBlock::Text { content_type, text } => {
                assert_eq!(content_type, "text");
                assert_eq!(text, "test");
            }
            _ => panic!("Expected Text variant"),
        }
    }

    #[test]
    fn test_image_content_helper() {
        let content = image_content("base64data", "image/png");
        match content {
            ContentBlock::Image {
                content_type,
                data,
                mime_type,
            } => {
                assert_eq!(content_type, "image");
                assert_eq!(data, "base64data");
                assert_eq!(mime_type, "image/png");
            }
            _ => panic!("Expected Image variant"),
        }
    }

    #[test]
    fn test_resource_content_helper() {
        let content = resource_content("file://test.txt", Some("content"), None);
        match content {
            ContentBlock::Resource {
                content_type,
                resource,
            } => {
                assert_eq!(content_type, "resource");
                assert_eq!(resource.uri, "file://test.txt");
                assert_eq!(resource.text, Some("content".to_string()));
            }
            _ => panic!("Expected Resource variant"),
        }
    }
}
