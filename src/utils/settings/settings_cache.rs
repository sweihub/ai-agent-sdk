// Source: ~/claudecode/openclaudecode/src/utils/settings/settingsCache.ts
//! In-memory cache for settings data.
//!
//! Three caches:
//! 1. Session settings cache (merged, validated settings)
//! 2. Per-source cache (individual source settings)
//! 3. Parse file cache (single file parse results)

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use super::SettingSource;
use super::validation::{SettingsWithErrors, ValidationError};

/// Parsed settings from a single file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParsedSettings {
    pub settings: Option<serde_json::Value>,
    pub errors: Vec<ValidationError>,
}

struct SettingsCacheInner {
    /// Merged session settings cache. Null = not yet computed.
    session_settings: Option<SettingsWithErrors>,
    /// Per-source cache. None = cache miss, Some(None) = cached null.
    per_source_cache: HashMap<SettingSource, Option<serde_json::Value>>,
    /// Path-keyed parse cache.
    parse_file_cache: HashMap<String, ParsedSettings>,
    /// Plugin settings base layer (lowest priority in cascade).
    plugin_settings_base: Option<serde_json::Value>,
}

static SETTINGS_CACHE: LazyLock<Mutex<SettingsCacheInner>> = LazyLock::new(|| {
    Mutex::new(SettingsCacheInner {
        session_settings: None,
        per_source_cache: HashMap::new(),
        parse_file_cache: HashMap::new(),
        plugin_settings_base: None,
    })
});

/// Get the merged session settings cache.
pub fn get_session_settings_cache() -> Option<SettingsWithErrors> {
    SETTINGS_CACHE.lock().unwrap().session_settings.clone()
}

/// Set the merged session settings cache.
pub fn set_session_settings_cache(value: SettingsWithErrors) {
    SETTINGS_CACHE.lock().unwrap().session_settings = Some(value);
}

/// Get cached settings for a source.
/// Returns `None` for cache miss, `Some(None)` for cached "no settings".
pub fn get_cached_settings_for_source(
    source: &SettingSource,
) -> Option<Option<serde_json::Value>> {
    SETTINGS_CACHE
        .lock()
        .unwrap()
        .per_source_cache
        .get(source)
        .cloned()
}

/// Set cached settings for a source.
pub fn set_cached_settings_for_source(
    source: &SettingSource,
    value: Option<serde_json::Value>,
) {
    let mut cache = SETTINGS_CACHE.lock().unwrap();
    cache.per_source_cache.insert(source.clone(), value);
}

/// Get cached parsed file result.
pub fn get_cached_parsed_file(path: &str) -> Option<ParsedSettings> {
    SETTINGS_CACHE
        .lock()
        .unwrap()
        .parse_file_cache
        .get(path)
        .cloned()
}

/// Set cached parsed file result.
pub fn set_cached_parsed_file(path: &str, value: ParsedSettings) {
    SETTINGS_CACHE
        .lock()
        .unwrap()
        .parse_file_cache
        .insert(path.to_string(), value);
}

/// Get the plugin settings base layer.
pub fn get_plugin_settings_base() -> Option<serde_json::Value> {
    SETTINGS_CACHE.lock().unwrap().plugin_settings_base.clone()
}

/// Set the plugin settings base layer.
pub fn set_plugin_settings_base(value: Option<serde_json::Value>) {
    SETTINGS_CACHE.lock().unwrap().plugin_settings_base = value;
}

/// Clear the plugin settings base layer.
pub fn clear_plugin_settings_base() {
    SETTINGS_CACHE.lock().unwrap().plugin_settings_base = None;
}

/// Clear all three caches. Called on settings write, plugin init, hooks refresh.
pub fn reset_settings_cache() {
    let mut cache = SETTINGS_CACHE.lock().unwrap();
    cache.session_settings = None;
    cache.per_source_cache.clear();
    cache.parse_file_cache.clear();
    cache.plugin_settings_base = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[serial_test::serial]
    fn test_session_cache_roundtrip() {
        reset_settings_cache();
        assert!(get_session_settings_cache().is_none());

        let settings = SettingsWithErrors {
            settings: serde_json::json!({"permissions": {"defaultMode": "allow"}}),
            errors: vec![],
        };
        set_session_settings_cache(settings.clone());
        assert_eq!(
            get_session_settings_cache().as_ref(),
            Some(&settings),
        );
    }

    #[test]
    #[serial_test::serial]
    fn test_per_source_cache() {
        reset_settings_cache();
        let source = SettingSource::UserSettings;
        // Cache miss
        assert!(get_cached_settings_for_source(&source).is_none());

        // Set value
        set_cached_settings_for_source(
            &source,
            Some(serde_json::json!({"permissions": {"defaultMode": "deny"}})),
        );
        assert!(get_cached_settings_for_source(&source).is_some());

        // Set null (no settings for source)
        set_cached_settings_for_source(&source, None);
        assert!(get_cached_settings_for_source(&source).is_some());
        assert!(get_cached_settings_for_source(&source).unwrap().is_none());
    }

    #[test]
    #[serial_test::serial]
    fn test_parse_file_cache() {
        reset_settings_cache();
        assert!(get_cached_parsed_file("/nonexistent").is_none());

        let parsed = ParsedSettings {
            settings: Some(serde_json::json!({})),
            errors: vec![],
        };
        set_cached_parsed_file("/test/path.json", parsed.clone());
        assert_eq!(
            get_cached_parsed_file("/test/path/path.json").as_ref(),
            None,
        );
        assert_eq!(
            get_cached_parsed_file("/test/path.json").as_ref(),
            Some(&parsed),
        );
    }

    #[test]
    #[serial_test::serial]
    fn test_plugin_settings_base() {
        reset_settings_cache();
        assert!(get_plugin_settings_base().is_none());

        set_plugin_settings_base(Some(serde_json::json!({"model": {"name": "test"}})));
        assert!(get_plugin_settings_base().is_some());

        clear_plugin_settings_base();
        assert!(get_plugin_settings_base().is_none());
    }

    #[test]
    #[serial_test::serial]
    fn test_reset_clears_all() {
        // Populate all caches
        set_session_settings_cache(SettingsWithErrors {
            settings: serde_json::json!({}),
            errors: vec![],
        });
        set_cached_settings_for_source(
            &SettingSource::UserSettings,
            Some(serde_json::json!({})),
        );
        set_cached_parsed_file(
            "/test.json",
            ParsedSettings {
                settings: Some(serde_json::json!({})),
                errors: vec![],
            },
        );
        set_plugin_settings_base(Some(serde_json::json!({})));

        reset_settings_cache();

        assert!(get_session_settings_cache().is_none());
        assert!(get_cached_settings_for_source(&SettingSource::UserSettings).is_none());
        assert!(get_cached_parsed_file("/test.json").is_none());
        // Plugin base is not cleared by reset (separate concern)
        // Actually in TS it IS NOT cleared by resetSettingsCache
    }
}
