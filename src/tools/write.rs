use crate::types::*;
use std::fs;

pub struct FileWriteTool;

impl FileWriteTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        "FileWrite"
    }

    pub fn description(&self) -> &str {
        "Write content to files"
    }

    pub fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema {
            schema_type: "object".to_string(),
            properties: serde_json::json!({
                "path": {
                    "type": "string",
                    "description": "The file path to write to"
                },
                "content": {
                    "type": "string",
                    "description": "The content to write"
                }
            }),
            required: Some(vec!["path".to_string(), "content".to_string()]),
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

        let content = input["content"]
            .as_str()
            .ok_or_else(|| crate::error::AgentError::Tool("content is required".to_string()))?;

        // Resolve relative paths using cwd from context
        let path = if std::path::Path::new(path).is_relative() {
            std::path::Path::new(&context.cwd).join(path)
        } else {
            std::path::Path::new(path).to_path_buf()
        };

        fs::write(&path, content).map_err(|e| crate::error::AgentError::Io(e))?;

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: format!("Successfully wrote to {}", path.display()),
            is_error: None,
            was_persisted: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_write_tool_name() {
        let tool = FileWriteTool::new();
        assert_eq!(tool.name(), "FileWrite");
    }

    #[test]
    fn test_file_write_tool_description_contains_write() {
        let tool = FileWriteTool::new();
        assert!(tool.description().to_lowercase().contains("write"));
    }

    #[test]
    fn test_file_write_tool_has_path_in_schema() {
        let tool = FileWriteTool::new();
        let schema = tool.input_schema();
        assert!(schema.properties.get("path").is_some());
    }

    #[test]
    fn test_file_write_tool_has_content_in_schema() {
        let tool = FileWriteTool::new();
        let schema = tool.input_schema();
        assert!(schema.properties.get("content").is_some());
    }

    #[tokio::test]
    async fn test_file_write_tool_creates_file() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_write_file.txt");

        let tool = FileWriteTool::new();
        let input = serde_json::json!({
            "path": temp_file.to_str().unwrap(),
            "content": "Test content"
        });
        let context = ToolContext::default();

        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());

        // Verify file was created with correct content
        let read_content = std::fs::read_to_string(&temp_file).unwrap();
        assert_eq!(read_content, "Test content");

        // Cleanup
        std::fs::remove_file(temp_file).ok();
    }

    #[tokio::test]
    async fn test_file_write_tool_overwrites_existing_file() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_overwrite_file.txt");
        std::fs::write(&temp_file, "Original content").unwrap();

        let tool = FileWriteTool::new();
        let input = serde_json::json!({
            "path": temp_file.to_str().unwrap(),
            "content": "New content"
        });
        let context = ToolContext::default();

        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());

        // Verify file was overwritten
        let read_content = std::fs::read_to_string(&temp_file).unwrap();
        assert_eq!(read_content, "New content");

        // Cleanup
        std::fs::remove_file(temp_file).ok();
    }
}
