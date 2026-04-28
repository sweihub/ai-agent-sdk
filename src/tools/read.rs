use crate::types::*;
use std::fs;

pub struct FileReadTool;

impl FileReadTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        "Read"
    }

    pub fn description(&self) -> &str {
        "Read files from filesystem"
    }

    pub fn user_facing_name(&self, _input: Option<&serde_json::Value>) -> String {
        "Read".to_string()
    }

    pub fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        input.and_then(|inp| inp["file_path"].as_str().map(String::from))
    }

    pub fn render_tool_result_message(
        &self,
        content: &serde_json::Value,
    ) -> Option<String> {
        let text = content["content"].as_str()?;
        let line_count = text.lines().count();
        Some(format!("{} {} {}", line_count, if line_count == 1 { "line" } else { "lines" }, "read"))
    }

    pub fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema {
            schema_type: "object".to_string(),
            properties: serde_json::json!({
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to read"
                }
            }),
            required: Some(vec!["file_path".to_string()]),
        }
    }

    pub async fn execute(
        &self,
        input: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult, crate::error::AgentError> {
        let path = input["file_path"]
            .as_str()
            .ok_or_else(|| crate::error::AgentError::Tool("file_path is required".to_string()))?;

        // Resolve relative paths using cwd from context
        let path_buf = std::path::PathBuf::from(path);

        // If path is absolute and doesn't exist, try to find it relative to cwd
        let final_path = if path_buf.is_absolute() && !path_buf.exists() {
            if let Some(filename) = path_buf.file_name() {
                std::path::Path::new(&context.cwd).join(filename)
            } else {
                std::path::Path::new(&context.cwd).join(path)
            }
        } else if path_buf.is_relative() {
            std::path::Path::new(&context.cwd).join(path)
        } else {
            path_buf
        };

        let content =
            fs::read_to_string(&final_path).map_err(|e| crate::error::AgentError::Io(e))?;

        // Extract file extension for token estimation
        let ext = final_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        // Validate content token budget (two-phase: rough estimate first, then API)
        // If rough estimate is under max/4, this returns immediately without API call
        let model = std::env::var("AI_MODEL")
            .ok()
            .unwrap_or_else(|| crate::utils::model::get_main_loop_model());
        if let Err(e) = crate::services::validate_content_tokens(
            &content, ext, None, None, None, &model,
        )
        .await
        {
            return Err(crate::error::AgentError::Tool(e.to_string()));
        }

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content,
            is_error: None,
            was_persisted: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_read_tool_name() {
        let tool = FileReadTool::new();
        assert_eq!(tool.name(), "Read");
    }

    #[test]
    fn test_file_read_tool_description_contains_read() {
        let tool = FileReadTool::new();
        assert!(tool.description().to_lowercase().contains("read"));
    }

    #[test]
    fn test_file_read_tool_has_path_in_schema() {
        let tool = FileReadTool::new();
        let schema = tool.input_schema();
        assert!(schema.properties.get("file_path").is_some());
    }

    #[tokio::test]
    async fn test_file_read_tool_execute_reads_file() {
        // Create a temp file to read
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_read_file.txt");
        std::fs::write(&temp_file, "Hello, World!").unwrap();

        let tool = FileReadTool::new();
        let input = serde_json::json!({
            "file_path": temp_file.to_str().unwrap()
        });
        let context = ToolContext::default();

        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert!(tool_result.content.contains("Hello, World!"));

        // Cleanup
        std::fs::remove_file(temp_file).ok();
    }

    #[tokio::test]
    async fn test_file_read_tool_returns_error_for_nonexistent_file() {
        let tool = FileReadTool::new();
        let input = serde_json::json!({
            "file_path": "/nonexistent/file/that/does/not/exist.txt"
        });
        let context = ToolContext::default();

        let result = tool.execute(input, &context).await;
        assert!(result.is_err());
    }
}
