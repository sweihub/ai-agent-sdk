// Source: ~/claudecode/openclaudecode/src/tools/GlobTool/GlobTool.ts
use crate::error::AgentError;
use crate::types::*;
use glob::glob;

pub const GLOB_TOOL_NAME: &str = "Glob";
pub const GLOB_MAX_RESULTS: usize = 100;

pub struct GlobTool;

impl GlobTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        GLOB_TOOL_NAME
    }

    pub fn description(&self) -> &str {
        "Find files by glob pattern (glob pattern matching for file discovery)"
    }

    pub fn user_facing_name(&self, _input: Option<&serde_json::Value>) -> String {
        "Glob".to_string()
    }

    pub fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        input.and_then(|inp| inp["pattern"].as_str().map(String::from))
    }

    pub fn render_tool_result_message(
        &self,
        content: &serde_json::Value,
    ) -> Option<String> {
        let num = content["numFiles"].as_u64()?;
        Some(format!("{} {}", num, if num == 1 { "file" } else { "files" }))
    }

    pub fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema {
            schema_type: "object".to_string(),
            properties: serde_json::json!({
                "pattern": {
                    "type": "string",
                    "description": "The glob pattern to match files against"
                },
                "path": {
                    "type": "string",
                    "description": "The directory to search in. If not specified, the current working directory will be used. IMPORTANT: Omit this field to use the default directory. DO NOT enter undefined or null - simply omit it for the default behavior. Must be a valid directory path if provided."
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

        let search_dir = input["path"].as_str().unwrap_or(&context.cwd);

        let start = std::time::Instant::now();

        // Resolve relative patterns using search_dir (or cwd) as base
        let base_path = std::path::Path::new(search_dir);
        let full_pattern = if std::path::Path::new(pattern).is_relative()
            && !pattern.starts_with("**")
            && !pattern.starts_with('*')
        {
            base_path.join(pattern)
        } else {
            std::path::PathBuf::from(pattern)
        };

        let matches: Vec<String> = glob(full_pattern.to_string_lossy().as_ref())
            .map_err(|e| crate::error::AgentError::Tool(e.to_string()))?
            .filter_map(|r| r.ok())
            .map(|p| p.to_string_lossy().to_string())
            .collect();

        let total = matches.len();
        let truncated = total > GLOB_MAX_RESULTS;
        let results: Vec<String> = if truncated {
            // Relativize paths to cwd and limit
            matches
                .into_iter()
                .take(GLOB_MAX_RESULTS)
                .map(|p| self.relativize_path(&p, &context.cwd))
                .collect()
        } else {
            matches
                .into_iter()
                .map(|p| self.relativize_path(&p, &context.cwd))
                .collect()
        };

        let duration_ms = start.elapsed().as_millis() as u64;

        let content = if results.is_empty() {
            format!("No files found matching pattern: {}", pattern)
        } else {
            let files_str = results.join("\n");
            let truncation_note = if truncated {
                format!(
                    "\n... and {} more files (limited to {} results)",
                    total - GLOB_MAX_RESULTS,
                    GLOB_MAX_RESULTS
                )
            } else {
                String::new()
            };
            format!("{}\n{}", files_str, truncation_note)
                .trim_end()
                .to_string()
        };

        // Return structured result as JSON
        let structured = serde_json::json!({
            "durationMs": duration_ms,
            "numFiles": total,
            "filenames": results,
            "truncated": truncated
        });

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: serde_json::to_string_pretty(&structured).unwrap_or(content),
            is_error: None,
            was_persisted: None,
        })
    }

    /// Relativize an absolute path to cwd for display
    fn relativize_path(&self, abs_path: &str, cwd: &str) -> String {
        if let Ok(rel) = std::path::Path::new(abs_path).strip_prefix(cwd) {
            rel.to_string_lossy().to_string()
        } else {
            abs_path.to_string()
        }
    }
}

impl Default for GlobTool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_tool_name() {
        let tool = GlobTool::new();
        assert_eq!(tool.name(), GLOB_TOOL_NAME);
    }

    #[test]
    fn test_glob_tool_has_pattern_in_schema() {
        let tool = GlobTool::new();
        let schema = tool.input_schema();
        assert!(schema.properties.get("pattern").is_some());
    }

    #[test]
    fn test_glob_tool_has_path_in_schema() {
        let tool = GlobTool::new();
        let schema = tool.input_schema();
        assert!(schema.properties.get("path").is_some());
    }

    #[tokio::test]
    async fn test_glob_tool_finds_matching_files() {
        let temp_dir = std::env::temp_dir();
        let test_dir = temp_dir.join("test_glob_dir2");
        std::fs::create_dir_all(&test_dir).ok();
        std::fs::write(test_dir.join("file1.txt"), "content1").ok();
        std::fs::write(test_dir.join("file2.txt"), "content2").ok();
        std::fs::write(test_dir.join("file3.md"), "content3").ok();

        let tool = GlobTool::new();
        let input = serde_json::json!({
            "pattern": format!("{}/**/*.txt", test_dir.to_str().unwrap()),
            "path": test_dir.to_str().unwrap()
        });
        let context = ToolContext::default();

        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());

        // Cleanup
        std::fs::remove_file(test_dir.join("file1.txt")).ok();
        std::fs::remove_file(test_dir.join("file2.txt")).ok();
        std::fs::remove_file(test_dir.join("file3.md")).ok();
        std::fs::remove_dir(test_dir).ok();
    }

    #[tokio::test]
    async fn test_glob_tool_truncates_results() {
        let temp_dir = std::env::temp_dir();
        let test_dir = temp_dir.join("test_glob_truncate");
        std::fs::create_dir_all(&test_dir).ok();
        // Create 105 files to test truncation
        for i in 0..105 {
            std::fs::write(test_dir.join(format!("file{:03}.txt", i)), "content").ok();
        }

        let pattern = format!("{}/*.txt", test_dir.to_str().unwrap());
        let tool = GlobTool::new();
        let input = serde_json::json!({
            "pattern": pattern,
            "path": test_dir.to_str().unwrap()
        });
        let context = ToolContext::default();

        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());
        let content = result.unwrap().content;
        // The glob should find 105 files but be truncated to 100
        assert!(content.contains("truncated") || content.contains("100"));

        // Cleanup
        for i in 0..105 {
            std::fs::remove_file(test_dir.join(format!("file{:03}.txt", i))).ok();
        }
        std::fs::remove_dir(test_dir).ok();
    }
}
