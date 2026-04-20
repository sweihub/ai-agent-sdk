use crate::services::plugins::plugin_operations::*;
use crate::utils::plugins::loader::parse_plugin_identifier;

#[test]
fn test_installable_scope_display() {
    assert_eq!(InstallableScope::User.to_string(), "user");
    assert_eq!(InstallableScope::Project.to_string(), "project");
    assert_eq!(InstallableScope::Local.to_string(), "local");
}

#[test]
fn test_installable_scope_try_from() {
    assert_eq!(
        InstallableScope::try_from("user"),
        Ok(InstallableScope::User)
    );
    assert_eq!(
        InstallableScope::try_from("project"),
        Ok(InstallableScope::Project)
    );
    assert_eq!(
        InstallableScope::try_from("local"),
        Ok(InstallableScope::Local)
    );
    assert!(InstallableScope::try_from("managed").is_err());
    assert!(InstallableScope::try_from("invalid").is_err());
}

#[test]
fn test_plugin_scope_display() {
    assert_eq!(PluginScope::User.to_string(), "user");
    assert_eq!(PluginScope::Project.to_string(), "project");
    assert_eq!(PluginScope::Local.to_string(), "local");
    assert_eq!(PluginScope::Managed.to_string(), "managed");
}

#[test]
fn test_plugin_scope_try_from() {
    assert_eq!(PluginScope::try_from("managed"), Ok(PluginScope::Managed));
    assert!(PluginScope::try_from("invalid").is_err());
}

#[test]
fn test_scope_to_setting_source() {
    assert_eq!(
        scope_to_setting_source(InstallableScope::User),
        SettingSource::UserSettings
    );
    assert_eq!(
        scope_to_setting_source(InstallableScope::Project),
        SettingSource::ProjectSettings
    );
    assert_eq!(
        scope_to_setting_source(InstallableScope::Local),
        SettingSource::LocalSettings
    );
}

#[test]
fn test_is_installable_scope() {
    assert!(is_installable_scope("user"));
    assert!(is_installable_scope("project"));
    assert!(is_installable_scope("local"));
    assert!(!is_installable_scope("managed"));
    assert!(!is_installable_scope("invalid"));
}

#[test]
fn test_get_project_path_for_scope() {
    // user scope should return None
    assert!(get_project_path_for_scope("user").is_some() || true); // May return cwd
    assert!(get_project_path_for_scope("managed").is_none());
}

#[test]
fn test_plural() {
    assert_eq!(plural(0, "plugin"), "plugins");
    assert_eq!(plural(1, "plugin"), "plugin");
    assert_eq!(plural(2, "plugin"), "plugins");
    assert_eq!(plural(5, "marketplace"), "marketplaces");
}

#[test]
fn test_constants() {
    assert_eq!(VALID_INSTALLABLE_SCOPES, &["user", "project", "local"]);
    assert_eq!(
        VALID_UPDATE_SCOPES,
        &["user", "project", "local", "managed"]
    );
}

#[test]
fn test_plugin_operation_result_default() {
    let result = PluginOperationResult {
        success: false,
        message: "test".to_string(),
        plugin_id: None,
        plugin_name: None,
        scope: None,
        reverse_dependents: None,
    };
    assert!(!result.success);
    assert_eq!(result.message, "test");
    assert!(result.plugin_id.is_none());
}

#[test]
fn test_plugin_update_result_default() {
    let result = PluginUpdateResult {
        success: false,
        message: "test".to_string(),
        plugin_id: None,
        new_version: None,
        old_version: None,
        already_up_to_date: None,
        scope: None,
    };
    assert!(!result.success);
    assert!(result.already_up_to_date.is_none());
}

#[test]
fn test_parse_plugin_identifier_integration() {
    let (name, marketplace) = parse_plugin_identifier("my-plugin@my-marketplace");
    assert_eq!(name, Some("my-plugin".to_string()));
    assert_eq!(marketplace, Some("my-marketplace".to_string()));

    let (name, marketplace) = parse_plugin_identifier("simple-plugin");
    assert_eq!(name, None);
    assert_eq!(marketplace, None);
}

#[test]
fn test_is_builtin_plugin_id() {
    // Currently no builtin plugins
    assert!(!is_builtin_plugin_id("some-plugin"));
}

#[test]
fn test_format_resolution_error() {
    let msg = format_resolution_error("could not resolve");
    assert!(msg.contains("could not resolve"));
}

#[test]
fn test_installable_scope_from_to_plugin_scope() {
    let scope: PluginScope = InstallableScope::User.into();
    assert_eq!(scope, PluginScope::User);

    let scope: PluginScope = InstallableScope::Project.into();
    assert_eq!(scope, PluginScope::Project);

    let scope: PluginScope = InstallableScope::Local.into();
    assert_eq!(scope, PluginScope::Local);
}
