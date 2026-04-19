use crate::types::*;
use std::fs;

pub const FILE_EDIT_TOOL_NAME: &str = "Edit";
pub const AI_FOLDER_PERMISSION_PATTERN: &str = "/.ai/**";
pub const GLOBAL_AI_FOLDER_PERMISSION_PATTERN: &str = "~/.ai/**";
pub const FILE_UNEXPECTEDLY_MODIFIED_ERROR: &str =
    "File has been unexpectedly modified. Read it again before attempting to write it.";

pub struct FileEditTool;

impl FileEditTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        "FileEdit"
    }

    pub fn description(&self) -> &str {
        "Edit files by performing exact string replacements"
    }

    pub fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema {
            schema_type: "object".to_string(),
            properties: serde_json::json!({
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to modify"
                },
                "old_string": {
                    "type": "string",
                    "description": "The exact text to find and replace"
                },
                "new_string": {
                    "type": "string",
                    "description": "The replacement text"
                },
                "replace_all": {
                    "type": "boolean",
                    "description": "Replace all occurrences (default false)"
                }
            }),
            required: Some(vec![
                "file_path".to_string(),
                "old_string".to_string(),
                "new_string".to_string(),
            ]),
        }
    }

    pub async fn execute(
        &self,
        input: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult, crate::error::AgentError> {
        let file_path = input["file_path"]
            .as_str()
            .ok_or_else(|| crate::error::AgentError::Tool("file_path is required".to_string()))?;

        let old_string = input["old_string"]
            .as_str()
            .ok_or_else(|| crate::error::AgentError::Tool("old_string is required".to_string()))?;

        let new_string = input["new_string"]
            .as_str()
            .ok_or_else(|| crate::error::AgentError::Tool("new_string is required".to_string()))?;

        let replace_all = input["replace_all"].as_bool().unwrap_or(false);

        if old_string == new_string {
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: "Error: old_string and new_string are identical".to_string(),
                is_error: Some(true),
                was_persisted: None,
            });
        }

        // Resolve relative paths using cwd from context
        let file_path = if std::path::Path::new(file_path).is_relative() {
            std::path::Path::new(&context.cwd).join(file_path)
        } else {
            std::path::PathBuf::from(file_path)
        };
        let file_path_buf = file_path.clone();

        let content =
            fs::read_to_string(&file_path).map_err(|e| crate::error::AgentError::Io(e))?;

        if !content.contains(old_string) {
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: format!("Error: old_string not found in {}. Make sure it matches exactly including whitespace.", file_path.display()),
                is_error: Some(true),
                was_persisted: None,
            });
        }

        let new_content = if replace_all {
            content.replace(old_string, new_string)
        } else {
            // Check uniqueness
            let count = content.matches(old_string).count();
            if count > 1 {
                return Ok(ToolResult {
                    result_type: "text".to_string(),
                    tool_use_id: "".to_string(),
                    content: format!("Error: old_string appears {} times in the file. Provide more context to make it unique, or set replace_all: true.", count),
                    is_error: Some(true),
                was_persisted: None,
                });
            }
            content.replacen(old_string, new_string, 1)
        };

        fs::write(&file_path_buf, &new_content).map_err(|e| crate::error::AgentError::Io(e))?;

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: format!("File edited: {}", file_path.display()),
            is_error: None,
            was_persisted: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_edit_tool_name() {
        let tool = FileEditTool::new();
        assert_eq!(tool.name(), "FileEdit");
    }

    #[test]
    fn test_file_edit_tool_description_contains_edit() {
        let tool = FileEditTool::new();
        assert!(tool.description().to_lowercase().contains("edit"));
    }

    #[test]
    fn test_file_edit_tool_has_file_path_in_schema() {
        let tool = FileEditTool::new();
        let schema = tool.input_schema();
        assert!(schema.properties.get("file_path").is_some());
    }

    #[test]
    fn test_file_edit_tool_has_old_string_in_schema() {
        let tool = FileEditTool::new();
        let schema = tool.input_schema();
        assert!(schema.properties.get("old_string").is_some());
    }

    #[test]
    fn test_file_edit_tool_has_new_string_in_schema() {
        let tool = FileEditTool::new();
        let schema = tool.input_schema();
        assert!(schema.properties.get("new_string").is_some());
    }

    #[test]
    fn test_file_edit_tool_has_replace_all_in_schema() {
        let tool = FileEditTool::new();
        let schema = tool.input_schema();
        assert!(schema.properties.get("replace_all").is_some());
    }

    #[tokio::test]
    async fn test_file_edit_tool_replaces_string() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_edit_file.txt");
        std::fs::write(&temp_file, "Hello, World!").unwrap();

        let tool = FileEditTool::new();
        let input = serde_json::json!({
            "file_path": temp_file.to_str().unwrap(),
            "old_string": "World",
            "new_string": "Rust"
        });
        let context = ToolContext::default();

        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());

        let read_content = std::fs::read_to_string(&temp_file).unwrap();
        assert_eq!(read_content, "Hello, Rust!");

        std::fs::remove_file(temp_file).ok();
    }

    #[tokio::test]
    async fn test_file_edit_tool_returns_error_for_identical_strings() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_edit_identical.txt");
        std::fs::write(&temp_file, "Hello, World!").unwrap();

        let tool = FileEditTool::new();
        let input = serde_json::json!({
            "file_path": temp_file.to_str().unwrap(),
            "old_string": "Hello",
            "new_string": "Hello"
        });
        let context = ToolContext::default();

        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert!(tool_result.is_error.is_some() && tool_result.is_error.unwrap());

        std::fs::remove_file(temp_file).ok();
    }

    #[tokio::test]
    async fn test_file_edit_tool_returns_error_for_non_existent_string() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_edit_not_found.txt");
        std::fs::write(&temp_file, "Hello, World!").unwrap();

        let tool = FileEditTool::new();
        let input = serde_json::json!({
            "file_path": temp_file.to_str().unwrap(),
            "old_string": "NonExistent",
            "new_string": "Something"
        });
        let context = ToolContext::default();

        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert!(tool_result.is_error.is_some() && tool_result.is_error.unwrap());

        std::fs::remove_file(temp_file).ok();
    }

    #[tokio::test]
    async fn test_file_edit_tool_returns_error_for_ambiguous_replacement() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_edit_ambiguous.txt");
        std::fs::write(&temp_file, "Hello World World").unwrap();

        let tool = FileEditTool::new();
        let input = serde_json::json!({
            "file_path": temp_file.to_str().unwrap(),
            "old_string": "World",
            "new_string": "Rust"
        });
        let context = ToolContext::default();

        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert!(tool_result.is_error.is_some() && tool_result.is_error.unwrap());

        std::fs::remove_file(temp_file).ok();
    }

    #[tokio::test]
    async fn test_file_edit_tool_replace_all() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_edit_all.txt");
        std::fs::write(&temp_file, "Hello World World").unwrap();

        let tool = FileEditTool::new();
        let input = serde_json::json!({
            "file_path": temp_file.to_str().unwrap(),
            "old_string": "World",
            "new_string": "Rust",
            "replace_all": true
        });
        let context = ToolContext::default();

        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());

        let read_content = std::fs::read_to_string(&temp_file).unwrap();
        assert_eq!(read_content, "Hello Rust Rust");

        std::fs::remove_file(temp_file).ok();
    }

    #[tokio::test]
    async fn test_file_edit_tool_returns_error_for_nonexistent_file() {
        let tool = FileEditTool::new();
        let input = serde_json::json!({
            "file_path": "/nonexistent/file/that/does/not/exist.txt",
            "old_string": "test",
            "new_string": "test2"
        });
        let context = ToolContext::default();

        let result = tool.execute(input, &context).await;
        assert!(result.is_err());
    }
}
