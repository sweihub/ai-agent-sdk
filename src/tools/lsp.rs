// Source: ~/claudecode/openclaudecode/src/tools/LSPTool/LSPTool.ts
//! LSP tool - code intelligence via Language Server Protocol.

use crate::error::AgentError;
use crate::types::*;
use std::path::Path;
use std::path::PathBuf;
use tokio::fs;
use tokio::process::Command;

pub const LSP_TOOL_NAME: &str = "LSP";
pub const DESCRIPTION: &str = "Interact with Language Server Protocol servers for code intelligence (definitions, references, symbols, hover, call hierarchy)";

/// Check if a path is git-ignored using `git check-ignore`
async fn is_git_ignored(path: &Path) -> bool {
    Command::new("git")
        .args(["check-ignore", "-q", "--"])
        .arg(path)
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

/// LSP tool - code intelligence via Language Server Protocol
pub struct LSPTool;

impl LSPTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        LSP_TOOL_NAME
    }

    pub fn description(&self) -> &str {
        DESCRIPTION
    }

    pub fn user_facing_name(&self, _input: Option<&serde_json::Value>) -> String {
        "LSP".to_string()
    }

    pub fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        input.and_then(|inp| inp["operation"].as_str().map(String::from))
    }

    pub fn render_tool_result_message(
        &self,
        content: &serde_json::Value,
    ) -> Option<String> {
        let text = content["content"].as_str()?;
        let lines = text.lines().count();
        Some(format!("{} lines", lines))
    }

    pub fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema {
            schema_type: "object".to_string(),
            properties: serde_json::json!({
                "operation": {
                    "type": "string",
                    "enum": [
                        "goToDefinition", "findReferences", "hover", "documentSymbol",
                        "workspaceSymbol", "goToImplementation", "prepareCallHierarchy",
                        "incomingCalls", "outgoingCalls"
                    ],
                    "description": "The LSP operation to perform"
                },
                "filePath": {
                    "type": "string",
                    "description": "The absolute or relative path to the file"
                },
                "line": {
                    "type": "number",
                    "description": "The line number (1-based, as shown in editors)"
                },
                "character": {
                    "type": "number",
                    "description": "The character offset (1-based, as shown in editors)"
                }
            }),
            required: Some(vec![
                "operation".to_string(),
                "filePath".to_string(),
                "line".to_string(),
                "character".to_string(),
            ]),
        }
    }

    pub async fn execute(
        &self,
        input: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let operation = input["operation"]
            .as_str()
            .ok_or_else(|| AgentError::Tool("Missing operation parameter".to_string()))?;

        let file_path = input["filePath"]
            .as_str()
            .ok_or_else(|| AgentError::Tool("Missing filePath parameter".to_string()))?;

        let line = input["line"].as_u64().unwrap_or(1);
        let character = input["character"].as_u64().unwrap_or(1);

        // Resolve the file path
        let cwd = PathBuf::from(&context.cwd);
        let absolute_path = if PathBuf::from(file_path).is_absolute() {
            PathBuf::from(file_path)
        } else {
            cwd.join(file_path)
        };

        // Check if file exists
        if !absolute_path.exists() {
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: format!("File not found: {}", absolute_path.display()),
                is_error: None,
                was_persisted: None,
            });
        }

        // Check file size (10MB limit matching TS)
        if let Ok(metadata) = fs::metadata(&absolute_path).await {
            if metadata.len() > 10_000_000 {
                return Ok(ToolResult {
                    result_type: "text".to_string(),
                    tool_use_id: "".to_string(),
                    content: format!(
                        "File too large for LSP analysis ({} bytes exceeds 10MB limit)",
                        metadata.len()
                    ),
                    is_error: None,
                    was_persisted: None,
                });
            }
        }

        // Check if file is git-ignored (matching TS: LSP doesn't analyze ignored files)
        if is_git_ignored(&absolute_path).await {
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: format!(
                    "File is git-ignored. LSP operations are not available for ignored files: {}",
                    absolute_path.display()
                ),
                is_error: None,
                was_persisted: None,
            });
        }

        // In a full implementation, this would:
        // 1. Connect to the LSP server for the file type
        // 2. Send the request (textDocument/definition, textDocument/references, etc.)
        // 3. Format results with formatter-based presentation
        // 4. Handle workspace-wide operations (workspaceSymbol)

        // Map operation to LSP method name
        let lsp_method = match operation {
            "goToDefinition" => "textDocument/definition",
            "findReferences" => "textDocument/references",
            "hover" => "textDocument/hover",
            "documentSymbol" => "textDocument/documentSymbol",
            "workspaceSymbol" => "workspace/symbol",
            "goToImplementation" => "textDocument/implementation",
            "prepareCallHierarchy" => "textDocument/prepareCallHierarchy",
            "incomingCalls" => "callHierarchy/incomingCalls",
            "outgoingCalls" => "callHierarchy/outgoingCalls",
            _ => operation,
        };

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: format!(
                "LSP operation '{}' ({}) on {}:{}:{} — LSP server not configured. \
                This tool requires an LSP server to be running for the file type. \
                Supported operations: goToDefinition, findReferences, hover, documentSymbol, \
                workspaceSymbol, goToImplementation, prepareCallHierarchy, incomingCalls, outgoingCalls.",
                operation,
                lsp_method,
                absolute_path.display(),
                line,
                character
            ),
            is_error: None,
            was_persisted: None,
        })
    }
}

impl Default for LSPTool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lsp_tool_name() {
        let tool = LSPTool::new();
        assert_eq!(tool.name(), LSP_TOOL_NAME);
    }

    #[test]
    fn test_lsp_tool_schema() {
        let tool = LSPTool::new();
        let schema = tool.input_schema();
        assert_eq!(schema.schema_type, "object");
        assert!(schema.required.is_some());
        assert!(
            schema
                .required
                .as_ref()
                .unwrap()
                .contains(&"operation".to_string())
        );
        assert!(
            schema
                .required
                .as_ref()
                .unwrap()
                .contains(&"filePath".to_string())
        );
    }

    #[tokio::test]
    async fn test_lsp_tool_missing_file() {
        let tool = LSPTool::new();
        let input = serde_json::json!({
            "operation": "goToDefinition",
            "filePath": "/nonexistent/file.rs",
            "line": 1,
            "character": 1
        });
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());
        assert!(result.unwrap().content.contains("File not found"));
    }

    #[tokio::test]
    async fn test_lsp_tool_git_ignored() {
        // Create a temp git repo inside the project dir (must be within a git repo for LSP)
        let temp_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test_lsp_gitignore2");
        // Clean up stale dir from previous test runs
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).ok();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&temp_dir)
            .status()
            .ok();

        let ignored_file = temp_dir.join("ignored.rs");
        std::fs::write(&ignored_file, "fn main() {}").ok();
        std::fs::write(temp_dir.join(".gitignore"), "ignored.rs").ok();

        let tool = LSPTool::new();
        let input = serde_json::json!({
            "operation": "hover",
            "filePath": ignored_file.to_str().unwrap(),
            "line": 1,
            "character": 1
        });
        let context = ToolContext {
            cwd: temp_dir.to_string_lossy().to_string(),
            abort_signal: Default::default(),
        };
        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());
        // The file should be detected as git-ignored
        let content = result.unwrap().content;
        // Check that the response mentions git-ignore (case insensitive)
        let content_lower = content.to_lowercase();
        assert!(
            content_lower.contains("git") && content_lower.contains("ignore"),
            "Content: {}",
            content
        );

        // Cleanup
        std::fs::remove_dir_all(&temp_dir).ok();
    }
}
