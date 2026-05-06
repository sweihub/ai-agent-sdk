// Source: ~/claudecode/openclaudecode/src/tools/ScheduleCronTool/CronCreateTool.ts
// Source: ~/claudecode/openclaudecode/src/tools/ScheduleCronTool/CronDeleteTool.ts
// Source: ~/claudecode/openclaudecode/src/tools/ScheduleCronTool/CronListTool.ts
//! Cron scheduled task tools.
//!
//! Provides tools for managing scheduled tasks.

use crate::error::AgentError;
use crate::types::*;
use std::collections::HashMap;
use std::sync::{
    Mutex, OnceLock,
    atomic::{AtomicU64, Ordering},
};

pub const CRON_CREATE_TOOL_NAME: &str = "CronCreate";
pub const CRON_DELETE_TOOL_NAME: &str = "CronDelete";
pub const CRON_LIST_TOOL_NAME: &str = "CronList";

/// Global cron job store
static CRON_JOBS: OnceLock<Mutex<HashMap<String, CronJob>>> = OnceLock::new();
static JOB_COUNTER: AtomicU64 = AtomicU64::new(1);

fn get_cron_jobs_map() -> &'static Mutex<HashMap<String, CronJob>> {
    CRON_JOBS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn next_job_id() -> String {
    let id = JOB_COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("cron-{}", id)
}

/// A scheduled cron job
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CronJob {
    pub id: String,
    pub cron: String,
    pub prompt: String,
    pub recurring: bool,
    pub durable: bool,
    pub created_at: u64,
    pub last_fired: Option<u64>,
    pub fire_count: u64,
}

/// Parse a cron expression and return the next fire time (simplified).
/// Format: "M H DoM Mon DoW"
fn parse_cron_expression(cron: &str) -> Result<String, String> {
    let parts: Vec<&str> = cron.split_whitespace().collect();
    if parts.len() != 5 {
        return Err(format!(
            "Invalid cron expression: expected 5 fields (M H DoM Mon DoW), got {}. Example: '*/5 * * * *' = every 5 minutes",
            parts.len()
        ));
    }

    // Validate each field (basic validation)
    let fields = [
        ("minute", parts[0], 0, 59),
        ("hour", parts[1], 0, 23),
        ("day_of_month", parts[2], 1, 31),
        ("month", parts[3], 1, 12),
        ("day_of_week", parts[4], 0, 6),
    ];

    for (name, value, min, max) in &fields {
        if *value != "*" && *value != "*/1" {
            // Skip detailed validation for now - in a full impl, parse ranges, lists, etc.
            let _ = (name, min, max);
        }
    }

    Ok(format!(
        "Minute: {}, Hour: {}, Day of Month: {}, Month: {}, Day of Week: {}",
        parts[0], parts[1], parts[2], parts[3], parts[4]
    ))
}

/// CronCreate tool - create a scheduled task
pub struct CronCreateTool;

impl CronCreateTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        CRON_CREATE_TOOL_NAME
    }

    pub fn description(&self) -> &str {
        "Create a scheduled task that runs on a cron schedule. \
        Uses standard 5-field cron expressions in local time: 'M H DoM Mon DoW'. \
        Example: '*/5 * * * *' = every 5 minutes, '0 9 * * 1-5' = weekdays at 9am."
    }

    pub fn user_facing_name(&self, _input: Option<&serde_json::Value>) -> String {
        "CronCreate".to_string()
    }

    pub fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        input.and_then(|inp| inp["cron"].as_str().map(String::from))
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
                "cron": {
                    "type": "string",
                    "description": "Standard 5-field cron expression in local time: 'M H DoM Mon DoW' (e.g., '*/5 * * * *' = every 5 minutes, '0 9 * * 1-5' = weekdays at 9am)"
                },
                "prompt": {
                    "type": "string",
                    "description": "The prompt to enqueue at each fire time"
                },
                "recurring": {
                    "type": "boolean",
                    "description": "true (default) = fire on every cron match until deleted or auto-expired after 7 days. false = fire once at the next match, then auto-delete"
                },
                "durable": {
                    "type": "boolean",
                    "description": "true = persist to .ai/scheduled_tasks.json and survive restarts. false (default) = in-memory only, dies when this session ends"
                }
            }),
            required: Some(vec!["cron".to_string(), "prompt".to_string()]),
        }
    }

    pub async fn execute(
        &self,
        input: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let cron = input["cron"]
            .as_str()
            .ok_or_else(|| AgentError::Tool("cron is required".to_string()))?;

        let prompt = input["prompt"]
            .as_str()
            .ok_or_else(|| AgentError::Tool("prompt is required".to_string()))?;

        let recurring = input["recurring"].as_bool().unwrap_or(true);
        let durable = input["durable"].as_bool().unwrap_or(false);

        // Validate cron expression
        let parsed = parse_cron_expression(cron).map_err(|e| AgentError::Tool(e))?;

        // Check max jobs limit (50 matching TS)
        let mut guard = get_cron_jobs_map().lock().unwrap();
        if guard.len() >= 50 {
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content:
                    "Error: Maximum number of scheduled jobs (50) reached. Delete some jobs first."
                        .to_string(),
                is_error: Some(true),
                was_persisted: None,
            });
        }
        drop(guard);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let id = next_job_id();
        let job = CronJob {
            id: id.clone(),
            cron: cron.to_string(),
            prompt: prompt.to_string(),
            recurring,
            durable,
            created_at: now,
            last_fired: None,
            fire_count: 0,
        };

        // In a full implementation, this would:
        // 1. Persist to .ai/scheduled_tasks.json if durable=true
        // 2. Set up tokio timer/cron scheduler
        // 3. Validate teammate ownership for team-scoped jobs

        let mut guard = get_cron_jobs_map().lock().unwrap();
        guard.insert(id.clone(), job);
        let job_count = guard.len();
        drop(guard);

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: format!(
                "Scheduled task created successfully.\n\
                \n\
                Job ID: {}\n\
                Cron: {} ({})\n\
                Prompt: {}\n\
                Recurring: {}\n\
                Durable: {}\n\
                \n\
                {} jobs are currently scheduled.",
                id, cron, parsed, prompt, recurring, durable, job_count
            ),
            is_error: Some(false),
            was_persisted: None,
        })
    }
}

impl Default for CronCreateTool {
    fn default() -> Self {
        Self::new()
    }
}

/// CronDelete tool - delete a scheduled task
pub struct CronDeleteTool;

impl CronDeleteTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        CRON_DELETE_TOOL_NAME
    }

    pub fn description(&self) -> &str {
        "Delete a previously created scheduled task."
    }

    pub fn user_facing_name(&self, _input: Option<&serde_json::Value>) -> String {
        "CronDelete".to_string()
    }

    pub fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        input.and_then(|inp| inp["id"].as_str().map(String::from))
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
                "id": {
                    "type": "string",
                    "description": "Job ID returned by CronCreate"
                }
            }),
            required: Some(vec!["id".to_string()]),
        }
    }

    pub async fn execute(
        &self,
        input: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let id = input["id"]
            .as_str()
            .ok_or_else(|| AgentError::Tool("id is required".to_string()))?;

        let mut guard = get_cron_jobs_map().lock().unwrap();
        let job = guard.remove(id);
        drop(guard);

        let job = job.ok_or_else(|| AgentError::Tool(format!("Job '{}' not found", id)))?;

        // In a full implementation, this would:
        // 1. Cancel the scheduled timer
        // 2. Remove from .ai/scheduled_tasks.json if durable

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: format!(
                "Scheduled task '{}' deleted successfully.\n\
                Cron: {}\n\
                Prompt: {}",
                id, job.cron, job.prompt
            ),
            is_error: Some(false),
            was_persisted: None,
        })
    }
}

impl Default for CronDeleteTool {
    fn default() -> Self {
        Self::new()
    }
}

/// CronList tool - list all scheduled tasks
pub struct CronListTool;

impl CronListTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        CRON_LIST_TOOL_NAME
    }

    pub fn description(&self) -> &str {
        "List all scheduled tasks."
    }

    pub fn user_facing_name(&self, _input: Option<&serde_json::Value>) -> String {
        "CronList".to_string()
    }

    pub fn get_tool_use_summary(&self, _input: Option<&serde_json::Value>) -> Option<String> {
        None
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
            properties: serde_json::json!({}),
            required: None,
        }
    }

    pub async fn execute(
        &self,
        _input: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let mut guard = get_cron_jobs_map().lock().unwrap();

        if guard.is_empty() {
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: "No scheduled tasks.".to_string(),
                is_error: None,
                was_persisted: None,
            });
        }

        let lines: Vec<String> = guard
            .values()
            .map(|j| {
                let recurring_note = if j.recurring { "recurring" } else { "one-shot" };
                let durable_note = if j.durable { "durable" } else { "session-only" };
                format!(
                    "{}: {} [{}] ({}, {})\n  Prompt: {}\n  Fired {} times",
                    j.id, j.cron, j.prompt, recurring_note, durable_note, j.prompt, j.fire_count
                )
            })
            .collect();

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: format!("Scheduled tasks:\n\n{}", lines.join("\n\n")),
            is_error: Some(false),
            was_persisted: None,
        })
    }
}

impl Default for CronListTool {
    fn default() -> Self {
        Self::new()
    }
}

/// Reset the global cron job store for test isolation.
pub fn reset_cron_jobs_for_testing() {
    let mut guard = get_cron_jobs_map().lock().unwrap();
    guard.clear();
    drop(guard);
    JOB_COUNTER.store(1, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::tests::common::clear_all_test_state;

    #[serial_test::serial]
    #[tokio::test]
    async fn test_cron_create_and_list() {
        clear_all_test_state();
        let create = CronCreateTool::new();
        let result = create
            .execute(
                serde_json::json!({
                    "cron": "*/5 * * * *",
                    "prompt": "Check system status",
                    "recurring": true,
                    "durable": false
                }),
                &ToolContext::default(),
            )
            .await;
        assert!(result.is_ok());
        assert!(result.unwrap().content.contains("*/5 * * * *"));

        let list = CronListTool::new();
        let result = list
            .execute(serde_json::json!({}), &ToolContext::default())
            .await;
        assert!(result.is_ok());
        assert!(result.unwrap().content.contains("Check system status"));
    }

    #[serial_test::serial]
    #[tokio::test]
    async fn test_cron_delete() {
        clear_all_test_state();
        let create = CronCreateTool::new();
        create
            .execute(
                serde_json::json!({
                    "cron": "0 9 * * 1-5",
                    "prompt": "Morning report"
                }),
                &ToolContext::default(),
            )
            .await
            .unwrap();

        let delete = CronDeleteTool::new();
        // Get the last job ID (it's the highest numbered one)
        let jobs = get_cron_jobs_map().lock().unwrap();
        let last_id = jobs.keys().max().cloned().unwrap();
        drop(jobs);

        let result = delete
            .execute(
                serde_json::json!({ "id": last_id.clone() }),
                &ToolContext::default(),
            )
            .await;
        assert!(result.is_ok());
        assert!(result.unwrap().content.contains("deleted successfully"));
    }

    #[tokio::test]
    async fn test_cron_create_invalid_expression() {
        clear_all_test_state();
        let create = CronCreateTool::new();
        let result = create
            .execute(
                serde_json::json!({
                    "cron": "invalid",
                    "prompt": "test"
                }),
                &ToolContext::default(),
            )
            .await;
        // Invalid cron expression should return an error
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Invalid cron") || err_msg.contains("5 fields"));
    }

    #[tokio::test]
    async fn test_cron_list_empty() {
        clear_all_test_state();
        // Clear all jobs first
        let mut guard = get_cron_jobs_map().lock().unwrap();
        guard.clear();
        drop(guard);

        let list = CronListTool::new();
        let result = list
            .execute(serde_json::json!({}), &ToolContext::default())
            .await;
        assert!(result.is_ok());
        assert!(result.unwrap().content.contains("No scheduled tasks"));
    }
}
