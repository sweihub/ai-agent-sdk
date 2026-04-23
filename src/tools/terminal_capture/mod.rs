// Source: ~/claudecode/openclaudecode/src/tools/TerminalCaptureTool/prompt.ts
//! TerminalCaptureTool — terminal screen capture (feature-gated stub)

pub mod prompt;
use prompt::TERMINAL_CAPTURE_TOOL_NAME;

/// TerminalCaptureTool — terminal screen capture
pub struct TerminalCaptureTool;

impl TerminalCaptureTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        TERMINAL_CAPTURE_TOOL_NAME
    }
}

impl Default for TerminalCaptureTool {
    fn default() -> Self {
        Self::new()
    }
}
