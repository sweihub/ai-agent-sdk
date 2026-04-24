use crate::permission::*;

// =====================================================================
// PermissionRule Tests
// =====================================================================

#[test]
fn test_permission_rule_allow() {
    let rule = PermissionRule::allow("Bash");
    assert_eq!(rule.tool_name, "Bash");
    assert_eq!(rule.behavior, PermissionBehavior::Allow);
    assert_eq!(rule.source, PermissionRuleSource::UserSettings);
    assert!(rule.rule_content.is_none());
}

#[test]
fn test_permission_rule_deny() {
    let rule = PermissionRule::deny("Edit");
    assert_eq!(rule.tool_name, "Edit");
    assert_eq!(rule.behavior, PermissionBehavior::Deny);
}

#[test]
fn test_permission_rule_ask() {
    let rule = PermissionRule::ask("Grep");
    assert_eq!(rule.tool_name, "Grep");
    assert_eq!(rule.behavior, PermissionBehavior::Ask);
}

#[test]
fn test_permission_rule_with_content() {
    let rule = PermissionRule::with_content("Bash", PermissionBehavior::Allow, "ls");
    assert_eq!(rule.tool_name, "Bash");
    assert_eq!(rule.behavior, PermissionBehavior::Allow);
    assert_eq!(rule.rule_content, Some("ls".to_string()));
}

#[test]
fn test_permission_rule_new() {
    let rule = PermissionRule::new("Read", PermissionBehavior::Allow);
    assert_eq!(rule.tool_name, "Read");
    assert_eq!(rule.behavior, PermissionBehavior::Allow);
}

#[test]
fn test_permission_rule_with_source() {
    let rule = PermissionRule {
        source: PermissionRuleSource::CliArg,
        behavior: PermissionBehavior::Allow,
        tool_name: "Bash".to_string(),
        rule_content: None,
    };
    assert_eq!(rule.source, PermissionRuleSource::CliArg);
}

// =====================================================================
// PermissionMetadata Tests
// =====================================================================

#[test]
fn test_permission_metadata() {
    let meta = PermissionMetadata::new("Bash");
    assert_eq!(meta.tool_name, "Bash");
    assert!(meta.description.is_none());
    assert!(meta.input.is_none());
    assert!(meta.cwd.is_none());
}

#[test]
fn test_permission_metadata_with_description() {
    let meta = PermissionMetadata::new("Bash").with_description("Run shell commands");
    assert_eq!(meta.description, Some("Run shell commands".to_string()));
}

#[test]
fn test_permission_metadata_with_input() {
    let meta = PermissionMetadata::new("Bash").with_input(serde_json::json!({"command": "ls"}));
    assert!(meta.input.is_some());
}

#[test]
fn test_permission_metadata_with_cwd() {
    let meta = PermissionMetadata::new("Bash").with_cwd("/home/user");
    assert_eq!(meta.cwd, Some("/home/user".to_string()));
}

// =====================================================================
// PermissionContext Tests - Deny Rules
// =====================================================================

#[test]
fn test_permission_context_deny_rule() {
    let ctx = PermissionContext::new().with_deny_rule(PermissionRule::deny("Bash"));

    let result = ctx.check_tool("Bash", None);
    assert!(result.is_denied());
}

#[test]
fn test_permission_context_deny_rule_not_matching() {
    let ctx = PermissionContext::new().with_deny_rule(PermissionRule::deny("Bash"));

    // Different tool should not be denied
    let result = ctx.check_tool("Read", None);
    assert!(!result.is_denied());
}

#[test]
fn test_permission_context_multiple_deny_rules() {
    let ctx = PermissionContext::new()
        .with_deny_rule(PermissionRule::deny("Bash"))
        .with_deny_rule(PermissionRule::deny("Edit"));

    assert!(ctx.check_tool("Bash", None).is_denied());
    assert!(ctx.check_tool("Edit", None).is_denied());
    assert!(!ctx.check_tool("Read", None).is_denied());
}

// =====================================================================
// PermissionContext Tests - Allow Rules
// =====================================================================

#[test]
fn test_permission_context_allow_rule() {
    let ctx = PermissionContext::new().with_allow_rule(PermissionRule::allow("Read"));

    let result = ctx.check_tool("Read", None);
    assert!(result.is_allowed());
}

#[test]
fn test_permission_context_allow_rule_with_content_match() {
    let ctx = PermissionContext::new().with_allow_rule(PermissionRule::with_content(
        "Bash",
        PermissionBehavior::Allow,
        "ls",
    ));

    let input = serde_json::json!({"command": "ls -la"});
    let result = ctx.check_tool("Bash", Some(&input));
    assert!(result.is_allowed());
}

#[test]
fn test_permission_context_allow_rule_with_content_no_match() {
    let ctx = PermissionContext::new().with_allow_rule(PermissionRule::with_content(
        "Bash",
        PermissionBehavior::Allow,
        "ls",
    ));

    let input = serde_json::json!({"command": "rm -rf /"});
    let result = ctx.check_tool("Bash", Some(&input));
    assert!(!result.is_allowed());
}

#[test]
fn test_permission_context_allow_rule_no_input() {
    // When rule has content but no input provided, should not match
    let ctx = PermissionContext::new().with_allow_rule(PermissionRule::with_content(
        "Bash",
        PermissionBehavior::Allow,
        "ls",
    ));

    let result = ctx.check_tool("Bash", None);
    // No input means content can't match, falls through to default
    assert!(!result.is_allowed());
}

// =====================================================================
// PermissionContext Tests - Ask Rules
// =====================================================================

#[test]
fn test_permission_context_ask_rule() {
    let ctx = PermissionContext::new().with_ask_rule(PermissionRule::ask("Grep"));

    let result = ctx.check_tool("Grep", None);
    assert!(result.is_ask());
}

#[test]
fn test_permission_context_ask_rule_not_matching() {
    let ctx = PermissionContext::new()
        .with_mode(PermissionMode::Bypass) // Use bypass mode to avoid default ask
        .with_ask_rule(PermissionRule::ask("Grep"));

    let result = ctx.check_tool("Read", None);
    // With bypass mode and no matching rule, should be allowed (not ask)
    assert!(!result.is_ask());
}

// =====================================================================
// PermissionContext Tests - Rule Priority
// =====================================================================

#[test]
fn test_permission_context_deny_overrides_allow() {
    // Deny should take precedence over allow
    let ctx = PermissionContext::new()
        .with_allow_rule(PermissionRule::allow("Bash"))
        .with_deny_rule(PermissionRule::deny("Bash"));

    let result = ctx.check_tool("Bash", None);
    assert!(result.is_denied());
}

#[test]
fn test_permission_context_allow_overrides_default() {
    let ctx = PermissionContext::new()
        .with_mode(PermissionMode::DontAsk) // Default deny
        .with_allow_rule(PermissionRule::allow("Read"));

    let result = ctx.check_tool("Read", None);
    assert!(result.is_allowed());
}

// =====================================================================
// PermissionContext Tests - Permission Modes
// =====================================================================

#[test]
fn test_permission_mode_default() {
    let ctx = PermissionContext::new().with_mode(PermissionMode::Default);
    let result = ctx.check_tool("Bash", None);
    // Default mode asks for permission
    assert!(result.is_ask());
}

#[test]
fn test_permission_mode_bypass() {
    let ctx = PermissionContext::new().with_mode(PermissionMode::Bypass);
    let result = ctx.check_tool("Bash", None);
    assert!(result.is_allowed());
}

#[test]
fn test_permission_mode_bypass_deny_rule_still_applies() {
    // Bypass mode can still be overridden by deny rules
    let ctx = PermissionContext::new()
        .with_mode(PermissionMode::Bypass)
        .with_deny_rule(PermissionRule::deny("Bash"));

    let result = ctx.check_tool("Bash", None);
    assert!(result.is_denied());
}

#[test]
fn test_permission_mode_dont_ask() {
    let ctx = PermissionContext::new().with_mode(PermissionMode::DontAsk);
    let result = ctx.check_tool("Bash", None);
    assert!(result.is_denied());
}

#[test]
fn test_permission_mode_accept_edits_allows_write() {
    let ctx = PermissionContext::new().with_mode(PermissionMode::AcceptEdits);
    let result = ctx.check_tool("Write", None);
    assert!(result.is_allowed());
}

#[test]
fn test_permission_mode_accept_edits_allows_edit() {
    let ctx = PermissionContext::new().with_mode(PermissionMode::AcceptEdits);
    let result = ctx.check_tool("Edit", None);
    assert!(result.is_allowed());
}

#[test]
fn test_permission_mode_accept_edits_allows_bash() {
    let ctx = PermissionContext::new().with_mode(PermissionMode::AcceptEdits);
    let result = ctx.check_tool("Bash", None);
    assert!(result.is_allowed());
}

#[test]
fn test_permission_mode_accept_edits_denies_read() {
    let ctx = PermissionContext::new().with_mode(PermissionMode::AcceptEdits);
    let result = ctx.check_tool("Read", None);
    // AcceptEdits allows write/edit/bash, but for other tools defaults to ask
    assert!(result.is_ask());
}

#[test]
fn test_permission_mode_plan() {
    let ctx = PermissionContext::new().with_mode(PermissionMode::Plan);
    let result = ctx.check_tool("Bash", None);
    // Plan mode should ask
    assert!(result.is_ask());
}

#[test]
fn test_permission_mode_auto() {
    let ctx = PermissionContext::new().with_mode(PermissionMode::Auto);
    let result = ctx.check_tool("Bash", None);
    // Auto mode should ask by default for non-allowlisted tools
    assert!(result.is_ask());
}

// =====================================================================
// Auto Mode - Allowlisted Tools
// =====================================================================

#[test]
fn test_permission_mode_auto_allows_file_read() {
    let ctx = PermissionContext::new().with_mode(PermissionMode::Auto);
    let result = ctx.check_tool("FileRead", None);
    assert!(result.is_allowed(), "FileRead should be allowed in auto mode");
}

#[test]
fn test_permission_mode_auto_allows_grep() {
    let ctx = PermissionContext::new().with_mode(PermissionMode::Auto);
    let result = ctx.check_tool("Grep", None);
    assert!(result.is_allowed(), "Grep should be allowed in auto mode");
}

#[test]
fn test_permission_mode_auto_allows_glob() {
    let ctx = PermissionContext::new().with_mode(PermissionMode::Auto);
    let result = ctx.check_tool("Glob", None);
    assert!(result.is_allowed(), "Glob should be allowed in auto mode");
}

#[test]
fn test_permission_mode_auto_allows_todo_write() {
    let ctx = PermissionContext::new().with_mode(PermissionMode::Auto);
    let result = ctx.check_tool("TodoWrite", None);
    assert!(result.is_allowed(), "TodoWrite should be allowed in auto mode");
}

#[test]
fn test_permission_mode_auto_asks_bash() {
    let ctx = PermissionContext::new().with_mode(PermissionMode::Auto);
    let result = ctx.check_tool("Bash", None);
    assert!(result.is_ask(), "Bash should ask in auto mode");
    // Verify the message contains "auto-classification"
    let msg = result.message().unwrap();
    assert!(
        msg.contains("auto-classification"),
        "Message should mention auto-classification, got: {}",
        msg
    );
}

#[test]
fn test_permission_mode_auto_asks_agent() {
    let ctx = PermissionContext::new().with_mode(PermissionMode::Auto);
    let result = ctx.check_tool("Agent", None);
    assert!(result.is_ask(), "Agent should ask in auto mode");
}

#[test]
fn test_permission_mode_auto_asks_write() {
    let ctx = PermissionContext::new().with_mode(PermissionMode::Auto);
    let result = ctx.check_tool("Write", None);
    assert!(result.is_ask(), "Write should ask in auto mode");
}

#[test]
fn test_permission_mode_auto_asks_edit() {
    let ctx = PermissionContext::new().with_mode(PermissionMode::Auto);
    let result = ctx.check_tool("Edit", None);
    assert!(result.is_ask(), "Edit should ask in auto mode");
}

// =====================================================================
// Auto Mode - Denial Tracking
// =====================================================================

#[test]
fn test_permission_mode_auto_denial_tracking_resets_on_allow() {
    let dt = crate::utils::permissions::denial_tracking::DenialTrackingState {
        consecutive_denials: 5,
        total_denials: 10,
    };
    let ctx = PermissionContext::new()
        .with_mode(PermissionMode::Auto)
        .with_denial_tracking(dt);

    // Check an allowlisted tool (resets consecutive denials)
    let _ = ctx.check_tool("FileRead", None);

    let dt = ctx.denial_tracking.read().unwrap();
    assert_eq!(dt.consecutive_denials, 0, "consecutive denials should be reset");
    assert_eq!(dt.total_denials, 10, "total denials should be preserved");
}

#[test]
fn test_permission_mode_auto_denial_tracking_counts_consecutive() {
    let ctx = PermissionContext::new().with_mode(PermissionMode::Auto);

    // Check 3 non-allowlisted tools
    for _ in 0..3 {
        let _ = ctx.check_tool("Bash", None);
    }

    let dt = ctx.denial_tracking.read().unwrap();
    assert_eq!(dt.consecutive_denials, 3);
    assert_eq!(dt.total_denials, 3);
}

#[test]
fn test_permission_mode_auto_fallback_message_after_limit() {
    let ctx = PermissionContext::new().with_mode(PermissionMode::Auto);

    // Trigger 3 consecutive denials (hitting the limit of 3)
    for _ in 0..3 {
        let _ = ctx.check_tool("Bash", None);
    }

    // The 4th check should include the fallback message
    let result = ctx.check_tool("Agent", None);
    let msg = result.message().unwrap();
    assert!(
        msg.contains("failed repeatedly"),
        "Message should include fallback warning after limit, got: {}",
        msg
    );
}

// =====================================================================
// Auto Mode - Rule Overrides
// =====================================================================

#[test]
fn test_permission_mode_auto_deny_rules_override_allowlist() {
    // Deny rules take precedence over auto mode allowlist
    let ctx = PermissionContext::new()
        .with_mode(PermissionMode::Auto)
        .with_deny_rule(PermissionRule::deny("FileRead"));

    let result = ctx.check_tool("FileRead", None);
    assert!(
        result.is_denied(),
        "Deny rule should override auto mode allowlist"
    );
}

#[test]
fn test_permission_mode_auto_allow_rules_override_mode() {
    // Allow rules take precedence over mode
    let ctx = PermissionContext::new()
        .with_mode(PermissionMode::Auto)
        .with_allow_rule(PermissionRule::allow("Bash"));

    let result = ctx.check_tool("Bash", None);
    assert!(
        result.is_allowed(),
        "Allow rule should override auto mode default"
    );
}

#[test]
fn test_permission_mode_bubble_allows_safe_tools() {
    let ctx = PermissionContext::new().with_mode(PermissionMode::Bubble);
    // Safe read-only tools should be allowed
    assert!(ctx.check_tool("Read", None).is_allowed());
    assert!(ctx.check_tool("Glob", None).is_allowed());
    assert!(ctx.check_tool("Grep", None).is_allowed());
}

#[test]
fn test_permission_mode_bubble_allows_write_edit_bash() {
    let ctx = PermissionContext::new().with_mode(PermissionMode::Bubble);
    // Write/Edit/Bash should be allowed without dangerous patterns
    assert!(
        ctx.check_tool(
            "Write",
            Some(&serde_json::json!({"path": "/tmp/test", "content": "hello"}))
        )
        .is_allowed()
    );
    assert!(
        ctx.check_tool(
            "Edit",
            Some(&serde_json::json!({"path": "/tmp/test", "old_string": "a", "new_string": "b"}))
        )
        .is_allowed()
    );
    assert!(
        ctx.check_tool("Bash", Some(&serde_json::json!({"command": "ls -la"})))
            .is_allowed()
    );
}

#[test]
fn test_permission_mode_bubble_blocks_dangerous_patterns() {
    let ctx = PermissionContext::new().with_mode(PermissionMode::Bubble);
    // Dangerous patterns should be blocked (ask)
    assert!(
        ctx.check_tool("Bash", Some(&serde_json::json!({"command": "rm -rf /tmp"})))
            .is_ask()
    );
    assert!(
        ctx.check_tool(
            "Bash",
            Some(&serde_json::json!({"command": "dd if=/dev/zero of=/dev/sda"}))
        )
        .is_ask()
    );
}

// =====================================================================
// PermissionDecisionReason Tests
// =====================================================================

#[test]
fn test_permission_decision_reason_rule() {
    let reason = PermissionDecisionReason::Rule {
        rule: PermissionRule::allow("Bash"),
    };
    match reason {
        PermissionDecisionReason::Rule { rule } => {
            assert_eq!(rule.tool_name, "Bash");
        }
        _ => panic!("Expected Rule reason"),
    }
}

#[test]
fn test_permission_decision_reason_mode() {
    let reason = PermissionDecisionReason::Mode {
        mode: PermissionMode::Bypass,
    };
    match reason {
        PermissionDecisionReason::Mode { mode } => {
            assert_eq!(mode, PermissionMode::Bypass);
        }
        _ => panic!("Expected Mode reason"),
    }
}

#[test]
fn test_permission_decision_reason_hook() {
    let reason = PermissionDecisionReason::Hook {
        hook_name: "test_hook".to_string(),
        reason: Some("blocked".to_string()),
    };
    match reason {
        PermissionDecisionReason::Hook { hook_name, reason } => {
            assert_eq!(hook_name, "test_hook");
            assert_eq!(reason, Some("blocked".to_string()));
        }
        _ => panic!("Expected Hook reason"),
    }
}

#[test]
fn test_permission_decision_reason_other() {
    let reason = PermissionDecisionReason::Other {
        reason: "custom reason".to_string(),
    };
    match reason {
        PermissionDecisionReason::Other { reason } => {
            assert_eq!(reason, "custom reason");
        }
        _ => panic!("Expected Other reason"),
    }
}

// =====================================================================
// PermissionDecision Tests
// =====================================================================

#[test]
fn test_permission_decision_is_allowed() {
    let decision = PermissionDecision::Allow(PermissionAllowDecision::new());
    assert!(decision.is_allowed());
    assert!(!decision.is_denied());
    assert!(!decision.is_ask());
}

#[test]
fn test_permission_decision_is_denied() {
    let decision = PermissionDecision::Deny(PermissionDenyDecision::new(
        "denied",
        PermissionDecisionReason::Other {
            reason: "test".to_string(),
        },
    ));
    assert!(!decision.is_allowed());
    assert!(decision.is_denied());
    assert!(!decision.is_ask());
}

#[test]
fn test_permission_decision_is_ask() {
    let decision = PermissionDecision::Ask(PermissionAskDecision::new("ask"));
    assert!(!decision.is_allowed());
    assert!(!decision.is_denied());
    assert!(decision.is_ask());
}

#[test]
fn test_permission_decision_message() {
    let decision = PermissionDecision::Ask(PermissionAskDecision::new("please allow"));
    assert_eq!(decision.message(), Some("please allow"));

    let decision = PermissionDecision::Allow(PermissionAllowDecision::new());
    assert_eq!(decision.message(), None);
}

// =====================================================================
// PermissionResult Tests
// =====================================================================

#[test]
fn test_permission_result_is_allowed() {
    let result = PermissionResult::Allow(PermissionAllowDecision::new());
    assert!(result.is_allowed());
}

#[test]
fn test_permission_result_passthrough_is_allowed() {
    let result = PermissionResult::Passthrough {
        message: "logged".to_string(),
        decision_reason: None,
    };
    assert!(result.is_allowed());
}

#[test]
fn test_permission_result_is_denied() {
    let result = PermissionResult::Deny(PermissionDenyDecision::new(
        "denied",
        PermissionDecisionReason::Other {
            reason: "test".to_string(),
        },
    ));
    assert!(result.is_denied());
}

#[test]
fn test_permission_result_is_ask() {
    let result = PermissionResult::Ask(PermissionAskDecision::new("ask"));
    assert!(result.is_ask());
}

#[test]
fn test_permission_result_message() {
    let result = PermissionResult::Ask(PermissionAskDecision::new("ask me"));
    assert_eq!(result.message(), Some("ask me"));

    let result = PermissionResult::Passthrough {
        message: "passthrough".to_string(),
        decision_reason: None,
    };
    assert_eq!(result.message(), Some("passthrough"));
}

#[test]
fn test_permission_result_to_decision() {
    let result = PermissionResult::Allow(PermissionAllowDecision::new());
    let decision = result.to_decision();
    assert!(decision.is_some());
    assert!(decision.unwrap().is_allowed());

    let result = PermissionResult::Passthrough {
        message: "test".to_string(),
        decision_reason: None,
    };
    let decision = result.to_decision();
    assert!(decision.is_none());
}

// =====================================================================
// PermissionHandler Tests
// =====================================================================

#[test]
fn test_permission_handler_default() {
    let handler = PermissionHandler::default();
    let meta = PermissionMetadata::new("Bash");
    let result = handler.check(meta);
    // Default context should ask
    assert!(result.is_ask());
}

#[test]
fn test_permission_handler_with_context() {
    let ctx = PermissionContext::new().with_mode(PermissionMode::Bypass);
    let handler = PermissionHandler::new(ctx);
    let meta = PermissionMetadata::new("Bash");
    let result = handler.check(meta);
    assert!(result.is_allowed());
}

#[test]
fn test_permission_handler_is_allowed() {
    let handler = PermissionHandler::new(
        PermissionContext::new().with_allow_rule(PermissionRule::allow("Read")),
    );
    let meta = PermissionMetadata::new("Read");
    assert!(handler.is_allowed(&meta));

    let meta = PermissionMetadata::new("Bash");
    assert!(!handler.is_allowed(&meta));
}

// =====================================================================
// Edge Cases
// =====================================================================

#[test]
fn test_permission_context_unknown_tool_defaults_to_ask() {
    let ctx = PermissionContext::new();
    let result = ctx.check_tool("UnknownTool", None);
    assert!(result.is_ask());
}

#[test]
fn test_permission_context_empty_rules() {
    let ctx = PermissionContext::new();
    let result = ctx.check_tool("Read", None);
    // No rules, default mode asks
    assert!(result.is_ask());
}

#[test]
fn test_permission_metadata_all_fields() {
    let meta = PermissionMetadata::new("Bash")
        .with_description("Run commands")
        .with_input(serde_json::json!({"command": "ls"}))
        .with_cwd("/home/user");

    assert_eq!(meta.tool_name, "Bash");
    assert_eq!(meta.description, Some("Run commands".to_string()));
    assert!(meta.input.is_some());
    assert_eq!(meta.cwd, Some("/home/user".to_string()));
}

#[test]
fn test_permission_mode_all_variants() {
    let modes = vec![
        PermissionMode::Default,
        PermissionMode::AcceptEdits,
        PermissionMode::Bypass,
        PermissionMode::DontAsk,
        PermissionMode::Plan,
        PermissionMode::Auto,
        PermissionMode::Bubble,
    ];

    for mode in modes {
        let ctx = PermissionContext::new().with_mode(mode);
        let result = ctx.check_tool("Read", None);
        // All modes should return some result
        assert!(result.is_allowed() || result.is_denied() || result.is_ask());
    }
}

#[test]
fn test_permission_behavior_all_variants() {
    assert_eq!(PermissionBehavior::Allow.as_str(), "allow");
    assert_eq!(PermissionBehavior::Deny.as_str(), "deny");
    assert_eq!(PermissionBehavior::Ask.as_str(), "ask");
}

#[test]
fn test_permission_rule_source_all_variants() {
    // Test each source variant individually
    let rule1 = PermissionRule {
        source: PermissionRuleSource::UserSettings,
        behavior: PermissionBehavior::Allow,
        tool_name: "Test".to_string(),
        rule_content: None,
    };
    assert_eq!(rule1.source, PermissionRuleSource::UserSettings);

    let rule2 = PermissionRule {
        source: PermissionRuleSource::ProjectSettings,
        behavior: PermissionBehavior::Allow,
        tool_name: "Test".to_string(),
        rule_content: None,
    };
    assert_eq!(rule2.source, PermissionRuleSource::ProjectSettings);

    let rule3 = PermissionRule {
        source: PermissionRuleSource::LocalSettings,
        behavior: PermissionBehavior::Allow,
        tool_name: "Test".to_string(),
        rule_content: None,
    };
    assert_eq!(rule3.source, PermissionRuleSource::LocalSettings);

    let rule4 = PermissionRule {
        source: PermissionRuleSource::CliArg,
        behavior: PermissionBehavior::Allow,
        tool_name: "Test".to_string(),
        rule_content: None,
    };
    assert_eq!(rule4.source, PermissionRuleSource::CliArg);

    let rule5 = PermissionRule {
        source: PermissionRuleSource::Session,
        behavior: PermissionBehavior::Allow,
        tool_name: "Test".to_string(),
        rule_content: None,
    };
    assert_eq!(rule5.source, PermissionRuleSource::Session);

    let rule6 = PermissionRule {
        source: PermissionRuleSource::Policy,
        behavior: PermissionBehavior::Allow,
        tool_name: "Test".to_string(),
        rule_content: None,
    };
    assert_eq!(rule6.source, PermissionRuleSource::Policy);

    let rule7 = PermissionRule {
        source: PermissionRuleSource::FlagSettings,
        behavior: PermissionBehavior::Allow,
        tool_name: "Test".to_string(),
        rule_content: None,
    };
    assert_eq!(rule7.source, PermissionRuleSource::FlagSettings);
}

#[test]
fn test_permission_decision_serialization() {
    let decision = PermissionDecision::Allow(PermissionAllowDecision::new());
    let json = serde_json::to_string(&decision).unwrap();
    assert!(json.contains("\"allow\""));

    let decision = PermissionDecision::Ask(PermissionAskDecision::new("test"));
    let json = serde_json::to_string(&decision).unwrap();
    assert!(json.contains("\"ask\""));

    let decision = PermissionDecision::Deny(PermissionDenyDecision::new(
        "test",
        PermissionDecisionReason::Other {
            reason: "test".to_string(),
        },
    ));
    let json = serde_json::to_string(&decision).unwrap();
    assert!(json.contains("\"deny\""));
}

#[test]
fn test_permission_result_serialization() {
    let result = PermissionResult::Allow(PermissionAllowDecision::new());
    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("\"allow\""));

    let result = PermissionResult::Ask(PermissionAskDecision::new("test"));
    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("\"ask\""));

    let result = PermissionResult::Deny(PermissionDenyDecision::new(
        "test",
        PermissionDecisionReason::Other {
            reason: "test".to_string(),
        },
    ));
    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("\"deny\""));

    let result = PermissionResult::Passthrough {
        message: "test".to_string(),
        decision_reason: None,
    };
    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("\"passthrough\""));
}

#[test]
fn test_permission_rule_serialization() {
    let rule = PermissionRule::allow("Bash");
    let json = serde_json::to_string(&rule).unwrap();
    assert!(json.contains("Bash"));
    assert!(json.contains("allow"));
}

#[test]
fn test_permission_context_serialization() {
    let ctx = PermissionContext::new()
        .with_mode(PermissionMode::Bypass)
        .with_allow_rule(PermissionRule::allow("Read"));

    // Context should be cloneable
    let ctx2 = ctx.clone();
    let result = ctx2.check_tool("Read", None);
    assert!(result.is_allowed());
}

#[test]
fn test_permission_ask_decision_with_blocked_path() {
    let decision = PermissionAskDecision::new("blocked").with_blocked_path("/etc/passwd");
    assert_eq!(decision.blocked_path, Some("/etc/passwd".to_string()));
}

#[test]
fn test_permission_allow_decision_with_reason() {
    let reason = PermissionDecisionReason::Mode {
        mode: PermissionMode::Bypass,
    };
    let decision = PermissionAllowDecision::new().with_reason(reason.clone());
    assert_eq!(decision.decision_reason, Some(reason));
}

#[test]
fn test_permission_deny_decision_with_reason() {
    let reason = PermissionDecisionReason::Other {
        reason: "not allowed".to_string(),
    };
    let decision = PermissionDenyDecision::new("denied", reason.clone());
    assert_eq!(decision.decision_reason, reason);
}

// =====================================================================
// PermissionContext Tests - 4-Step Deny Rule Matching
// =====================================================================

#[test]
fn test_deny_rule_exact_match() {
    let ctx = PermissionContext::new().with_deny_rule(PermissionRule::deny("Bash"));

    assert!(ctx.check_tool("Bash", None).is_denied());
    // Similar name should NOT be denied
    assert!(!ctx.check_tool("BashHelper", None).is_denied());
    assert!(!ctx.check_tool("Read", None).is_denied());
}

#[test]
fn test_deny_rule_wildcard_match() {
    let ctx = PermissionContext::new()
        .with_mode(PermissionMode::Bypass)
        .with_deny_rule(PermissionRule::deny("*"));

    // Wildcard denies everything
    assert!(ctx.check_tool("Bash", None).is_denied());
    assert!(ctx.check_tool("Read", None).is_denied());
    assert!(ctx.check_tool("mcp__fs_read", None).is_denied());
}

#[test]
fn test_deny_rule_mcp_server_prefix_double_underscore() {
    // Rule ending with "__" matches all tools starting with that prefix
    let ctx = PermissionContext::new()
        .with_mode(PermissionMode::Bypass)
        .with_deny_rule(PermissionRule::deny("mcp__fs__"));

    // Tools starting with "mcp__fs__" should be denied
    assert!(ctx.check_tool("mcp__fs__read", None).is_denied());
    assert!(ctx.check_tool("mcp__fs__write", None).is_denied());
    // Different server should NOT be denied
    assert!(!ctx.check_tool("mcp__git_status", None).is_denied());
    // Partial prefix without full double-underscore start should NOT match
    assert!(!ctx.check_tool("mcp__fs_read", None).is_denied());
}

#[test]
fn test_deny_rule_mcp_tool_prefix_single_underscore() {
    // Rule ending with "_" (but not "__") matches all tools starting with that prefix
    let ctx = PermissionContext::new()
        .with_mode(PermissionMode::Bypass)
        .with_deny_rule(PermissionRule::deny("mcp__fs_"));

    // Tools starting with "mcp__fs_" should be denied
    assert!(ctx.check_tool("mcp__fs_read", None).is_denied());
    assert!(ctx.check_tool("mcp__fs_write", None).is_denied());
    // Different server should NOT be denied
    assert!(!ctx.check_tool("mcp__git_status", None).is_denied());
}

#[test]
fn test_deny_rule_prefix_does_not_match_partial_tool_name() {
    let ctx = PermissionContext::new()
        .with_mode(PermissionMode::Bypass)
        .with_deny_rule(PermissionRule::deny("mcp__fs_"));

    // Tool name that contains prefix but doesn't start with it
    assert!(!ctx.check_tool("custom_mcp__fs_read", None).is_denied());
}

#[test]
fn test_deny_rule_priority_exact_over_prefix() {
    // Exact match should take precedence (checked first in 4-step)
    let ctx = PermissionContext::new()
        .with_mode(PermissionMode::Bypass)
        .with_deny_rule(PermissionRule::deny("mcp__fs_read"));

    assert!(ctx.check_tool("mcp__fs_read", None).is_denied());
    // Other tools from same server should NOT be denied by exact match
    assert!(!ctx.check_tool("mcp__fs_write", None).is_denied());
}

#[test]
fn test_deny_rule_content_pattern_does_not_match_tool_name() {
    // A rule with content pattern (e.g., Bash(ls)) should NOT match
    // at the tool-name level - it needs content evaluation
    let ctx = PermissionContext::new()
        .with_mode(PermissionMode::Bypass)
        .with_deny_rule(PermissionRule::with_content(
            "Bash",
            PermissionBehavior::Deny,
            "rm",
        ));

    // Content-pattern rules should NOT match at tool-name level
    // Falls through to bypass mode -> allowed
    assert!(ctx.check_tool("Bash", None).is_allowed());
}

#[test]
fn test_allow_rule_wildcard_match() {
    let ctx = PermissionContext::new()
        .with_mode(PermissionMode::DontAsk)
        .with_allow_rule(PermissionRule::allow("*"));

    // Wildcard allow should permit all tools
    assert!(ctx.check_tool("Bash", None).is_allowed());
    assert!(ctx.check_tool("Read", None).is_allowed());
    assert!(ctx.check_tool("mcp__fs_read", None).is_allowed());
}

#[test]
fn test_allow_rule_prefix_match() {
    let ctx = PermissionContext::new()
        .with_mode(PermissionMode::DontAsk)
        .with_allow_rule(PermissionRule::allow("mcp__fs_"));

    // Prefix allow should permit all tools starting with prefix
    assert!(ctx.check_tool("mcp__fs_read", None).is_allowed());
    assert!(ctx.check_tool("mcp__fs_write", None).is_allowed());
    // Different tool should fall through to mode (DontAsk -> deny)
    assert!(ctx.check_tool("mcp__git_status", None).is_denied());
}

#[test]
fn test_ask_rule_prefix_match() {
    let ctx = PermissionContext::new()
        .with_mode(PermissionMode::Bypass)
        .with_ask_rule(PermissionRule::ask("mcp__dangerous_"));

    // Prefix ask should require permission for matching tools
    assert!(ctx.check_tool("mcp__dangerous_delete", None).is_ask());
    assert!(ctx.check_tool("mcp__dangerous_format", None).is_ask());
    // Non-matching tool should be allowed by bypass mode
    assert!(ctx.check_tool("mcp__safe_read", None).is_allowed());
}

#[test]
fn test_deny_rule_matching_order_exact_before_wildcard() {
    // When both exact and wildcard rules exist, exact is checked first
    // but since deny is deny, the result is the same. The order matters
    // for determining WHICH rule matched first (short-circuit).
    let ctx = PermissionContext::new()
        .with_mode(PermissionMode::Bypass)
        .with_deny_rule(PermissionRule::deny("Bash"))
        .with_deny_rule(PermissionRule::deny("*"));

    // Both should be denied
    assert!(ctx.check_tool("Bash", None).is_denied());
    assert!(ctx.check_tool("Read", None).is_denied());
}

#[test]
fn test_deny_rule_server_prefix_before_tool_prefix() {
    // Server-prefix (__) and tool-prefix (_) should both work
    let ctx = PermissionContext::new()
        .with_mode(PermissionMode::Bypass)
        .with_deny_rule(PermissionRule::deny("mcp__server__"))
        .with_deny_rule(PermissionRule::deny("mcp__other_"));

    // Server-prefix (__) match
    assert!(ctx.check_tool("mcp__server__read", None).is_denied());
    assert!(ctx.check_tool("mcp__server__write", None).is_denied());

    // Tool-prefix (_) match
    assert!(ctx.check_tool("mcp__other_read", None).is_denied());
    assert!(ctx.check_tool("mcp__other_write", None).is_denied());

    // Non-matching
    assert!(ctx.check_tool("mcp__safe_read", None).is_allowed());
}
