// Source: ~/claudecode/openclaudecode/src/tools/NotebookEditTool/NotebookEditTool.ts
//! NotebookEdit tool - edit Jupyter notebook cells.
//!
//! Provides tools for editing Jupyter notebook (.ipynb) files.

use crate::error::AgentError;
use crate::types::*;
use std::fs;
use std::path::Path;

pub const NOTEBOOK_EDIT_TOOL_NAME: &str = "NotebookEdit";

/// Parse cell ID like "cell-5" into numeric index
fn parse_cell_id(cell_id: &str) -> Option<usize> {
    if let Some(rest) = cell_id.strip_prefix("cell-") {
        rest.parse::<usize>().ok()
    } else {
        None
    }
}

/// NotebookEdit tool - edit Jupyter notebook (.ipynb) cells
pub struct NotebookEditTool;

impl NotebookEditTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        NOTEBOOK_EDIT_TOOL_NAME
    }

    pub fn description(&self) -> &str {
        "Edit Jupyter notebook (.ipynb) cells: replace, insert, or delete cell content"
    }

    pub fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema {
            schema_type: "object".to_string(),
            properties: serde_json::json!({
                "notebook_path": {
                    "type": "string",
                    "description": "The absolute path to the Jupyter notebook file to edit (must be absolute, not relative)"
                },
                "cell_id": {
                    "type": "string",
                    "description": "The ID of the cell to edit. When inserting a new cell, the new cell will be inserted after the cell with this ID, or at the beginning if not specified."
                },
                "new_source": {
                    "type": "string",
                    "description": "The new source for the cell"
                },
                "cell_type": {
                    "type": "string",
                    "enum": ["code", "markdown"],
                    "description": "The type of the cell (code or markdown). If not specified, it defaults to the current cell type. If using edit_mode=insert, this is required."
                },
                "edit_mode": {
                    "type": "string",
                    "enum": ["replace", "insert", "delete"],
                    "description": "The type of edit to make (replace, insert, delete). Defaults to replace."
                }
            }),
            required: Some(vec!["notebook_path".to_string(), "new_source".to_string()]),
        }
    }

    pub async fn execute(
        &self,
        input: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let notebook_path = input["notebook_path"]
            .as_str()
            .ok_or_else(|| AgentError::Tool("notebook_path is required".to_string()))?;

        let new_source = input["new_source"]
            .as_str()
            .ok_or_else(|| AgentError::Tool("new_source is required".to_string()))?;

        let cell_id = input["cell_id"].as_str();
        let cell_type = input["cell_type"].as_str();
        let edit_mode = input["edit_mode"].as_str().unwrap_or("replace");

        // Validate edit_mode
        if !["replace", "insert", "delete"].contains(&edit_mode) {
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: "Error: Edit mode must be replace, insert, or delete.".to_string(),
                is_error: Some(true),
                was_persisted: None,
            });
        }

        // Cell type required for insert mode
        if edit_mode == "insert" && cell_type.is_none() {
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: "Error: Cell type is required when using edit_mode=insert.".to_string(),
                is_error: Some(true),
                was_persisted: None,
            });
        }

        // Resolve path
        let path_buf = if Path::new(notebook_path).is_absolute() {
            Path::new(notebook_path).to_path_buf()
        } else {
            Path::new(&context.cwd).join(notebook_path)
        };

        // Check .ipynb extension
        if path_buf.extension().map(|e| e.to_str()) != Some(Some("ipynb")) {
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: "Error: File must be a Jupyter notebook (.ipynb file). For editing other file types, use the FileEdit tool.".to_string(),
                is_error: Some(true),
                was_persisted: None,
            });
        }

        // Check file exists
        if !path_buf.exists() {
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: "Error: Notebook file does not exist.".to_string(),
                is_error: Some(true),
                was_persisted: None,
            });
        }

        // Read file
        let content = fs::read_to_string(&path_buf)
            .map_err(|e| AgentError::Tool(format!("Failed to read notebook: {}", e)))?;

        // Parse JSON
        let mut notebook: serde_json::Value =
            match serde_json::from_str(&content) {
                Ok(v) => v,
                Err(_) => {
                    return Ok(ToolResult {
                        result_type: "text".to_string(),
                        tool_use_id: "".to_string(),
                        content: "Error: Notebook is not valid JSON.".to_string(),
                        is_error: Some(true),
                was_persisted: None,
                    });
                }
            };

        // Get notebook metadata BEFORE getting mutable cells reference
        let language = notebook["metadata"]["language_info"]["name"]
            .as_str()
            .unwrap_or("python")
            .to_string();

        let nbformat = notebook["nbformat"].as_i64().unwrap_or(4);
        let nbformat_minor = notebook["nbformat_minor"].as_i64().unwrap_or(0);

        let cells = notebook["cells"]
            .as_array_mut()
            .ok_or_else(|| AgentError::Tool("Invalid notebook: no cells array".to_string()))?;

        let original_content = content.clone();

        // Determine cell index
        let cell_index = if cell_id.is_none() {
            if edit_mode != "insert" {
                return Ok(ToolResult {
                    result_type: "text".to_string(),
                    tool_use_id: "".to_string(),
                    content: "Error: Cell ID must be specified when not inserting a new cell.".to_string(),
                    is_error: Some(true),
                was_persisted: None,
                });
            }
            0 // Default to inserting at the beginning
        } else {
            let cid = cell_id.unwrap();
            // First try to find by actual ID
            let idx = cells.iter().position(|c| c.get("id").and_then(|v| v.as_str()) == Some(cid));
            if let Some(i) = idx {
                i
            } else {
                // Try to parse as numeric index (cell-N format)
                if let Some(parsed) = parse_cell_id(cid) {
                    if parsed >= cells.len() {
                        return Ok(ToolResult {
                            result_type: "text".to_string(),
                            tool_use_id: "".to_string(),
                            content: format!("Error: Cell with index {} does not exist in notebook.", parsed),
                            is_error: Some(true),
                was_persisted: None,
                        });
                    }
                    parsed
                } else {
                    return Ok(ToolResult {
                        result_type: "text".to_string(),
                        tool_use_id: "".to_string(),
                        content: format!("Error: Cell with ID \"{}\" not found in notebook.", cid),
                        is_error: Some(true),
                was_persisted: None,
                    });
                }
            }
        };

        let actual_cell_index = if edit_mode == "insert" {
            cell_index + 1 // Insert after the cell with this ID
        } else {
            cell_index
        };

        // Convert replace to insert if trying to replace one past the end
        let mut actual_edit_mode = edit_mode.to_string();
        let mut actual_cell_type = cell_type.map(|s| s.to_string());

        if actual_edit_mode == "replace" && actual_cell_index == cells.len() {
            actual_edit_mode = "insert".to_string();
            if actual_cell_type.is_none() {
                actual_cell_type = Some("code".to_string());
            }
        }

        let mut new_cell_id: Option<String> = None;

        // Check nbformat version for cell ID generation
        let needs_cell_ids = nbformat > 4 || (nbformat == 4 && nbformat_minor >= 5);

        if needs_cell_ids {
            if actual_edit_mode == "insert" {
                // Generate random cell ID
                new_cell_id = Some(
                    (0..13)
                        .map(|_| {
                            let c = "abcdefghijklmnopqrstuvwxyz0123456789"
                                .as_bytes()
                                [rand::random::<u8>() as usize % 36];
                            c as char
                        })
                        .collect(),
                );
            } else if let Some(cid) = cell_id {
                new_cell_id = Some(cid.to_string());
            }
        }

        match actual_edit_mode.as_str() {
            "delete" => {
                if actual_cell_index >= cells.len() {
                    return Ok(ToolResult {
                        result_type: "text".to_string(),
                        tool_use_id: "".to_string(),
                        content: format!("Error: Cell index {} out of bounds", actual_cell_index),
                        is_error: Some(true),
                was_persisted: None,
                    });
                }
                cells.remove(actual_cell_index);
            }
            "insert" => {
                let ct = actual_cell_type.as_deref().unwrap_or("code");
                let mut new_cell = serde_json::json!({
                    "cell_type": ct,
                    "source": new_source,
                    "metadata": serde_json::json!({})
                });
                if needs_cell_ids {
                    if let Some(id) = &new_cell_id {
                        new_cell["id"] = serde_json::json!(id);
                    }
                }
                if ct != "markdown" {
                    new_cell["execution_count"] = serde_json::json!(null);
                    new_cell["outputs"] = serde_json::json!([]);
                }
                cells.insert(actual_cell_index, new_cell);
            }
            "replace" => {
                if actual_cell_index >= cells.len() {
                    return Ok(ToolResult {
                        result_type: "text".to_string(),
                        tool_use_id: "".to_string(),
                        content: format!("Error: Cell index {} out of bounds", actual_cell_index),
                        is_error: Some(true),
                was_persisted: None,
                    });
                }
                let target_cell = &mut cells[actual_cell_index];
                // Set source as lines array
                let source_lines: Vec<String> = new_source
                    .lines()
                    .enumerate()
                    .map(|(i, l)| {
                        if i < new_source.lines().count() - 1 {
                            format!("{}\n", l)
                        } else {
                            l.to_string()
                        }
                    })
                    .collect();
                target_cell["source"] = serde_json::json!(source_lines);
                if target_cell.get("cell_type").and_then(|v| v.as_str()) == Some("code") {
                    // Reset execution count and clear outputs
                    target_cell["execution_count"] = serde_json::json!(null);
                    target_cell["outputs"] = serde_json::json!([]);
                }
                if let Some(ct) = &actual_cell_type {
                    if target_cell.get("cell_type").and_then(|v| v.as_str()) != Some(ct.as_str()) {
                        target_cell["cell_type"] = serde_json::json!(ct);
                    }
                }
            }
            _ => {
                return Ok(ToolResult {
                    result_type: "text".to_string(),
                    tool_use_id: "".to_string(),
                    content: format!("Error: Unknown edit mode: {}", actual_edit_mode),
                    is_error: Some(true),
                was_persisted: None,
                });
            }
        }

        // Write back to file with indent=1 (matching TS: IPYNB_INDENT = 1)
        let updated_content =
            serde_json::to_string_pretty(&notebook).map_err(|e| {
                AgentError::Tool(format!("Failed to serialize notebook: {}", e))
            })?;

        fs::write(&path_buf, &updated_content)
            .map_err(|e| AgentError::Tool(format!("Failed to write notebook: {}", e)))?;

        let result_cell_id = new_cell_id.or_else(|| cell_id.map(|s| s.to_string()));

        let display_cell_id = result_cell_id.as_deref().unwrap_or("unknown");

        let message = match actual_edit_mode.as_str() {
            "replace" => format!("Updated cell {} with {}", display_cell_id, new_source),
            "insert" => format!("Inserted cell {} with {}", display_cell_id, new_source),
            "delete" => format!("Deleted cell {}", display_cell_id),
            _ => "Unknown edit mode".to_string(),
        };

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: message,
            is_error: None,
            was_persisted: None,
        })
    }
}

impl Default for NotebookEditTool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_notebook() -> serde_json::Value {
        serde_json::json!({
            "nbformat": 4,
            "nbformat_minor": 5,
            "metadata": {
                "language_info": { "name": "python" }
            },
            "cells": [
                {
                    "cell_type": "code",
                    "execution_count": 1,
                    "metadata": {},
                    "outputs": [{"name": "stdout", "output_type": "stream", "text": ["hello\n"]}],
                    "source": ["print('hello')\n"],
                    "id": "abc123"
                },
                {
                    "cell_type": "markdown",
                    "metadata": {},
                    "source": ["# Title\n"],
                    "id": "def456"
                }
            ]
        })
    }

    #[test]
    fn test_notebook_edit_tool_name() {
        let tool = NotebookEditTool::new();
        assert_eq!(tool.name(), NOTEBOOK_EDIT_TOOL_NAME);
    }

    #[test]
    fn test_parse_cell_id() {
        assert_eq!(parse_cell_id("cell-5"), Some(5));
        assert_eq!(parse_cell_id("cell-0"), Some(0));
        assert_eq!(parse_cell_id("abc123"), None);
        assert_eq!(parse_cell_id("cell-"), None);
    }

    #[tokio::test]
    async fn test_notebook_edit_tool_replace_cell() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_nb_replace.ipynb");
        let notebook = create_test_notebook();
        std::fs::write(&temp_file, serde_json::to_string_pretty(&notebook).unwrap()).unwrap();

        let tool = NotebookEditTool::new();
        let input = serde_json::json!({
            "notebook_path": temp_file.to_str().unwrap(),
            "cell_id": "abc123",
            "new_source": "print('replaced')",
            "edit_mode": "replace"
        });
        let context = ToolContext::default();

        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());

        let content = std::fs::read_to_string(&temp_file).unwrap();
        let nb: serde_json::Value = serde_json::from_str(&content).unwrap();
        // Cell source should be updated
        assert_eq!(
            nb["cells"][0]["source"].as_array().unwrap()[0],
            "print('replaced')"
        );
        // Execution count should be reset
        assert!(nb["cells"][0]["execution_count"].is_null());
        // Outputs should be cleared
        assert!(nb["cells"][0]["outputs"].as_array().unwrap().is_empty());

        std::fs::remove_file(temp_file).ok();
    }

    #[tokio::test]
    async fn test_notebook_edit_tool_insert_cell() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_nb_insert.ipynb");
        let notebook = create_test_notebook();
        std::fs::write(&temp_file, serde_json::to_string_pretty(&notebook).unwrap()).unwrap();

        let tool = NotebookEditTool::new();
        let input = serde_json::json!({
            "notebook_path": temp_file.to_str().unwrap(),
            "cell_id": "abc123",
            "new_source": "x = 1",
            "cell_type": "code",
            "edit_mode": "insert"
        });
        let context = ToolContext::default();

        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());

        let content = std::fs::read_to_string(&temp_file).unwrap();
        let nb: serde_json::Value = serde_json::from_str(&content).unwrap();
        // Should now have 3 cells
        assert_eq!(nb["cells"].as_array().unwrap().len(), 3);
        // New cell inserted after index 0
        assert_eq!(
            nb["cells"][1]["source"].as_str().unwrap(),
            "x = 1"
        );

        std::fs::remove_file(temp_file).ok();
    }

    #[tokio::test]
    async fn test_notebook_edit_tool_delete_cell() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_nb_delete.ipynb");
        let notebook = create_test_notebook();
        std::fs::write(&temp_file, serde_json::to_string_pretty(&notebook).unwrap()).unwrap();

        let tool = NotebookEditTool::new();
        let input = serde_json::json!({
            "notebook_path": temp_file.to_str().unwrap(),
            "cell_id": "def456",
            "new_source": "",
            "edit_mode": "delete"
        });
        let context = ToolContext::default();

        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());

        let content = std::fs::read_to_string(&temp_file).unwrap();
        let nb: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(nb["cells"].as_array().unwrap().len(), 1);
        assert_eq!(nb["cells"][0]["cell_type"], "code");

        std::fs::remove_file(temp_file).ok();
    }

    #[tokio::test]
    async fn test_notebook_edit_tool_cell_id_numeric_fallback() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_nb_numeric.ipynb");
        let notebook = create_test_notebook();
        std::fs::write(&temp_file, serde_json::to_string_pretty(&notebook).unwrap()).unwrap();

        let tool = NotebookEditTool::new();
        let input = serde_json::json!({
            "notebook_path": temp_file.to_str().unwrap(),
            "cell_id": "cell-1",
            "new_source": "# Updated markdown",
            "edit_mode": "replace"
        });
        let context = ToolContext::default();

        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());

        let content = std::fs::read_to_string(&temp_file).unwrap();
        let nb: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert!(nb["cells"][1]["source"].as_array().unwrap()[0]
            .to_string()
            .contains("Updated markdown"));

        std::fs::remove_file(temp_file).ok();
    }

    #[tokio::test]
    async fn test_notebook_edit_tool_rejects_non_ipynb() {
        let tool = NotebookEditTool::new();
        let input = serde_json::json!({
            "notebook_path": "/tmp/test.txt",
            "new_source": "test",
            "edit_mode": "replace"
        });
        let context = ToolContext::default();

        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert!(tool_result.is_error.is_some() && tool_result.is_error.unwrap());
        assert!(tool_result.content.contains(".ipynb"));
    }
}
