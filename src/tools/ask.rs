// Source: ~/claudecode/openclaudecode/src/tools/AskUserQuestionTool/prompt.ts
//! Ask user question tool.
//!
//! Provides tool for asking the user for input with multiple choice options.

use crate::error::AgentError;
use crate::types::*;

pub const ASK_USER_QUESTION_TOOL_NAME: &str = "AskUserQuestion";

/// AskUserQuestion tool - ask the user for input
pub struct AskUserQuestionTool;

impl AskUserQuestionTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        ASK_USER_QUESTION_TOOL_NAME
    }

    pub fn description(&self) -> &str {
        "Ask the user a question with multiple choice options. Use this when you need user input to proceed."
    }

    pub fn user_facing_name(&self, _input: Option<&serde_json::Value>) -> String {
        "AskUserQuestion".to_string()
    }

    pub fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        input.and_then(|inp| inp["question"].as_str().map(String::from))
    }

    pub fn render_tool_result_message(
        &self,
        content: &serde_json::Value,
    ) -> Option<String> {
        content["content"].as_str().map(|s| s.to_string())
    }

    pub fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema {
            schema_type: "object".to_string(),
            properties: serde_json::json!({
                "question": {
                    "type": "string",
                    "description": "The complete question to ask the user"
                },
                "header": {
                    "type": "string",
                    "description": "Very short label displayed as a chip/tag (max 12 chars)"
                },
                "options": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "label": { "type": "string", "description": "The display text for this option (1-5 words)" },
                            "description": { "type": "string", "description": "Explanation of what this option means or what will happen if chosen" }
                        },
                        "required": ["label", "description"]
                    },
                    "description": "Available choices for this question. Must have 2-4 options."
                },
                "multiSelect": {
                    "type": "boolean",
                    "description": "Set to true to allow the user to select multiple options instead of just one"
                },
                "preview": {
                    "type": "object",
                    "properties": {
                        "type": { "type": "string", "enum": ["html", "markdown"] },
                        "content": { "type": "string" }
                    },
                    "description": "Optional HTML or Markdown preview to show the user alongside the question"
                }
            }),
            required: Some(vec![
                "question".to_string(),
                "header".to_string(),
                "options".to_string(),
            ]),
        }
    }

    pub async fn execute(
        &self,
        input: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let question = input["question"]
            .as_str()
            .ok_or_else(|| AgentError::Tool("question is required".to_string()))?;

        let header = input["header"]
            .as_str()
            .ok_or_else(|| AgentError::Tool("header is required".to_string()))?;

        let options = input["options"]
            .as_array()
            .ok_or_else(|| AgentError::Tool("options is required".to_string()))?;

        if options.len() < 2 || options.len() > 4 {
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: "Error: options must have between 2 and 4 choices.".to_string(),
                is_error: Some(true),
                was_persisted: None,
            });
        }

        let multi_select = input["multiSelect"].as_bool().unwrap_or(false);

        // Format options for display
        let option_lines: Vec<String> = options
            .iter()
            .filter_map(|v| {
                let label = v.get("label")?.as_str()?;
                let desc = v.get("description").and_then(|v| v.as_str()).unwrap_or("");
                Some(format!("  - {}: {}", label, desc))
            })
            .collect();

        let multi_select_note = if multi_select {
            "\n(Note: multiple selections are allowed)"
        } else {
            ""
        };

        let preview_note = if let Some(preview) = input.get("preview") {
            let preview_type = preview.get("type").and_then(|v| v.as_str()).unwrap_or("");
            format!("\n[{} preview provided]", preview_type)
        } else {
            String::new()
        };

        let response = format!(
            "Asking user: {}\n\n\
            Options: {}\n\n{}\
            {}\n\
            {}\n\n\
            Note: In a full implementation, this would present a UI dialog to the user\n\
            and wait for their response. The selected option(s) would be returned\n\
            as the tool result.",
            question,
            options.len(),
            option_lines.join("\n"),
            multi_select_note,
            preview_note
        );

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "ask_user_question".to_string(),
            content: response,
            is_error: Some(false),
            was_persisted: None,
        })
    }
}

impl Default for AskUserQuestionTool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ask_user_question_name() {
        let tool = AskUserQuestionTool::new();
        assert_eq!(tool.name(), ASK_USER_QUESTION_TOOL_NAME);
    }

    #[test]
    fn test_ask_user_question_schema() {
        let tool = AskUserQuestionTool::new();
        let schema = tool.input_schema();
        assert!(schema.properties.get("question").is_some());
        assert!(schema.properties.get("header").is_some());
        assert!(schema.properties.get("options").is_some());
        assert!(schema.properties.get("multiSelect").is_some());
        assert!(schema.properties.get("preview").is_some());
    }

    #[tokio::test]
    async fn test_ask_user_question_requires_options() {
        let tool = AskUserQuestionTool::new();
        let input = serde_json::json!({
            "question": "Test?",
            "header": "Test"
        });
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        // Missing options returns an Err result
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("options is required"));
    }

    #[tokio::test]
    async fn test_ask_user_question_valid_options() {
        let tool = AskUserQuestionTool::new();
        let input = serde_json::json!({
            "question": "Which approach?",
            "header": "Approach",
            "options": [
                { "label": "Option A", "description": "First approach" },
                { "label": "Option B", "description": "Second approach" }
            ],
            "multiSelect": false
        });
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());
        let content = result.unwrap().content;
        assert!(content.contains("Which approach?"));
        assert!(content.contains("Option A"));
        assert!(content.contains("Option B"));
    }

    #[tokio::test]
    async fn test_ask_user_question_too_few_options() {
        let tool = AskUserQuestionTool::new();
        let input = serde_json::json!({
            "question": "Which approach?",
            "header": "Approach",
            "options": [
                { "label": "Only One", "description": "Single option" }
            ]
        });
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        assert!(result.unwrap().content.contains("2 and 4"));
    }
}
