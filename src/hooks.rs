// Source: /data/home/swei/claudecode/openclaudecode/src/commands/hooks/hooks.tsx
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Command;
use tokio::time::{Duration, timeout};

/// All supported hook events.
pub const HOOK_EVENTS: &[&str] = &[
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "Notification",
    "UserPromptSubmit",
    "SessionStart",
    "SessionEnd",
    "Stop",
    "StopFailure",
    "SubagentStart",
    "SubagentStop",
    "PreCompact",
    "PostCompact",
    "PermissionRequest",
    "PermissionDenied",
    "Setup",
    "TeammateIdle",
    "TaskCreated",
    "TaskCompleted",
    "Elicitation",
    "ElicitationResult",
    "ConfigChange",
    "WorktreeCreate",
    "WorktreeRemove",
    "InstructionsLoaded",
    "CwdChanged",
    "FileChanged",
];

/// Reasons for session end.
pub const EXIT_REASONS: &[&str] = &[
    "clear",
    "resume",
    "logout",
    "prompt_input_exit",
    "other",
    "bypass_permissions_disabled",
];

/// Reasons for loading instructions.
pub const INSTRUCTIONS_LOAD_REASONS: &[&str] = &[
    "session_start",
    "nested_traversal",
    "path_glob_match",
    "include",
    "compact",
];

/// Types of instructions memory.
pub const INSTRUCTIONS_MEMORY_TYPES: &[&str] = &["User", "Project", "Local", "Managed"];

/// Sources of config changes.
pub const CONFIG_CHANGE_SOURCES: &[&str] = &[
    "user_settings",
    "project_settings",
    "local_settings",
    "policy_settings",
    "skills",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HookEvent {
    PreToolUse,
    PostToolUse,
    PostToolUseFailure,
    Notification,
    UserPromptSubmit,
    SessionStart,
    SessionEnd,
    Stop,
    StopFailure,
    SubagentStart,
    SubagentStop,
    PreCompact,
    PostCompact,
    PermissionRequest,
    PermissionDenied,
    Setup,
    TeammateIdle,
    TaskCreated,
    TaskCompleted,
    Elicitation,
    ElicitationResult,
    ConfigChange,
    WorktreeCreate,
    WorktreeRemove,
    InstructionsLoaded,
    CwdChanged,
    FileChanged,
}

impl HookEvent {
    pub fn as_str(&self) -> &'static str {
        match self {
            HookEvent::PreToolUse => "PreToolUse",
            HookEvent::PostToolUse => "PostToolUse",
            HookEvent::PostToolUseFailure => "PostToolUseFailure",
            HookEvent::Notification => "Notification",
            HookEvent::UserPromptSubmit => "UserPromptSubmit",
            HookEvent::SessionStart => "SessionStart",
            HookEvent::SessionEnd => "SessionEnd",
            HookEvent::Stop => "Stop",
            HookEvent::StopFailure => "StopFailure",
            HookEvent::SubagentStart => "SubagentStart",
            HookEvent::SubagentStop => "SubagentStop",
            HookEvent::PreCompact => "PreCompact",
            HookEvent::PostCompact => "PostCompact",
            HookEvent::PermissionRequest => "PermissionRequest",
            HookEvent::PermissionDenied => "PermissionDenied",
            HookEvent::Setup => "Setup",
            HookEvent::TeammateIdle => "TeammateIdle",
            HookEvent::TaskCreated => "TaskCreated",
            HookEvent::TaskCompleted => "TaskCompleted",
            HookEvent::Elicitation => "Elicitation",
            HookEvent::ElicitationResult => "ElicitationResult",
            HookEvent::ConfigChange => "ConfigChange",
            HookEvent::WorktreeCreate => "WorktreeCreate",
            HookEvent::WorktreeRemove => "WorktreeRemove",
            HookEvent::InstructionsLoaded => "InstructionsLoaded",
            HookEvent::CwdChanged => "CwdChanged",
            HookEvent::FileChanged => "FileChanged",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "PreToolUse" => Some(HookEvent::PreToolUse),
            "PostToolUse" => Some(HookEvent::PostToolUse),
            "PostToolUseFailure" => Some(HookEvent::PostToolUseFailure),
            "Notification" => Some(HookEvent::Notification),
            "UserPromptSubmit" => Some(HookEvent::UserPromptSubmit),
            "SessionStart" => Some(HookEvent::SessionStart),
            "SessionEnd" => Some(HookEvent::SessionEnd),
            "Stop" => Some(HookEvent::Stop),
            "StopFailure" => Some(HookEvent::StopFailure),
            "SubagentStart" => Some(HookEvent::SubagentStart),
            "SubagentStop" => Some(HookEvent::SubagentStop),
            "PreCompact" => Some(HookEvent::PreCompact),
            "PostCompact" => Some(HookEvent::PostCompact),
            "PermissionRequest" => Some(HookEvent::PermissionRequest),
            "PermissionDenied" => Some(HookEvent::PermissionDenied),
            "Setup" => Some(HookEvent::Setup),
            "TeammateIdle" => Some(HookEvent::TeammateIdle),
            "TaskCreated" => Some(HookEvent::TaskCreated),
            "TaskCompleted" => Some(HookEvent::TaskCompleted),
            "Elicitation" => Some(HookEvent::Elicitation),
            "ElicitationResult" => Some(HookEvent::ElicitationResult),
            "ConfigChange" => Some(HookEvent::ConfigChange),
            "WorktreeCreate" => Some(HookEvent::WorktreeCreate),
            "WorktreeRemove" => Some(HookEvent::WorktreeRemove),
            "InstructionsLoaded" => Some(HookEvent::InstructionsLoaded),
            "CwdChanged" => Some(HookEvent::CwdChanged),
            "FileChanged" => Some(HookEvent::FileChanged),
            _ => None,
        }
    }
}

/// Hook definition.
#[derive(Debug, Clone)]
pub struct HookDefinition {
    /// Shell command to execute
    pub command: Option<String>,
    /// Function handler (stored as async fn pointer)
    pub timeout: Option<u64>,
    /// Tool name matcher (regex pattern)
    pub matcher: Option<String>,
}

impl<'de> Deserialize<'de> for HookDefinition {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct HookDef {
            command: Option<String>,
            timeout: Option<u64>,
            matcher: Option<String>,
        }

        let def = HookDef::deserialize(deserializer)?;
        Ok(HookDefinition {
            command: def.command,
            timeout: def.timeout.or(Some(30000)),
            matcher: def.matcher,
        })
    }
}

/// Hook input passed to handlers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookInput {
    pub event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_input: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_output: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl HookInput {
    pub fn new(event: &str) -> Self {
        Self {
            event: event.to_string(),
            tool_name: None,
            tool_input: None,
            tool_output: None,
            tool_use_id: None,
            session_id: None,
            cwd: None,
            error: None,
        }
    }
}

/// Hook output returned by handlers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_update: Option<PermissionUpdate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notification: Option<Notification>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionUpdate {
    pub tool: String,
    pub behavior: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notification {
    pub title: String,
    pub body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
}

/// Hook configuration (from settings).
pub type HookConfig = HashMap<String, Vec<HookDefinition>>;

/// Hook registry for managing and executing hooks.
#[derive(Debug, Default, Clone)]
pub struct HookRegistry {
    hooks: HashMap<String, Vec<HookDefinition>>,
}

impl HookRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            hooks: HashMap::new(),
        }
    }

    /// Register hooks from configuration.
    pub fn register_from_config(&mut self, config: HookConfig) {
        for (event, definitions) in config {
            if !HOOK_EVENTS.contains(&event.as_str()) {
                continue;
            }
            let existing = self.hooks.entry(event).or_insert_with(Vec::new);
            existing.extend(definitions);
        }
    }

    /// Register a single hook.
    pub fn register(&mut self, event: &str, definition: HookDefinition) {
        if !HOOK_EVENTS.contains(&event) {
            return;
        }
        let existing = self.hooks.entry(event.to_string()).or_insert_with(Vec::new);
        existing.push(definition);
    }

    /// Execute hooks for an event.
    pub async fn execute(&self, event: &str, mut input: HookInput) -> Vec<HookOutput> {
        let definitions = match self.hooks.get(event) {
            Some(d) => d,
            None => return vec![],
        };

        input.event = event.to_string();
        let mut results = Vec::new();

        for def in definitions {
            // Check matcher for tool-specific hooks
            if let Some(matcher) = &def.matcher {
                if let Some(tool_name) = &input.tool_name {
                    if let Ok(re) = regex::Regex::new(matcher) {
                        if !re.is_match(tool_name) {
                            continue;
                        }
                    }
                }
            }

            if let Some(command) = &def.command {
                match execute_shell_hook(command, &input, def.timeout.unwrap_or(30000)).await {
                    Ok(output) => {
                        if let Some(o) = output {
                            results.push(o);
                        }
                    }
                    Err(e) => {
                        eprintln!("[Hook] {} hook failed: {}", event, e);
                    }
                }
            }
            // Note: Function handlers would require storing function pointers,
            // which is complex in Rust. Shell commands are the primary mechanism.
        }

        results
    }

    /// Check if any hooks are registered for an event.
    pub fn has_hooks(&self, event: &str) -> bool {
        self.hooks
            .get(event)
            .map(|h| !h.is_empty())
            .unwrap_or(false)
    }

    /// Clear all hooks.
    pub fn clear(&mut self) {
        self.hooks.clear();
    }
}

/// Execute a shell command as a hook.
async fn execute_shell_hook(
    command: &str,
    input: &HookInput,
    timeout_ms: u64,
) -> Result<Option<HookOutput>, crate::error::AgentError> {
    let input_json = serde_json::to_string(input).map_err(crate::error::AgentError::Json)?;

    // Clone data needed in the blocking task
    let cmd_str = command.to_string();
    let event = input.event.clone();
    let tool_name = input.tool_name.clone();
    let session_id = input.session_id.clone();
    let cwd = input.cwd.clone();

    let result = timeout(
        Duration::from_millis(timeout_ms),
        tokio::task::spawn_blocking(move || {
            let mut cmd = Command::new("bash");
            cmd.args(["-c", &cmd_str])
                .env("HOOK_EVENT", &event)
                .env("HOOK_TOOL_NAME", tool_name.as_deref().unwrap_or(""))
                .env("HOOK_SESSION_ID", session_id.as_deref().unwrap_or(""))
                .env("HOOK_CWD", cwd.as_deref().unwrap_or(""))
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());

            let mut child = cmd.spawn()?;

            use std::io::Write;
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(input_json.as_bytes())?;
            }

            let output = child.wait_with_output()?;

            if !output.status.success() {
                return Ok(None);
            }

            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if stdout.is_empty() {
                return Ok(None);
            }

            // Try to parse as JSON
            if let Ok(hook_output) = serde_json::from_str::<HookOutput>(&stdout) {
                Ok(Some(hook_output))
            } else {
                // Non-JSON output treated as message
                Ok(Some(HookOutput {
                    message: Some(stdout),
                    permission_update: None,
                    block: None,
                    notification: None,
                }))
            }
        }),
    )
    .await;

    match result {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => {
            let err = std::io::Error::new(std::io::ErrorKind::Other, e.to_string());
            Err(crate::error::AgentError::Io(err))
        }
        Err(_) => {
            let err = std::io::Error::new(std::io::ErrorKind::TimedOut, "Hook timeout");
            Err(crate::error::AgentError::Io(err))
        }
    }
}

/// Create a default hook registry.
pub fn create_hook_registry(config: Option<HookConfig>) -> HookRegistry {
    let mut registry = HookRegistry::new();
    if let Some(c) = config {
        registry.register_from_config(c);
    }
    registry
}

/// Free function: Run PreToolUse hooks from a registry.
/// Returns Ok(true) if any hook blocked, Ok(false) otherwise.
/// Can be called from closures where &self is not available.
pub async fn run_pre_tool_use_hooks(
    registry: &HookRegistry,
    tool_name: &str,
    tool_input: &serde_json::Value,
    tool_use_id: &str,
    cwd: &str,
) -> Result<bool, crate::error::AgentError> {
    if !registry.has_hooks("PreToolUse") {
        return Ok(false);
    }
    let input = HookInput {
        event: "PreToolUse".to_string(),
        tool_name: Some(tool_name.to_string()),
        tool_input: Some(tool_input.clone()),
        tool_output: None,
        tool_use_id: Some(tool_use_id.to_string()),
        session_id: None,
        cwd: Some(cwd.to_string()),
        error: None,
    };
    let results = registry.execute("PreToolUse", input).await;
    for output in results {
        if output.block == Some(true) {
            return Err(crate::error::AgentError::Tool(format!(
                "Tool '{}' blocked by PreToolUse hook",
                tool_name
            )));
        }
    }
    Ok(false)
}

/// Free function: Run PostToolUse hooks from a registry.
pub async fn run_post_tool_use_hooks(
    registry: &HookRegistry,
    tool_name: &str,
    tool_output: &crate::types::ToolResult,
    tool_use_id: &str,
    cwd: &str,
) {
    if !registry.has_hooks("PostToolUse") {
        return;
    }
    let input = HookInput {
        event: "PostToolUse".to_string(),
        tool_name: Some(tool_name.to_string()),
        tool_input: None,
        tool_output: Some(serde_json::json!({
            "result_type": tool_output.result_type,
            "content": tool_output.content,
            "is_error": tool_output.is_error,
        })),
        tool_use_id: Some(tool_use_id.to_string()),
        session_id: None,
        cwd: Some(cwd.to_string()),
        error: None,
    };
    let _ = registry.execute("PostToolUse", input).await;
}

/// Free function: Run PostToolUseFailure hooks from a registry.
pub async fn run_post_tool_use_failure_hooks(
    registry: &HookRegistry,
    tool_name: &str,
    error: &str,
    tool_use_id: &str,
    cwd: &str,
) {
    if !registry.has_hooks("PostToolUseFailure") {
        return;
    }
    let input = HookInput {
        event: "PostToolUseFailure".to_string(),
        tool_name: Some(tool_name.to_string()),
        tool_input: None,
        tool_output: None,
        tool_use_id: Some(tool_use_id.to_string()),
        session_id: None,
        cwd: Some(cwd.to_string()),
        error: Some(error.to_string()),
    };
    let _ = registry.execute("PostToolUseFailure", input).await;
}

/// Free function: Run Stop hooks from a registry.
/// Returns prevent_continuation and any blocking error messages.
pub async fn run_stop_hooks(
    registry: &HookRegistry,
    cwd: &str,
    final_text: &str,
) -> StopHookResult {
    if !registry.has_hooks("Stop") {
        return StopHookResult::default();
    }
    let input = HookInput {
        event: "Stop".to_string(),
        tool_name: None,
        tool_input: None,
        tool_output: Some(serde_json::json!({ "text": final_text })),
        tool_use_id: None,
        session_id: None,
        cwd: Some(cwd.to_string()),
        error: None,
    };
    let results = registry.execute("Stop", input).await;
    let mut prevent_continuation = false;
    let mut blocking_errors = Vec::new();
    for output in results {
        if output.block == Some(true) {
            if let Some(msg) = output.message {
                blocking_errors.push(msg);
            }
        }
    }
    StopHookResult {
        prevent_continuation: blocking_errors.is_empty(),
        blocking_errors,
    }
}

/// Result of running Stop hooks.
#[derive(Debug, Default)]
pub struct StopHookResult {
    pub prevent_continuation: bool,
    pub blocking_errors: Vec<String>,
}

/// Free function: Run StopFailure hooks (fire-and-forget).
pub async fn run_stop_failure_hooks(
    registry: &HookRegistry,
    error: &str,
    cwd: &str,
) {
    if !registry.has_hooks("StopFailure") {
        return;
    }
    let input = HookInput {
        event: "StopFailure".to_string(),
        tool_name: None,
        tool_input: None,
        tool_output: None,
        tool_use_id: None,
        session_id: None,
        cwd: Some(cwd.to_string()),
        error: Some(error.to_string()),
    };
    let _ = registry.execute("StopFailure", input).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_event_as_str() {
        assert_eq!(HookEvent::PreToolUse.as_str(), "PreToolUse");
        assert_eq!(HookEvent::PostToolUse.as_str(), "PostToolUse");
        assert_eq!(HookEvent::SessionStart.as_str(), "SessionStart");
    }

    #[test]
    fn test_hook_event_from_str() {
        assert_eq!(
            HookEvent::from_str("PreToolUse"),
            Some(HookEvent::PreToolUse)
        );
        assert_eq!(HookEvent::from_str("Invalid"), None);
    }

    #[test]
    fn test_hook_events_constant() {
        assert!(HOOK_EVENTS.contains(&"PreToolUse"));
        assert!(HOOK_EVENTS.contains(&"PostToolUse"));
        assert!(HOOK_EVENTS.contains(&"SessionStart"));
    }

    #[test]
    fn test_hook_registry_new() {
        let registry = HookRegistry::new();
        assert!(!registry.has_hooks("PreToolUse"));
    }

    #[test]
    fn test_hook_registry_register() {
        let mut registry = HookRegistry::new();
        registry.register(
            "PreToolUse",
            HookDefinition {
                command: Some("echo test".to_string()),
                timeout: Some(5000),
                matcher: Some("Read.*".to_string()),
            },
        );
        assert!(registry.has_hooks("PreToolUse"));
    }

    #[test]
    fn test_hook_registry_clear() {
        let mut registry = HookRegistry::new();
        registry.register(
            "PreToolUse",
            HookDefinition {
                command: Some("echo test".to_string()),
                timeout: None,
                matcher: None,
            },
        );
        registry.clear();
        assert!(!registry.has_hooks("PreToolUse"));
    }

    #[test]
    fn test_hook_input_new() {
        let input = HookInput::new("PreToolUse");
        assert_eq!(input.event, "PreToolUse");
    }

    #[test]
    fn test_hook_output_serialization() {
        let output = HookOutput {
            message: Some("test message".to_string()),
            permission_update: None,
            block: Some(true),
            notification: None,
        };
        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("test message"));
    }

    #[test]
    fn test_create_hook_registry() {
        let registry = create_hook_registry(None);
        assert!(!registry.has_hooks("PreToolUse"));
    }

    #[tokio::test]
    async fn test_execute_no_hooks() {
        let registry = HookRegistry::new();
        let input = HookInput::new("PreToolUse");
        let results = registry.execute("PreToolUse", input).await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_execute_with_invalid_event() {
        let registry = HookRegistry::new();
        let input = HookInput::new("InvalidEvent");
        let results = registry.execute("InvalidEvent", input).await;
        assert!(results.is_empty());
    }
}
