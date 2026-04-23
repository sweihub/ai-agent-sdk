// Source: ~/claudecode/openclaudecode/src/tools/DiscoverSkillsTool/prompt.ts
//! DiscoverSkillsTool — on-demand skill discovery (feature-gated stub)

pub mod prompt;
use prompt::DISCOVER_SKILLS_TOOL_NAME;

/// DiscoverSkillsTool — on-demand skill discovery
pub struct DiscoverSkillsTool;

impl DiscoverSkillsTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        DISCOVER_SKILLS_TOOL_NAME
    }
}

impl Default for DiscoverSkillsTool {
    fn default() -> Self {
        Self::new()
    }
}
