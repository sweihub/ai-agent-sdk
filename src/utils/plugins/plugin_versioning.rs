// Source: ~/claudecode/openclaudecode/src/utils/plugins/pluginVersioning.ts
#![allow(dead_code)]

use std::path::Path;

use super::schemas::PluginManifest;

/// Get the git commit SHA for a directory that is a git repository.
pub fn get_git_commit_sha(dir_path: &str) -> Result<Option<String>, String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir_path)
        .output()
        .map_err(|e| format!("git rev-parse failed: {}", e))?;

    if output.status.success() {
        let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !sha.is_empty() {
            return Ok(Some(sha));
        }
    }
    Ok(None)
}

/// Calculate the version for a plugin based on its source.
pub async fn calculate_plugin_version(
    plugin_id: &str,
    _source: &super::schemas::PluginSource,
    manifest: Option<&PluginManifest>,
    install_path: Option<&str>,
    provided_version: Option<&str>,
    git_commit_sha: Option<&str>,
) -> String {
    // 1. Use explicit version from plugin.json if available
    if let Some(m) = manifest {
        if let Some(ref v) = m.version {
            log::debug!("Using manifest version for {}: {}", plugin_id, v);
            return v.clone();
        }
    }

    // 2. Use provided version (typically from marketplace entry)
    if let Some(v) = provided_version {
        log::debug!("Using provided version for {}: {}", plugin_id, v);
        return v.to_string();
    }

    // 3. Use pre-resolved git SHA
    if let Some(sha) = git_commit_sha {
        let short_sha = &sha[..sha.len().min(12)];
        log::debug!(
            "Using pre-resolved git SHA for {}: {}",
            plugin_id,
            short_sha
        );
        return short_sha.to_string();
    }

    // 4. Try to get git SHA from install path
    if let Some(path) = install_path {
        if let Ok(Some(sha)) = get_git_commit_sha(path) {
            let short_sha = &sha[..sha.len().min(12)];
            log::debug!("Using git SHA for {}: {}", plugin_id, short_sha);
            return short_sha.to_string();
        }
    }

    // 5. Return 'unknown' as last resort
    log::debug!("No version found for {}, using 'unknown'", plugin_id);
    "unknown".to_string()
}

/// Extract version from a versioned cache path.
pub fn get_version_from_path(install_path: &str) -> Option<String> {
    let parts: Vec<&str> = install_path.split('/').filter(|p| !p.is_empty()).collect();

    let cache_index = parts.iter().position(|&p| p == "cache");

    if let Some(idx) = cache_index {
        let components_after_cache = &parts[idx + 1..];
        if components_after_cache.len() >= 3 {
            return Some(components_after_cache[2].to_string());
        }
    }

    None
}

/// Check if a path is a versioned plugin path.
pub fn is_versioned_path(path: &str) -> bool {
    get_version_from_path(path).is_some()
}
