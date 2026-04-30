//! Cost tracking facade - delegates to services::model_cost for full implementation.

use crate::services::model_cost as cost;

#[derive(Debug, Clone)]
pub struct CostEntry {
    pub timestamp: i64,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_read_input_tokens: u32,
    pub cache_creation_input_tokens: u32,
    pub model: String,
}

/// Track a cost entry - updates global cost state with per-model usage.
pub fn track_cost(entry: CostEntry) {
    let usage = cost::TokenUsage {
        input_tokens: entry.input_tokens,
        output_tokens: entry.output_tokens,
        prompt_cache_write_tokens: entry.cache_creation_input_tokens,
        prompt_cache_read_tokens: entry.cache_read_input_tokens,
    };
    let model_cost = cost::calculate_cost(&entry.model, &usage);
    cost::add_to_total_model_usage(
        model_cost,
        entry.input_tokens,
        entry.output_tokens,
        entry.cache_read_input_tokens,
        entry.cache_creation_input_tokens,
        0, // web_search_requests
        &entry.model,
    );
}

/// Get total cost in USD across all models.
pub fn get_total_cost_usd() -> f64 {
    crate::services::model_cost::get_global_cost_state().total_cost_usd
}

/// Format the total cost for display.
pub fn format_total_cost() -> String {
    cost::format_total_cost()
}

/// Initialize cost tracking for a new session.
pub fn init_session(session_id: &str) {
    cost::init_cost_state(session_id);
}

/// Restore cost state when resuming a session.
pub fn restore_session(session_id: &str) -> bool {
    cost::restore_cost_state_for_session(session_id)
}

/// Save current session costs to disk.
pub fn save_session_costs() {
    cost::save_current_session_costs();
}
