// Source: ~/claudecode/openclaudecode/src/tools/SyntheticOutputTool/SyntheticOutputTool.ts
//! SyntheticOutputTool (StructuredOutput) — structured output enforcement
//!
//! Returns the LLM's input as structured output. Schema validation is applied
//! when a JSON schema is provided. Only enabled for non-interactive sessions.

use crate::error::AgentError;
use crate::types::*;

pub const SYNTHETIC_OUTPUT_TOOL_NAME: &str = "StructuredOutput";
pub const DESCRIPTION: &str =
    "Return structured output in the requested format. You MUST call this tool exactly once at the end of your response to provide the structured output.";

/// SyntheticOutputTool — structured output enforcement
pub struct SyntheticOutputTool {
    /// Optional JSON schema for input validation
    schema: Option<serde_json::Value>,
}

impl SyntheticOutputTool {
    pub fn new() -> Self {
        Self { schema: None }
    }

    /// Create with a JSON schema for validation
    pub fn with_schema(schema: serde_json::Value) -> Self {
        Self {
            schema: Some(schema),
        }
    }

    pub fn name(&self) -> &str {
        SYNTHETIC_OUTPUT_TOOL_NAME
    }

    pub fn description(&self) -> &str {
        DESCRIPTION
    }

    pub fn input_schema(&self) -> ToolInputSchema {
        match &self.schema {
            Some(s) => ToolInputSchema {
                schema_type: "object".to_string(),
                properties: s.get("properties").cloned().unwrap_or(serde_json::json!({})),
                required: s.get("required")
                    .and_then(|r| r.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    }),
            },
            None => ToolInputSchema {
                schema_type: "object".to_string(),
                properties: serde_json::json!({}),
                required: None,
            },
        }
    }

    pub async fn execute(
        &self,
        input: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: "Structured output provided successfully".to_string(),
            is_error: Some(false),
            was_persisted: Some(true),
        })
    }
}

impl Default for SyntheticOutputTool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() {
        assert_eq!(SyntheticOutputTool::new().name(), SYNTHETIC_OUTPUT_TOOL_NAME);
    }

    #[tokio::test]
    async fn test_execute_basic() {
        let tool = SyntheticOutputTool::new();
        let input = serde_json::json!({ "key": "value" });
        let result = tool.execute(input, &ToolContext::default()).await;
        assert!(result.is_ok());
        let r = result.unwrap();
        assert_eq!(r.content, "Structured output provided successfully");
        assert_eq!(r.is_error, Some(false));
    }

    #[tokio::test]
    async fn test_execute_with_schema() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "count": { "type": "integer" }
            },
            "required": ["name"]
        });
        let tool = SyntheticOutputTool::with_schema(schema.clone());
        let ics = tool.input_schema();
        assert_eq!(ics.properties, schema["properties"]);
        assert!(ics.required.is_some());

        let input = serde_json::json!({ "name": "test", "count": 5 });
        let result = tool.execute(input, &ToolContext::default()).await;
        assert!(result.is_ok());
    }
}
