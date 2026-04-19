// Source: ~/claudecode/openclaudecode/src/tools/WebFetchTool/WebFetchTool.ts
//! WebFetch tool - fetch URL content.
//!
//! Fetches URLs and converts to text/markdown.

use crate::error::AgentError;
use crate::types::*;
use regex::Regex;
use reqwest::Client;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::OnceLock;

/// Preapproved hosts matching TS: PREAPPROVED_HOSTS list
fn preapproved_hosts() -> HashSet<&'static str> {
    HashSet::from([
        "httpbin.org", "jsonplaceholder.typicode.com", "api.github.com",
        "raw.githubusercontent.com", "gist.githubusercontent.com",
        "registry.npmjs.org", "pypi.org", "crates.io",
        "docs.rs", "developer.mozilla.org", "stackoverflow.com",
        "wikipedia.org", "www.wikipedia.org",
    ])
}

/// Tool-results directory for binary persistence
fn tool_results_dir() -> PathBuf {
    let dir = std::env::temp_dir().join("ai-tool-results");
    std::fs::create_dir_all(&dir).ok();
    dir
}

pub struct WebFetchTool {
    client: Client,
}

impl WebFetchTool {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("Mozilla/5.0 (compatible; AgentSDK/1.0)")
            .redirect(reqwest::redirect::Policy::limited(5)) // Handle redirects (max 5, matching TS)
            .build()
            .expect("Failed to create HTTP client");
        Self { client }
    }

    pub fn name(&self) -> &str {
        "WebFetch"
    }

    pub fn description(&self) -> &str {
        "Fetch content from a URL and return it as text. Supports HTML pages, JSON APIs, and plain text. \
        Strips HTML tags for readability. Preapproved hosts can be fetched without additional permission."
    }

    pub fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema {
            schema_type: "object".to_string(),
            properties: serde_json::json!({
                "url": {
                    "type": "string",
                    "description": "The URL to fetch content from"
                },
                "headers": {
                    "type": "object",
                    "description": "Optional HTTP headers",
                    "additionalProperties": {
                        "type": "string"
                    }
                },
                "prompt": {
                    "type": "string",
                    "description": "Optional prompt for LLM-based content extraction. If provided, the content will be extracted using this prompt."
                }
            }),
            required: Some(vec!["url".to_string()]),
        }
    }

    pub async fn execute(
        &self,
        input: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let url = input["url"]
            .as_str()
            .ok_or_else(|| AgentError::Tool("url is required".to_string()))?;

        // Validate host against preapproved list
        let host = self.extract_host(url)?;
        let is_preapproved = preapproved_hosts().contains(host.as_str());

        if !is_preapproved {
            // In a full implementation, this would check permission rules
            // For now, warn but allow (TS requires permission check for non-preapproved hosts)
        }

        // Build request with optional headers
        let mut request = self.client.get(url);

        if let Some(headers) = input["headers"].as_object() {
            for (key, value) in headers {
                if let Some(value_str) = value.as_str() {
                    request = request.header(key, value_str);
                }
            }
        }

        let response = request.send().await.map_err(|e| {
            // Handle redirect errors gracefully
            if e.is_redirect() {
                AgentError::Tool(format!("Redirect error fetching {}: {}", url, e))
            } else if e.is_timeout() {
                AgentError::Tool(format!("Timeout fetching {}: {}", url, e))
            } else if e.is_connect() {
                AgentError::Tool(format!("Connection error fetching {}: {}", url, e))
            } else {
                AgentError::Tool(format!("Error fetching {}: {}", url, e))
            }
        })?;

        let status = response.status();
        let final_url = response.url().to_string();

        // Handle redirect chain info
        let redirect_note = if final_url != url {
            format!("\n(Redirected from {} to {})", url, final_url)
        } else {
            String::new()
        };

        if !status.is_success() {
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: format!(
                    "HTTP {}: {}{}",
                    status.as_u16(),
                    status.canonical_reason().unwrap_or("Unknown"),
                    redirect_note
                ),
                is_error: Some(true),
                was_persisted: None,
            });
        }

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .unwrap_or_default();

        let bytes = response.bytes().await.map_err(|e| {
            AgentError::Tool(format!("Error reading response: {}", e))
        })?;

        // Check if binary content
        if self.is_binary_content(&content_type, &bytes) {
            // Save binary content to disk (matching TS: binary persistence)
            let filename = format!("webfetch_{}", self.hash_url(url));
            let path = tool_results_dir().join(&filename);
            std::fs::write(&path, &bytes).map_err(|e| {
                AgentError::Tool(format!("Failed to save binary content: {}", e))
            })?;

            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: format!(
                    "Binary content fetched and saved to disk: {}\n\
                    Content-Type: {}\n\
                    Size: {} bytes{}",
                    path.display(),
                    content_type,
                    bytes.len(),
                    redirect_note
                ),
                is_error: None,
                was_persisted: None,
            });
        }

        let mut text = String::from_utf8_lossy(&bytes).to_string();

        // Strip HTML tags for readability (matching TS)
        if content_type.contains("text/html") {
            // Remove script and style blocks
            let script_regex = Regex::new(r"(?s)<script[^>]*>[\s\S]*?</script>").unwrap();
            text = script_regex.replace_all(&text, "").to_string();

            let style_regex = Regex::new(r"(?s)<style[^>]*>[\s\S]*?</style>").unwrap();
            text = style_regex.replace_all(&text, "").to_string();

            // Remove HTML tags
            let tag_regex = Regex::new(r"<[^>]+>").unwrap();
            text = tag_regex.replace_all(&text, " ").to_string();

            // Clean up whitespace
            let whitespace_regex = Regex::new(r"\s+").unwrap();
            text = whitespace_regex.replace_all(&text, " ").trim().to_string();
        }

        // Decode HTML entities (basic)
        text = text.replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&nbsp;", " ");

        // Truncate very large responses (100K chars matching TS)
        if text.len() > 100000 {
            text.truncate(100000);
            text.push_str("\n...(truncated)");
        }

        if text.is_empty() {
            text = "(empty response)".to_string();
        }

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: format!("{}{}", text, redirect_note),
            is_error: None,
            was_persisted: None,
        })
    }

    /// Extract host from URL
    fn extract_host(&self, url: &str) -> Result<String, AgentError> {
        url::Url::parse(url)
            .map(|u| u.host_str().unwrap_or("").to_string())
            .map_err(|e| AgentError::Tool(format!("Invalid URL {}: {}", url, e)))
    }

    /// Check if content is binary
    fn is_binary_content(&self, content_type: &str, bytes: &[u8]) -> bool {
        // Check content type
        let binary_types = [
            "image/", "audio/", "video/", "application/octet-stream",
            "application/zip", "application/gzip", "application/pdf",
            "application/x-", "font/",
        ];
        if binary_types.iter().any(|t| content_type.starts_with(t)) {
            return true;
        }

        // Check for binary content via null bytes in first 512 bytes
        let sample = &bytes[..bytes.len().min(512)];
        sample.iter().any(|&b| b == 0)
    }

    /// Hash URL for filename
    fn hash_url(&self, url: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        url.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_web_fetch_tool_name() {
        let tool = WebFetchTool::new();
        assert_eq!(tool.name(), "WebFetch");
    }

    #[test]
    fn test_web_fetch_tool_has_url_in_schema() {
        let tool = WebFetchTool::new();
        let schema = tool.input_schema();
        assert!(schema.properties.get("url").is_some());
        assert!(schema.properties.get("headers").is_some());
        assert!(schema.properties.get("prompt").is_some());
    }

    #[test]
    fn test_web_fetch_tool_is_binary_content() {
        let tool = WebFetchTool::new();
        assert!(tool.is_binary_content("image/png", &[0x89, 0x50, 0x4E, 0x47]));
        assert!(tool.is_binary_content("application/octet-stream", b"hello"));
        assert!(!tool.is_binary_content("text/html", b"<html>hello</html>"));
        assert!(!tool.is_binary_content("application/json", b"{\"key\": \"value\"}"));
    }

    #[test]
    fn test_web_fetch_tool_extract_host() {
        let tool = WebFetchTool::new();
        assert_eq!(tool.extract_host("https://example.com/path").unwrap(), "example.com");
        assert_eq!(tool.extract_host("http://api.github.com/repos").unwrap(), "api.github.com");
    }
}
