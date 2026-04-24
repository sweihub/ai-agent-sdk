//! Permission management for agent tool access control.
//!
//! This module provides a permission system similar to claude code's permissions,
//! with support for permission modes, rules, and decisions.

use serde::{Deserialize, Serialize};

/// Permission behavior - what to do when a tool is used
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PermissionBehavior {
    /// Always allow the tool
    Allow,
    /// Always deny the tool
    Deny,
    /// Ask the user for permission
    #[default]
    Ask,
}

impl PermissionBehavior {
    /// Get string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            PermissionBehavior::Allow => "allow",
            PermissionBehavior::Deny => "deny",
            PermissionBehavior::Ask => "ask",
        }
    }
}

/// Permission mode - controls how permissions are handled globally
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PermissionMode {
    /// Default mode - ask for permission
    #[default]
    Default,
    /// Accept edits without asking
    AcceptEdits,
    /// Bypass all permission checks
    Bypass,
    /// Deny all without asking
    DontAsk,
    /// Plan mode - for planning operations
    Plan,
    /// Auto mode - automatically decide based on context
    Auto,
    /// Bubble mode - prompt-free for most operations, escalate on certain patterns
    Bubble,
}

/// Source of a permission rule
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionRuleSource {
    /// User-level settings (~/.ai/)
    UserSettings,
    /// Project-level settings (./.ai/)
    ProjectSettings,
    /// Local settings (./.ai.local/)
    LocalSettings,
    /// From CLI arguments
    CliArg,
    /// From command/session
    Session,
    /// From policy
    Policy,
    /// From flag settings
    FlagSettings,
}

/// A permission rule - specifies behavior for a tool
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PermissionRule {
    /// Source of this rule
    pub source: PermissionRuleSource,
    /// Behavior (allow/deny/ask)
    pub behavior: PermissionBehavior,
    /// The tool name this rule applies to
    pub tool_name: String,
    /// Optional content pattern to match
    pub rule_content: Option<String>,
}

impl PermissionRule {
    /// Create a new permission rule
    pub fn new(tool_name: &str, behavior: PermissionBehavior) -> Self {
        Self {
            source: PermissionRuleSource::UserSettings,
            behavior,
            tool_name: tool_name.to_string(),
            rule_content: None,
        }
    }

    /// Create a rule with content pattern
    pub fn with_content(tool_name: &str, behavior: PermissionBehavior, content: &str) -> Self {
        Self {
            source: PermissionRuleSource::UserSettings,
            behavior,
            tool_name: tool_name.to_string(),
            rule_content: Some(content.to_string()),
        }
    }

    /// Create an allow rule
    pub fn allow(tool_name: &str) -> Self {
        Self::new(tool_name, PermissionBehavior::Allow)
    }

    /// Create a deny rule
    pub fn deny(tool_name: &str) -> Self {
        Self::new(tool_name, PermissionBehavior::Deny)
    }

    /// Create an ask rule
    pub fn ask(tool_name: &str) -> Self {
        Self::new(tool_name, PermissionBehavior::Ask)
    }
}

/// Permission metadata for a tool request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionMetadata {
    /// Tool name
    pub tool_name: String,
    /// Tool description
    pub description: Option<String>,
    /// The input/arguments to the tool
    pub input: Option<serde_json::Value>,
    /// Current working directory
    pub cwd: Option<String>,
}

impl PermissionMetadata {
    /// Create new metadata
    pub fn new(tool_name: &str) -> Self {
        Self {
            tool_name: tool_name.to_string(),
            description: None,
            input: None,
            cwd: None,
        }
    }

    /// Set description
    pub fn with_description(mut self, description: &str) -> Self {
        self.description = Some(description.to_string());
        self
    }

    /// Set input
    pub fn with_input(mut self, input: serde_json::Value) -> Self {
        self.input = Some(input);
        self
    }

    /// Set cwd
    pub fn with_cwd(mut self, cwd: &str) -> Self {
        self.cwd = Some(cwd.to_string());
        self
    }
}

/// Reason for a permission decision
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PermissionDecisionReason {
    /// Matched a permission rule
    Rule { rule: PermissionRule },
    /// Determined by permission mode
    Mode { mode: PermissionMode },
    /// From a hook
    Hook {
        hook_name: String,
        reason: Option<String>,
    },
    /// Sandbox override
    SandboxOverride { reason: String },
    /// Safety check failed
    SafetyCheck { reason: String },
    /// Other reason
    Other { reason: String },
}

/// Result when permission is allowed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionAllowDecision {
    pub behavior: PermissionBehavior,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_input: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_modified: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_reason: Option<PermissionDecisionReason>,
}

impl PermissionAllowDecision {
    /// Create an allow decision
    pub fn new() -> Self {
        Self {
            behavior: PermissionBehavior::Allow,
            updated_input: None,
            user_modified: None,
            decision_reason: None,
        }
    }

    /// Create with reason
    pub fn with_reason(mut self, reason: PermissionDecisionReason) -> Self {
        self.decision_reason = Some(reason);
        self
    }
}

impl Default for PermissionAllowDecision {
    fn default() -> Self {
        Self::new()
    }
}

/// Result when permission should be asked
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionAskDecision {
    pub behavior: PermissionBehavior,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_input: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_reason: Option<PermissionDecisionReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_path: Option<String>,
}

impl PermissionAskDecision {
    /// Create an ask decision with message
    pub fn new(message: &str) -> Self {
        Self {
            behavior: PermissionBehavior::Ask,
            message: message.to_string(),
            updated_input: None,
            decision_reason: None,
            blocked_path: None,
        }
    }

    /// Create with reason
    pub fn with_reason(mut self, reason: PermissionDecisionReason) -> Self {
        self.decision_reason = Some(reason);
        self
    }

    /// Create with blocked path
    pub fn with_blocked_path(mut self, path: &str) -> Self {
        self.blocked_path = Some(path.to_string());
        self
    }
}

/// Result when permission is denied
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionDenyDecision {
    pub behavior: PermissionBehavior,
    pub message: String,
    pub decision_reason: PermissionDecisionReason,
}

impl PermissionDenyDecision {
    /// Create a deny decision with message
    pub fn new(message: &str, reason: PermissionDecisionReason) -> Self {
        Self {
            behavior: PermissionBehavior::Deny,
            message: message.to_string(),
            decision_reason: reason,
        }
    }
}

/// A permission decision - allow, ask, or deny
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "behavior", rename_all = "lowercase")]
pub enum PermissionDecision {
    Allow(PermissionAllowDecision),
    Ask(PermissionAskDecision),
    Deny(PermissionDenyDecision),
}

impl PermissionDecision {
    /// Check if allowed
    pub fn is_allowed(&self) -> bool {
        matches!(self, PermissionDecision::Allow(_))
    }

    /// Check if denied
    pub fn is_denied(&self) -> bool {
        matches!(self, PermissionDecision::Deny(_))
    }

    /// Check if asking
    pub fn is_ask(&self) -> bool {
        matches!(self, PermissionDecision::Ask(_))
    }

    /// Get the message if present
    pub fn message(&self) -> Option<&str> {
        match self {
            PermissionDecision::Allow(_) => None,
            PermissionDecision::Ask(d) => Some(&d.message),
            PermissionDecision::Deny(d) => Some(&d.message),
        }
    }
}

/// Permission result with additional passthrough option
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "behavior", rename_all = "lowercase")]
pub enum PermissionResult {
    Allow(PermissionAllowDecision),
    Ask(PermissionAskDecision),
    Deny(PermissionDenyDecision),
    /// Passthrough - allow but log/notify
    Passthrough {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        decision_reason: Option<PermissionDecisionReason>,
    },
}

impl PermissionResult {
    /// Convert to decision
    pub fn to_decision(self) -> Option<PermissionDecision> {
        match self {
            PermissionResult::Allow(d) => Some(PermissionDecision::Allow(d)),
            PermissionResult::Ask(d) => Some(PermissionDecision::Ask(d)),
            PermissionResult::Deny(d) => Some(PermissionDecision::Deny(d)),
            PermissionResult::Passthrough { .. } => None,
        }
    }

    /// Check if allowed (including passthrough)
    pub fn is_allowed(&self) -> bool {
        matches!(
            self,
            PermissionResult::Allow(_) | PermissionResult::Passthrough { .. }
        )
    }

    /// Check if denied
    pub fn is_denied(&self) -> bool {
        matches!(self, PermissionResult::Deny(_))
    }

    /// Check if asking
    pub fn is_ask(&self) -> bool {
        matches!(self, PermissionResult::Ask(_))
    }

    /// Get the message
    pub fn message(&self) -> Option<&str> {
        match self {
            PermissionResult::Allow(_) => None,
            PermissionResult::Ask(d) => Some(&d.message),
            PermissionResult::Deny(d) => Some(&d.message),
            PermissionResult::Passthrough { message, .. } => Some(message),
        }
    }
}

/// Permission context for checking tool access
pub struct PermissionContext {
    /// Current permission mode
    pub mode: PermissionMode,
    /// Always allow rules
    pub allow_rules: Vec<PermissionRule>,
    /// Always deny rules
    pub deny_rules: Vec<PermissionRule>,
    /// Always ask rules
    pub ask_rules: Vec<PermissionRule>,
    /// Denial tracking state
    pub denial_tracking: std::sync::RwLock<crate::utils::permissions::denial_tracking::DenialTrackingState>,
}

impl std::fmt::Debug for PermissionContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PermissionContext")
            .field("mode", &self.mode)
            .field("allow_rules", &self.allow_rules)
            .field("deny_rules", &self.deny_rules)
            .field("ask_rules", &self.ask_rules)
            .finish_non_exhaustive()
    }
}

impl Clone for PermissionContext {
    fn clone(&self) -> Self {
        let dt = self.denial_tracking.read().map(|dt| *dt).unwrap_or_default();
        Self {
            mode: self.mode,
            allow_rules: self.allow_rules.clone(),
            deny_rules: self.deny_rules.clone(),
            ask_rules: self.ask_rules.clone(),
            denial_tracking: std::sync::RwLock::new(dt),
        }
    }
}

impl Default for PermissionContext {
    fn default() -> Self {
        Self {
            mode: PermissionMode::default(),
            allow_rules: Vec::new(),
            deny_rules: Vec::new(),
            ask_rules: Vec::new(),
            denial_tracking: std::sync::RwLock::new(
                crate::utils::permissions::denial_tracking::DenialTrackingState::default(),
            ),
        }
    }
}

/// Checks if a tool name matches a PermissionRule's tool_name using 4-step matching.
///
/// Step 1: Exact match (`Bash` matches `Bash`)
/// Step 2: MCP server-prefix match (`mcp__fs__` blocks all tools starting with `mcp__fs__`)
/// Step 3: MCP tool-prefix match (`mcp__fs_` blocks all tools starting with `mcp__fs_`)
/// Step 4: Wildcard (`*` matches everything)
fn tool_name_matches_rule(tool_name: &str, rule: &PermissionRule) -> bool {
    let rule_tool = &rule.tool_name;

    // Step 1: Exact match
    if rule_tool == tool_name {
        return true;
    }
    // Step 2: MCP server-prefix match (rule ends with "__")
    if rule_tool.ends_with("__") && tool_name.starts_with(rule_tool.as_str()) {
        return true;
    }
    // Step 3: MCP tool-prefix match (rule ends with "_")
    if rule_tool.ends_with('_') && tool_name.starts_with(rule_tool.as_str()) {
        return true;
    }
    // Step 4: Wildcard
    if rule_tool == "*" {
        return true;
    }
    false
}

impl PermissionContext {
    /// Create a new permission context
    pub fn new() -> Self {
        Self::default()
    }

    /// Set permission mode
    pub fn with_mode(mut self, mode: PermissionMode) -> Self {
        self.mode = mode;
        self
    }

    /// Add an allow rule
    pub fn with_allow_rule(mut self, rule: PermissionRule) -> Self {
        self.allow_rules.push(rule);
        self
    }

    /// Add a deny rule
    pub fn with_deny_rule(mut self, rule: PermissionRule) -> Self {
        self.deny_rules.push(rule);
        self
    }

    /// Add an ask rule
    pub fn with_ask_rule(mut self, rule: PermissionRule) -> Self {
        self.ask_rules.push(rule);
        self
    }

    /// Set denial tracking state
    pub fn with_denial_tracking(
        mut self,
        state: crate::utils::permissions::denial_tracking::DenialTrackingState,
    ) -> Self {
        let guard = self.denial_tracking.get_mut().unwrap();
        *guard = state;
        self
    }

    /// Check if a deny rule matches a tool (4-step: exact, server-prefix, tool-prefix, wildcard).
    /// Content-pattern deny rules do not match at the tool-name level.
    fn deny_rule_matches(&self, tool_name: &str, rule: &PermissionRule) -> bool {
        if rule.rule_content.is_some() {
            return false;
        }
        tool_name_matches_rule(tool_name, rule)
    }

    /// Check if an allow rule matches a tool AND (optionally) its input content.
    fn allow_rule_matches(
        &self,
        tool_name: &str,
        input: Option<&serde_json::Value>,
        rule: &PermissionRule,
    ) -> bool {
        if !tool_name_matches_rule(tool_name, rule) {
            return false;
        }
        // If rule has content pattern, input must also match
        if let Some(content) = &rule.rule_content {
            if let Some(input) = input {
                let input_str = input.to_string();
                return input_str.contains(content);
            }
            return false;
        }
        true
    }

    /// Check if a tool is allowed
    pub fn check_tool(
        &self,
        tool_name: &str,
        input: Option<&serde_json::Value>,
    ) -> PermissionResult {
        // Check deny rules first (4-step matching)
        for rule in &self.deny_rules {
            if self.deny_rule_matches(tool_name, rule) {
                return PermissionResult::Deny(PermissionDenyDecision::new(
                    &format!("Tool '{}' is denied by rule", tool_name),
                    PermissionDecisionReason::Rule { rule: rule.clone() },
                ));
            }
        }

        // Check allow rules (4-step matching + content)
        for rule in &self.allow_rules {
            if self.allow_rule_matches(tool_name, input, rule) {
                return PermissionResult::Allow(
                    PermissionAllowDecision::new()
                        .with_reason(PermissionDecisionReason::Rule { rule: rule.clone() }),
                );
            }
        }

        // Check ask rules (4-step matching)
        for rule in &self.ask_rules {
            if self.deny_rule_matches(tool_name, rule) {
                return PermissionResult::Ask(
                    PermissionAskDecision::new(&format!(
                        "Tool '{}' requires permission",
                        tool_name
                    ))
                    .with_reason(PermissionDecisionReason::Rule { rule: rule.clone() }),
                );
            }
        }

        // Check permission mode
        match self.mode {
            PermissionMode::Bypass => {
                return PermissionResult::Allow(PermissionAllowDecision::new().with_reason(
                    PermissionDecisionReason::Mode {
                        mode: PermissionMode::Bypass,
                    },
                ));
            }
            PermissionMode::DontAsk => {
                return PermissionResult::Deny(PermissionDenyDecision::new(
                    "Permission mode is 'dontAsk'",
                    PermissionDecisionReason::Mode {
                        mode: PermissionMode::DontAsk,
                    },
                ));
            }
            PermissionMode::AcceptEdits => {
                // Allow edit tools
                if tool_name == "Write" || tool_name == "Edit" || tool_name == "Bash" {
                    return PermissionResult::Allow(PermissionAllowDecision::new().with_reason(
                        PermissionDecisionReason::Mode {
                            mode: PermissionMode::AcceptEdits,
                        },
                    ));
                }
            }
            PermissionMode::Bubble => {
                // Bubble mode: allow most tools without prompting, but check for dangerous patterns
                // Allow read-only tools and safe tools automatically
                let safe_tools = ["Read", "Glob", "Grep", "Search", "WebFetch", "WebSearch"];
                if safe_tools.iter().any(|&t| t == tool_name) {
                    return PermissionResult::Allow(PermissionAllowDecision::new().with_reason(
                        PermissionDecisionReason::Mode {
                            mode: PermissionMode::Bubble,
                        },
                    ));
                }
                // Check input for dangerous patterns before allowing write/edit/bash
                if let Some(input_val) = input {
                    let input_str = input_val.to_string();
                    // Block potentially dangerous patterns
                    let dangerous_patterns = [
                        "rm -rf",
                        "rm /",
                        "del /",
                        "format",
                        "dd if=",
                        "> /dev/sd",
                        "chmod 777",
                        "chown -R",
                    ];
                    for pattern in dangerous_patterns {
                        if input_str.contains(pattern) {
                            // Dangerous pattern detected - ask for permission
                            return PermissionResult::Ask(
                                PermissionAskDecision::new(&format!(
                                    "Tool '{}' contains potentially dangerous pattern: {}",
                                    tool_name, pattern
                                ))
                                .with_reason(
                                    PermissionDecisionReason::Mode {
                                        mode: PermissionMode::Bubble,
                                    },
                                ),
                            );
                        }
                    }
                }
                // Allow write/edit/bash if no dangerous patterns
                if tool_name == "Write"
                    || tool_name == "Edit"
                    || tool_name == "Bash"
                    || tool_name == "FileEdit"
                    || tool_name == "FileWrite"
                {
                    return PermissionResult::Allow(PermissionAllowDecision::new().with_reason(
                        PermissionDecisionReason::Mode {
                            mode: PermissionMode::Bubble,
                        },
                    ));
                }
            }
            PermissionMode::Auto => {
                if crate::utils::permissions::classifier_decision::is_auto_mode_allowlisted_tool(
                    tool_name,
                ) {
                    if let Ok(mut dt) = self.denial_tracking.write() {
                        *dt = crate::utils::permissions::denial_tracking::record_success(*dt);
                    }
                    return PermissionResult::Allow(
                        PermissionAllowDecision::new()
                            .with_reason(PermissionDecisionReason::Mode {
                                mode: PermissionMode::Auto,
                            }),
                    );
                }
                // Record denial
                if let Ok(mut dt) = self.denial_tracking.write() {
                    *dt = crate::utils::permissions::denial_tracking::record_denial(*dt);
                }
                let should_fallback = if let Ok(dt) = self.denial_tracking.read() {
                    crate::utils::permissions::denial_tracking::should_fallback_to_prompting(*dt)
                } else {
                    false
                };
                let mut msg = format!("Tool '{}' requires auto-classification", tool_name);
                if should_fallback {
                    msg = format!(
                        "{}. Auto mode has failed repeatedly — consider switching to a different permission mode.",
                        msg
                    );
                }
                return PermissionResult::Ask(
                    PermissionAskDecision::new(&msg).with_reason(PermissionDecisionReason::Mode {
                        mode: PermissionMode::Auto,
                    }),
                );
            }
            _ => {}
        }

        // Default: ask
        PermissionResult::Ask(
            PermissionAskDecision::new(&format!("Permission required to use {}", tool_name))
                .with_reason(PermissionDecisionReason::Mode { mode: self.mode }),
        )
    }
}

/// Callback type for permission checks
pub type PermissionCallback =
    Box<dyn Fn(PermissionMetadata, PermissionResult) -> PermissionResult + Send + Sync>;

/// Permission handler with callback support
pub struct PermissionHandler {
    context: PermissionContext,
    callback: Option<PermissionCallback>,
}

impl PermissionHandler {
    /// Create a new permission handler
    pub fn new(context: PermissionContext) -> Self {
        Self {
            context,
            callback: None,
        }
    }

    /// Create with a callback
    pub fn with_callback(context: PermissionContext, callback: PermissionCallback) -> Self {
        Self {
            context,
            callback: Some(callback),
        }
    }

    /// Check permission for a tool
    pub fn check(&self, metadata: PermissionMetadata) -> PermissionResult {
        let result = self
            .context
            .check_tool(&metadata.tool_name, metadata.input.as_ref());

        // If there's a callback, let it override the decision
        if let Some(callback) = &self.callback {
            return callback(metadata, result);
        }

        result
    }

    /// Check if tool is allowed
    pub fn is_allowed(&self, metadata: &PermissionMetadata) -> bool {
        self.check(metadata.clone()).is_allowed()
    }
}

impl PermissionHandler {
    /// Create a default permission handler
    pub fn default() -> Self {
        Self::new(PermissionContext::default())
    }
}
