// Source: /data/home/swei/claudecode/openclaudecode/src/utils/sideQuery.ts
//! Side query - lightweight non-streaming API call wrapper.

use crate::AgentError;

/// Configuration for a side query request.
pub struct SideQueryOptions {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub system_prompt: String,
    pub message: String,
    pub max_tokens: Option<u32>,
    pub tools: Option<Vec<serde_json::Value>>,
}

impl SideQueryOptions {
    pub fn new(base_url: String, api_key: String, model: String) -> Self {
        Self {
            base_url,
            api_key,
            model,
            system_prompt: String::new(),
            message: String::new(),
            max_tokens: Some(4096),
            tools: None,
        }
    }

    pub fn system_prompt(mut self, prompt: String) -> Self {
        self.system_prompt = prompt;
        self
    }

    pub fn message(mut self, message: String) -> Self {
        self.message = message;
        self
    }

    pub fn max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    pub fn tools(mut self, tools: Vec<serde_json::Value>) -> Self {
        self.tools = Some(tools);
        self
    }
}

/// Parsed memory selection result from side query response.
#[derive(Debug, Clone)]
pub struct SideQueryMemorySelection {
    pub filenames: Vec<String>,
    pub reasoning: String,
}

impl SideQueryMemorySelection {
    pub fn from_response(response: &str) -> Self {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(response) {
            let filenames = val
                .get("filenames")
                .and_then(|f| f.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            let reasoning = val
                .get("reasoning")
                .and_then(|r| r.as_str())
                .map(|s| s.to_string())
                .unwrap_or_default();
            return Self { filenames, reasoning };
        }
        let filenames = extract_filenames_from_text(response);
        Self {
            reasoning: response.to_string(),
            filenames,
        }
    }
}

/// Extract filenames from arbitrary text response.
fn extract_filenames_from_text(text: &str) -> Vec<String> {
    let mut filenames = Vec::new();
    for line in text.lines() {
        let clean = line.trim()
            .trim_start_matches('-')
            .trim_start_matches('*')
            .trim_start_matches('`')
            .trim_end_matches('`')
            .trim()
            .to_string();
        if clean.is_empty() || filenames.contains(&clean) {
            continue;
        }
        if clean.ends_with(".md")
            || clean.ends_with(".txt")
            || clean.ends_with(".json")
            || clean.ends_with(".rs")
        {
            filenames.push(clean);
        }
    }
    filenames
}

/// Execute a side query (non-streaming API call) with Anthropic-compatible format.
pub async fn side_query(opts: &SideQueryOptions) -> Result<String, AgentError> {
    let client = reqwest::Client::new();
    let mut body = serde_json::json!({
        "model": opts.model,
        "max_tokens": opts.max_tokens.unwrap_or(4096),
        "messages": [{ "role": "user", "content": opts.message }]
    });
    if !opts.system_prompt.is_empty() {
        body.as_object_mut()
            .unwrap()
            .insert("system".to_string(), serde_json::json!(opts.system_prompt));
    }
    let url = format!("{}/v1/messages", opts.base_url.trim_end_matches('/'));
    let resp = client
        .post(&url)
        .header("x-api-key", &opts.api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| AgentError::Api(e.to_string()))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_else(|_| "No error body".to_string());
        return Err(AgentError::Api(format!(
            "Side query failed with status {}: {}",
            status, body_text
        )));
    }
    let json: serde_json::Value =
        resp.json().await.map_err(|e| AgentError::Api(e.to_string()))?;
    let content = json
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|c| c.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();
    Ok(content)
}

/// Execute a side query with OpenAI-compatible format.
pub async fn side_query_simple(opts: &SideQueryOptions) -> Result<String, AgentError> {
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "model": opts.model,
        "max_tokens": opts.max_tokens.unwrap_or(4096),
        "messages": [
            { "role": "system", "content": opts.system_prompt },
            { "role": "user", "content": opts.message }
        ]
    });
    let url = format!("{}/v1/chat/completions", opts.base_url.trim_end_matches('/'));
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", opts.api_key))
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| AgentError::Api(e.to_string()))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_else(|_| "No error body".to_string());
        return Err(AgentError::Api(format!(
            "Side query failed with status {}: {}",
            status, body_text
        )));
    }
    let json: serde_json::Value =
        resp.json().await.map_err(|e| AgentError::Api(e.to_string()))?;
    let content = json
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();
    Ok(content)
}

/// Execute a side query with tool support.
pub async fn side_query_with_tools(
    opts: &SideQueryOptions,
) -> Result<serde_json::Value, AgentError> {
    let client = reqwest::Client::new();
    let mut body = serde_json::json!({
        "model": opts.model,
        "max_tokens": opts.max_tokens.unwrap_or(4096),
        "messages": [{ "role": "user", "content": opts.message }]
    });
    if !opts.system_prompt.is_empty() {
        body.as_object_mut()
            .unwrap()
            .insert("system".to_string(), serde_json::json!(opts.system_prompt));
    }
    if let Some(ref tools) = opts.tools {
        body.as_object_mut()
            .unwrap()
            .insert("tools".to_string(), serde_json::json!(tools));
    }
    let url = format!("{}/v1/messages", opts.base_url.trim_end_matches('/'));
    let resp = client
        .post(&url)
        .header("x-api-key", &opts.api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| AgentError::Api(e.to_string()))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_else(|_| "No error body".to_string());
        return Err(AgentError::Api(format!(
            "Side query with tools failed with status {}: {}",
            status, body_text
        )));
    }
    let json: serde_json::Value =
        resp.json().await.map_err(|e| AgentError::Api(e.to_string()))?;
    Ok(json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_side_query_options_builder() {
        let opts = SideQueryOptions::new(
            "https://api.anthropic.com".to_string(),
            "test-key".to_string(),
            "claude-sonnet-4-6".to_string(),
        )
        .system_prompt("You are helpful.".to_string())
        .message("Hello".to_string())
        .max_tokens(2048);
        assert_eq!(opts.base_url, "https://api.anthropic.com");
        assert_eq!(opts.model, "claude-sonnet-4-6");
        assert_eq!(opts.system_prompt, "You are helpful.");
        assert_eq!(opts.message, "Hello");
        assert_eq!(opts.max_tokens, Some(2048));
    }

    #[test]
    fn test_memory_selection_from_json() {
        let json_response = r#"{"filenames": ["notes.md", "ideas.txt"], "reasoning": "These files are relevant"}"#;
        let selection = SideQueryMemorySelection::from_response(json_response);
        assert_eq!(selection.filenames, vec!["notes.md", "ideas.txt"]);
        assert_eq!(selection.reasoning, "These files are relevant");
    }

    #[test]
    fn test_memory_selection_from_text() {
        let text_response = "Based on the query, these files seem relevant:\n- notes.md\n- ideas.txt\n- project.rs\n";
        let selection = SideQueryMemorySelection::from_response(text_response);
        assert!(selection.filenames.contains(&"notes.md".to_string()));
        assert!(selection.filenames.contains(&"ideas.txt".to_string()));
        assert!(selection.filenames.contains(&"project.rs".to_string()));
    }

    #[test]
    fn test_extract_filenames_from_text() {
        let text = "Here are some files:\n- memory.md\n* scratch.txt\nconfig.json\nnot a file\nregular text\n";
        let filenames = extract_filenames_from_text(text);
        assert_eq!(filenames.len(), 3);
        assert!(filenames.contains(&"memory.md".to_string()));
        assert!(filenames.contains(&"scratch.txt".to_string()));
        assert!(filenames.contains(&"config.json".to_string()));
    }
}
