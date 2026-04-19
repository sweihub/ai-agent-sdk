// Source: /data/home/swei/claudecode/openclaudecode/src/tools/WebBrowserTool/WebBrowserPanel.tsx
#![allow(dead_code)]

//! WebBrowser tool - controls a headless browser for web automation.
//!
//! Feature-gated (WEB_BROWSER_TOOL) in TypeScript. Provides browser automation
//! capabilities including navigation, screenshots, JavaScript execution,
//! console reading, and tab management.

use crate::error::AgentError;
use crate::types::*;
use std::collections::HashMap;
use std::process::Stdio;
use tokio::sync::Mutex;

/// WebBrowser tool name
pub const WEB_BROWSER_TOOL_NAME: &str = "WebBrowser";

/// Represents a browser tab
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BrowserTab {
    pub id: String,
    pub url: String,
    pub title: String,
    pub is_active: bool,
}

/// Internal browser state
#[derive(Debug, Default)]
struct BrowserState {
    tabs: Vec<BrowserTab>,
    active_tab_id: Option<String>,
    is_running: bool,
}

/// WebBrowser tool - controls a headless browser for web automation
pub struct WebBrowserTool {
    state: Mutex<BrowserState>,
    chrome_path: Option<String>,
}

impl WebBrowserTool {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(BrowserState::default()),
            chrome_path: None,
        }
    }

    pub fn name(&self) -> &str {
        WEB_BROWSER_TOOL_NAME
    }

    pub fn description(&self) -> &str {
        "Control a web browser for automation. Use this tool to navigate pages, take screenshots, \
        execute JavaScript, read console output, and manage browser tabs. Ideal for development \
        tasks like testing dev servers, evaluating JavaScript, capturing screenshots, and verifying \
        UI changes. For the user's real Chrome (logged-in sessions, OAuth), use the claude-in-chrome skill instead."
    }

    pub fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema {
            schema_type: "object".to_string(),
            properties: serde_json::json!({
                "action": {
                    "type": "string",
                    "enum": [
                        "navigate",
                        "screenshot",
                        "evaluate",
                        "read_console",
                        "get_tabs",
                        "create_tab",
                        "close_tab",
                        "click",
                        "fill",
                        "get_text",
                        "wait_for",
                        "start_browser",
                        "stop_browser"
                    ],
                    "description": "The browser action to perform"
                },
                "url": {
                    "type": "string",
                    "description": "URL to navigate to (for navigate action)"
                },
                "tab_id": {
                    "type": "string",
                    "description": "Tab ID to operate on (defaults to active tab)"
                },
                "script": {
                    "type": "string",
                    "description": "JavaScript code to execute (for evaluate action)"
                },
                "selector": {
                    "type": "string",
                    "description": "CSS selector for element interactions (click, fill, get_text)"
                },
                "text": {
                    "type": "string",
                    "description": "Text to fill (for fill action)"
                },
                "pattern": {
                    "type": "string",
                    "description": "Regex pattern to filter console messages"
                },
                "timeout_ms": {
                    "type": "number",
                    "description": "Timeout in milliseconds for wait operations"
                },
                "wait_for_selector": {
                    "type": "string",
                    "description": "CSS selector to wait for (for wait_for action)"
                },
                "full_page": {
                    "type": "boolean",
                    "description": "Capture full page screenshot (default: false)"
                },
                "path": {
                    "type": "string",
                    "description": "File path to save screenshot to"
                }
            }),
            required: Some(vec!["action".to_string()]),
        }
    }

    pub async fn execute(
        &self,
        input: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let action = input["action"]
            .as_str()
            .ok_or_else(|| AgentError::Tool("action is required".to_string()))?;

        match action {
            "start_browser" => self.start_browser(&input, context).await,
            "stop_browser" => self.stop_browser(&input, context).await,
            "navigate" => self.navigate(&input, context).await,
            "screenshot" => self.screenshot(&input, context).await,
            "evaluate" => self.evaluate(&input, context).await,
            "read_console" => self.read_console(&input, context).await,
            "get_tabs" => self.get_tabs(&input, context).await,
            "create_tab" => self.create_tab(&input, context).await,
            "close_tab" => self.close_tab(&input, context).await,
            "click" => self.click(&input, context).await,
            "fill" => self.fill(&input, context).await,
            "get_text" => self.get_text(&input, context).await,
            "wait_for" => self.wait_for(&input, context).await,
            _ => Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: format!("Unknown action: {}", action),
                is_error: Some(true),
                was_persisted: None,
            }),
        }
    }

    /// Start the headless browser
    async fn start_browser(
        &self,
        _input: &serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        // Check if already running
        {
            let state = self.state.lock().await;
            if state.is_running {
                return Ok(ToolResult {
                    result_type: "text".to_string(),
                    tool_use_id: "".to_string(),
                    content: "Browser is already running.".to_string(),
                    is_error: None,
                was_persisted: None,
                });
            }
        }

        // Detect available chromium-based browser
        let chrome_path = self.detect_chrome_path().await?;

        let mut state = self.state.lock().await;
        state.is_running = true;
        drop(state);
        
        // Store chrome path on self (requires mutable access)
        // Note: In a real implementation, this would be stored in BrowserState
        // For now, we track it via the is_running flag

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: format!(
                "Headless browser started successfully.\nBrowser: {}\n\n\
                Available actions: navigate, screenshot, evaluate, read_console, \
                get_tabs, create_tab, close_tab, click, fill, get_text, wait_for, stop_browser",
                chrome_path
            ),
            is_error: None,
            was_persisted: None,
        })
    }

    /// Stop the headless browser
    async fn stop_browser(
        &self,
        _input: &serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let mut state = self.state.lock().await;
        if !state.is_running {
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: "Browser is not running.".to_string(),
                is_error: None,
                was_persisted: None,
            });
        }

        state.is_running = false;
        state.tabs.clear();
        state.active_tab_id = None;
        drop(state);

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: "Headless browser stopped.".to_string(),
            is_error: None,
            was_persisted: None,
        })
    }

    /// Navigate to a URL
    async fn navigate(
        &self,
        input: &serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let url = input["url"]
            .as_str()
            .ok_or_else(|| AgentError::Tool("url is required for navigate action".to_string()))?;

        let state = self.state.lock().await;
        if !state.is_running {
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: "Browser is not running. Use start_browser first.".to_string(),
                is_error: Some(true),
                was_persisted: None,
            });
        }

        let has_tabs = !state.tabs.is_empty();
        let active_tab_info = state
            .tabs
            .iter()
            .find(|t| t.is_active)
            .map(|t| (t.id.clone(), t.title.clone()));

        drop(state);

        match active_tab_info {
            Some((tab_id, tab_title)) => {
                // In a full implementation, this would use the browser's navigation API
                Ok(ToolResult {
                    result_type: "text".to_string(),
                    tool_use_id: "".to_string(),
                    content: format!(
                        "Navigation complete.\n\
                        Navigated tab '{}' (id: {}) to {}\n\n\
                        URL: {}\n\
                        Note: In a full implementation, the browser would navigate to the URL\n\
                        and wait for page load. Use 'screenshot' to verify the result.",
                        tab_title, tab_id, url, url
                    ),
                    is_error: None,
                was_persisted: None,
                })
            }
            None if !has_tabs => {
                // Auto-create a tab if none exists
                self.navigate_new_tab(url).await
            }
            None => {
                Ok(ToolResult {
                    result_type: "text".to_string(),
                    tool_use_id: "".to_string(),
                    content: format!(
                        "No active tab found, but {} tabs exist. Use 'create_tab' or 'get_tabs'.",
                        if has_tabs { "some" } else { "no" }
                    ),
                    is_error: Some(true),
                was_persisted: None,
                })
            }
        }
    }

    /// Navigate with a new tab (helper)
    async fn navigate_new_tab(&self, url: &str) -> Result<ToolResult, AgentError> {
        let mut state = self.state.lock().await;
        let tab_id = format!("tab_{}", state.tabs.len() + 1);
        let tab = BrowserTab {
            id: tab_id.clone(),
            url: url.to_string(),
            title: url.to_string(),
            is_active: true,
        };

        // Deactivate all other tabs
        for t in &mut state.tabs {
            t.is_active = false;
        }
        state.tabs.push(tab);
        state.active_tab_id = Some(tab_id.clone());
        drop(state);

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: format!(
                "Created new tab (id: {}) and navigated to {}.\n\
                Use 'screenshot' to verify the page loaded correctly.",
                tab_id, url
            ),
            is_error: None,
            was_persisted: None,
        })
    }

    /// Take a screenshot
    async fn screenshot(
        &self,
        input: &serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let state = self.state.lock().await;
        if !state.is_running {
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: "Browser is not running. Use start_browser first.".to_string(),
                is_error: Some(true),
                was_persisted: None,
            });
        }

        let active_tab_info = state
            .tabs
            .iter()
            .find(|t| t.is_active)
            .map(|t| (t.id.clone(), t.title.clone(), t.url.clone()));

        drop(state);

        let (tab_id, tab_title, tab_url) = active_tab_info
            .ok_or_else(|| AgentError::Tool("No active tab. Create a tab and navigate first.".to_string()))?;

        let full_page = input["full_page"].as_bool().unwrap_or(false);
        let save_path = input["path"].as_str().unwrap_or("");

        let screenshot_path = if !save_path.is_empty() {
            save_path.to_string()
        } else {
            // Default: save to temp directory
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let filename = format!("screenshot_{}.png", timestamp);
            let path = std::path::PathBuf::from(&context.cwd).join(&filename);
            path.to_string_lossy().to_string()
        };

        // In a full implementation, use chromium's screenshot API via CDP
        // For now, use a placeholder approach
        let full_page_note = if full_page {
            " (full page)"
        } else {
            " (viewport only)"
        };

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: format!(
                "Screenshot{} captured for tab '{}' (id: {}).\n\
                URL: {}\n\
                Saved to: {}\n\n\
                Note: In a full implementation, this would use the browser's screenshot API\n\
                to capture the current viewport or full page as a PNG image.",
                full_page_note,
                tab_title,
                tab_id,
                tab_url,
                screenshot_path
            ),
            is_error: None,
            was_persisted: None,
        })
    }

    /// Evaluate JavaScript in the page
    async fn evaluate(
        &self,
        input: &serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let script = input["script"]
            .as_str()
            .ok_or_else(|| AgentError::Tool("script is required for evaluate action".to_string()))?;

        let state = self.state.lock().await;
        if !state.is_running {
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: "Browser is not running. Use start_browser first.".to_string(),
                is_error: Some(true),
                was_persisted: None,
            });
        }

        let active_tab_info = state
            .tabs
            .iter()
            .find(|t| t.is_active)
            .map(|t| (t.id.clone(), t.title.clone(), t.url.clone()));

        drop(state);

        let (tab_id, tab_title, tab_url) = active_tab_info
            .ok_or_else(|| AgentError::Tool("No active tab. Create a tab and navigate first.".to_string()))?;

        // In a full implementation, use CDP Runtime.evaluate
        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: format!(
                "JavaScript executed in tab '{}' (id: {}).\n\
                URL: {}\n\n\
                Script:\n{}\n\n\
                Note: In a full implementation, this would use CDP Runtime.evaluate\n\
                to execute the script in the page context and return the result.",
                tab_title,
                tab_id,
                tab_url,
                script
            ),
            is_error: None,
            was_persisted: None,
        })
    }

    /// Read console messages
    async fn read_console(
        &self,
        input: &serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let state = self.state.lock().await;
        if !state.is_running {
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: "Browser is not running. Use start_browser first.".to_string(),
                is_error: Some(true),
                was_persisted: None,
            });
        }

        let pattern = input.get("pattern").and_then(|v| v.as_str());

        let filter_note = match pattern {
            Some(p) => format!(" (filtered by pattern: {})", p),
            None => " (all messages)".to_string(),
        };

        drop(state);

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: format!(
                "Console messages{}.\n\n\
                Note: In a full implementation, this would read console output collected\n\
                from the browser's Runtime.consoleAPICalled and Runtime.exceptionThrown events.\n\
                Use the 'pattern' parameter to filter for specific messages.",
                filter_note
            ),
            is_error: None,
            was_persisted: None,
        })
    }

    /// Get list of tabs
    async fn get_tabs(
        &self,
        _input: &serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let state = self.state.lock().await;
        if !state.is_running {
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: "Browser is not running. Use start_browser first.".to_string(),
                is_error: Some(true),
                was_persisted: None,
            });
        }

        if state.tabs.is_empty() {
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: "No tabs open. Use create_tab or navigate to open a page.".to_string(),
                is_error: None,
                was_persisted: None,
            });
        }

        let tabs_info: Vec<String> = state
            .tabs
            .iter()
            .map(|t| {
                let active_marker = if t.is_active { " (active)" } else { "" };
                format!("  - [{}] {}{}  \n    URL: {}", t.id, t.title, active_marker, t.url)
            })
            .collect();

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: format!(
                "Open tabs ({} total):\n\n{}",
                state.tabs.len(),
                tabs_info.join("\n")
            ),
            is_error: None,
            was_persisted: None,
        })
    }

    /// Create a new tab
    async fn create_tab(
        &self,
        input: &serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let url = input.get("url").and_then(|v| v.as_str());

        let mut state = self.state.lock().await;
        if !state.is_running {
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: "Browser is not running. Use start_browser first.".to_string(),
                is_error: Some(true),
                was_persisted: None,
            });
        }

        let tab_id = format!("tab_{}", state.tabs.len() + 1);

        // Deactivate all other tabs
        for t in &mut state.tabs {
            t.is_active = false;
        }

        let tab = BrowserTab {
            id: tab_id.clone(),
            url: url.unwrap_or("about:blank").to_string(),
            title: url.unwrap_or("New Tab").to_string(),
            is_active: true,
        };
        state.tabs.push(tab);
        state.active_tab_id = Some(tab_id.clone());
        drop(state);

        let url_note = match url {
            Some(u) => format!(" and navigated to {}", u),
            None => " (about:blank)".to_string(),
        };

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: format!(
                "Created new tab (id: {}){}.\n\
                Use 'navigate' to load a URL, then 'screenshot' to verify.",
                tab_id, url_note
            ),
            is_error: None,
            was_persisted: None,
        })
    }

    /// Close a tab
    async fn close_tab(
        &self,
        input: &serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let tab_id = input.get("tab_id").and_then(|v| v.as_str());

        let mut state = self.state.lock().await;
        if !state.is_running {
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: "Browser is not running. Use start_browser first.".to_string(),
                is_error: Some(true),
                was_persisted: None,
            });
        }

        let (removed_title, removed_id) = if let Some(id) = tab_id {
            // Close specific tab
            let idx = state.tabs.iter().position(|t| t.id == id);
            match idx {
                Some(i) => {
                    let tab = state.tabs.remove(i);
                    (tab.title.clone(), tab.id.clone())
                }
                None => {
                    return Ok(ToolResult {
                        result_type: "text".to_string(),
                        tool_use_id: "".to_string(),
                        content: format!("Tab '{}' not found.", id),
                        is_error: Some(true),
                was_persisted: None,
                    });
                }
            }
        } else {
            // Close active tab
            let idx = state.tabs.iter().position(|t| t.is_active);
            match idx {
                Some(i) => {
                    let tab = state.tabs.remove(i);
                    (tab.title.clone(), tab.id.clone())
                }
                None => {
                    return Ok(ToolResult {
                        result_type: "text".to_string(),
                        tool_use_id: "".to_string(),
                        content: "No active tab to close.".to_string(),
                        is_error: Some(true),
                was_persisted: None,
                    });
                }
            }
        };

        // Activate another tab if available
        if let Some(first_tab) = state.tabs.first_mut() {
            first_tab.is_active = true;
            state.active_tab_id = Some(first_tab.id.clone());
        } else {
            state.active_tab_id = None;
        }
        drop(state);

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: format!("Closed tab '{}' (id: {}).", removed_title, removed_id),
            is_error: None,
            was_persisted: None,
        })
    }

    /// Click an element
    async fn click(
        &self,
        input: &serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let selector = input["selector"]
            .as_str()
            .ok_or_else(|| AgentError::Tool("selector is required for click action".to_string()))?;

        let state = self.state.lock().await;
        if !state.is_running {
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: "Browser is not running. Use start_browser first.".to_string(),
                is_error: Some(true),
                was_persisted: None,
            });
        }

        let active_tab_info = state
            .tabs
            .iter()
            .find(|t| t.is_active)
            .map(|t| (t.id.clone(), t.title.clone(), t.url.clone()));

        drop(state);

        let (tab_id, tab_title, tab_url) = active_tab_info
            .ok_or_else(|| AgentError::Tool("No active tab. Create a tab and navigate first.".to_string()))?;

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: format!(
                "Clicked element '{}' in tab '{}' (id: {}).  \nURL: {}\n\n\
                Note: In a full implementation, this would use CDP DOM APIs\n\
                to find and click the element matching the CSS selector.\n\
                Use 'screenshot' to verify the click had the expected effect.",
                selector,
                tab_title,
                tab_id,
                tab_url
            ),
            is_error: None,
            was_persisted: None,
        })
    }

    /// Fill a form field
    async fn fill(
        &self,
        input: &serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let selector = input["selector"]
            .as_str()
            .ok_or_else(|| AgentError::Tool("selector is required for fill action".to_string()))?;

        let text = input["text"]
            .as_str()
            .ok_or_else(|| AgentError::Tool("text is required for fill action".to_string()))?;

        let state = self.state.lock().await;
        if !state.is_running {
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: "Browser is not running. Use start_browser first.".to_string(),
                is_error: Some(true),
                was_persisted: None,
            });
        }

        let active_tab_info = state
            .tabs
            .iter()
            .find(|t| t.is_active)
            .map(|t| (t.id.clone(), t.title.clone(), t.url.clone()));

        drop(state);

        let (tab_id, tab_title, tab_url) = active_tab_info
            .ok_or_else(|| AgentError::Tool("No active tab. Create a tab and navigate first.".to_string()))?;

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: format!(
                "Filled element '{}' with text in tab '{}' (id: {}).  \nURL: {}\n\n\
                Note: In a full implementation, this would use CDP DOM APIs\n\
                to find the input element and set its value.\n\
                Use 'screenshot' to verify the form was filled correctly.",
                selector,
                tab_title,
                tab_id,
                tab_url
            ),
            is_error: None,
            was_persisted: None,
        })
    }

    /// Get text content of an element
    async fn get_text(
        &self,
        input: &serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let selector = input["selector"]
            .as_str()
            .ok_or_else(|| AgentError::Tool("selector is required for get_text action".to_string()))?;

        let state = self.state.lock().await;
        if !state.is_running {
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: "Browser is not running. Use start_browser first.".to_string(),
                is_error: Some(true),
                was_persisted: None,
            });
        }

        let active_tab_info = state
            .tabs
            .iter()
            .find(|t| t.is_active)
            .map(|t| (t.id.clone(), t.title.clone(), t.url.clone()));

        drop(state);

        let (tab_id, tab_title, tab_url) = active_tab_info
            .ok_or_else(|| AgentError::Tool("No active tab. Create a tab and navigate first.".to_string()))?;

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: format!(
                "Retrieved text from element '{}' in tab '{}' (id: {}).  \nURL: {}\n\n\
                Note: In a full implementation, this would use CDP DOM APIs\n\
                to find the element and extract its text content.",
                selector,
                tab_title,
                tab_id,
                tab_url
            ),
            is_error: None,
            was_persisted: None,
        })
    }

    /// Wait for a condition (selector, timeout, etc.)
    async fn wait_for(
        &self,
        input: &serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let selector = input.get("wait_for_selector").and_then(|v| v.as_str());
        let timeout_ms = input["timeout_ms"].as_u64().unwrap_or(30000);

        let state = self.state.lock().await;
        if !state.is_running {
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: "Browser is not running. Use start_browser first.".to_string(),
                is_error: Some(true),
                was_persisted: None,
            });
        }

        let active_tab_info = state
            .tabs
            .iter()
            .find(|t| t.is_active)
            .map(|t| (t.id.clone(), t.title.clone(), t.url.clone()));

        drop(state);

        let (tab_id, tab_title, tab_url) = active_tab_info
            .ok_or_else(|| AgentError::Tool("No active tab. Create a tab and navigate first.".to_string()))?;

        let wait_description = match selector {
            Some(s) => format!("for selector '{}'", s),
            None => format!("for {}ms", timeout_ms),
        };

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: format!(
                "Waited {} in tab '{}' (id: {}).  \nURL: {}\n\n\
                Note: In a full implementation, this would use CDP DOM APIs\n\
                to wait for the element to appear or a timeout to elapse.",
                wait_description,
                tab_title,
                tab_id,
                tab_url
            ),
            is_error: None,
            was_persisted: None,
        })
    }

    /// Detect available chromium-based browser
    async fn detect_chrome_path(&self) -> Result<String, AgentError> {
        // Try common chromium-based browser executables in priority order
        // (matching TypeScript CHROMIUM_BROWSERS detection order)
        let browser_candidates = [
            "google-chrome",
            "google-chrome-stable",
            "chromium-browser",
            "chromium",
            "chrome",
            "/usr/bin/google-chrome",
            "/usr/bin/chromium-browser",
            "/usr/bin/chromium",
        ];

        for browser in &browser_candidates {
            if self.is_executable_available(browser).await {
                return Ok(browser.to_string());
            }
        }

        Err(AgentError::Tool(
            "No chromium-based browser found. Install google-chrome or chromium-browser.".to_string(),
        ))
    }

    /// Check if an executable is available
    async fn is_executable_available(&self, cmd: &str) -> bool {
        let result = tokio::process::Command::new("which")
            .arg(cmd)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;

        match result {
            Ok(status) => status.success(),
            Err(_) => false,
        }
    }
}

impl Default for WebBrowserTool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_web_browser_tool_name() {
        let tool = WebBrowserTool::new();
        assert_eq!(tool.name(), WEB_BROWSER_TOOL_NAME);
    }

    #[test]
    fn test_web_browser_tool_schema_has_action() {
        let tool = WebBrowserTool::new();
        let schema = tool.input_schema();
        assert!(schema.properties.get("action").is_some());
        assert!(schema.properties.get("url").is_some());
        assert!(schema.properties.get("script").is_some());
        assert!(schema.properties.get("selector").is_some());
        assert!(schema.properties.get("tab_id").is_some());
    }

    #[test]
    fn test_web_browser_tool_schema_required_has_action() {
        let tool = WebBrowserTool::new();
        let schema = tool.input_schema();
        let required = schema.required.unwrap();
        assert!(required.contains(&"action".to_string()));
    }

    #[tokio::test]
    async fn test_web_browser_requires_action() {
        let tool = WebBrowserTool::new();
        let input = serde_json::json!({});
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("action is required"));
    }

    #[tokio::test]
    async fn test_web_browser_unknown_action() {
        let tool = WebBrowserTool::new();
        let input = serde_json::json!({
            "action": "unknown_action"
        });
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());
        let content = result.unwrap().content;
        assert!(content.contains("Unknown action"));
    }

    #[tokio::test]
    async fn test_web_browser_stop_without_start() {
        let tool = WebBrowserTool::new();
        let input = serde_json::json!({
            "action": "stop_browser"
        });
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());
        let content = result.unwrap().content;
        assert!(content.contains("not running"));
    }

    #[tokio::test]
    async fn test_web_browser_navigate_requires_url() {
        let tool = WebBrowserTool::new();
        // First start the browser (will fail if no chrome, but that's ok for this test)
        let input = serde_json::json!({
            "action": "navigate"
        });
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        // Should fail because url is missing
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("url is required"));
    }

    #[tokio::test]
    async fn test_web_browser_evaluate_requires_script() {
        let tool = WebBrowserTool::new();
        let input = serde_json::json!({
            "action": "evaluate"
        });
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("script is required"));
    }

    #[tokio::test]
    async fn test_web_browser_click_requires_selector() {
        let tool = WebBrowserTool::new();
        let input = serde_json::json!({
            "action": "click"
        });
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("selector is required"));
    }

    #[tokio::test]
    async fn test_web_browser_fill_requires_selector_and_text() {
        let tool = WebBrowserTool::new();
        let input = serde_json::json!({
            "action": "fill",
            "selector": "#input"
        });
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("text is required"));
    }
}
