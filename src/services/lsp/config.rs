// Source: ~/claudecode/openclaudecode/src/services/lsp/config.ts
//! LSP configuration module
//! Loads LSP server configurations from plugins

use std::collections::HashMap;
use crate::plugin::builtin_plugins::get_builtin_plugins;
use crate::plugin::types::LoadedPlugin;
use super::types::LspServerConfig;

/// Result of loading all LSP servers
#[derive(Debug, Clone, Default)]
pub struct LspServersResult {
    /// Servers keyed by scoped server name
    pub servers: HashMap<String, LspServerConfig>,
}

/// Log for debugging
fn log_for_debugging(message: &str) {
    log::debug!("[LSP] {}", message);
}

/// Get all configured LSP servers from plugins.
/// LSP servers are only supported via plugins, not user/project settings.
///
/// Returns a HashMap containing servers configuration keyed by scoped server name.
/// This function collects LSP server configs from all enabled plugins and merges
/// them into a single map. Later plugins take precedence on key collisions.
pub async fn get_all_lsp_servers() -> LspServersResult {
    let mut all_servers: HashMap<String, LspServerConfig> = HashMap::new();

    // Get all enabled plugins from built-in plugins
    let builtin_result = get_builtin_plugins();
    let enabled_plugins: Vec<&LoadedPlugin> = builtin_result.enabled.iter().collect();

    // Also try to load plugins from directory if available
    let mut dir_plugins = Vec::new();
    let custom_plugin_dir = std::env::var("AI_CODE_PLUGIN_DIR");
    if let Ok(plugin_dir_path) = &custom_plugin_dir {
        dir_plugins = crate::plugin::loader::load_plugins_from_dir(
            &std::path::PathBuf::from(plugin_dir_path),
        )
        .await;
    }

    // Try loading from default plugin path
    if custom_plugin_dir.is_err() {
        if let Some(home) = dirs::home_dir() {
            let default_plugin_dir = home.join(".ai").join("plugins");
            if default_plugin_dir.exists() {
                dir_plugins.extend(
                    crate::plugin::loader::load_plugins_from_dir(&default_plugin_dir).await,
                );
            }
        }
    }

    let all_enabled: Vec<&LoadedPlugin> = enabled_plugins
        .into_iter()
        .chain(dir_plugins.iter())
        .collect();

    // Extract LSP servers from each plugin
    for plugin in all_enabled {
        if let Some(lsp_servers) = &plugin.lsp_servers {
            for (server_name, config_value) in lsp_servers {
                match serde_json::from_value(config_value.clone()) {
                    Ok(config) => {
                        all_servers.insert(server_name.clone(), config);
                    }
                    Err(e) => {
                        log_for_debugging(&format!(
                            "Failed to parse LSP server config '{}' from plugin '{}': {}",
                            server_name, plugin.name, e
                        ));
                    }
                }
            }
        }
    }

    log_for_debugging(&format!(
        "Total LSP servers loaded: {}",
        all_servers.len()
    ));

    LspServersResult {
        servers: all_servers,
    }
}

/// Legacy function name for backward compatibility
pub async fn get_all_lsp_servers_compatible() -> Result<LspServersResult, String> {
    Ok(get_all_lsp_servers().await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_all_lsp_servers_empty() {
        // With no plugins configured, should return empty result
        let result = get_all_lsp_servers().await;
        assert!(result.servers.is_empty());
    }

    #[tokio::test]
    async fn test_get_all_lsp_servers_compatible() {
        let result = get_all_lsp_servers_compatible().await;
        assert!(result.is_ok());
    }
}
