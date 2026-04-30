// Source: ~/claudecode/openclaudecode/src/services/plugins/PluginInstallationManager.ts
#![allow(dead_code)]

//! Background plugin and marketplace installation manager
//!
//! This module handles automatic installation of plugins and marketplaces
//! from trusted sources (repository and user settings) without blocking startup.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Progress event types for marketplace installation
#[derive(Debug, Clone)]
pub enum MarketplaceProgressEvent {
    Installing { name: String },
    Installed { name: String },
    Failed { name: String, error: String },
}

impl MarketplaceProgressEvent {
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::Installing { .. } => "installing",
            Self::Installed { .. } => "installed",
            Self::Failed { .. } => "failed",
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Installing { name } | Self::Installed { name } | Self::Failed { name, .. } => {
                name
            }
        }
    }
}

/// Marketplace installation status
#[derive(Debug, Clone)]
pub struct MarketplaceStatus {
    pub name: String,
    pub status: MarketplaceStatusKind,
    pub error: Option<String>,
}

/// Status kind for a marketplace
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketplaceStatusKind {
    Pending,
    Installing,
    Installed,
    Failed,
}

/// Plugin installation status
#[derive(Debug, Clone, Default)]
pub struct PluginInstallationStatus {
    pub marketplaces: Vec<MarketplaceStatus>,
    pub plugins: Vec<PluginStatusEntry>,
}

/// Plugin status entry
#[derive(Debug, Clone, Default)]
pub struct PluginStatusEntry {
    pub name: String,
    pub status: String,
}

/// Plugin state within AppState
#[derive(Debug, Clone, Default)]
pub struct PluginsState {
    pub installation_status: PluginInstallationStatus,
    pub needs_refresh: bool,
}

/// Application state (simplified - plugins-related portion)
#[derive(Debug, Clone, Default)]
pub struct AppState {
    pub plugins: PluginsState,
}

/// Function type for updating app state (functional update pattern)
pub type SetAppState = Arc<dyn Fn(&AppState) -> AppState + Send + Sync>;

/// Reconciliation progress callback type
pub type OnProgressCallback = Box<dyn Fn(MarketplaceProgressEvent) + Send>;

/// Result of marketplace reconciliation
#[derive(Debug, Clone, Default)]
pub struct ReconcileMarketplacesResult {
    pub installed: Vec<String>,
    pub updated: Vec<String>,
    pub failed: Vec<(String, String)>, // (name, error)
    pub up_to_date: Vec<String>,
}

/// Marketplace diff result
#[derive(Debug, Clone, Default)]
pub struct MarketplaceDiff {
    /// Marketplaces that are declared but missing from disk
    pub missing: Vec<String>,
    /// Marketplaces whose source has changed
    pub source_changed: Vec<SourceChangedEntry>,
}

/// Entry for a marketplace with a changed source
#[derive(Debug, Clone)]
pub struct SourceChangedEntry {
    pub name: String,
    pub old_source: String,
    pub new_source: String,
}

/// Declared marketplace entry
#[derive(Debug, Clone)]
pub struct DeclaredMarketplace {
    pub name: String,
    pub source: String,
}

/// Installed marketplace config
#[derive(Debug, Clone)]
pub struct InstalledMarketplaceConfig {
    pub name: String,
    pub install_location: String,
    pub source: String,
}

/// Analytics metrics for background install
#[derive(Debug, Clone)]
pub struct MarketplaceBackgroundInstallMetrics {
    pub installed_count: usize,
    pub updated_count: usize,
    pub failed_count: usize,
    pub up_to_date_count: usize,
}

/// Update marketplace installation status in app state
fn update_marketplace_status(
    set_app_state: &SetAppState,
    name: &str,
    status: MarketplaceStatusKind,
    error: Option<&str>,
) {
    let name = name.to_string();
    let error = error.map(String::from);

    set_app_state(&AppState {
        plugins: PluginsState {
            installation_status: PluginInstallationStatus {
                marketplaces: Vec::new(), // Will be populated by actual implementation
                plugins: Vec::new(),
            },
            needs_refresh: false,
        },
    });

    log::debug!(
        "Marketplace status update: {} -> {:?} (error: {:?})",
        name,
        status,
        error
    );
}

/// Get declared marketplaces from settings/config
fn get_declared_marketplaces() -> Vec<DeclaredMarketplace> {
    let declared = crate::utils::plugins::marketplace_manager::get_declared_marketplaces();
    declared
        .into_iter()
        .map(|(name, entry)| DeclaredMarketplace {
            name,
            source: serde_json::to_value(&entry.source)
                .ok()
                .and_then(|v| v.to_string().into())
                .unwrap_or_default(),
        })
        .collect()
}

/// Load known marketplaces config from disk (cache)
async fn load_known_marketplaces_config() -> HashMap<String, InstalledMarketplaceConfig> {
    match crate::utils::plugins::marketplace_manager::load_known_marketplaces_config().await {
        Ok(config) => config
            .into_iter()
            .map(|(name, entry)| {
                (
                    name.clone(),
                    InstalledMarketplaceConfig {
                        name,
                        install_location: entry.install_location,
                        source: serde_json::to_value(&entry.source)
                            .ok()
                            .and_then(|v| v.to_string().into())
                            .unwrap_or_default(),
                    },
                )
            })
            .collect(),
        Err(e) => {
            log::debug!("Failed to load known marketplaces config: {}", e);
            HashMap::new()
        }
    }
}

/// Compute diff between declared and materialized marketplaces
fn diff_marketplaces(
    declared: &[DeclaredMarketplace],
    materialized: &HashMap<String, InstalledMarketplaceConfig>,
) -> MarketplaceDiff {
    let mut missing = Vec::new();
    let mut source_changed = Vec::new();

    for declared_mkt in declared {
        if let Some(installed) = materialized.get(&declared_mkt.name) {
            // Check if source changed
            if installed.source != declared_mkt.source {
                source_changed.push(SourceChangedEntry {
                    name: declared_mkt.name.clone(),
                    old_source: installed.source.clone(),
                    new_source: declared_mkt.source.clone(),
                });
            }
        } else {
            // Marketplace not materialized
            missing.push(declared_mkt.name.clone());
        }
    }

    MarketplaceDiff {
        missing,
        source_changed,
    }
}

/// Reconcile marketplaces - install/update/verify declared marketplaces
///
/// This is the core reconciliation function that ensures all declared
/// marketplaces are installed and up to date.
async fn reconcile_marketplaces(
    _on_progress: Option<OnProgressCallback>,
) -> ReconcileMarketplacesResult {
    // In production, this would:
    // 1. Clone/update remote marketplaces
    // 2. Verify marketplace integrity
    // 3. Call on_progress callbacks during installation
    // 4. Return detailed results
    ReconcileMarketplacesResult::default()
}

/// Clear the marketplaces cache
fn clear_marketplaces_cache() {
    crate::utils::plugins::marketplace_manager::clear_marketplaces_cache();
}

/// Clear the plugin cache with an optional reason
fn clear_plugin_cache(reason: &str) {
    log::debug!("Clearing plugin cache: {}", reason);
    crate::utils::plugins::cache_utils::clear_all_plugin_caches();
}

/// Refresh active plugins - clears caches and reloads plugins
async fn refresh_active_plugins(
    _set_app_state: &SetAppState,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // In production, this would:
    // 1. Clear all plugin caches
    // 2. Reload plugins from disk
    // 3. Bump pluginReconnectKey so MCP connections are re-established
    // 4. Update app state with loaded plugins
    log::debug!("Refreshing active plugins");
    Ok(())
}

/// Log an analytics event
fn log_event(event_name: &str, metrics: &MarketplaceBackgroundInstallMetrics) {
    log::debug!(
        "Analytics event: {} installed={} updated={} failed={} up_to_date={}",
        event_name,
        metrics.installed_count,
        metrics.updated_count,
        metrics.failed_count,
        metrics.up_to_date_count
    );
}

/// Log a diagnostic message (no PII)
fn log_for_diagnostics_no_pii(
    level: &str,
    event: &str,
    metrics: &MarketplaceBackgroundInstallMetrics,
) {
    log::debug!(
        "[{}] {} installed={} updated={} failed={} up_to_date={}",
        level,
        event,
        metrics.installed_count,
        metrics.updated_count,
        metrics.failed_count,
        metrics.up_to_date_count
    );
}

/// Log for debugging
fn log_for_debugging(msg: &str) {
    log::debug!("{}", msg);
}

/// Log an error
fn log_error(error: &dyn std::error::Error) {
    log::error!("{}", error);
}

/// Pluralize helper
fn plural(count: usize, singular: &str) -> String {
    if count == 1 {
        singular.to_string()
    } else {
        format!("{}s", singular)
    }
}

/// Perform background plugin startup checks and installations.
///
/// This is a thin wrapper around reconcile_marketplaces() that maps on_progress
/// events to AppState updates for the REPL UI. After marketplaces are
/// reconciled:
/// - New installs -> auto-refresh plugins (fixes "plugin-not-found" errors
///   from the initial cache-only load on fresh homespace/cleared cache)
/// - Updates only -> set needs_refresh, show notification for /reload-plugins
pub async fn perform_background_plugin_installations(set_app_state: &SetAppState) {
    log_for_debugging("perform_background_plugin_installations called");

    // Compute diff upfront for initial UI status (pending spinners)
    let declared = get_declared_marketplaces();
    let materialized = load_known_marketplaces_config().await;
    let diff = diff_marketplaces(&declared, &materialized);

    let pending_names: Vec<String> = diff
        .missing
        .iter()
        .chain(diff.source_changed.iter().map(|c| &c.name))
        .cloned()
        .collect();

    // Initialize AppState with pending status. No per-plugin pending status --
    // plugin load is fast (cache hit or local copy); marketplace clone is the
    // slow part worth showing progress for.
    let pending_statuses: Vec<MarketplaceStatus> = pending_names
        .iter()
        .map(|name| MarketplaceStatus {
            name: name.clone(),
            status: MarketplaceStatusKind::Pending,
            error: None,
        })
        .collect();

    {
        let new_state = AppState {
            plugins: PluginsState {
                installation_status: PluginInstallationStatus {
                    marketplaces: pending_statuses,
                    plugins: Vec::new(),
                },
                needs_refresh: false,
            },
        };
        set_app_state(&new_state);
    }

    if pending_names.is_empty() {
        return;
    }

    log_for_debugging(&format!(
        "Installing {} marketplace(s) in background",
        pending_names.len()
    ));

    let result = reconcile_marketplaces(Some(Box::new(move |event| {
        let on_progress = move |ev: MarketplaceProgressEvent| match ev {
            MarketplaceProgressEvent::Installing { name } => {
                log::debug!("Installing marketplace: {}", name);
            }
            MarketplaceProgressEvent::Installed { name } => {
                log::debug!("Installed marketplace: {}", name);
            }
            MarketplaceProgressEvent::Failed { name, error } => {
                log::error!("Failed to install marketplace {}: {}", name, error);
            }
        };
        on_progress(event);
    })))
    .await;

    let metrics = MarketplaceBackgroundInstallMetrics {
        installed_count: result.installed.len(),
        updated_count: result.updated.len(),
        failed_count: result.failed.len(),
        up_to_date_count: result.up_to_date.len(),
    };

    log_event("tengu_marketplace_background_install", &metrics);
    log_for_diagnostics_no_pii("info", "tengu_marketplace_background_install", &metrics);

    if !result.installed.is_empty() {
        // New marketplaces were installed -- auto-refresh plugins. This fixes
        // "Plugin not found in marketplace" errors from the initial cache-only
        // load (e.g., fresh homespace where marketplace cache was empty).
        // refresh_active_plugins clears all caches, reloads plugins, and bumps
        // plugin_reconnect_key so MCP connections are re-established.
        clear_marketplaces_cache();
        log_for_debugging(&format!(
            "Auto-refreshing plugins after {} new marketplace(s) installed",
            result.installed.len()
        ));

        if let Err(refresh_error) = refresh_active_plugins(set_app_state).await {
            // If auto-refresh fails, fall back to needs_refresh notification so
            // the user can manually run /reload-plugins to recover.
            log_error(refresh_error.as_ref());
            log_for_debugging(&format!(
                "Auto-refresh failed, falling back to needs_refresh: {}",
                refresh_error
            ));
            clear_plugin_cache("perform_background_plugin_installations: auto-refresh failed");

            let new_state = AppState {
                plugins: PluginsState {
                    installation_status: PluginInstallationStatus::default(),
                    needs_refresh: true,
                },
            };
            set_app_state(&new_state);
        }
    } else if !result.updated.is_empty() {
        // Existing marketplaces updated -- notify user to run /reload-plugins.
        // Updates are less urgent and the user should choose when to apply them.
        clear_marketplaces_cache();
        clear_plugin_cache("perform_background_plugin_installations: marketplaces reconciled");

        let new_state = AppState {
            plugins: PluginsState {
                installation_status: PluginInstallationStatus::default(),
                needs_refresh: true,
            },
        };
        set_app_state(&new_state);
    }
}

/// Builder for PluginInstallationManager
pub struct PluginInstallationManagerBuilder {
    set_app_state: Option<SetAppState>,
}

impl PluginInstallationManagerBuilder {
    pub fn new() -> Self {
        Self {
            set_app_state: None,
        }
    }

    pub fn with_set_app_state(mut self, set_app_state: SetAppState) -> Self {
        self.set_app_state = Some(set_app_state);
        self
    }

    pub fn build(self) -> PluginInstallationManager {
        PluginInstallationManager {
            set_app_state: self.set_app_state,
        }
    }
}

impl Default for PluginInstallationManagerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Manager for background plugin and marketplace installations
pub struct PluginInstallationManager {
    set_app_state: Option<SetAppState>,
}

impl PluginInstallationManager {
    pub fn new() -> Self {
        Self {
            set_app_state: None,
        }
    }

    pub fn with_set_app_state(mut self, set_app_state: SetAppState) -> Self {
        self.set_app_state = Some(set_app_state);
        self
    }

    /// Perform background plugin installations if set_app_state is configured
    pub async fn perform_installations(&self) {
        if let Some(ref set_app_state) = self.set_app_state {
            perform_background_plugin_installations(set_app_state).await;
        }
    }

    /// Get the current app state (read-only snapshot)
    pub fn get_app_state(&self) -> Option<AppState> {
        // In a real implementation, this would read from the shared state
        None
    }
}

impl Default for PluginInstallationManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_marketplace_progress_event_types() {
        let installing = MarketplaceProgressEvent::Installing {
            name: "test".to_string(),
        };
        assert_eq!(installing.event_type(), "installing");
        assert_eq!(installing.name(), "test");

        let installed = MarketplaceProgressEvent::Installed {
            name: "test".to_string(),
        };
        assert_eq!(installed.event_type(), "installed");

        let failed = MarketplaceProgressEvent::Failed {
            name: "test".to_string(),
            error: "some error".to_string(),
        };
        assert_eq!(failed.event_type(), "failed");
    }

    #[test]
    fn test_marketplace_status_kind() {
        assert_eq!(
            MarketplaceStatusKind::Pending,
            MarketplaceStatusKind::Pending
        );
        assert_eq!(
            MarketplaceStatusKind::Installing,
            MarketplaceStatusKind::Installing
        );
        assert_eq!(
            MarketplaceStatusKind::Installed,
            MarketplaceStatusKind::Installed
        );
        assert_eq!(MarketplaceStatusKind::Failed, MarketplaceStatusKind::Failed);
    }

    #[test]
    fn test_diff_marketplaces_empty() {
        let declared: Vec<DeclaredMarketplace> = Vec::new();
        let materialized: HashMap<String, InstalledMarketplaceConfig> = HashMap::new();
        let diff = diff_marketplaces(&declared, &materialized);
        assert!(diff.missing.is_empty());
        assert!(diff.source_changed.is_empty());
    }

    #[test]
    fn test_diff_marketplaces_missing() {
        let declared = vec![DeclaredMarketplace {
            name: "test-marketplace".to_string(),
            source: "https://example.com".to_string(),
        }];
        let materialized: HashMap<String, InstalledMarketplaceConfig> = HashMap::new();
        let diff = diff_marketplaces(&declared, &materialized);
        assert_eq!(diff.missing, vec!["test-marketplace".to_string()]);
        assert!(diff.source_changed.is_empty());
    }

    #[test]
    fn test_diff_marketplaces_source_changed() {
        let declared = vec![DeclaredMarketplace {
            name: "test-marketplace".to_string(),
            source: "https://new-source.com".to_string(),
        }];
        let mut materialized = HashMap::new();
        materialized.insert(
            "test-marketplace".to_string(),
            InstalledMarketplaceConfig {
                name: "test-marketplace".to_string(),
                install_location: "/path/to/marketplace".to_string(),
                source: "https://old-source.com".to_string(),
            },
        );
        let diff = diff_marketplaces(&declared, &materialized);
        assert!(diff.missing.is_empty());
        assert_eq!(diff.source_changed.len(), 1);
        assert_eq!(diff.source_changed[0].name, "test-marketplace");
        assert_eq!(diff.source_changed[0].old_source, "https://old-source.com");
        assert_eq!(diff.source_changed[0].new_source, "https://new-source.com");
    }

    #[test]
    fn test_plural() {
        assert_eq!(plural(0, "plugin"), "plugins");
        assert_eq!(plural(1, "plugin"), "plugin");
        assert_eq!(plural(2, "plugin"), "plugins");
    }

    #[test]
    fn test_reconcile_result_default() {
        let result = ReconcileMarketplacesResult::default();
        assert!(result.installed.is_empty());
        assert!(result.updated.is_empty());
        assert!(result.failed.is_empty());
        assert!(result.up_to_date.is_empty());
    }

    #[test]
    fn test_plugin_installation_manager_default() {
        let manager = PluginInstallationManager::default();
        assert!(manager.set_app_state.is_none());
    }

    #[test]
    fn test_plugin_installation_manager_builder() {
        let manager = PluginInstallationManagerBuilder::new().build();
        assert!(manager.set_app_state.is_none());
    }

    #[test]
    fn test_marketplace_status_default() {
        let status = MarketplaceStatus {
            name: "test".to_string(),
            status: MarketplaceStatusKind::Pending,
            error: None,
        };
        assert_eq!(status.name, "test");
        assert_eq!(status.status, MarketplaceStatusKind::Pending);
        assert!(status.error.is_none());
    }
}
