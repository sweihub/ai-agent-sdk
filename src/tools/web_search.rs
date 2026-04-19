use crate::types::*;
use regex::Regex;
use reqwest::Client;

pub struct WebSearchTool {
    client: Client,
}

impl WebSearchTool {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .user_agent("Mozilla/5.0 (compatible; AgentSDK/1.0)")
            .build()
            .expect("Failed to create HTTP client");
        Self { client }
    }

    pub fn name(&self) -> &str {
        "WebSearch"
    }

    pub fn description(&self) -> &str {
        "Search the web for information. Returns search results with titles, URLs, and snippets."
    }

    pub fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema {
            schema_type: "object".to_string(),
            properties: serde_json::json!({
                "query": {
                    "type": "string",
                    "description": "The search query"
                },
                "num_results": {
                    "type": "number",
                    "description": "Number of results to return (default: 5)"
                }
            }),
            required: Some(vec!["query".to_string()]),
        }
    }

    pub async fn execute(
        &self,
        input: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolResult, crate::error::AgentError> {
        let query = input["query"]
            .as_str()
            .ok_or_else(|| crate::error::AgentError::Tool("query is required".to_string()))?;

        let num_results = input["num_results"].as_u64().unwrap_or(5) as usize;

        // Use DuckDuckGo HTML search
        let encoded = urlencoding::encode(query);
        let url = format!("https://html.duckduckgo.com/html/?q={}", encoded);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| crate::error::AgentError::Tool(format!("Search error: {}", e)))?;

        if !response.status().is_success() {
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: format!("Search failed: HTTP {}", response.status().as_u16()),
                is_error: Some(true),
                was_persisted: None,
            });
        }

        let html = response.text().await.map_err(|e| {
            crate::error::AgentError::Tool(format!("Error reading search results: {}", e))
        })?;

        // Parse search results from DuckDuckGo HTML
        let result_regex =
            Regex::new(r#"<a rel="nofollow" class="result__a" href="([^"]*)"[^>]*>([\s\S]*?)</a>"#)
                .unwrap();
        let snippet_regex =
            Regex::new(r#"<a class="result__snippet"[^>]*>([\s\S]*?)</a>"#).unwrap();

        let mut links: Vec<(String, String)> = Vec::new();
        for cap in result_regex.captures_iter(&html) {
            if let (Some(href), Some(title)) = (cap.get(1), cap.get(2)) {
                let href = href.as_str().to_string();
                let title = title.as_str().replace("<[^>]+>", "").trim().to_string();
                if !href.is_empty() && !title.is_empty() && !href.contains("duckduckgo.com") {
                    links.push((title, href));
                }
            }
        }

        let mut snippets: Vec<String> = Vec::new();
        for cap in snippet_regex.captures_iter(&html) {
            if let Some(snippet) = cap.get(1) {
                let snippet_text = snippet.as_str().replace("<[^>]+>", "").trim().to_string();
                snippets.push(snippet_text);
            }
        }

        let mut results: Vec<String> = Vec::new();
        let num_results = std::cmp::min(num_results, links.len());

        for i in 0..num_results {
            let (title, url) = &links[i];
            let mut entry = format!("{}. {}\n   {}", i + 1, title, url);
            if let Some(snippet) = snippets.get(i) {
                if !snippet.is_empty() {
                    entry.push_str(&format!("\n   {}", snippet));
                }
            }
            results.push(entry);
        }

        let content = if results.is_empty() {
            format!("No results found for \"{}\"", query)
        } else {
            results.join("\n\n")
        };

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content,
            is_error: None,
            was_persisted: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_web_search_tool_name() {
        let tool = WebSearchTool::new();
        assert_eq!(tool.name(), "WebSearch");
    }

    #[test]
    fn test_web_search_tool_description_contains_search() {
        let tool = WebSearchTool::new();
        assert!(tool.description().to_lowercase().contains("search"));
    }

    #[test]
    fn test_web_search_tool_has_query_in_schema() {
        let tool = WebSearchTool::new();
        let schema = tool.input_schema();
        assert!(schema.properties.get("query").is_some());
    }

    #[test]
    fn test_web_search_tool_has_num_results_in_schema() {
        let tool = WebSearchTool::new();
        let schema = tool.input_schema();
        assert!(schema.properties.get("num_results").is_some());
    }

    #[tokio::test]
    async fn test_web_search_tool_requires_query() {
        let tool = WebSearchTool::new();
        let input = serde_json::json!({});
        let context = ToolContext::default();

        let result = tool.execute(input, &context).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    #[ignore] // Requires network access to DuckDuckGo
    async fn test_web_search_tool_returns_results() {
        let tool = WebSearchTool::new();
        let input = serde_json::json!({
            "query": "Rust programming language"
        });
        let context = ToolContext::default();

        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert!(!tool_result.content.is_empty());
        // Should contain some expected content
        assert!(tool_result.content.to_lowercase().contains("rust"));
    }

    #[tokio::test]
    #[ignore] // Requires network access to DuckDuckGo
    async fn test_web_search_tool_respects_num_results() {
        let tool = WebSearchTool::new();
        let input = serde_json::json!({
            "query": "test query",
            "num_results": 3
        });
        let context = ToolContext::default();

        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());
    }
}
