use crate::types::*;
use std::process::Command;

pub struct BashTool;

impl BashTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        "Bash"
    }

    pub fn description(&self) -> &str {
        "Execute a shell command and return its output"
    }

    pub fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema {
            schema_type: "object".to_string(),
            properties: serde_json::json!({
                "command": { "type": "string", "description": "Shell command to execute" },
                "description": { "type": "string", "description": "What this command does" }
            }),
            required: Some(vec!["command".to_string()]),
        }
    }

    pub async fn execute(
        &self,
        input: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult, crate::error::AgentError> {
        let command = input["command"]
            .as_str()
            .ok_or_else(|| crate::error::AgentError::Tool("command required".to_string()))?
            .to_string();

        let cwd = context.cwd.clone();
        let output = tokio::task::spawn_blocking(move || {
            let mut cmd = Command::new("sh");
            cmd.arg("-c").arg(&command);
            if !cwd.is_empty() {
                cmd.current_dir(&cwd);
            }
            cmd.output()
        })
        .await
        .map_err(|e| crate::error::AgentError::Tool(e.to_string()))?
        .map_err(|e| crate::error::AgentError::Tool(e.to_string()))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let content = if !stdout.is_empty() {
            stdout.to_string()
        } else {
            stderr.to_string()
        };

        let is_error = !output.status.success();

        Ok(ToolResult {
            result_type: "tool_result".to_string(),
            tool_use_id: "".to_string(),
            content,
            is_error: Some(is_error),
            was_persisted: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_bash_tool() {
        let tool = BashTool::new();
        let result = tool
            .execute(
                serde_json::json!({"command": "echo hello"}),
                &ToolContext {
                    cwd: "/tmp".to_string(),
                    abort_signal: None,
                },
            )
            .await;
        assert!(result.is_ok());
    }
}
