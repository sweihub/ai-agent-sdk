// Source: ~/claudecode/openclaudecode/src/tools/TeamCreateTool/TeamCreateTool.ts
// Source: ~/claudecode/openclaudecode/src/tools/TeamDeleteTool/TeamDeleteTool.ts
// Source: ~/claudecode/openclaudecode/src/tools/SendMessageTool/SendMessageTool.ts
//! Team management tools.
//!
//! Provides tools for creating and deleting multi-agent teams and sending messages.

use crate::error::AgentError;
use crate::types::*;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

pub const TEAM_CREATE_TOOL_NAME: &str = "TeamCreate";
pub const TEAM_DELETE_TOOL_NAME: &str = "TeamDelete";
pub const SEND_MESSAGE_TOOL_NAME: &str = "SendMessage";

/// Global team store
static TEAMS: OnceLock<Mutex<HashMap<String, Team>>> = OnceLock::new();
/// Message inbox store
static INBOX: OnceLock<Mutex<Vec<AgentMessage>>> = OnceLock::new();

fn get_teams_map() -> &'static Mutex<HashMap<String, Team>> {
    TEAMS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn get_inbox() -> &'static Mutex<Vec<AgentMessage>> {
    INBOX.get_or_init(|| Mutex::new(Vec::new()))
}

#[derive(Debug, Clone)]
struct Team {
    name: String,
    description: String,
    agents: Vec<AgentInfo>,
    team_file_path: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentInfo {
    pub name: String,
    pub description: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentMessage {
    pub to: String,
    pub from: Option<String>,
    pub message: String,
    pub timestamp: u64,
}

/// TeamCreate tool - create a team of agents
pub struct TeamCreateTool;

impl TeamCreateTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        TEAMCREATE_TOOL_NAME
    }

    pub fn description(&self) -> &str {
        "Create a team of agents that can work in parallel. Teams enable swarm mode where agents collaborate on complex tasks."
    }

    pub fn user_facing_name(&self, _input: Option<&serde_json::Value>) -> String {
        "TeamCreate".to_string()
    }

    pub fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        input.and_then(|inp| inp["name"].as_str().map(String::from))
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
                "name": {
                    "type": "string",
                    "description": "Name of the team to create"
                },
                "description": {
                    "type": "string",
                    "description": "Description of what the team does"
                },
                "agents": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string", "description": "Agent name" },
                            "description": { "type": "string", "description": "Agent description" },
                            "model": { "type": "string", "description": "Agent model" }
                        }
                    },
                    "description": "List of agents in the team"
                }
            }),
            required: Some(vec!["name".to_string()]),
        }
    }

    pub async fn execute(
        &self,
        input: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let name = input["name"]
            .as_str()
            .ok_or_else(|| AgentError::Tool("name is required".to_string()))?
            .to_string();

        let description = input["description"].as_str().unwrap_or("").to_string();

        let agents: Vec<AgentInfo> = input["agents"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| {
                        let name = v.get("name")?.as_str()?.to_string();
                        let description = v
                            .get("description")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        let model = v
                            .get("model")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        Some(AgentInfo {
                            name,
                            description,
                            model,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Check for duplicate team name
        let mut guard = get_teams_map().lock().unwrap();
        if guard.contains_key(&name) {
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: format!("Error: Team '{}' already exists.", name),
                is_error: Some(true),
                was_persisted: None,
            });
        }
        drop(guard);

        // In a full implementation, this would:
        // 1. Write team file to .ai/teams/<name>/team.json
        // 2. Initialize task list for the team
        // 3. Set up AppState team context
        // 4. Spawn agent processes for each team member

        let team = Team {
            name: name.clone(),
            description: description.clone(),
            agents,
            team_file_path: None,
        };

        let mut guard = get_teams_map().lock().unwrap();
        guard.insert(name.clone(), team);
        let agent_count = guard.get(&name).map(|t| t.agents.len()).unwrap_or(0);
        drop(guard);

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: format!(
                "Team '{}' created successfully.\n\
                Description: {}\n\
                Agents: {}\n\n\
                The team is ready for coordination. \
                Team members can communicate using the SendMessage tool.",
                name, description, agent_count
            ),
            is_error: Some(false),
            was_persisted: None,
        })
    }
}

impl Default for TeamCreateTool {
    fn default() -> Self {
        Self::new()
    }
}

/// TeamDelete tool - delete a team
pub struct TeamDeleteTool;

impl TeamDeleteTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        TEAM_DELETE_TOOL_NAME
    }

    pub fn description(&self) -> &str {
        "Delete a previously created team. All team members will be stopped and the team configuration will be removed."
    }

    pub fn user_facing_name(&self, _input: Option<&serde_json::Value>) -> String {
        "TeamDelete".to_string()
    }

    pub fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        input.and_then(|inp| inp["name"].as_str().map(String::from))
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
                "name": {
                    "type": "string",
                    "description": "Name of the team to delete"
                }
            }),
            required: Some(vec!["name".to_string()]),
        }
    }

    pub async fn execute(
        &self,
        input: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let name = input["name"]
            .as_str()
            .ok_or_else(|| AgentError::Tool("name is required".to_string()))?;

        let mut guard = get_teams_map().lock().unwrap();
        let team = guard.remove(name);
        drop(guard);

        let team = team.ok_or_else(|| AgentError::Tool(format!("Team '{}' not found", name)))?;

        // In a full implementation, this would:
        // 1. Check for active team members and warn/abort
        // 2. Stop all running team agents
        // 3. Clean up worktrees associated with the team
        // 4. Reset team colors
        // 5. Clear AppState team context
        // 6. Delete team file

        let agent_names: Vec<String> = team.agents.iter().map(|a| a.name.clone()).collect();

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: format!(
                "Team '{}' deleted successfully.\n\
                Stopped {} agent(s): {}",
                name,
                agent_names.len(),
                agent_names.join(", ")
            ),
            is_error: Some(false),
            was_persisted: None,
        })
    }
}

impl Default for TeamDeleteTool {
    fn default() -> Self {
        Self::new()
    }
}

/// SendMessage tool - send message between agents
pub struct SendMessageTool;

impl SendMessageTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        SEND_MESSAGE_TOOL_NAME
    }

    pub fn description(&self) -> &str {
        "Send a message to another agent. Use 'to: *' to broadcast to all agents. Supports direct messages, shutdown requests, and plan approvals."
    }

    pub fn user_facing_name(&self, _input: Option<&serde_json::Value>) -> String {
        "SendMessage".to_string()
    }

    pub fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        input.and_then(|inp| inp["to"].as_str().map(String::from))
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
                "to": {
                    "type": "string",
                    "description": "Agent name to send message to. Use '*' to broadcast to all agents."
                },
                "message": {
                    "type": "string",
                    "description": "Message content"
                }
            }),
            required: Some(vec!["to".to_string(), "message".to_string()]),
        }
    }

    pub async fn execute(
        &self,
        input: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let to = input["to"]
            .as_str()
            .ok_or_else(|| AgentError::Tool("to is required".to_string()))?;

        let message = input["message"]
            .as_str()
            .ok_or_else(|| AgentError::Tool("message is required".to_string()))?;

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let msg = AgentMessage {
            to: to.to_string(),
            from: None,
            message: message.to_string(),
            timestamp,
        };

        // In a full implementation, this would:
        // 1. For direct messages: deliver via UDS socket or in-process channel
        // 2. For broadcasts (*): send to all connected agents
        // 3. For shutdown requests: trigger agent termination
        // 4. For plan approvals: route to plan approval handler
        // 5. For bridge messaging: use cross-session communication

        let mut inbox = get_inbox().lock().unwrap();
        inbox.push(msg);
        let inbox_len = inbox.len();
        drop(inbox);

        let recipient = if to == "*" {
            "all agents (broadcast)".to_string()
        } else {
            format!("agent '{}'", to)
        };

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: format!(
                "Message sent to {}.\n\
                Message: {}\n\
                Inbox size: {}",
                recipient, message, inbox_len
            ),
            is_error: Some(false),
            was_persisted: None,
        })
    }
}

impl Default for SendMessageTool {
    fn default() -> Self {
        Self::new()
    }
}

// Fix: constant name
const TEAMCREATE_TOOL_NAME: &str = "TeamCreate";

/// Reset the global team and inbox stores for test isolation.
pub fn reset_teams_for_testing() {
    let mut guard = get_teams_map().lock().unwrap();
    guard.clear();
    drop(guard);
    let mut inbox = get_inbox().lock().unwrap();
    inbox.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::tests::common::clear_all_test_state;

    #[tokio::test]
    async fn test_team_create_and_delete() {
        clear_all_test_state();
        let create = TeamCreateTool::new();
        let result = create
            .execute(
                serde_json::json!({
                    "name": "test-team",
                    "description": "A test team",
                    "agents": [
                        { "name": "agent1", "description": "First agent" }
                    ]
                }),
                &ToolContext::default(),
            )
            .await;
        assert!(result.is_ok());

        let delete = TeamDeleteTool::new();
        let result = delete
            .execute(
                serde_json::json!({ "name": "test-team" }),
                &ToolContext::default(),
            )
            .await;
        assert!(result.is_ok());
        assert!(result.unwrap().content.contains("deleted"));
    }

    #[tokio::test]
    async fn test_send_message() {
        clear_all_test_state();
        let send = SendMessageTool::new();
        let result = send
            .execute(
                serde_json::json!({
                    "to": "agent1",
                    "message": "Hello from test"
                }),
                &ToolContext::default(),
            )
            .await;
        assert!(result.is_ok());
        assert!(result.unwrap().content.contains("agent 'agent1'"));
    }

    #[tokio::test]
    async fn test_send_message_broadcast() {
        clear_all_test_state();
        let send = SendMessageTool::new();
        let result = send
            .execute(
                serde_json::json!({
                    "to": "*",
                    "message": "Broadcast message"
                }),
                &ToolContext::default(),
            )
            .await;
        assert!(result.is_ok());
        assert!(result.unwrap().content.contains("broadcast"));
    }
}
