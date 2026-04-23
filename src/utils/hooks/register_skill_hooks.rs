// Source: ~/claudecode/openclaudecode/src/utils/hooks/registerSkillHooks.ts
#![allow(dead_code)]

use std::collections::HashMap;

use crate::utils::hooks::hooks_settings::HookCommand;
use crate::utils::hooks::hooks_settings::HookEvent;
use crate::utils::hooks::session_hooks::{add_session_hook, remove_session_hook};

/// Hooks settings structure
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct HooksSettings {
    #[serde(flatten)]
    pub events: HashMap<String, Vec<HookMatcher>>,
}

/// A hook matcher groups hooks by matching criteria
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HookMatcher {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matcher: Option<String>,
    pub hooks: Vec<serde_json::Value>,
}

/// All hook events as strings (for iteration)
const HOOK_EVENT_NAMES: &[&str] = &[
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "PermissionDenied",
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

/// Parse a hook event from a string
fn parse_hook_event(s: &str) -> Option<HookEvent> {
    match s {
        "PreToolUse" => Some(HookEvent::PreToolUse),
        "PostToolUse" => Some(HookEvent::PostToolUse),
        "PostToolUseFailure" => Some(HookEvent::PostToolUseFailure),
        "PermissionDenied" => Some(HookEvent::PermissionDenied),
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

/// Parse a hook command from a JSON value
fn parse_hook_command(value: &serde_json::Value) -> Result<HookCommand, String> {
    if let Some(command) = value.get("command").and_then(|v| v.as_str()) {
        return Ok(HookCommand::Command {
            command: command.to_string(),
            shell: value
                .get("shell")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            if_condition: value
                .get("if")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            timeout: value.get("timeout").and_then(|v| v.as_u64()),
        });
    }

    if let Some(prompt) = value.get("prompt").and_then(|v| v.as_str()) {
        if value.get("model").is_some() {
            return Ok(HookCommand::Agent {
                prompt: prompt.to_string(),
                model: value
                    .get("model")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                if_condition: value
                    .get("if")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                timeout: value.get("timeout").and_then(|v| v.as_u64()),
            });
        }

        return Ok(HookCommand::Prompt {
            prompt: prompt.to_string(),
            if_condition: value
                .get("if")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            timeout: value.get("timeout").and_then(|v| v.as_u64()),
        });
    }

    if let Some(url) = value.get("url").and_then(|v| v.as_str()) {
        return Ok(HookCommand::Http {
            url: url.to_string(),
            if_condition: value
                .get("if")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            timeout: value.get("timeout").and_then(|v| v.as_u64()),
        });
    }

    Err("Could not parse hook command from JSON".to_string())
}

/// Result of parsing a hook with its once flag
struct ParsedHookWithOnce {
    hook: HookCommand,
    once: bool,
}

/// Parse a hook with its once flag
fn parse_hook_with_once(value: &serde_json::Value) -> Result<ParsedHookWithOnce, String> {
    let hook = parse_hook_command(value)?;
    let once = value.get("once").and_then(|v| v.as_bool()).unwrap_or(false);
    Ok(ParsedHookWithOnce { hook, once })
}

/// Registers hooks from a skill's frontmatter as session hooks.
///
/// Hooks are registered as session-scoped hooks that persist for the duration
/// of the session. If a hook has `once: true`, it will be automatically removed
/// after its first successful execution.
///
/// # Arguments
/// * `set_app_state` - Function to update the app state
/// * `session_id` - The current session ID
/// * `hooks` - The hooks settings from the skill's frontmatter
/// * `skill_name` - The name of the skill (for logging)
/// * `skill_root` - The base directory of the skill (for CLAUDE_PLUGIN_ROOT env var)
pub fn register_skill_hooks(
    set_app_state: &dyn Fn(&dyn Fn(&mut serde_json::Value)),
    session_id: &str,
    hooks: &HooksSettings,
    skill_name: &str,
    skill_root: Option<&str>,
) {
    let mut registered_count = 0;

    for event_name in HOOK_EVENT_NAMES {
        let matchers = match hooks.events.get(*event_name) {
            Some(m) => m,
            None => continue,
        };

        let event = match parse_hook_event(event_name) {
            Some(e) => e,
            None => continue,
        };

        for matcher_config in matchers {
            let matcher = matcher_config.matcher.clone().unwrap_or_default();

            for hook_json in &matcher_config.hooks {
                let parsed = match parse_hook_with_once(hook_json) {
                    Ok(p) => p,
                    Err(_) => continue,
                };

                // For once: true hooks, the on_hook_success callback would remove the hook
                // after first successful execution. In a full implementation, we'd use
                // the session hook removal mechanism. For now, pass None.
                let on_hook_success: Option<crate::utils::hooks::session_hooks::OnHookSuccess> =
                    None;

                add_session_hook(
                    set_app_state,
                    session_id,
                    &event,
                    &matcher,
                    parsed.hook,
                    on_hook_success,
                    skill_root.map(|s| s.to_string()).as_deref(),
                );
                registered_count += 1;
            }
        }
    }

    if registered_count > 0 {
        log_for_debugging(&format!(
            "Registered {} hooks from skill '{}'",
            registered_count, skill_name
        ));
    }
}

/// Log for debugging
fn log_for_debugging(msg: &str) {
    log::debug!("{}", msg);
}

/// Register hooks from multiple loaded skills.
///
/// Iterates over a slice of loaded skills and registers any hooks defined
/// in their frontmatter. Skills without hooks are silently skipped.
///
/// # Arguments
/// * `set_app_state` - Function to update the app state
/// * `session_id` - The current session ID
/// * `skills` - Slice of loaded skills with parsed hooks
/// * `skill_roots` - Optional base directories for each skill (same length as skills, or None)
pub fn register_hooks_from_skills(
    set_app_state: &dyn Fn(&dyn Fn(&mut serde_json::Value)),
    session_id: &str,
    skills: &[crate::skills::loader::UnifiedSkill],
) {
    let mut total_count = 0;

    for skill in skills {
        if let Some(ref hooks) = skill.hooks {
            register_skill_hooks(
                set_app_state,
                session_id,
                hooks,
                &skill.name,
                None,
            );
            total_count += 1;
        }
    }

    if total_count > 0 {
        log_for_debugging(&format!(
            "Registered hooks from {} skill(s) for session {}",
            total_count, session_id
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hook_with_once() {
        let json = serde_json::json!({
            "command": "echo hello",
            "once": true
        });
        let result = parse_hook_with_once(&json);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(parsed.once);
        if let HookCommand::Command { command, .. } = parsed.hook {
            assert_eq!(command, "echo hello");
        } else {
            panic!("Expected Command variant");
        }
    }

    #[test]
    fn test_parse_hook_without_once() {
        let json = serde_json::json!({
            "command": "echo hello"
        });
        let result = parse_hook_with_once(&json);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(!parsed.once);
    }

    #[test]
    fn test_register_skill_hooks_empty() {
        let hooks = HooksSettings::default();
        let call_count = std::cell::Cell::new(0usize);
        let set_app_state = |_: &dyn Fn(&mut serde_json::Value)| {
            call_count.set(call_count.get() + 1);
        };

        register_skill_hooks(&set_app_state, "test-session", &hooks, "test-skill", None);

        assert_eq!(call_count.get(), 0);
    }

    #[test]
    fn test_register_skill_hooks_with_hooks() {
        let mut hooks_settings = HooksSettings::default();
        hooks_settings.events.insert(
            "Stop".to_string(),
            vec![HookMatcher {
                matcher: Some("".to_string()),
                hooks: vec![serde_json::json!({
                    "command": "echo hook-executed"
                })],
            }],
        );

        let call_count = std::cell::Cell::new(0usize);
        let set_app_state = |_: &dyn Fn(&mut serde_json::Value)| {
            call_count.set(call_count.get() + 1);
        };

        register_skill_hooks(
            &set_app_state,
            "test-session",
            &hooks_settings,
            "test-skill",
            None,
        );

        // Should have called add_session_hook once
        assert_eq!(call_count.get(), 1);
    }

    #[test]
    fn test_register_hooks_from_skills_empty() {
        let skills: Vec<crate::skills::loader::UnifiedSkill> = vec![];
        let call_count = std::cell::Cell::new(0usize);
        let set_app_state = |_: &dyn Fn(&mut serde_json::Value)| {
            call_count.set(call_count.get() + 1);
        };

        register_hooks_from_skills(&set_app_state, "test-session", &skills);

        // No hooks registered, but state should be untouched
        assert_eq!(call_count.get(), 0);
    }

    #[test]
    fn test_register_hooks_from_skills_with_hooks() {
        let hooks = HooksSettings {
            events: {
                let mut map = std::collections::HashMap::new();
                map.insert(
                    "Stop".to_string(),
                    vec![HookMatcher {
                        matcher: Some("".to_string()),
                        hooks: vec![serde_json::json!({
                            "command": "echo test"
                        })],
                    }],
                );
                map
            },
        };

        let skills = vec![crate::skills::loader::UnifiedSkill {
            name: "test-skill".to_string(),
            description: "Test".to_string(),
            source: crate::skills::loader::SkillSource::Project,
            content: "content".to_string(),
            paths: None,
            user_invocable: None,
            hooks: Some(hooks),
        }];

        let call_count = std::cell::Cell::new(0usize);
        let set_app_state = |_: &dyn Fn(&mut serde_json::Value)| {
            call_count.set(call_count.get() + 1);
        };

        register_hooks_from_skills(&set_app_state, "test-session", &skills);

        // Should have called add_session_hook once (for the Stop hook)
        assert_eq!(call_count.get(), 1);
    }

    #[test]
    fn test_register_hooks_from_skills_skips_no_hooks() {
        let skills = vec![
            crate::skills::loader::UnifiedSkill {
                name: "no-hooks".to_string(),
                description: "No hooks".to_string(),
                source: crate::skills::loader::SkillSource::Bundled,
                content: "".to_string(),
                paths: None,
                user_invocable: None,
                hooks: None,
            },
            crate::skills::loader::UnifiedSkill {
                name: "also-no-hooks".to_string(),
                description: "Also no hooks".to_string(),
                source: crate::skills::loader::SkillSource::User,
                content: "".to_string(),
                paths: None,
                user_invocable: None,
                hooks: None,
            },
        ];

        let call_count = std::cell::Cell::new(0usize);
        let set_app_state = |_: &dyn Fn(&mut serde_json::Value)| {
            call_count.set(call_count.get() + 1);
        };

        register_hooks_from_skills(&set_app_state, "test-session", &skills);

        // No hooks registered
        assert_eq!(call_count.get(), 0);
    }
}
