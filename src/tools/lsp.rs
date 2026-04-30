// Source: ~/claudecode/openclaudecode/src/tools/LSPTool/LSPTool.ts
//! LSP tool - code intelligence via Language Server Protocol.

use crate::error::AgentError;
use crate::services::lsp::manager::{
    get_initialization_status, get_lsp_server_manager, wait_for_initialization,
    InitializationStatus,
};
use crate::types::*;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::path::PathBuf;
use tokio::fs;
use tokio::process::Command;

pub const LSP_TOOL_NAME: &str = "LSP";
pub const DESCRIPTION: &str =
    "Interact with Language Server Protocol servers for code intelligence (definitions, references, symbols, hover, call hierarchy)";

const MAX_LSP_FILE_SIZE_BYTES: u64 = 10_000_000;

/// Check if a path is git-ignored using `git check-ignore`
async fn is_git_ignored(path: &Path) -> bool {
    Command::new("git")
        .args(["check-ignore", "-q", "--"])
        .arg(path)
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Batch-check which paths are git-ignored, returning the ignored set.
async fn batch_git_ignored(paths: &[String], cwd: &str) -> HashSet<String> {
    let mut ignored = HashSet::new();
    if paths.is_empty() {
        return ignored;
    }

    const BATCH_SIZE: usize = 50;
    for batch in paths.chunks(BATCH_SIZE) {
        let out = Command::new("git")
            .args(["check-ignore"])
            .args(batch)
            .current_dir(cwd)
            .output()
            .await;

        if let Ok(output) = out {
            if output.status.code() == Some(0) {
                for line in String::from_utf8_lossy(&output.stdout).lines() {
                    let trimmed = line.trim().to_string();
                    if !trimmed.is_empty() {
                        ignored.insert(trimmed);
                    }
                }
            }
        }
    }
    ignored
}

/// Convert a file:// URI back to a file path
fn uri_to_file_path(uri: &str) -> String {
    if uri.starts_with("file://") {
        let path_part = &uri[7..];
        // Windows: /C:/path -> C:/path
        let cleaned = if cfg!(windows) && path_part.len() > 2 && path_part.starts_with('/') && path_part.chars().nth(1) == Some(':') {
            path_part[1..].to_string()
        } else {
            path_part.to_string()
        };
        percent_decode_str(&cleaned)
    } else {
        uri.to_string()
    }
}

/// Simple percent-decoder for file URIs
fn percent_decode_str(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.bytes();
    while let Some(b) = chars.next() {
        if b == b'%' {
            let h1 = chars.next();
            let h2 = chars.next();
            if let (Some(h1), Some(h2)) = (h1, h2) {
                let hex = format!("{}{}", h1 as char, h2 as char);
                if let Ok(val) = u8::from_str_radix(&hex, 16) {
                    result.push(val as char);
                    continue;
                }
            }
            result.push('%');
            if let Some(h1) = h1 {
                result.push(h1 as char);
            }
            if let Some(h2) = h2 {
                result.push(h2 as char);
            }
        } else {
            result.push(b as char);
        }
    }
    result
}

/// Format a URI for display, making it relative to cwd when shorter
fn format_uri(uri: &str, cwd: Option<&str>) -> String {
    let file_path = uri_to_file_path(uri);
    if let Some(cwd) = cwd {
        if let Ok(rel) = std::path::Path::new(&file_path).strip_prefix(cwd) {
            let rel_str = rel.to_string_lossy().to_string();
            if rel_str.len() < file_path.len() && !rel_str.starts_with("../../") {
                return rel_str;
            }
        }
    }
    file_path
}

/// Format a location as "filePath:line:character"
fn format_location(loc: &serde_json::Value, cwd: Option<&str>) -> String {
    let uri = loc.get("uri").and_then(|u| u.as_str()).unwrap_or("");
    let file_path = format_uri(uri, cwd);
    let line = loc.get("range")
        .and_then(|r| r.get("start"))
        .and_then(|s| s.get("line"))
        .and_then(|l| l.as_u64())
        .unwrap_or(0);
    let character = loc.get("range")
        .and_then(|r| r.get("start"))
        .and_then(|s| s.get("character"))
        .and_then(|c| c.as_u64())
        .unwrap_or(0);
    format!("{}:{}:{}", file_path, line + 1, character + 1)
}

/// Map LSP SymbolKind to human-readable string
fn symbol_kind_to_string(kind: u64) -> &'static str {
    match kind {
        1 => "File",
        2 => "Module",
        3 => "Namespace",
        4 => "Package",
        5 => "Class",
        6 => "Method",
        7 => "Property",
        8 => "Field",
        9 => "Constructor",
        10 => "Enum",
        11 => "Interface",
        12 => "Function",
        13 => "Variable",
        14 => "Constant",
        15 => "String",
        16 => "Number",
        17 => "Boolean",
        18 => "Array",
        19 => "Object",
        20 => "Key",
        21 => "Null",
        22 => "EnumMember",
        23 => "Struct",
        24 => "Event",
        25 => "Operator",
        26 => "TypeParameter",
        _ => "Unknown",
    }
}

/// Pluralize: "1 call" vs "N calls"
fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        format!("{} {}", n, word)
    } else {
        format!("{} {}s", n, word)
    }
}

/// Extract text from hover contents (MarkupContent | MarkedString | MarkedString[])
fn extract_hover_text(contents: &serde_json::Value) -> String {
    if contents.is_array() {
        contents
            .as_array()
            .unwrap()
            .iter()
            .map(|item| {
                if item.is_string() {
                    item.as_str().unwrap().to_string()
                } else {
                    item.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    } else if contents.is_string() {
        contents.as_str().unwrap().to_string()
    } else if contents.get("kind").is_some() {
        // MarkupContent
        contents.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string()
    } else {
        // MarkedString object
        contents.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string()
    }
}

/// Format goToDefinition / goToImplementation result
fn format_definition_result(result: &serde_json::Value, cwd: Option<&str>) -> String {
    if result.is_null() || !result.is_array() && !result.is_object() {
        return "No definition found. This may occur if the cursor is not on a symbol, or if the definition is in an external library not indexed by the LSP server.".to_string();
    }

    let locations: Vec<&serde_json::Value> = if result.is_array() {
        result.as_array().unwrap().iter().collect()
    } else {
        vec![result]
    };

    let valid: Vec<&serde_json::Value> = locations
        .into_iter()
        .filter(|l| l.get("uri").is_some() || l.get("targetUri").is_some())
        .collect();

    if valid.is_empty() {
        return "No definition found. This may occur if the cursor is not on a symbol, or if the definition is in an external library not indexed by the LSP server.".to_string();
    }

    if valid.len() == 1 {
        let loc = to_location(valid[0]);
        return format!("Defined in {}", format_location(&loc, cwd));
    }

    let lines: Vec<_> = valid.iter().map(|loc| {
        let l = to_location(loc);
        format!("  {}", format_location(&l, cwd))
    }).collect();
    format!("Found {} definitions:\n{}", valid.len(), lines.join("\n"))
}

/// Convert LocationLink to Location for uniform handling
fn to_location(item: &serde_json::Value) -> serde_json::Value {
    if item.get("targetUri").is_some() {
        let target_uri = item.get("targetUri").and_then(|u| u.as_str()).unwrap_or("");
        let target_sel = item.get("targetSelectionRange");
        let target_range = item.get("targetRange");
        let range = target_sel.or(target_range);
        serde_json::json!({
            "uri": target_uri,
            "range": range,
        })
    } else {
        item.clone()
    }
}

/// Format findReferences result
fn format_references_result(result: &serde_json::Value, cwd: Option<&str>) -> String {
    let locations: Vec<&serde_json::Value> = if result.is_array() {
        result.as_array().unwrap().iter()
            .filter(|l| l.get("uri").is_some())
            .collect()
    } else {
        vec![]
    };

    if locations.is_empty() {
        return "No references found. This may occur if the symbol has no usages, or if the LSP server has not fully indexed the workspace.".to_string();
    }

    if locations.len() == 1 {
        return format!("Found 1 reference:\n  {}", format_location(locations[0], cwd));
    }

    // Group by file
    let mut by_file: HashMap<String, Vec<&serde_json::Value>> = HashMap::new();
    for loc in &locations {
        let uri = loc.get("uri").and_then(|u| u.as_str()).unwrap_or("");
        let fp = format_uri(uri, cwd);
        by_file.entry(fp).or_default().push(loc);
    }

    let mut lines = vec![format!("Found {} references across {} files:", locations.len(), by_file.len())];
    for (fp, locs) in &by_file {
        lines.push(format!("\n{fp}:"));
        for loc in locs {
            let line = loc.get("range").and_then(|r| r.get("start")).and_then(|s| s.get("line")).and_then(|l| l.as_u64()).unwrap_or(0);
            let character = loc.get("range").and_then(|r| r.get("start")).and_then(|s| s.get("character")).and_then(|c| c.as_u64()).unwrap_or(0);
            lines.push(format!("  Line {}:{}", line + 1, character + 1));
        }
    }
    lines.join("\n")
}

/// Format hover result
fn format_hover_result(result: &serde_json::Value, _cwd: Option<&str>) -> String {
    if result.is_null() {
        return "No hover information available. This may occur if the cursor is not on a symbol, or if the LSP server has not fully indexed the file.".to_string();
    }

    let contents = result.get("contents");
    if contents.is_none() {
        return "No hover information available.".to_string();
    }
    let content = extract_hover_text(contents.unwrap());

    if let Some(range) = result.get("range") {
        let line = range.get("start").and_then(|s| s.get("line")).and_then(|l| l.as_u64()).unwrap_or(0);
        let character = range.get("start").and_then(|s| s.get("character")).and_then(|c| c.as_u64()).unwrap_or(0);
        return format!("Hover info at {}:{}:\n\n{}", line + 1, character + 1, content);
    }
    content
}

/// Count nested symbols in DocumentSymbol array
fn count_symbols(symbols: &[&serde_json::Value]) -> usize {
    let mut count = symbols.len();
    for sym in symbols {
        if let Some(children) = sym.get("children").and_then(|c| c.as_array()) {
            count += count_symbols(&children.iter().collect::<Vec<_>>());
        }
    }
    count
}

/// Format a single document symbol node
fn format_document_symbol_node(sym: &serde_json::Value, indent: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let prefix = "  ".repeat(indent);
    let name = sym.get("name").and_then(|n| n.as_str()).unwrap_or("<unknown>");
    let kind = sym.get("kind").and_then(|k| k.as_u64()).unwrap_or(0);
    let kind_str = symbol_kind_to_string(kind);
    let detail = sym.get("detail").and_then(|d| d.as_str());
    let symbol_line = sym.get("range")
        .and_then(|r| r.get("start"))
        .and_then(|s| s.get("line"))
        .and_then(|l| l.as_u64())
        .unwrap_or(0);

    let mut line = format!("{}{} ({})", prefix, name, kind_str);
    if let Some(det) = detail {
        line.push_str(" ");
        line.push_str(det);
    }
    line.push_str(&format!(" - Line {}", symbol_line + 1));
    lines.push(line);

    if let Some(children) = sym.get("children").and_then(|c| c.as_array()) {
        for child in children {
            lines.extend(format_document_symbol_node(child, indent + 1));
        }
    }
    lines
}

/// Format documentSymbol result
fn format_document_symbol_result(result: &serde_json::Value, cwd: Option<&str>) -> String {
    let symbols: Vec<&serde_json::Value> = if result.is_array() {
        result.as_array().unwrap().iter().collect()
    } else {
        vec![]
    };

    if symbols.is_empty() {
        return "No symbols found in document. This may occur if the file is empty, not supported by the LSP server, or if the server has not fully indexed the file.".to_string();
    }

    // Detect format: DocumentSymbol has 'range', SymbolInformation has 'location'
    let is_symbol_info = symbols[0].get("location").is_some();

    if is_symbol_info {
        return format_workspace_symbol_result(result, cwd);
    }

    let mut lines = vec!["Document symbols:".to_string()];
    for sym in &symbols {
        lines.extend(format_document_symbol_node(sym, 0));
    }
    lines.join("\n")
}

/// Format workspaceSymbol result
fn format_workspace_symbol_result(result: &serde_json::Value, cwd: Option<&str>) -> String {
    let symbols: Vec<&serde_json::Value> = if result.is_array() {
        result.as_array().unwrap().iter()
            .filter(|s| s.get("location").and_then(|l| l.get("uri")).is_some())
            .collect()
    } else {
        vec![]
    };

    if symbols.is_empty() {
        return "No symbols found in workspace. This may occur if the workspace is empty, or if the LSP server has not finished indexing the project.".to_string();
    }

    let mut by_file: HashMap<String, Vec<&serde_json::Value>> = HashMap::new();
    for sym in &symbols {
        let uri = sym.get("location").and_then(|l| l.get("uri")).and_then(|u| u.as_str()).unwrap_or("");
        let fp = format_uri(uri, cwd);
        by_file.entry(fp).or_default().push(sym);
    }

    let mut lines = vec![format!("Found {} in workspace:", plural(symbols.len(), "symbol"))];
    for (fp, syms) in &by_file {
        lines.push(format!("\n{fp}:"));
        for sym in syms {
            let name = sym.get("name").and_then(|n| n.as_str()).unwrap_or("<unknown>");
            let kind = sym.get("kind").and_then(|k| k.as_u64()).unwrap_or(0);
            let kind_str = symbol_kind_to_string(kind);
            let line = sym.get("location").and_then(|l| l.get("range"))
                .and_then(|r| r.get("start"))
                .and_then(|s| s.get("line"))
                .and_then(|l| l.as_u64())
                .unwrap_or(0);
            let container = sym.get("containerName").and_then(|c| c.as_str());
            let mut sym_line = format!("  {} ({}) - Line {}", name, kind_str, line + 1);
            if let Some(cnt) = container {
                sym_line.push_str(" in ");
                sym_line.push_str(cnt);
            }
            lines.push(sym_line);
        }
    }
    lines.join("\n")
}

/// Format prepareCallHierarchy result
fn format_call_hierarchy_result(result: &serde_json::Value, cwd: Option<&str>) -> String {
    let items: Vec<&serde_json::Value> = if result.is_array() {
        result.as_array().unwrap().iter().collect()
    } else {
        vec![]
    };

    if items.is_empty() {
        return "No call hierarchy item found at this position".to_string();
    }

    if items.len() == 1 {
        let item = items[0];
        let name = item.get("name").and_then(|n| n.as_str()).unwrap_or("<unknown>");
        let kind = item.get("kind").and_then(|k| k.as_u64()).unwrap_or(0);
        let kind_str = symbol_kind_to_string(kind);
        let uri = item.get("uri").and_then(|u| u.as_str()).unwrap_or("");
        let file_path = format_uri(uri, cwd);
        let line = item.get("range").and_then(|r| r.get("start")).and_then(|s| s.get("line")).and_then(|l| l.as_u64()).unwrap_or(0);
        let detail = item.get("detail").and_then(|d| d.as_str());
        let mut r = format!("Call hierarchy item: {} ({}) - {}:{}", name, kind_str, file_path, line + 1);
        if let Some(det) = detail {
            r.push_str(" [");
            r.push_str(det);
            r.push(']');
        }
        return r;
    }

    let mut lines = vec![format!("Found {} call hierarchy items:", items.len())];
    for item in &items {
        let name = item.get("name").and_then(|n| n.as_str()).unwrap_or("<unknown>");
        let kind = item.get("kind").and_then(|k| k.as_u64()).unwrap_or(0);
        let kind_str = symbol_kind_to_string(kind);
        let uri = item.get("uri").and_then(|u| u.as_str()).unwrap_or("");
        let file_path = format_uri(uri, cwd);
        let line = item.get("range").and_then(|r| r.get("start")).and_then(|s| s.get("line")).and_then(|l| l.as_u64()).unwrap_or(0);
        let detail = item.get("detail").and_then(|d| d.as_str());
        let mut r = format!("  {} ({}) - {}:{}", name, kind_str, file_path, line + 1);
        if let Some(det) = detail {
            r.push_str(" [");
            r.push_str(det);
            r.push(']');
        }
        lines.push(r);
    }
    lines.join("\n")
}

/// Format incomingCalls result
fn format_incoming_calls_result(result: &serde_json::Value, cwd: Option<&str>) -> String {
    let calls: Vec<&serde_json::Value> = if result.is_array() {
        result.as_array().unwrap().iter()
            .filter(|c| c.get("from").is_some())
            .collect()
    } else {
        vec![]
    };

    if calls.is_empty() {
        return "No incoming calls found (nothing calls this function)".to_string();
    }

    let mut by_file: HashMap<String, Vec<&serde_json::Value>> = HashMap::new();
    for call in &calls {
        let uri = call.get("from").and_then(|f| f.get("uri")).and_then(|u| u.as_str()).unwrap_or("");
        let fp = format_uri(uri, cwd);
        by_file.entry(fp).or_default().push(call);
    }

    let mut lines = vec![format!("Found {}:", plural(calls.len(), "incoming call"))];
    for (fp, call_group) in &by_file {
        lines.push(format!("\n{fp}:"));
        for call in call_group {
            let from = call.get("from").unwrap();
            let name = from.get("name").and_then(|n| n.as_str()).unwrap_or("<unknown>");
            let kind = from.get("kind").and_then(|k| k.as_u64()).unwrap_or(0);
            let kind_str = symbol_kind_to_string(kind);
            let line = from.get("range").and_then(|r| r.get("start")).and_then(|s| s.get("line")).and_then(|l| l.as_u64()).unwrap_or(0);
            let mut call_line = format!("  {} ({}) - Line {}", name, kind_str, line + 1);
            if let Some(ranges) = from.get("fromRanges").and_then(|r| r.as_array()) {
                let sites: Vec<_> = ranges.iter().map(|r| {
                    let l = r.get("start").and_then(|s| s.get("line")).and_then(|l| l.as_u64()).unwrap_or(0);
                    let c = r.get("start").and_then(|s| s.get("character")).and_then(|c| c.as_u64()).unwrap_or(0);
                    format!("{}:{}", l + 1, c + 1)
                }).collect();
                call_line.push_str(" [calls at: ");
                call_line.push_str(&sites.join(", "));
                call_line.push(']');
            }
            lines.push(call_line);
        }
    }
    lines.join("\n")
}

/// Format outgoingCalls result
fn format_outgoing_calls_result(result: &serde_json::Value, cwd: Option<&str>) -> String {
    let calls: Vec<&serde_json::Value> = if result.is_array() {
        result.as_array().unwrap().iter()
            .filter(|c| c.get("to").is_some())
            .collect()
    } else {
        vec![]
    };

    if calls.is_empty() {
        return "No outgoing calls found (this function calls nothing)".to_string();
    }

    let mut by_file: HashMap<String, Vec<&serde_json::Value>> = HashMap::new();
    for call in &calls {
        let uri = call.get("to").and_then(|t| t.get("uri")).and_then(|u| u.as_str()).unwrap_or("");
        let fp = format_uri(uri, cwd);
        by_file.entry(fp).or_default().push(call);
    }

    let mut lines = vec![format!("Found {}:", plural(calls.len(), "outgoing call"))];
    for (fp, call_group) in &by_file {
        lines.push(format!("\n{fp}:"));
        for call in call_group {
            let to = call.get("to").unwrap();
            let name = to.get("name").and_then(|n| n.as_str()).unwrap_or("<unknown>");
            let kind = to.get("kind").and_then(|k| k.as_u64()).unwrap_or(0);
            let kind_str = symbol_kind_to_string(kind);
            let line = to.get("range").and_then(|r| r.get("start")).and_then(|s| s.get("line")).and_then(|l| l.as_u64()).unwrap_or(0);
            let mut call_line = format!("  {} ({}) - Line {}", name, kind_str, line + 1);
            if let Some(ranges) = to.get("fromRanges").and_then(|r| r.as_array()) {
                let sites: Vec<_> = ranges.iter().map(|r| {
                    let l = r.get("start").and_then(|s| s.get("line")).and_then(|l| l.as_u64()).unwrap_or(0);
                    let c = r.get("start").and_then(|s| s.get("character")).and_then(|c| c.as_u64()).unwrap_or(0);
                    format!("{}:{}", l + 1, c + 1)
                }).collect();
                call_line.push_str(" [called from: ");
                call_line.push_str(&sites.join(", "));
                call_line.push(']');
            }
            lines.push(call_line);
        }
    }
    lines.join("\n")
}

/// LSP tool - code intelligence via Language Server Protocol
pub struct LSPTool;

impl LSPTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        LSP_TOOL_NAME
    }

    pub fn description(&self) -> &str {
        DESCRIPTION
    }

    pub fn user_facing_name(&self, _input: Option<&serde_json::Value>) -> String {
        "LSP".to_string()
    }

    pub fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        input.and_then(|inp| inp["operation"].as_str().map(String::from))
    }

    pub fn render_tool_result_message(
        &self,
        content: &serde_json::Value,
    ) -> Option<String> {
        let text = content["content"].as_str()?;
        let lines = text.lines().count();
        Some(format!("{} lines", lines))
    }

    pub fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema {
            schema_type: "object".to_string(),
            properties: serde_json::json!({
                "operation": {
                    "type": "string",
                    "enum": [
                        "goToDefinition", "findReferences", "hover", "documentSymbol",
                        "workspaceSymbol", "goToImplementation", "prepareCallHierarchy",
                        "incomingCalls", "outgoingCalls"
                    ],
                    "description": "The LSP operation to perform"
                },
                "filePath": {
                    "type": "string",
                    "description": "The absolute or relative path to the file"
                },
                "line": {
                    "type": "number",
                    "description": "The line number (1-based, as shown in editors)"
                },
                "character": {
                    "type": "number",
                    "description": "The character offset (1-based, as shown in editors)"
                }
            }),
            required: Some(vec![
                "operation".to_string(),
                "filePath".to_string(),
                "line".to_string(),
                "character".to_string(),
            ]),
        }
    }

    /// Build LSP method and params from tool input
    fn build_method_and_params(
        &self,
        operation: &str,
        absolute_path: &str,
        line: u64,
        character: u64,
    ) -> (String, serde_json::Value) {
        let uri = url::Url::from_file_path(absolute_path)
            .map(|u| u.to_string())
            .unwrap_or_else(|_| format!("file://{}", absolute_path));

        // Convert from 1-based (user-friendly) to 0-based (LSP protocol)
        let position = serde_json::json!({
            "line": line - 1,
            "character": character - 1,
        });

        match operation {
            "goToDefinition" => (
                "textDocument/definition".to_string(),
                serde_json::json!({
                    "textDocument": { "uri": uri },
                    "position": position,
                }),
            ),
            "findReferences" => (
                "textDocument/references".to_string(),
                serde_json::json!({
                    "textDocument": { "uri": uri },
                    "position": position,
                    "context": { "includeDeclaration": true },
                }),
            ),
            "hover" => (
                "textDocument/hover".to_string(),
                serde_json::json!({
                    "textDocument": { "uri": uri },
                    "position": position,
                }),
            ),
            "documentSymbol" => (
                "textDocument/documentSymbol".to_string(),
                serde_json::json!({
                    "textDocument": { "uri": uri },
                }),
            ),
            "workspaceSymbol" => (
                "workspace/symbol".to_string(),
                serde_json::json!({
                    "query": "",
                }),
            ),
            "goToImplementation" => (
                "textDocument/implementation".to_string(),
                serde_json::json!({
                    "textDocument": { "uri": uri },
                    "position": position,
                }),
            ),
            "prepareCallHierarchy" | "incomingCalls" | "outgoingCalls" => (
                "textDocument/prepareCallHierarchy".to_string(),
                serde_json::json!({
                    "textDocument": { "uri": uri },
                    "position": position,
                }),
            ),
            _ => (operation.to_string(), serde_json::json!({})),
        }
    }

    /// Filter git-ignored URIs from an array of location-based results
    async fn filter_git_ignored_locations(
        &self,
        result: &serde_json::Value,
        cwd: &str,
    ) -> serde_json::Value {
        if !result.is_array() {
            return result.clone();
        }

        let arr = result.as_array().unwrap();
        // Extract unique URIs
        let mut unique_uris: Vec<String> = Vec::new();
        let mut uri_set = HashSet::new();
        for item in arr {
            let loc = to_location(item);
            if let Some(uri) = loc.get("uri").and_then(|u| u.as_str()) {
                if uri_set.insert(uri.to_string()) {
                    unique_uris.push(uri.to_string());
                }
            }
        }

        if unique_uris.is_empty() {
            return result.clone();
        }

        // Convert URIs to file paths
        let paths: Vec<String> = unique_uris.iter().map(|u| uri_to_file_path(u)).collect();
        let ignored = batch_git_ignored(&paths, cwd).await;
        let ignored_uris: HashSet<String> = paths.iter()
            .zip(unique_uris.iter())
            .filter(|(path, _)| ignored.contains(path.as_str()))
            .map(|(_, uri)| uri.clone())
            .collect();

        // Filter
        let filtered: Vec<_> = arr.iter().map(|item| {
            let loc = to_location(item);
            let uri = loc.get("uri").and_then(|u| u.as_str()).unwrap_or("");
            if ignored_uris.contains(uri) {
                serde_json::Value::Null
            } else {
                item.clone()
            }
        }).collect();

        let filtered: Vec<_> = filtered.into_iter().filter(|v| !v.is_null()).collect();
        serde_json::json!(filtered)
    }

    pub async fn execute(
        &self,
        input: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let operation = input["operation"]
            .as_str()
            .ok_or_else(|| AgentError::Tool("Missing operation parameter".to_string()))?;

        let file_path = input["filePath"]
            .as_str()
            .ok_or_else(|| AgentError::Tool("Missing filePath parameter".to_string()))?;

        let line = input["line"].as_u64().unwrap_or(1);
        let character = input["character"].as_u64().unwrap_or(1);

        let cwd = context.cwd.clone();

        // Resolve the file path
        let cwd_path = PathBuf::from(&cwd);
        let absolute_path = if PathBuf::from(file_path).is_absolute() {
            PathBuf::from(file_path)
        } else {
            cwd_path.join(file_path)
        };

        // Check if file exists
        if !absolute_path.exists() {
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: format!("File not found: {}", absolute_path.display()),
                is_error: None,
                was_persisted: None,
            });
        }

        // Check file size (10MB limit matching TS)
        if let Ok(metadata) = fs::metadata(&absolute_path).await {
            if metadata.len() > MAX_LSP_FILE_SIZE_BYTES {
                return Ok(ToolResult {
                    result_type: "text".to_string(),
                    tool_use_id: "".to_string(),
                    content: format!(
                        "File too large for LSP analysis ({} bytes exceeds 10MB limit)",
                        metadata.len()
                    ),
                    is_error: None,
                    was_persisted: None,
                });
            }
        }

        // Check if file is git-ignored
        if is_git_ignored(&absolute_path).await {
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: format!(
                    "File is git-ignored. LSP operations are not available for ignored files: {}",
                    absolute_path.display()
                ),
                is_error: None,
                was_persisted: None,
            });
        }

        // Wait for LSP initialization if still pending
        match get_initialization_status() {
            InitializationStatus::Pending => {
                let _ = wait_for_initialization().await;
            }
            _ => {}
        }

        // Get the LSP server manager
        let manager = match get_lsp_server_manager() {
            Some(m) => m,
            None => {
                return Ok(ToolResult {
                    result_type: "text".to_string(),
                    tool_use_id: "".to_string(),
                    content: "LSP server manager not initialized. This may indicate a startup issue.".to_string(),
                    is_error: None,
                    was_persisted: None,
                });
            }
        };

        let abs_path_str = absolute_path.to_string_lossy().to_string();

        // Ensure file is open in LSP server
        if !manager.is_file_open(&abs_path_str) {
            if let Ok(file_content) = fs::read_to_string(&absolute_path).await {
                if let Err(e) = manager.open_file(&abs_path_str, file_content).await {
                    return Ok(ToolResult {
                        result_type: "text".to_string(),
                        tool_use_id: "".to_string(),
                        content: format!("Failed to open file in LSP server: {}", e),
                        is_error: Some(true),
                        was_persisted: None,
                    });
                }
            }
        }

        // Build LSP request
        let (method, params) = self.build_method_and_params(operation, &abs_path_str, line, character);

        // Send request to LSP server
        let result = match manager.send_request(&abs_path_str, method, params).await {
            Ok(r) => r,
            Err(e) => {
                return Ok(ToolResult {
                    result_type: "text".to_string(),
                    tool_use_id: "".to_string(),
                    content: format!("Error performing {}: {}", operation, e),
                    is_error: Some(true),
                    was_persisted: None,
                });
            }
        };

        // Check for null/undefined result
        if result.is_null() {
            let ext = absolute_path.extension().and_then(|e| e.to_str()).unwrap_or("");
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: format!("No LSP server available for file type: .{ext}",),
                is_error: None,
                was_persisted: None,
            });
        }

        // Handle two-step call hierarchy operations
        let (final_result, is_call_hierarchy_sub) = if operation == "incomingCalls" || operation == "outgoingCalls" {
            let call_items = if result.is_array() && !result.as_array().unwrap().is_empty() {
                result
            } else {
                return Ok(ToolResult {
                    result_type: "text".to_string(),
                    tool_use_id: "".to_string(),
                    content: "No call hierarchy item found at this position".to_string(),
                    is_error: None,
                    was_persisted: None,
                });
            };

            let call_method = if operation == "incomingCalls" {
                "callHierarchy/incomingCalls"
            } else {
                "callHierarchy/outgoingCalls"
            };

            match manager.send_request(
                &abs_path_str,
                call_method.to_string(),
                serde_json::json!({ "item": call_items[0] }),
            ).await {
                Ok(r) => (r, true),
                Err(_) => (call_items, false),
            }
        } else {
            (result, false)
        };

        // Filter git-ignored files from location-based results
        let filtered_result = if matches!(operation, "findReferences" | "goToDefinition" | "goToImplementation" | "workspaceSymbol") {
            self.filter_git_ignored_locations(&final_result, &cwd).await
        } else {
            final_result
        };

        // Format result based on operation type
        let formatted = match operation {
            "goToDefinition" | "goToImplementation" => format_definition_result(&filtered_result, Some(&cwd)),
            "findReferences" => format_references_result(&filtered_result, Some(&cwd)),
            "hover" => format_hover_result(&filtered_result, Some(&cwd)),
            "documentSymbol" => format_document_symbol_result(&filtered_result, Some(&cwd)),
            "workspaceSymbol" => format_workspace_symbol_result(&filtered_result, Some(&cwd)),
            "prepareCallHierarchy" => format_call_hierarchy_result(&filtered_result, Some(&cwd)),
            "incomingCalls" => format_incoming_calls_result(&filtered_result, Some(&cwd)),
            "outgoingCalls" => format_outgoing_calls_result(&filtered_result, Some(&cwd)),
            _ => format!("Unknown operation: {operation}"),
        };

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: formatted,
            is_error: None,
            was_persisted: None,
        })
    }
}

impl Default for LSPTool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lsp_tool_name() {
        let tool = LSPTool::new();
        assert_eq!(tool.name(), LSP_TOOL_NAME);
    }

    #[test]
    fn test_lsp_tool_schema() {
        let tool = LSPTool::new();
        let schema = tool.input_schema();
        assert_eq!(schema.schema_type, "object");
        assert!(schema.required.is_some());
        assert!(
            schema
                .required
                .as_ref()
                .unwrap()
                .contains(&"operation".to_string())
        );
        assert!(
            schema
                .required
                .as_ref()
                .unwrap()
                .contains(&"filePath".to_string())
        );
    }

    #[tokio::test]
    async fn test_lsp_tool_missing_file() {
        let tool = LSPTool::new();
        let input = serde_json::json!({
            "operation": "goToDefinition",
            "filePath": "/nonexistent/file.rs",
            "line": 1,
            "character": 1
        });
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());
        assert!(result.unwrap().content.contains("File not found"));
    }

    #[tokio::test]
    async fn test_lsp_tool_git_ignored() {
        let temp_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test_lsp_gitignore2");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).ok();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&temp_dir)
            .status()
            .ok();

        let ignored_file = temp_dir.join("ignored.rs");
        std::fs::write(&ignored_file, "fn main() {}").ok();
        std::fs::write(temp_dir.join(".gitignore"), "ignored.rs").ok();

        let tool = LSPTool::new();
        let input = serde_json::json!({
            "operation": "hover",
            "filePath": ignored_file.to_str().unwrap(),
            "line": 1,
            "character": 1
        });
        let context = ToolContext {
            cwd: temp_dir.to_string_lossy().to_string(),
            abort_signal: Default::default(),
        };
        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());
        let content = result.unwrap().content;
        let content_lower = content.to_lowercase();
        assert!(
            content_lower.contains("git") && content_lower.contains("ignore"),
            "Content: {}",
            content
        );

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_symbol_kind_to_string() {
        assert_eq!(symbol_kind_to_string(5), "Class");
        assert_eq!(symbol_kind_to_string(12), "Function");
        assert_eq!(symbol_kind_to_string(999), "Unknown");
    }

    #[test]
    fn test_format_uri_relative() {
        let result = format_uri("file:///home/user/project/src/main.rs", Some("/home/user/project"));
        assert!(result.contains("src/main.rs"));
    }

    #[test]
    fn test_format_uri_absolute() {
        let result = format_uri("file:///tmp/other/file.rs", Some("/home/user/project"));
        assert!(result.contains("/tmp/other/file.rs"));
    }

    #[test]
    fn test_hover_format_no_result() {
        let result = format_hover_result(&serde_json::json!(null), None);
        assert!(result.contains("No hover information available"));
    }

    #[test]
    fn test_definition_format_no_result() {
        let result = format_definition_result(&serde_json::json!(null), None);
        assert!(result.contains("No definition found"));
    }

    #[test]
    fn test_references_format_no_result() {
        let result = format_references_result(&serde_json::json!([]), None);
        assert!(result.contains("No references found"));
    }

    #[test]
    fn test_workspace_symbol_format_no_result() {
        let result = format_workspace_symbol_result(&serde_json::json!([]), None);
        assert!(result.contains("No symbols found"));
    }

    #[test]
    fn test_call_hierarchy_format_no_result() {
        let result = format_call_hierarchy_result(&serde_json::json!([]), None);
        assert!(result.contains("No call hierarchy item found"));
    }
}
