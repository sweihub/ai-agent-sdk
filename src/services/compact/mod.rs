//! Compact service module - handles conversation compaction

pub mod api_microcompact;
pub mod auto_compact;
pub mod cached_mc_config;
pub mod compact;
pub mod compact_warning_hook;
pub mod compact_warning_state;
pub mod grouping;
pub mod microcompact;
pub mod post_compact_cleanup;
pub mod prompt;
pub mod reactive_compact;
pub mod session_memory_compact;
pub mod snip_compact;
pub mod snip_projection;
pub mod time_based_mc_config;
pub mod tool_result_budget;

pub use api_microcompact::get_api_context_management;
pub use auto_compact::*;
pub use cached_mc_config::*;
pub use compact::*;
pub use compact_warning_hook::*;
pub use compact_warning_state::*;
pub use microcompact::*;
pub use post_compact_cleanup::run_post_compact_cleanup;
pub use prompt::*;
pub use reactive_compact::*;
pub use session_memory_compact::*;
pub use snip_compact::snip_compact_if_known;
pub use snip_projection::is_snip_boundary_message as is_snip_projection_boundary;
pub use time_based_mc_config::*;
pub use tool_result_budget::{
    apply_tool_result_budget, enforce_tool_result_budget, create_content_replacement_state,
    reconstruct_content_replacement_state, ContentReplacementState, ToolResultReplacementRecord,
};

// Re-export compactable tools list for other modules
pub use microcompact::{
    TIME_BASED_MC_CLEARED_MESSAGE, TimeBasedMCResult, collect_compactable_tool_ids,
    maybe_time_based_microcompact, reset_microcompact_state, truncate_tool_result_content,
};

// Re-export compaction helpers from crate::compact
pub use crate::compact::{
    merge_hook_instructions, create_compact_boundary_message, build_post_compact_messages,
    annotate_boundary_with_preserved_segment, re_append_session_metadata,
    CompactMetadata, PreservedSegment,
};
