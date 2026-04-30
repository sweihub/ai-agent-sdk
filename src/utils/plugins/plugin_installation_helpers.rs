// Source: ~/claudecode/openclaudecode/src/utils/plugins/pluginInstallationHelpers.ts
#![allow(dead_code)]

use super::schemas::{PluginMarketplaceEntry, PluginManifest, PluginScope};

/// Get current ISO timestamp.
pub fn get_current_timestamp() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Validate that a resolved path stays within a base directory.
pub fn _validate_path_within_base(
    base_path: &str,
    relative_path: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let base = std::fs::canonicalize(base_path)?;
    let resolved = std::fs::canonicalize(std::path::Path::new(base_path).join(relative_path))?;

    let normalized_base = base.to_string_lossy();
    let normalized_base_with_sep = format!("{}{}", normalized_base, std::path::MAIN_SEPARATOR);

    if !resolved.starts_with(&normalized_base_with_sep) && resolved != base {
        return Err(format!(
            "Path traversal detected: \"{}\" would escape the base directory",
            relative_path
        )
        .into());
    }

    Ok(resolved.to_string_lossy().to_string())
}

/// Cache a plugin and add it to installed_plugins.json.
pub async fn cache_and_register_plugin(
    plugin_id: &str,
    entry: &PluginMarketplaceEntry,
    scope: PluginScope,
    project_path: Option<&str>,
    local_source_path: Option<&str>,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    // For local sources, use the provided path directly
    if let Some(local_path) = local_source_path {
        // Validate path exists
        if !std::path::Path::new(local_path).exists() {
            return Err(format!("Local plugin path does not exist: {}", local_path).into());
        }

        // Calculate version from local path
        let version =
            super::plugin_versioning::calculate_plugin_version(
                plugin_id,
                &entry.source,
                Some(&PluginManifest {
                    name: entry.name.clone(),
                    version: entry.version.clone(),
                    description: None,
                    author: None,
                    dependencies: None,
                    user_config: None,
                }),
                Some(local_path),
                entry.version.as_deref(),
                None,
            )
            .await;
        let version_str = version;

        // Copy to versioned cache
        let cache_path = super::loader::get_versioned_cache_path(plugin_id, &version_str);
        let zip_cache_mode = super::zip_cache::is_plugin_zip_cache_enabled();

        if let Some(parent) = std::path::Path::new(&cache_path).parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create cache parent dir: {}", e))?;
        }

        // Copy source to cache
        copy_directory(std::path::Path::new(local_path), std::path::Path::new(&cache_path))?;

        // Register in installed_plugins.json
        let metadata = super::schemas::PluginInstallationEntry {
            scope: scope.clone(),
            install_path: cache_path.clone(),
            version: Some(version_str),
            installed_at: get_current_timestamp(),
            last_updated: get_current_timestamp(),
            git_commit_sha: None,
            project_path: project_path.map(|s| s.to_string()),
        };

        super::installed_plugins_manager::add_plugin_installation(
            plugin_id,
            scope,
            &cache_path,
            &metadata,
            project_path,
        );

        return Ok(cache_path);
    }

    // For remote sources, use the loader's cache_plugin
    return Err("Remote plugin caching should be handled by plugin_operations::cache_plugin".into());
}

/// Copy a directory recursively
fn copy_directory(src: &std::path::Path, dst: &std::path::Path) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_directory(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

/// Register a plugin installation without caching.
pub fn _register_plugin_installation(
    _plugin_id: &str,
    _install_path: &str,
    _version: Option<&str>,
    _scope: PluginScope,
    _project_path: Option<&str>,
) {
    // Stub
}

/// Format a failed ResolutionResult into a user-facing message.
pub fn _format_resolution_error(_r: &super::dependency_resolver::ResolutionResult) -> String {
    "Unknown resolution error".to_string()
}
