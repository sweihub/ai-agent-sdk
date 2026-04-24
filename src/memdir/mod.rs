// Source: /data/home/swei/claudecode/openclaudecode/src/memdir/*.ts
//! Memory directory system - persistent file-based memory for the agent.
//!
//! This module provides the memory system that allows the agent to remember
//! information across conversations. It follows the TypeScript flavor of the
//! original Claude Code project.

pub mod find_relevant_memories;

pub use find_relevant_memories::{
    FindRelevantMemoriesConfig, RelevantMemory, find_relevant_memories,
    find_relevant_memories_with_config,
};

pub mod memdir;
pub mod memory_age;
pub mod memory_scan;
pub mod memory_shape_telemetry;
pub mod memory_types;
pub mod paths;
pub mod team_mem_paths;
pub mod team_mem_prompts;

pub use memdir::{
    BuildMemoryPromptParams, DIR_EXISTS_GUIDANCE, ENTRYPOINT_NAME,
    MAX_ENTRYPOINT_BYTES as MEMDIR_MAX_ENTRYPOINT_BYTES,
    MAX_ENTRYPOINT_LINES as MEMDIR_MAX_ENTRYPOINT_LINES, build_memory_lines, build_memory_prompt,
    load_memory_prompt, truncate_entrypoint_content,
};
pub use memory_age::{memory_age, memory_age_days, memory_freshness_note, memory_freshness_text};
pub use memory_scan::{MAX_MEMORY_FILES, MemoryHeader, format_memory_manifest, scan_memory_files};
pub use memory_types::{
    EntrypointTruncation, MAX_ENTRYPOINT_BYTES, MAX_ENTRYPOINT_LINES, MEMORY_TYPES, Memory,
    MemoryFrontmatter, MemoryType, extract_content, parse_frontmatter, parse_memory_type,
    truncate_entrypoint,
};
pub use paths::{
    get_auto_mem_daily_log_path, get_auto_mem_entrypoint, get_auto_mem_path, get_memory_base_dir,
    has_auto_mem_path_override, is_auto_mem_path, is_auto_memory_enabled, sanitize_path_component,
};
pub use team_mem_paths::{
    PathTraversalError, get_team_mem_entypoint, get_team_mem_path, is_team_mem_file,
    is_team_mem_path, is_team_memory_enabled, validate_team_mem_key, validate_team_mem_write_path,
};
pub use team_mem_prompts::build_combined_memory_prompt;

/// Entrypoint filename
pub const ENTRYPOINT_NAME_CONST: &str = "MEMORY.md";

/// Ensure the memory directory exists.
pub fn ensure_memory_dir_exists(path: &std::path::Path) -> std::io::Result<()> {
    paths::ensure_memory_dir_exists(path)
}

/// Get the memory entrypoint path.
pub fn get_memory_entrypoint() -> std::path::PathBuf {
    paths::get_auto_mem_entrypoint()
}

/// Synchronous wrapper for loading memory prompt.
/// Calls the async load_memory_prompt and returns the result.
pub fn load_memory_prompt_sync() -> Option<String> {
    // Check if auto memory is enabled
    if !paths::is_auto_memory_enabled() {
        return None;
    }

    let auto_dir = paths::get_auto_mem_path();
    // Ensure the directory exists
    let _ = paths::ensure_memory_dir_exists(&auto_dir);

    // Build the memory prompt synchronously
    Some(memdir::build_memory_prompt(
        memdir::BuildMemoryPromptParams {
            display_name: "auto memory",
            extra_guidelines: None,
        },
    ))
}
