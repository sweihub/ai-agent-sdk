// Source: ~/claudecode/openclaudecode/src/utils/permissions/permissions.ts
#![allow(dead_code)]

//! Core permission checking logic.
//!
//! Handles rule matching, permission decisions, and the main permission pipeline.

use super::permission_rule_parser::{
    permission_rule_value_from_string, permission_rule_value_to_string,
};
use crate::types::permissions::{
    PermissionBehavior, PermissionDecision, PermissionDecisionReason, PermissionDenyDecision,
    PermissionResult, PermissionRule, PermissionRuleSource, PermissionRuleValue, PermissionUpdate,
    ToolPermissionContext,
};
use serde_json::Value;
use std::collections::HashMap;

/// Permission rule sources in priority order.
const PERMISSION_RULE_SOURCES: &[&str] = &[
    "userSettings",
    "projectSettings",
    "localSettings",
    "flagSettings",
    "policySettings",
    "cliArg",
    "command",
    "session",
];

/// Gets the display string for a permission rule source.
pub fn permission_rule_source_display_string(source: &str) -> String {
    source.to_string()
}

/// Gets all allow rules from context.
pub fn get_allow_rules(context: &ToolPermissionContext) -> Vec<PermissionRule> {
    let mut rules = Vec::new();
    for source in PERMISSION_RULE_SOURCES {
        let rule_strings = match *source {
            "userSettings" => &context.always_allow_rules.user_settings,
            "projectSettings" => &context.always_allow_rules.project_settings,
            "localSettings" => &context.always_allow_rules.local_settings,
            "flagSettings" => &context.always_allow_rules.flag_settings,
            "policySettings" => &context.always_allow_rules.policy_settings,
            "cliArg" => &context.always_allow_rules.cli_arg,
            "command" => &context.always_allow_rules.command,
            "session" => &context.always_allow_rules.session,
            _ => &None,
        };
        if let Some(strings) = rule_strings {
            for rule_string in strings {
                rules.push(PermissionRule {
                    source: source_to_enum(source),
                    rule_behavior: PermissionBehavior::Allow,
                    rule_value: permission_rule_value_from_string(rule_string),
                });
            }
        }
    }
    rules
}

/// Gets all deny rules from context.
pub fn get_deny_rules(context: &ToolPermissionContext) -> Vec<PermissionRule> {
    let mut rules = Vec::new();
    for source in PERMISSION_RULE_SOURCES {
        let rule_strings = match *source {
            "userSettings" => &context.always_deny_rules.user_settings,
            "projectSettings" => &context.always_deny_rules.project_settings,
            "localSettings" => &context.always_deny_rules.local_settings,
            "flagSettings" => &context.always_deny_rules.flag_settings,
            "policySettings" => &context.always_deny_rules.policy_settings,
            "cliArg" => &context.always_deny_rules.cli_arg,
            "command" => &context.always_deny_rules.command,
            "session" => &context.always_deny_rules.session,
            _ => &None,
        };
        if let Some(strings) = rule_strings {
            for rule_string in strings {
                rules.push(PermissionRule {
                    source: source_to_enum(source),
                    rule_behavior: PermissionBehavior::Deny,
                    rule_value: permission_rule_value_from_string(rule_string),
                });
            }
        }
    }
    rules
}

/// Gets all ask rules from context.
pub fn get_ask_rules(context: &ToolPermissionContext) -> Vec<PermissionRule> {
    let mut rules = Vec::new();
    for source in PERMISSION_RULE_SOURCES {
        let rule_strings = match *source {
            "userSettings" => &context.always_ask_rules.user_settings,
            "projectSettings" => &context.always_ask_rules.project_settings,
            "localSettings" => &context.always_ask_rules.local_settings,
            "flagSettings" => &context.always_ask_rules.flag_settings,
            "policySettings" => &context.always_ask_rules.policy_settings,
            "cliArg" => &context.always_ask_rules.cli_arg,
            "command" => &context.always_ask_rules.command,
            "session" => &context.always_ask_rules.session,
            _ => &None,
        };
        if let Some(strings) = rule_strings {
            for rule_string in strings {
                rules.push(PermissionRule {
                    source: source_to_enum(source),
                    rule_behavior: PermissionBehavior::Ask,
                    rule_value: permission_rule_value_from_string(rule_string),
                });
            }
        }
    }
    rules
}

/// Converts a source string to PermissionRuleSource enum.
fn source_to_enum(source: &str) -> PermissionRuleSource {
    match source {
        "userSettings" => PermissionRuleSource::UserSettings,
        "projectSettings" => PermissionRuleSource::ProjectSettings,
        "localSettings" => PermissionRuleSource::LocalSettings,
        "flagSettings" => PermissionRuleSource::FlagSettings,
        "policySettings" => PermissionRuleSource::PolicySettings,
        "cliArg" => PermissionRuleSource::CliArg,
        "command" => PermissionRuleSource::Command,
        "session" => PermissionRuleSource::Session,
        _ => PermissionRuleSource::Session,
    }
}

/// Creates a permission request message.
pub fn create_permission_request_message(
    tool_name: &str,
    decision_reason: Option<&PermissionDecisionReason>,
) -> String {
    if let Some(reason) = decision_reason {
        match reason {
            PermissionDecisionReason::Hook {
                hook_name,
                reason: Some(r),
                ..
            } => {
                return format!("Hook '{}' blocked this action: {}", hook_name, r);
            }
            PermissionDecisionReason::Hook { hook_name, .. } => {
                return format!(
                    "Hook '{}' requires approval for this {} command",
                    hook_name, tool_name
                );
            }
            PermissionDecisionReason::Rule { rule, .. } => {
                let rule_string = permission_rule_value_to_string(&rule.rule_value);
                let source_string = permission_rule_source_display_string(rule.source.as_str());
                return format!(
                    "Permission rule '{}' from {} requires approval for this {} command",
                    rule_string, source_string, tool_name
                );
            }
            PermissionDecisionReason::Mode { mode } => {
                return format!(
                    "Current permission mode ({}) requires approval for this {} command",
                    mode, tool_name
                );
            }
            PermissionDecisionReason::SandboxOverride { .. } => {
                return "Run outside of the sandbox".to_string();
            }
            PermissionDecisionReason::WorkingDir { reason }
            | PermissionDecisionReason::SafetyCheck { reason, .. }
            | PermissionDecisionReason::Other { reason }
            | PermissionDecisionReason::AsyncAgent { reason } => {
                return reason.clone();
            }
            PermissionDecisionReason::Classifier { classifier, reason } => {
                return format!(
                    "Classifier '{}' requires approval for this {} command: {}",
                    classifier, tool_name, reason
                );
            }
            _ => {}
        }
    }

    format!(
        "Claude requested permissions to use {}, but you haven't granted it yet.",
        tool_name
    )
}

/// Checks if a tool name matches a rule using 4-step matching.
///
/// Step 1: Exact match (`Bash` matches `Bash`)
/// Step 2: MCP server-prefix match (`mcp__fs_` blocks all tools starting with `mcp__fs_`)
/// Step 3: MCP tool-prefix match (`mcp__fs_` blocks all tools starting with `mcp__fs_`)
/// Step 4: Wildcard (`*` matches everything)
///
/// Rules with content patterns (e.g., `Bash(ls)`) do not match at the
/// tool-name level — they require content-level evaluation.
fn tool_matches_rule(tool_name: &str, rule: &PermissionRule) -> bool {
    if rule.rule_value.rule_content.is_some() {
        return false;
    }
    let rule_tool = &rule.rule_value.tool_name;

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

/// Checks if a tool is in the always-allow rules.
pub fn tool_always_allowed_rule(
    context: &ToolPermissionContext,
    tool_name: &str,
) -> Option<PermissionRule> {
    get_allow_rules(context)
        .into_iter()
        .find(|rule| tool_matches_rule(tool_name, rule))
}

/// Gets the deny rule for a tool.
pub fn get_deny_rule_for_tool(
    context: &ToolPermissionContext,
    tool_name: &str,
) -> Option<PermissionRule> {
    get_deny_rules(context)
        .into_iter()
        .find(|rule| tool_matches_rule(tool_name, rule))
}

/// Gets the ask rule for a tool.
pub fn get_ask_rule_for_tool(
    context: &ToolPermissionContext,
    tool_name: &str,
) -> Option<PermissionRule> {
    get_ask_rules(context)
        .into_iter()
        .find(|rule| tool_matches_rule(tool_name, rule))
}

/// Gets rules by contents for a tool.
pub fn get_rule_by_contents_for_tool_name(
    context: &ToolPermissionContext,
    tool_name: &str,
    behavior: &str,
) -> HashMap<String, PermissionRule> {
    let rules = match behavior {
        "allow" => get_allow_rules(context),
        "deny" => get_deny_rules(context),
        "ask" => get_ask_rules(context),
        _ => vec![],
    };

    rules
        .into_iter()
        .filter(|rule| {
            rule.rule_value.tool_name == tool_name && rule.rule_value.rule_content.is_some()
        })
        .map(|rule| {
            let content = rule.rule_value.rule_content.clone().unwrap();
            (content, rule)
        })
        .collect()
}

/// Applies permission rules to a permission context.
pub fn apply_permission_rules_to_permission_context(
    tool_permission_context: ToolPermissionContext,
    rules: Vec<PermissionRule>,
) -> ToolPermissionContext {
    use super::permission_update::{apply_permission_updates, convert_rules_to_updates};
    let updates = convert_rules_to_updates(&rules, "addRules");
    apply_permission_updates(tool_permission_context, &updates)
}

/// Syncs permission rules from disk.
pub fn sync_permission_rules_from_disk(context: ToolPermissionContext) -> ToolPermissionContext {
    use super::permissions_loader::load_all_permission_rules_from_disk;
    let rules = load_all_permission_rules_from_disk();
    apply_permission_rules_to_permission_context(context, rules)
}

/// Deletes a permission rule.
pub async fn delete_permission_rule(
    rule: &PermissionRule,
    _initial_context: &ToolPermissionContext,
) -> Result<(), String> {
    match rule.source {
        PermissionRuleSource::PolicySettings
        | PermissionRuleSource::FlagSettings
        | PermissionRuleSource::Command => {
            return Err("Cannot delete permission rules from read-only settings".to_string());
        }
        _ => {}
    }
    // In a full implementation, this would persist the deletion
    Ok(())
}

/// Helper to get updated input or fallback to original.
pub fn get_updated_input_or_fallback(
    _tool_permission_result: &PermissionResult,
    input: &HashMap<String, Value>,
) -> HashMap<String, Value> {
    input.clone()
}
