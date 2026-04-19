use crate::types::*;
use std::fs;

pub struct FileReadTool;

impl FileReadTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        "FileRead"
    }

    pub fn description(&self) -> &str {
        "Read files from filesystem"
    }

    pub fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema {
            schema_type: "object".to_string(),
            properties: serde_json::json!({
                "path": {
                    "type": "string",
                    "description": "The file path to read"
                }
            }),
            required: Some(vec!["path".to_string()]),
        }
    }

    pub async fn execute(
        &self,
        input: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult, crate::error::AgentError> {
        let path = input["path"]
            .as_str()
            .ok_or_else(|| crate::error::AgentError::Tool("path is required".to_string()))?;

        // Resolve relative paths using cwd from context
        let path_buf = std::path::PathBuf::from(path);

        // If path is absolute and doesn't exist, try to find it relative to cwd
        let final_path = if path_buf.is_absolute() && !path_buf.exists() {
            // Extract filename and try relative to cwd
            if let Some(filename) = path_buf.file_name() {
                std::path::Path::new(&context.cwd).join(filename)
            } else {
                // Fallback: just use cwd for safety
                std::path::Path::new(&context.cwd).join(path)
            }
        } else if path_buf.is_relative() {
            std::path::Path::new(&context.cwd).join(path)
        } else {
            path_buf
        };

        let content =
            fs::read_to_string(&final_path).map_err(|e| crate::error::AgentError::Io(e))?;

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
        assert_eq!(tool.name(), "FileRead");
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
        assert!(schema.properties.get("path").is_some());
    }

    #[tokio::test]
    async fn test_file_read_tool_execute_reads_file() {
        // Create a temp file to read
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_read_file.txt");
        std::fs::write(&temp_file, "Hello, World!").unwrap();

        let tool = FileReadTool::new();
        let input = serde_json::json!({
            "path": temp_file.to_str().unwrap()
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
            "path": "/nonexistent/file/that/does/not/exist.txt"
        });
        let context = ToolContext::default();

        let result = tool.execute(input, &context).await;
        assert!(result.is_err());
    }
}
