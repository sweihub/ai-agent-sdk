// Source: ~/claudecode/openclaudecode/src/tools/AgentTool/loadAgentsDir.ts
#![allow(dead_code)]

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use super::agent_memory::AgentMemoryScope;

/// Type for MCP server specification in agent definitions.
#[derive(Debug, Clone)]
pub enum AgentMcpServerSpec {
    /// Reference to existing server by name
    Reference(String),
    /// Inline definition as { name: config }
    Inline {
        name: String,
        config: serde_json::Value,
    },
}

/// Base type with common fields for all agents
#[derive(Clone)]
pub struct AgentDefinition {
    pub agent_type: String,
    pub when_to_use: String,
    pub tools: Vec<String>,
    pub disallowed_tools: Vec<String>,
    pub source: String,
    pub base_dir: String,
    pub get_system_prompt: Arc<dyn Fn() -> String + Send + Sync>,
    pub model: Option<String>,
    pub max_turns: Option<usize>,
    pub permission_mode: Option<String>,
    pub effort: Option<String>,
    pub color: Option<String>,
    pub mcp_servers: Vec<AgentMcpServerSpec>,
    pub hooks: Option<serde_json::Value>,
    pub skills: Vec<String>,
    pub background: bool,
    pub initial_prompt: Option<String>,
    pub memory: Option<AgentMemoryScope>,
    pub isolation: Option<String>,
    pub required_mcp_servers: Vec<String>,
    pub omit_claude_md: bool,
    pub critical_system_reminder_experimental: Option<String>,
}

impl std::fmt::Debug for AgentDefinition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentDefinition")
            .field("agent_type", &self.agent_type)
            .field("when_to_use", &self.when_to_use)
            .field("tools", &self.tools)
            .field("disallowed_tools", &self.disallowed_tools)
            .field("source", &self.source)
            .field("base_dir", &self.base_dir)
            .field("model", &self.model)
            .field("max_turns", &self.max_turns)
            .field("permission_mode", &self.permission_mode)
            .field("effort", &self.effort)
            .field("color", &self.color)
            .field("mcp_servers", &self.mcp_servers)
            .field("skills", &self.skills)
            .field("background", &self.background)
            .field("initial_prompt", &self.initial_prompt)
            .field("memory", &self.memory)
            .field("isolation", &self.isolation)
            .field("required_mcp_servers", &self.required_mcp_servers)
            .field("omit_claude_md", &self.omit_claude_md)
            .field(
                "critical_system_reminder_experimental",
                &self.critical_system_reminder_experimental,
            )
            .finish_non_exhaustive()
    }
}

impl AgentDefinition {
    pub fn system_prompt(&self) -> String {
        (self.get_system_prompt)()
    }

    pub fn is_built_in(&self) -> bool {
        self.source == "built-in"
    }
}

/// Result from loading agent definitions.
pub struct AgentDefinitionsResult {
    pub active_agents: Vec<AgentDefinition>,
    pub all_agents: Vec<AgentDefinition>,
    pub failed_files: Vec<(String, String)>,
    pub allowed_agent_types: Option<Vec<String>>,
}

/// Get the effective list of active agents from all agents,
/// applying priority rules (built-in < plugin < user < project < flag < managed).
pub fn get_active_agents_from_list(all_agents: &[AgentDefinition]) -> Vec<AgentDefinition> {
    // Priority order: built-in < plugin < userSettings < projectSettings < flagSettings < policySettings
    let priority = [
        "built-in",
        "plugin",
        "userSettings",
        "projectSettings",
        "flagSettings",
        "policySettings",
    ];

    let mut agent_map: HashMap<String, (usize, AgentDefinition)> = HashMap::new();

    for agent in all_agents {
        let priority_idx = priority
            .iter()
            .position(|&p| p == agent.source)
            .unwrap_or(0);
        let entry = agent_map.entry(agent.agent_type.clone());
        entry
            .and_modify(|(existing_priority, existing_agent)| {
                if priority_idx > *existing_priority {
                    *existing_priority = priority_idx;
                    *existing_agent = agent.clone();
                }
            })
            .or_insert((priority_idx, agent.clone()));
    }

    agent_map.into_values().map(|(_, agent)| agent).collect()
}

/// Check if an agent's required MCP servers are available.
pub fn has_required_mcp_servers(agent: &AgentDefinition, available_servers: &[&str]) -> bool {
    if agent.required_mcp_servers.is_empty() {
        return true;
    }
    agent.required_mcp_servers.iter().all(|pattern| {
        available_servers
            .iter()
            .any(|server| server.to_lowercase().contains(&pattern.to_lowercase()))
    })
}

/// Filter agents based on MCP server requirements.
pub fn filter_agents_by_mcp_requirements<'a>(
    agents: impl IntoIterator<Item = &'a AgentDefinition>,
    available_servers: &[&str],
) -> Vec<&'a AgentDefinition> {
    agents
        .into_iter()
        .filter(|agent| has_required_mcp_servers(agent, available_servers))
        .collect()
}

/// Parse agent definition from JSON data.
pub fn parse_agent_from_json(
    name: &str,
    definition: &serde_json::Value,
    source: &str,
) -> Option<AgentDefinition> {
    let when_to_use = definition.get("description")?.as_str()?.to_string();
    let prompt = definition.get("prompt")?.as_str()?.to_string();

    let tools = definition
        .get("tools")
        .and_then(|t| t.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect()
        });

    let disallowed_tools = definition
        .get("disallowedTools")
        .and_then(|t| t.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();

    let model = definition.get("model").and_then(|m| m.as_str()).map(|m| {
        let trimmed = m.trim();
        if trimmed.to_lowercase() == "inherit" {
            "inherit".to_string()
        } else {
            trimmed.to_string()
        }
    });

    let max_turns = definition
        .get("maxTurns")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);

    let permission_mode = definition
        .get("permissionMode")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let effort = definition.get("effort").map(|v| v.to_string());

    let background = definition
        .get("background")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let memory = definition
        .get("memory")
        .and_then(|v| v.as_str())
        .and_then(AgentMemoryScope::from_str);

    let isolation = definition
        .get("isolation")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let initial_prompt = definition
        .get("initialPrompt")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let skills = definition
        .get("skills")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();

    // Build system prompt function
    let memory_prompt = if memory.is_some() {
        Some(super::agent_memory::load_agent_memory_prompt(
            name,
            memory.unwrap(),
        ))
    } else {
        None
    };

    let system_prompt = prompt.clone();
    let get_system_prompt: Arc<dyn Fn() -> String + Send + Sync> = Arc::new(move || {
        if let Some(ref mp) = memory_prompt {
            format!("{}\n\n{}", system_prompt, mp)
        } else {
            system_prompt.clone()
        }
    });

    // Convert MCP servers from JSON
    let mcp_servers = definition
        .get("mcpServers")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    if let Some(s) = item.as_str() {
                        Some(AgentMcpServerSpec::Reference(s.to_string()))
                    } else if let Some(obj) = item.as_object() {
                        if let Some(name) = obj.keys().next() {
                            Some(AgentMcpServerSpec::Inline {
                                name: name.clone(),
                                config: obj[name].clone(),
                            })
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    Some(AgentDefinition {
        agent_type: name.to_string(),
        when_to_use,
        tools: tools.unwrap_or_default(),
        disallowed_tools,
        source: source.to_string(),
        base_dir: source.to_string(),
        get_system_prompt,
        model,
        max_turns,
        permission_mode,
        effort,
        color: None,
        mcp_servers,
        hooks: definition.get("hooks").cloned(),
        skills,
        background,
        initial_prompt,
        memory,
        isolation,
        required_mcp_servers: vec![],
        omit_claude_md: false,
        critical_system_reminder_experimental: None,
    })
}

/// Parse multiple agents from a JSON object.
pub fn parse_agents_from_json(
    agents_json: &serde_json::Value,
    source: &str,
) -> Vec<AgentDefinition> {
    if let Some(obj) = agents_json.as_object() {
        obj.iter()
            .filter_map(|(name, def)| parse_agent_from_json(name, def, source))
            .collect()
    } else {
        vec![]
    }
}

/// Parse tools from frontmatter field (comma-separated or array).
pub fn parse_agent_tools_from_frontmatter(value: &serde_json::Value) -> Option<Vec<String>> {
    if let Some(arr) = value.as_array() {
        Some(
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect(),
        )
    } else if let Some(s) = value.as_str() {
        if s.is_empty() {
            return None;
        }
        Some(
            s.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
        )
    } else {
        None
    }
}

/// Parse slash-command-style tools from frontmatter (comma-separated).
pub fn parse_slash_command_tools_from_frontmatter(value: &serde_json::Value) -> Vec<String> {
    parse_agent_tools_from_frontmatter(value).unwrap_or_default()
}

/// Load agent definitions from the agents directory.
/// Scans .claude/agents/ directory for markdown files with frontmatter.
pub fn load_agents_dir(cwd: &Path) -> AgentDefinitionsResult {
    let agents_dir = cwd.join(".claude").join("agents");

    if !agents_dir.exists() {
        let built_ins = super::built_in_agents::get_built_in_agents();
        return AgentDefinitionsResult {
            active_agents: get_active_agents_from_list(&built_ins),
            all_agents: built_ins,
            failed_files: vec![],
            allowed_agent_types: None,
        };
    }

    let mut all_agents = super::built_in_agents::get_built_in_agents();
    let mut failed_files: Vec<(String, String)> = Vec::new();

    // Scan for markdown files
    if let Ok(entries) = std::fs::read_dir(&agents_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }

            match parse_agent_from_markdown(&path) {
                Some(agent) => all_agents.push(agent),
                None => {
                    failed_files.push((
                        path.display().to_string(),
                        "Failed to parse agent definition".to_string(),
                    ));
                }
            }
        }
    }

    let active_agents = get_active_agents_from_list(&all_agents);

    AgentDefinitionsResult {
        active_agents,
        all_agents,
        failed_files,
        allowed_agent_types: None,
    }
}

/// Parse an agent definition from a markdown file.
/// Extracts frontmatter from YAML frontmatter (--- delimited).
fn parse_agent_from_markdown(path: &Path) -> Option<AgentDefinition> {
    let content = std::fs::read_to_string(path).ok()?;

    // Parse YAML frontmatter
    let (frontmatter, body) = parse_markdown_frontmatter(&content)?;

    let agent_type = frontmatter.get("name")?.as_str()?.to_string();
    let when_to_use = frontmatter
        .get("description")?
        .as_str()?
        .replace("\\n", "\n");

    // Parse optional fields
    let model = frontmatter.get("model").and_then(|v| {
        v.as_str().map(|m| {
            let trimmed = m.trim();
            if trimmed.to_lowercase() == "inherit" {
                "inherit".to_string()
            } else {
                trimmed.to_string()
            }
        })
    });

    let background = frontmatter
        .get("background")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let memory = frontmatter
        .get("memory")
        .and_then(|v| v.as_str())
        .and_then(AgentMemoryScope::from_str);

    let isolation = frontmatter
        .get("isolation")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let max_turns = frontmatter
        .get("maxTurns")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);

    let permission_mode = frontmatter
        .get("permissionMode")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let effort = frontmatter.get("effort").map(|v| v.to_string());

    let initial_prompt = frontmatter
        .get("initialPrompt")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let color = frontmatter
        .get("color")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let tools = frontmatter
        .get("tools")
        .and_then(parse_agent_tools_from_frontmatter)
        .unwrap_or_default();

    let disallowed_tools = frontmatter
        .get("disallowedTools")
        .and_then(parse_agent_tools_from_frontmatter)
        .unwrap_or_default();

    let skills = parse_slash_command_tools_from_frontmatter(
        frontmatter
            .get("skills")
            .unwrap_or(&serde_json::Value::Null),
    );

    let system_prompt = body.trim().to_string();

    // Build system prompt function with optional memory integration
    let memory_prompt_val =
        memory.map(|m| super::agent_memory::load_agent_memory_prompt(&agent_type, m));

    let get_system_prompt: Arc<dyn Fn() -> String + Send + Sync> = {
        let prompt = system_prompt.clone();
        let memory_prompt = memory_prompt_val.clone();
        Arc::new(move || {
            if let Some(ref mp) = memory_prompt {
                format!("{}\n\n{}", prompt, mp)
            } else {
                prompt.clone()
            }
        })
    };

    // Parse MCP servers
    let mcp_servers = frontmatter
        .get("mcpServers")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    if let Some(s) = item.as_str() {
                        Some(AgentMcpServerSpec::Reference(s.to_string()))
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    let filename = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();

    Some(AgentDefinition {
        agent_type,
        when_to_use,
        tools,
        disallowed_tools,
        source: "userSettings".to_string(),
        base_dir: "agents".to_string(),
        get_system_prompt,
        model,
        max_turns,
        permission_mode,
        effort,
        color,
        mcp_servers,
        hooks: frontmatter.get("hooks").cloned(),
        skills,
        background,
        initial_prompt,
        memory,
        isolation,
        required_mcp_servers: vec![],
        omit_claude_md: false,
        critical_system_reminder_experimental: None,
    })
}

/// Parse YAML frontmatter from markdown content.
/// Returns (frontmatter as JSON value, body content).
fn parse_markdown_frontmatter(content: &str) -> Option<(serde_json::Value, String)> {
    let content = content.trim();
    if !content.starts_with("---") {
        return None;
    }

    let rest = &content[3..];
    let end = rest.find("---")?;
    let yaml_str = &rest[..end].trim();

    // Simple YAML parsing: handle basic key-value pairs
    let mut map: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();

    for line in yaml_str.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some(pos) = line.find(':') {
            let key = line[..pos].trim().to_string();
            let value = line[pos + 1..].trim();

            if value.is_empty() {
                continue;
            }

            let json_value = if value.starts_with('[') {
                // Parse as JSON array
                serde_json::from_str(value)
                    .ok()
                    .unwrap_or(serde_json::Value::String(value.to_string()))
            } else if value.starts_with('{') {
                // Parse as JSON object
                serde_json::from_str(value)
                    .ok()
                    .unwrap_or(serde_json::Value::String(value.to_string()))
            } else if let Ok(b) = value.parse::<bool>() {
                serde_json::Value::Bool(b)
            } else if let Ok(n) = value.parse::<u64>() {
                serde_json::Value::Number(serde_json::Number::from(n))
            } else {
                // Remove quotes if present
                let trimmed = value.trim_matches(|c: char| c == '"' || c == '\'');
                serde_json::Value::String(trimmed.to_string())
            };

            map.insert(key, json_value);
        }
    }

    let body = content[3 + end + 3..].trim().to_string();
    Some((serde_json::Value::Object(map), body))
}

/// Initialize agent memory snapshots for agents with memory enabled.
pub async fn initialize_agent_memory_snapshots(agents: &mut [AgentDefinition]) {
    for agent in agents.iter_mut() {
        if let Some(scope) = agent.memory {
            match super::agent_memory_snapshot::check_agent_memory_snapshot(
                &agent.agent_type,
                scope,
            )
            .await
            {
                super::agent_memory_snapshot::SnapshotAction::Initialize {
                    ref snapshot_timestamp,
                } => {
                    log::debug!(
                        "Initializing {} memory from project snapshot",
                        agent.agent_type
                    );
                    let _ = super::agent_memory_snapshot::initialize_from_snapshot(
                        &agent.agent_type,
                        scope,
                        snapshot_timestamp,
                    )
                    .await;
                }
                super::agent_memory_snapshot::SnapshotAction::PromptUpdate {
                    ref snapshot_timestamp,
                } => {
                    log::debug!("Newer snapshot available for {} memory", agent.agent_type);
                    // Store timestamp for later use
                    let _ = snapshot_timestamp.clone();
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_agent(agent_type: &str, source: &str) -> AgentDefinition {
        AgentDefinition {
            agent_type: agent_type.to_string(),
            when_to_use: "test".to_string(),
            tools: vec!["*".to_string()],
            disallowed_tools: vec![],
            source: source.to_string(),
            base_dir: source.to_string(),
            get_system_prompt: Arc::new(|| String::new()),
            model: None,
            max_turns: None,
            permission_mode: None,
            effort: None,
            color: None,
            mcp_servers: vec![],
            hooks: None,
            skills: vec![],
            background: false,
            initial_prompt: None,
            memory: None,
            isolation: None,
            required_mcp_servers: vec![],
            omit_claude_md: false,
            critical_system_reminder_experimental: None,
        }
    }

    #[test]
    fn test_get_active_agents_priority() {
        let agents = vec![
            make_agent("test", "built-in"),
            make_agent("test", "userSettings"),
        ];
        let active = get_active_agents_from_list(&agents);
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].source, "userSettings");
    }

    #[test]
    fn test_parse_markdown_frontmatter() {
        let content = r#"---
name: test-agent
description: A test agent
tools: [Bash, Read]
---

System prompt content"#;
        let (fm, body) = parse_markdown_frontmatter(content).unwrap();
        assert_eq!(fm["name"].as_str().unwrap(), "test-agent");
        assert_eq!(body, "System prompt content");
    }

    #[test]
    fn test_has_required_mcp_servers() {
        let agent = make_agent("test", "built-in");
        assert!(has_required_mcp_servers(&agent, &[]));

        let agent_with_req = AgentDefinition {
            required_mcp_servers: vec!["slack".to_string()],
            ..agent
        };
        assert!(has_required_mcp_servers(&agent_with_req, &["slack-api"]));
        assert!(!has_required_mcp_servers(&agent_with_req, &["other"]));
    }
}
