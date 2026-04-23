// Source: ~/claudecode/openclaudecode/src/tools/SnipTool/prompt.ts
//! SnipTool — model-callable compaction tool (feature-gated stub)

pub mod prompt;
use prompt::SNIP_TOOL_NAME;

/// SnipTool — model-callable compaction tool
pub struct SnipTool;

impl SnipTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        SNIP_TOOL_NAME
    }
}

impl Default for SnipTool {
    fn default() -> Self {
        Self::new()
    }
}
