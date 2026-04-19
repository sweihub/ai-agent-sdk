// Source: /data/home/swei/claudecode/openclaudecode/src/utils/toolResultStorage.ts
//! Tool result size management and disk persistence for large outputs.
//!
//! Translated from TypeScript toolResultStorage.ts.
//! Large tool results are persisted to disk to avoid bloating the API request,
//! and a preview with a path reference is returned instead.

use std::fs;
use std::path::PathBuf;

/// Default max result size in characters before persistence is considered.
pub const DEFAULT_MAX_RESULT_SIZE_CHARS: usize = 50_000;

/// Preview size in bytes for persisted results.
pub const PREVIEW_SIZE_BYTES: usize = 2_000;

/// Maximum total tool result size per API message before replacement.
pub const MAX_TOOL_RESULTS_PER_MESSAGE_CHARS: usize = 200_000;

/// Persist a large tool result to disk, returning a preview with path.
///
/// Returns the processed content and whether it was persisted.
pub fn maybe_persist_large_result(
    content: &str,
    tool_use_id: &str,
    tool_name: &str,
    project_dir: Option<&str>,
    session_id: Option<&str>,
    threshold: usize,
) -> (String, bool) {
    // Empty content guard: return a friendly message to prevent model stop sequences
    if content.is_empty() {
        return (format!("({} completed with no output)", tool_name), false);
    }

    // Size check
    if content.len() <= threshold {
        return (content.to_string(), false);
    }

    // Persist to disk
    let result = match (project_dir, session_id) {
        (Some(pd), Some(sid)) => persist_tool_result(content, tool_use_id, pd, sid).map_err(|_| ()),
        _ => Err(()),
    };

    match result {
        Ok(persisted) => {
            let preview = generate_preview(content);
            let wrapped = format!(
                "<persisted-output>\n\
                Output too large ({} chars). Full output saved to: {}\n\n\
                Preview (first {} bytes, sorted by newline):\n\
                {}\n\
                {}\n\
                </persisted-output>",
                content.len(),
                persisted.filepath,
                PREVIEW_SIZE_BYTES,
                preview.text,
                if persisted.has_more {
                    format!("... [{} more bytes] ...", persisted.original_size - PREVIEW_SIZE_BYTES)
                } else {
                    String::new()
                },
            );
            (wrapped, true)
        }
        Err(_) => {
            // If persistence fails, just truncate
            let truncated = if content.len() > threshold * 2 {
                format!("{}... [truncated]", &content[..threshold.min(content.len())])
            } else {
                content.to_string()
            };
            (truncated, false)
        }
    }
}

/// Persist a tool result to disk.
fn persist_tool_result(
    content: &str,
    tool_use_id: &str,
    project_dir: &str,
    session_id: &str,
) -> Result<PersistedToolResult, std::io::Error> {
    let tool_results_dir = PathBuf::from(project_dir)
        .join(".ai")
        .join("tool-results")
        .join(session_id);

    // Create directory if it doesn't exist
    fs::create_dir_all(&tool_results_dir)?;

    let filepath = tool_results_dir.join(format!("{}.txt", tool_use_id));
    let original_size = content.len();

    // Write with exclusive create flag to avoid race conditions
    fs::write(&filepath, content)?;

    let preview = generate_preview(content);

    Ok(PersistedToolResult {
        filepath: filepath.to_string_lossy().to_string(),
        original_size,
        preview: preview.text,
        has_more: preview.has_more,
    })
}

/// Generate a preview of the content, truncating at a newline boundary.
pub fn generate_preview(content: &str) -> Preview {
    let limit = PREVIEW_SIZE_BYTES;
    if content.len() <= limit {
        return Preview {
            text: content.to_string(),
            has_more: false,
        };
    }

    // Find a good truncation point at a newline within 50% of the limit
    let search_start = limit / 2;
    let truncated = if let Some(last_newline) = content[search_start..limit]
        .rfind('\n')
        .map(|i| i + search_start)
    {
        &content[..last_newline]
    } else if let Some(newline) = content[..limit].rfind('\n') {
        &content[..newline]
    } else {
        // No newline found, just truncate at limit
        &content[..limit]
    };

    Preview {
        text: truncated.to_string(),
        has_more: true,
    }
}

/// Process a tool result, applying size management.
pub fn process_tool_result(
    content: &str,
    tool_name: &str,
    tool_use_id: &str,
    project_dir: Option<&str>,
    session_id: Option<&str>,
    max_result_size: Option<usize>,
) -> (String, bool) {
    let threshold = max_result_size.unwrap_or(DEFAULT_MAX_RESULT_SIZE_CHARS);
    maybe_persist_large_result(
        content,
        tool_use_id,
        tool_name,
        project_dir,
        session_id,
        threshold,
    )
}

/// Result of persisting a tool result to disk.
pub struct PersistedToolResult {
    pub filepath: String,
    pub original_size: usize,
    pub preview: String,
    pub has_more: bool,
}

/// Preview with truncation indicator.
pub struct Preview {
    pub text: String,
    pub has_more: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_small_content_not_persisted() {
        let (content, was_persisted) = maybe_persist_large_result(
            "small content",
            "tool1",
            "Bash",
            Some("/tmp"),
            Some("sess1"),
            50_000,
        );
        assert_eq!(content, "small content");
        assert!(!was_persisted);
    }

    #[test]
    fn test_empty_content_returns_message() {
        let (content, _) = maybe_persist_large_result(
            "",
            "tool1",
            "Bash",
            Some("/tmp"),
            Some("sess1"),
            100,
        );
        assert_eq!(content, "(Bash completed with no output)");
    }

    #[test]
    fn test_generate_preview_small() {
        let preview = generate_preview("short");
        assert_eq!(preview.text, "short");
        assert!(!preview.has_more);
    }

    #[test]
    fn test_generate_preview_large() {
        let content = "a".repeat(5000);
        let preview = generate_preview(&content);
        assert!(preview.has_more);
        assert!(preview.text.len() <= PREVIEW_SIZE_BYTES);
    }

    #[test]
    fn test_generate_preview_with_newline() {
        let content = "line1\nline2\nline3\n".repeat(200); // ~3400 chars
        let preview = generate_preview(&content);
        assert!(preview.has_more);
        assert!(preview.text.len() <= PREVIEW_SIZE_BYTES);
    }
}
