use crate::types::*;
use std::path::Path;
use tokio::process::Command;

pub struct GrepTool;

impl GrepTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        "Grep"
    }

    pub fn description(&self) -> &str {
        "Search file contents using regex patterns. Uses ripgrep (rg) if available, falls back to grep."
    }

    pub fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema {
            schema_type: "object".to_string(),
            properties: serde_json::json!({
                "pattern": {
                    "type": "string",
                    "description": "The regex pattern to search for"
                },
                "path": {
                    "type": "string",
                    "description": "File or directory to search in (defaults to cwd)"
                },
                "glob": {
                    "type": "string",
                    "description": "Glob pattern to filter files (e.g., \"*.ts\", \"*.{js,jsx}\")"
                },
                "type": {
                    "type": "string",
                    "description": "File type filter (e.g., \"ts\", \"py\", \"js\")"
                },
                "output_mode": {
                    "type": "string",
                    "enum": ["content", "files_with_matches", "count"],
                    "description": "Output mode (default: files_with_matches)"
                },
                "-i": {
                    "type": "boolean",
                    "description": "Case insensitive search"
                },
                "-n": {
                    "type": "boolean",
                    "description": "Show line numbers (default: true)"
                },
                "-A": {
                    "type": "number",
                    "description": "Lines after match"
                },
                "-B": {
                    "type": "number",
                    "description": "Lines before match"
                },
                "-C": {
                    "type": "number",
                    "description": "Context lines"
                },
                "context": {
                    "type": "number",
                    "description": "Context lines (alias for -C)"
                },
                "head_limit": {
                    "type": "number",
                    "description": "Limit output entries (default: 250)"
                }
            }),
            required: Some(vec!["pattern".to_string()]),
        }
    }

    pub async fn execute(
        &self,
        input: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult, crate::error::AgentError> {
        let pattern = input["pattern"]
            .as_str()
            .ok_or_else(|| crate::error::AgentError::Tool("pattern is required".to_string()))?;

        let search_path = input["path"]
            .as_str()
            .map(|p| {
                if Path::new(p).is_absolute() {
                    p.to_string()
                } else {
                    Path::new(&context.cwd)
                        .join(p)
                        .to_string_lossy()
                        .to_string()
                }
            })
            .unwrap_or_else(|| context.cwd.clone());

        let output_mode = input["output_mode"]
            .as_str()
            .unwrap_or("files_with_matches");

        let head_limit = input["head_limit"].as_u64().unwrap_or(250) as usize;

        // Try ripgrep first
        let result = self
            .run_rg(
                input.clone(),
                pattern,
                &search_path,
                output_mode,
                head_limit,
            )
            .await;

        match result {
            Ok(output) => Ok(output),
            Err(_) => {
                // Fall back to grep
                self.run_grep(
                    input.clone(),
                    pattern,
                    &search_path,
                    output_mode,
                    head_limit,
                )
                .await
            }
        }
    }

    async fn run_rg(
        &self,
        input: serde_json::Value,
        pattern: &str,
        search_path: &str,
        output_mode: &str,
        head_limit: usize,
    ) -> Result<ToolResult, crate::error::AgentError> {
        let mut args: Vec<String> = vec![];

        if output_mode == "files_with_matches" {
            args.push("--files-with-matches".to_string());
        } else if output_mode == "count" {
            args.push("--count".to_string());
        } else if output_mode == "content" && input["-n"].as_bool().unwrap_or(true) {
            args.push("--line-number".to_string());
        }

        if input["-i"].as_bool().unwrap_or(false) {
            args.push("--ignore-case".to_string());
        }

        if let Some(n) = input["-A"].as_u64() {
            args.push("-A".to_string());
            args.push(n.to_string());
        }

        if let Some(n) = input["-B"].as_u64() {
            args.push("-B".to_string());
            args.push(n.to_string());
        }

        let ctx = input["-C"].as_u64().or_else(|| input["context"].as_u64());
        if let Some(n) = ctx {
            args.push("-C".to_string());
            args.push(n.to_string());
        }

        if let Some(glob) = input["glob"].as_str() {
            args.push("--glob".to_string());
            args.push(glob.to_string());
        }

        if let Some(t) = input["type"].as_str() {
            args.push("--type".to_string());
            args.push(t.to_string());
        }

        args.push("--".to_string());
        args.push(pattern.to_string());
        args.push(search_path.to_string());

        let output = Command::new("rg")
            .args(&args)
            .output()
            .await
            .map_err(|e| crate::error::AgentError::Tool(e.to_string()))?;

        if !output.status.success() {
            return Err(crate::error::AgentError::Tool("rg failed".to_string()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let result = stdout.trim();

        if result.is_empty() {
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: format!("No matches found for pattern \"{}\"", pattern),
                is_error: None,
                was_persisted: None,
            });
        }

        // Apply head limit
        let content = self.apply_head_limit(result, head_limit);

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content,
            is_error: None,
            was_persisted: None,
        })
    }

    async fn run_grep(
        &self,
        input: serde_json::Value,
        pattern: &str,
        search_path: &str,
        output_mode: &str,
        head_limit: usize,
    ) -> Result<ToolResult, crate::error::AgentError> {
        let mut args: Vec<String> = vec!["-r".to_string()];

        if input["-i"].as_bool().unwrap_or(false) {
            args.push("-i".to_string());
        }

        if output_mode == "files_with_matches" {
            args.push("-l".to_string());
        } else if output_mode == "count" {
            args.push("-c".to_string());
        } else if output_mode == "content" && input["-n"].as_bool().unwrap_or(true) {
            args.push("-n".to_string());
        }

        if let Some(glob) = input["glob"].as_str() {
            args.push("--include".to_string());
            args.push(glob.to_string());
        }

        args.push("--".to_string());
        args.push(pattern.to_string());
        args.push(search_path.to_string());

        let output = Command::new("grep")
            .args(&args)
            .output()
            .await
            .map_err(|e| crate::error::AgentError::Tool(e.to_string()))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let result = stdout.trim();

        if result.is_empty() {
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: format!("No matches found for pattern \"{}\"", pattern),
                is_error: None,
                was_persisted: None,
            });
        }

        // Apply head limit
        let content = self.apply_head_limit(result, head_limit);

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content,
            is_error: None,
            was_persisted: None,
        })
    }

    fn apply_head_limit(&self, result: &str, head_limit: usize) -> String {
        if head_limit > 0 {
            let lines: Vec<&str> = result.lines().collect();
            if lines.len() > head_limit {
                let limited: Vec<&str> = lines.iter().take(head_limit).cloned().collect();
                let remaining = lines.len() - head_limit;
                return format!("{}\n... ({} more)", limited.join("\n"), remaining);
            }
        }
        result.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_grep_tool() {
        // Create test file
        tokio::fs::write("/tmp/test_grep.txt", "hello world\nfoo bar\ntest line")
            .await
            .unwrap();

        let tool = GrepTool::new();
        let result = tool.execute(
            serde_json::json!({"pattern": "hello", "path": "/tmp/test_grep.txt", "output_mode": "content"}),
            &ToolContext { cwd: "/tmp".to_string(), abort_signal: None },
        ).await;
        assert!(result.is_ok());
        let content = result.unwrap().content;
        assert!(content.contains("hello"));
    }
}
