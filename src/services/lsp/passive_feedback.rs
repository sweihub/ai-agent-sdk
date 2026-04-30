// Source: ~/claudecode/openclaudecode/src/services/lsp/passiveFeedback.ts
//! LSP passive feedback - handles diagnostics from LSP servers
//!
//! Registers notification handlers on all LSP servers to capture
//! textDocument/publishDiagnostics notifications and route them
//! to the diagnostic tracking system.

use std::collections::HashMap;

use crate::utils::debug::{log_for_debugging, DebugLogLevel};

use crate::services::lsp::lsp_server_manager::LspServerManager;
use crate::services::lsp::lsp_client::NotificationHandler;

/// Diagnostic severity mapping from LSP to Claude
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
    Hint,
}

impl DiagnosticSeverity {
    /// Map LSP severity to Claude diagnostic severity
    /// LSP DiagnosticSeverity enum: 1=Error, 2=Warning, 3=Information, 4=Hint
    pub fn from_lsp(severity: Option<u32>) -> Self {
        match severity.unwrap_or(1) {
            1 => DiagnosticSeverity::Error,
            2 => DiagnosticSeverity::Warning,
            3 => DiagnosticSeverity::Info,
            4 => DiagnosticSeverity::Hint,
            _ => DiagnosticSeverity::Error,
        }
    }
}

/// LSP diagnostic range
#[derive(Debug, Clone)]
pub struct LspRange {
    pub start: LspPosition,
    pub end: LspPosition,
}

/// LSP position
#[derive(Debug, Clone)]
pub struct LspPosition {
    pub line: u32,
    pub character: u32,
}

/// LSP diagnostic
#[derive(Debug, Clone)]
pub struct LspDiagnostic {
    pub message: String,
    pub severity: Option<u32>,
    pub range: LspRange,
    pub source: Option<String>,
    pub code: Option<String>,
}

/// Publish diagnostics params from LSP
#[derive(Debug, Clone)]
pub struct PublishDiagnosticsParams {
    pub uri: String,
    pub diagnostics: Vec<LspDiagnostic>,
}

/// Diagnostic file for Claude attachment system
#[derive(Debug, Clone)]
pub struct DiagnosticFile {
    pub uri: String,
    pub diagnostics: Vec<FormattedDiagnostic>,
}

/// Formatted diagnostic for attachment
#[derive(Debug, Clone)]
pub struct FormattedDiagnostic {
    pub message: String,
    pub severity: String,
    pub range: DiagnosticRange,
    pub source: Option<String>,
    pub code: Option<String>,
}

/// Diagnostic range
#[derive(Debug, Clone)]
pub struct DiagnosticRange {
    pub start: DiagnosticPosition,
    pub end: DiagnosticPosition,
}

/// Diagnostic position
#[derive(Debug, Clone)]
pub struct DiagnosticPosition {
    pub line: u32,
    pub character: u32,
}

/// Map LSP severity to Claude diagnostic severity string
pub fn map_lsp_severity_to_string(severity: Option<u32>) -> String {
    match DiagnosticSeverity::from_lsp(severity) {
        DiagnosticSeverity::Error => "Error".to_string(),
        DiagnosticSeverity::Warning => "Warning".to_string(),
        DiagnosticSeverity::Info => "Info".to_string(),
        DiagnosticSeverity::Hint => "Hint".to_string(),
    }
}

/// Convert LSP URI to file path
/// Handles both file:// URIs and plain paths
pub fn uri_to_file_path(uri: &str) -> String {
    if uri.starts_with("file://") {
        if let Ok(parsed) = url::Url::parse(uri) {
            if let Ok(file_path) = parsed.to_file_path() {
                return file_path.to_string_lossy().to_string();
            }
        }
    }
    uri.to_string()
}

/// Convert LSP diagnostics to Claude diagnostic format
pub fn format_diagnostics_for_attachment(
    params: PublishDiagnosticsParams,
) -> Vec<DiagnosticFile> {
    let uri = uri_to_file_path(&params.uri);

    let diagnostics: Vec<FormattedDiagnostic> = params
        .diagnostics
        .into_iter()
        .map(|diag| FormattedDiagnostic {
            message: diag.message,
            severity: map_lsp_severity_to_string(diag.severity),
            range: DiagnosticRange {
                start: DiagnosticPosition {
                    line: diag.range.start.line,
                    character: diag.range.start.character,
                },
                end: DiagnosticPosition {
                    line: diag.range.end.line,
                    character: diag.range.end.character,
                },
            },
            source: diag.source,
            code: diag.code,
        })
        .collect();

    vec![DiagnosticFile { uri, diagnostics }]
}

/// Handler registration result
#[derive(Debug, Clone)]
pub struct HandlerRegistrationResult {
    pub total_servers: usize,
    pub success_count: usize,
    pub registration_errors: Vec<RegistrationError>,
    pub diagnostic_failures: HashMap<String, FailureInfo>,
}

/// Registration error
#[derive(Debug, Clone)]
pub struct RegistrationError {
    pub server_name: String,
    pub error: String,
}

/// Failure info
#[derive(Debug, Clone)]
pub struct FailureInfo {
    pub count: u32,
    pub last_error: String,
}

/// Register LSP notification handlers on all servers in the manager.
///
/// Sets up handlers to listen for textDocument/publishDiagnostics
/// notifications from all LSP servers and log them for debugging.
pub fn register_lsp_notification_handlers(manager: &LspServerManager) -> HandlerRegistrationResult {
    let servers = manager.get_all_servers();
    let mut success_count = 0usize;
    let mut registration_errors = Vec::new();
    let diagnostic_failures = HashMap::new();

    for (server_name, server_instance) in &servers {
        let server_name_owned = server_name.clone();
        server_instance.on_notification(
            "textDocument/publishDiagnostics".to_string(),
            Box::new(move |params: serde_json::Value| {
                log_for_debugging(
                    &format!(
                        "[PASSIVE DIAGNOSTICS] Handler invoked for {}! Params type: {:?}",
                        server_name_owned,
                        params.get("diagnostics").map(|d| {
                            if d.is_array() {
                                format!("array({})", d.as_array().unwrap().len())
                            } else {
                                "unknown".to_string()
                            }
                        }).unwrap_or_else(|| "missing".to_string()),
                    ),
                    DebugLogLevel::Debug,
                );

                // Validate params structure
                if params.get("uri").is_none() || params.get("diagnostics").is_none() {
                    log_for_debugging(
                        &format!(
                            "[LSP] Invalid diagnostic params from {} (missing uri or diagnostics)",
                            server_name_owned
                        ),
                        DebugLogLevel::Error,
                    );
                    return;
                }

                let diagnostics = params["diagnostics"].as_array();
                if diagnostics.map(|d| d.is_empty()).unwrap_or(true) {
                    log_for_debugging(
                        &format!(
                            "[LSP] Skipping empty diagnostics from {} for {}",
                            server_name_owned,
                            params["uri"].as_str().unwrap_or("unknown")
                        ),
                        DebugLogLevel::Debug,
                    );
                    return;
                }

                log_for_debugging(
                    &format!(
                        "[LSP PASSIVE DIAGNOSTICS] Received {} diagnostic(s) from {}",
                        diagnostics.map(|d| d.len()).unwrap_or(0),
                        server_name_owned
                    ),
                    DebugLogLevel::Debug,
                );
            }) as NotificationHandler,
        );

        log_for_debugging(
            &format!("[LSP] Registered diagnostics handler for {}", server_name),
            DebugLogLevel::Debug,
        );
        success_count += 1;
    }

    let total_servers = servers.len();
    if registration_errors.is_empty() {
        log_for_debugging(
            &format!(
                "[LSP] Notification handlers registered for all {} server(s)",
                total_servers
            ),
            DebugLogLevel::Debug,
        );
    } else {
        let failed_servers: Vec<String> = registration_errors
            .iter()
            .map(|e: &RegistrationError| format!("{} ({})", e.server_name, e.error))
            .collect();
        log_for_debugging(
            &format!(
                "[LSP] Handler registration: {}/{} succeeded. Failed: {}",
                success_count,
                total_servers,
                failed_servers.join(", ")
            ),
            DebugLogLevel::Error,
        );
    }

    HandlerRegistrationResult {
        total_servers,
        success_count,
        registration_errors,
        diagnostic_failures,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_lsp_severity_error() {
        assert_eq!(map_lsp_severity_to_string(Some(1)), "Error");
    }

    #[test]
    fn test_map_lsp_severity_warning() {
        assert_eq!(map_lsp_severity_to_string(Some(2)), "Warning");
    }

    #[test]
    fn test_map_lsp_severity_info() {
        assert_eq!(map_lsp_severity_to_string(Some(3)), "Info");
    }

    #[test]
    fn test_map_lsp_severity_hint() {
        assert_eq!(map_lsp_severity_to_string(Some(4)), "Hint");
    }

    #[test]
    fn test_map_lsp_severity_default() {
        assert_eq!(map_lsp_severity_to_string(None), "Error");
        assert_eq!(map_lsp_severity_to_string(Some(999)), "Error");
    }

    #[test]
    fn test_uri_to_file_path_plain() {
        assert_eq!(uri_to_file_path("/some/path/file.rs"), "/some/path/file.rs");
    }
}
