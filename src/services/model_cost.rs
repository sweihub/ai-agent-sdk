//! Model cost calculation.
//!
//! Provides cost estimation for different AI models similar to claude code.

use serde::{Deserialize, Serialize};

/// Model cost configuration (per million tokens)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCosts {
    /// Input tokens cost per million
    pub input_tokens: f64,
    /// Output tokens cost per million
    pub output_tokens: f64,
    /// Prompt cache write tokens cost per million
    pub prompt_cache_write_tokens: f64,
    /// Prompt cache read tokens cost per million
    pub prompt_cache_read_tokens: f64,
    /// Web search requests cost per search
    pub web_search_requests: f64,
}

impl ModelCosts {
    /// Calculate cost for input tokens
    pub fn input_cost(&self, tokens: u32) -> f64 {
        (tokens as f64 / 1_000_000.0) * self.input_tokens
    }

    /// Calculate cost for output tokens
    pub fn output_cost(&self, tokens: u32) -> f64 {
        (tokens as f64 / 1_000_000.0) * self.output_tokens
    }

    /// Calculate cost for cache write tokens
    pub fn cache_write_cost(&self, tokens: u32) -> f64 {
        (tokens as f64 / 1_000_000.0) * self.prompt_cache_write_tokens
    }

    /// Calculate cost for cache read tokens
    pub fn cache_read_cost(&self, tokens: u32) -> f64 {
        (tokens as f64 / 1_000_000.0) * self.prompt_cache_read_tokens
    }

    /// Calculate total cost for a usage record
    pub fn total_cost(&self, usage: &TokenUsage) -> f64 {
        self.input_cost(usage.input_tokens)
            + self.output_cost(usage.output_tokens)
            + self.cache_write_cost(usage.prompt_cache_write_tokens)
            + self.cache_read_cost(usage.prompt_cache_read_tokens)
    }
}

/// Token usage from API response
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    #[serde(rename = "promptCacheWriteTokens")]
    pub prompt_cache_write_tokens: u32,
    #[serde(rename = "promptCacheReadTokens")]
    pub prompt_cache_read_tokens: u32,
}

impl TokenUsage {
    /// Total tokens used
    pub fn total(&self) -> u32 {
        self.input_tokens
            + self.output_tokens
            + self.prompt_cache_write_tokens
            + self.prompt_cache_read_tokens
    }
}

/// Model information for listing available models
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Model identifier
    pub id: String,
    /// Display name
    pub name: String,
    /// Description
    pub description: String,
    /// Context window size in tokens
    pub context_window: u32,
}

/// Common cost tiers

/// Standard pricing: $3 input / $15 output per M tokens
pub const COST_TIER_3_15: ModelCosts = ModelCosts {
    input_tokens: 3.0,
    output_tokens: 15.0,
    prompt_cache_write_tokens: 3.75,
    prompt_cache_read_tokens: 0.3,
    web_search_requests: 0.01,
};

/// Opus pricing: $15 input / $75 output per M tokens
pub const COST_TIER_15_75: ModelCosts = ModelCosts {
    input_tokens: 15.0,
    output_tokens: 75.0,
    prompt_cache_write_tokens: 18.75,
    prompt_cache_read_tokens: 1.5,
    web_search_requests: 0.01,
};

/// Mid-tier pricing: $5 input / $25 output per M tokens
pub const COST_TIER_5_25: ModelCosts = ModelCosts {
    input_tokens: 5.0,
    output_tokens: 25.0,
    prompt_cache_write_tokens: 6.25,
    prompt_cache_read_tokens: 0.5,
    web_search_requests: 0.01,
};

/// Fast mode pricing: $30 input / $150 output per M tokens
pub const COST_TIER_30_150: ModelCosts = ModelCosts {
    input_tokens: 30.0,
    output_tokens: 150.0,
    prompt_cache_write_tokens: 37.5,
    prompt_cache_read_tokens: 3.0,
    web_search_requests: 0.01,
};

/// Haiku 3.5 pricing: $0.80 input / $4 output per M tokens
pub const COST_HAIKU_35: ModelCosts = ModelCosts {
    input_tokens: 0.8,
    output_tokens: 4.0,
    prompt_cache_write_tokens: 1.0,
    prompt_cache_read_tokens: 0.08,
    web_search_requests: 0.01,
};

/// Haiku 4.5 pricing: $1 input / $5 output per M tokens
pub const COST_HAIKU_45: ModelCosts = ModelCosts {
    input_tokens: 1.0,
    output_tokens: 5.0,
    prompt_cache_write_tokens: 1.25,
    prompt_cache_read_tokens: 0.1,
    web_search_requests: 0.01,
};

/// Default cost for unknown models
pub const COST_DEFAULT: ModelCosts = COST_TIER_5_25;

/// Model cost registry
pub struct ModelCostRegistry {
    costs: std::collections::HashMap<String, ModelCosts>,
}

impl ModelCostRegistry {
    pub fn new() -> Self {
        let mut costs = std::collections::HashMap::new();

        // Anthropic models
        costs.insert("claude-opus-4-6".to_string(), COST_TIER_5_25);
        costs.insert("claude-opus-4-5".to_string(), COST_TIER_5_25);
        costs.insert("claude-opus-4-1".to_string(), COST_TIER_15_75);
        costs.insert("claude-opus-4".to_string(), COST_TIER_15_75);
        costs.insert("claude-sonnet-4-6".to_string(), COST_TIER_3_15);
        costs.insert("claude-sonnet-4-5".to_string(), COST_TIER_3_15);
        costs.insert("claude-sonnet-4".to_string(), COST_TIER_3_15);
        costs.insert("claude-sonnet-3-5".to_string(), COST_TIER_3_15);
        costs.insert("claude-haiku-4-5".to_string(), COST_HAIKU_45);
        costs.insert("claude-haiku-3-5".to_string(), COST_HAIKU_35);

        // MiniMax models
        costs.insert("MiniMaxAI/MiniMax-M2.5".to_string(), COST_TIER_3_15);
        costs.insert("MiniMaxAI/MiniMax-M2".to_string(), COST_TIER_3_15);

        // OpenAI models (for compatibility)
        costs.insert("gpt-4o".to_string(), COST_TIER_5_25);
        costs.insert("gpt-4o-mini".to_string(), COST_HAIKU_35);
        costs.insert("gpt-4-turbo".to_string(), COST_TIER_10_30);
        costs.insert("gpt-4".to_string(), COST_TIER_30_60);

        Self { costs }
    }

    /// Get cost for a model
    pub fn get(&self, model: &str) -> &ModelCosts {
        // Try exact match first
        if let Some(cost) = self.costs.get(model) {
            return cost;
        }

        // Try prefix match
        for (key, cost) in &self.costs {
            if model.starts_with(key) || key.starts_with(model) {
                return cost;
            }
        }

        &COST_DEFAULT
    }

    /// Register a custom model cost
    pub fn register(&mut self, model: &str, costs: ModelCosts) {
        self.costs.insert(model.to_string(), costs);
    }
}

impl Default for ModelCostRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Pricing tier for GPT-4: $30 input / $60 output per M tokens
pub const COST_TIER_30_60: ModelCosts = ModelCosts {
    input_tokens: 30.0,
    output_tokens: 60.0,
    prompt_cache_write_tokens: 30.0,
    prompt_cache_read_tokens: 10.0,
    web_search_requests: 0.01,
};

/// Pricing tier for GPT-4 Turbo: $10 input / $30 output per M tokens
pub const COST_TIER_10_30: ModelCosts = ModelCosts {
    input_tokens: 10.0,
    output_tokens: 30.0,
    prompt_cache_write_tokens: 10.0,
    prompt_cache_read_tokens: 3.0,
    web_search_requests: 0.01,
};

/// Calculate cost from model name and usage
pub fn calculate_cost(model: &str, usage: &TokenUsage) -> f64 {
    let registry = ModelCostRegistry::new();
    let costs = registry.get(model);
    costs.total_cost(usage)
}

/// Calculate cost from raw token counts (avoids TokenUsage struct conversion)
pub fn calculate_cost_for_tokens(
    model: &str,
    input_tokens: u32,
    output_tokens: u32,
    cache_read_input_tokens: u32,
    cache_creation_input_tokens: u32,
) -> f64 {
    let registry = ModelCostRegistry::new();
    let costs = registry.get(model);
    costs.input_cost(input_tokens)
        + costs.output_cost(output_tokens)
        + costs.cache_read_cost(cache_read_input_tokens)
        + costs.cache_write_cost(cache_creation_input_tokens)
}

/// Get list of available models with their display names and descriptions
pub fn get_available_models() -> Vec<ModelInfo> {
    vec![
        ModelInfo {
            id: "claude-opus-4-6".to_string(),
            name: "Opus".to_string(),
            description: "Most capable for complex work".to_string(),
            context_window: 200_000,
        },
        ModelInfo {
            id: "claude-sonnet-4-6".to_string(),
            name: "Sonnet".to_string(),
            description: "Best for everyday tasks".to_string(),
            context_window: 200_000,
        },
        ModelInfo {
            id: "claude-sonnet-4-6-20250520".to_string(),
            name: "Sonnet 4.6".to_string(),
            description: "Latest Sonnet model".to_string(),
            context_window: 200_000,
        },
        ModelInfo {
            id: "claude-haiku-4-5".to_string(),
            name: "Haiku".to_string(),
            description: "Fastest for quick answers".to_string(),
            context_window: 200_000,
        },
        ModelInfo {
            id: "claude-opus-4-5".to_string(),
            name: "Opus 4.5".to_string(),
            description: "Previous Opus version".to_string(),
            context_window: 200_000,
        },
        ModelInfo {
            id: "claude-sonnet-4-5".to_string(),
            name: "Sonnet 4.5".to_string(),
            description: "Previous Sonnet version".to_string(),
            context_window: 200_000,
        },
        ModelInfo {
            id: "MiniMaxAI/MiniMax-M2.5".to_string(),
            name: "MiniMax M2.5".to_string(),
            description: "Fast and capable (default)".to_string(),
            context_window: 1_000_000,
        },
    ]
}

/// Format cost as dollars
pub fn format_cost(cost: f64) -> String {
    if cost < 0.01 {
        format!("${:.4}", cost)
    } else if cost < 1.0 {
        format!("${:.2}", cost)
    } else {
        format!("${:.4}", cost)
    }
}

/// Cost summary for display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostSummary {
    pub input_cost: f64,
    pub output_cost: f64,
    pub cache_write_cost: f64,
    pub cache_read_cost: f64,
    pub total_cost: f64,
}

impl CostSummary {
    pub fn from_usage(model: &str, usage: &TokenUsage) -> Self {
        let registry = ModelCostRegistry::new();
        let costs = registry.get(model);

        Self {
            input_cost: costs.input_cost(usage.input_tokens),
            output_cost: costs.output_cost(usage.output_tokens),
            cache_write_cost: costs.cache_write_cost(usage.prompt_cache_write_tokens),
            cache_read_cost: costs.cache_read_cost(usage.prompt_cache_read_tokens),
            total_cost: costs.total_cost(usage),
        }
    }
}

use crate::utils::config::{
    ModelUsage as ConfigModelUsage, get_current_project_config, save_current_project_config,
};

/// Stored cost state from project config
#[derive(Debug, Clone, Default)]
pub struct StoredCostState {
    pub total_cost_usd: f64,
    pub total_api_duration: u64,
    pub total_api_duration_without_retries: u64,
    pub total_tool_duration: u64,
    pub total_lines_added: u32,
    pub total_lines_removed: u32,
    pub last_duration: Option<u64>,
    pub model_usage: Option<std::collections::HashMap<String, ConfigModelUsage>>,
}

/// Get stored cost state from project config for a specific session.
/// Returns the cost data if the session ID matches, or None otherwise.
/// Use this to read costs BEFORE overwriting the config with save_current_session_costs().
pub fn get_stored_session_costs(session_id: &str) -> Option<StoredCostState> {
    let project_config = get_current_project_config();

    // Only return costs if this is the same session that was last saved
    if project_config.last_session_id.as_deref() != Some(session_id) {
        return None;
    }

    Some(StoredCostState {
        total_cost_usd: project_config.last_cost.unwrap_or(0.0),
        total_api_duration: project_config.last_api_duration.unwrap_or(0),
        total_api_duration_without_retries: project_config
            .last_api_duration_without_retries
            .unwrap_or(0),
        total_tool_duration: project_config.last_tool_duration.unwrap_or(0),
        total_lines_added: project_config.last_lines_added.unwrap_or(0),
        total_lines_removed: project_config.last_lines_removed.unwrap_or(0),
        last_duration: project_config.last_duration,
        model_usage: project_config.last_model_usage,
    })
}

/// Restores cost state from project config when resuming a session.
/// Only restores if the session ID matches the last saved session.
/// Returns true if cost state was restored, false otherwise.
pub fn restore_cost_state_for_session(session_id: &str) -> bool {
    let stored = get_stored_session_costs(session_id);
    let Some(stored) = stored else {
        return false;
    };

    update_global_cost_state(|state| {
        state.total_cost_usd = stored.total_cost_usd;
        state.total_api_duration = stored.total_api_duration;
        state.total_api_duration_without_retries = stored.total_api_duration_without_retries;
        state.total_tool_duration = stored.total_tool_duration;
        state.total_lines_added = stored.total_lines_added;
        state.total_lines_removed = stored.total_lines_removed;
        state.last_duration = stored.last_duration;
        state.model_usage = stored
            .model_usage
            .map(|mu| {
                mu.into_iter()
                    .map(|(k, v)| {
                        (
                            k,
                            ModelUsageInfo {
                                input_tokens: v.input_tokens,
                                output_tokens: v.output_tokens,
                                cache_read_input_tokens: v.cache_read_input_tokens,
                                cache_creation_input_tokens: v.cache_creation_input_tokens,
                                web_search_requests: v.web_search_requests,
                                cost_usd: v.cost_usd,
                                context_window: 0,
                                max_output_tokens: 0,
                            },
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        state.session_id = session_id.to_string();
    });

    true
}

/// Saves the current session's costs to project config.
/// Call this before switching sessions to avoid losing accumulated costs.
pub fn save_current_session_costs() {
    let cost_state = get_global_cost_state();

    let model_usage_map: Option<std::collections::HashMap<String, ConfigModelUsage>> =
        if cost_state.model_usage.is_empty() {
            None
        } else {
            let mut map = std::collections::HashMap::new();
            for (model, usage) in &cost_state.model_usage {
                map.insert(
                    model.clone(),
                    ConfigModelUsage {
                        input_tokens: usage.input_tokens,
                        output_tokens: usage.output_tokens,
                        cache_read_input_tokens: usage.cache_read_input_tokens,
                        cache_creation_input_tokens: usage.cache_creation_input_tokens,
                        web_search_requests: usage.web_search_requests,
                        cost_usd: usage.cost_usd,
                    },
                );
            }
            Some(map)
        };

    let mut config = get_current_project_config();
    config.last_cost = Some(cost_state.total_cost_usd);
    config.last_api_duration = Some(cost_state.total_api_duration);
    config.last_api_duration_without_retries = Some(cost_state.total_api_duration_without_retries);
    config.last_tool_duration = Some(cost_state.total_tool_duration);
    config.last_duration = cost_state.last_duration;
    config.last_lines_added = Some(cost_state.total_lines_added);
    config.last_lines_removed = Some(cost_state.total_lines_removed);
    config.last_total_input_tokens = Some(cost_state.total_input_tokens);
    config.last_total_output_tokens = Some(cost_state.total_output_tokens);
    config.last_total_cache_creation_input_tokens =
        Some(cost_state.total_cache_creation_input_tokens);
    config.last_total_cache_read_input_tokens = Some(cost_state.total_cache_read_input_tokens);
    config.last_total_web_search_requests = Some(cost_state.total_web_search_requests);
    config.last_model_usage = model_usage_map;
    config.last_session_id = Some(cost_state.session_id.clone());

    let _ = save_current_project_config(config);
}

/// Format cost for display with variable decimal places
fn format_cost_for_display(cost: f64, max_decimal_places: usize) -> String {
    if cost > 0.5 {
        format!("${:.2}", (cost * 100.0).round() / 100.0)
    } else {
        format!("${:.width$}", cost, width = max_decimal_places + 2)
    }
}

/// Format a number with thousands separator
fn format_number(n: u32) -> String {
    let s = n.to_string();
    let mut result = String::new();
    let len = s.len();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result
}

/// Model usage for cost tracking (includes context window info)
#[derive(Debug, Clone, Default)]
pub struct ModelUsageInfo {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_read_input_tokens: u32,
    pub cache_creation_input_tokens: u32,
    pub web_search_requests: u32,
    pub cost_usd: f64,
    pub context_window: u32,
    pub max_output_tokens: u32,
}

/// Get canonical name for a model (short name)
fn get_canonical_name(model: &str) -> String {
    // Extract short name from model identifier
    if model.contains("opus") {
        "Opus".to_string()
    } else if model.contains("sonnet") {
        "Sonnet".to_string()
    } else if model.contains("haiku") {
        "Haiku".to_string()
    } else if model.contains("MiniMax") {
        "MiniMax".to_string()
    } else if model.contains("gpt") {
        "GPT".to_string()
    } else {
        model.to_string()
    }
}

/// Format model usage for display
pub fn format_model_usage() -> String {
    let cost_state = get_global_cost_state();

    if cost_state.model_usage.is_empty() {
        return "Usage:                 0 input, 0 output, 0 cache read, 0 cache write".to_string();
    }

    // Accumulate usage by short name
    let mut usage_by_short_name: std::collections::HashMap<String, ModelUsageInfo> =
        std::collections::HashMap::new();
    for (model, usage) in &cost_state.model_usage {
        let short_name = get_canonical_name(model);
        let entry = usage_by_short_name
            .entry(short_name)
            .or_insert_with(|| ModelUsageInfo::default());
        entry.input_tokens += usage.input_tokens;
        entry.output_tokens += usage.output_tokens;
        entry.cache_read_input_tokens += usage.cache_read_input_tokens;
        entry.cache_creation_input_tokens += usage.cache_creation_input_tokens;
        entry.web_search_requests += usage.web_search_requests;
        entry.cost_usd += usage.cost_usd;
    }

    let mut result = "Usage by model:".to_string();
    for (short_name, usage) in &usage_by_short_name {
        let usage_string = format!(
            "  {} input, {} output, {} cache read, {} cache write{}{} (${})",
            format_number(usage.input_tokens),
            format_number(usage.output_tokens),
            format_number(usage.cache_read_input_tokens),
            format_number(usage.cache_creation_input_tokens),
            if usage.web_search_requests > 0 {
                format!(", {} web search", format_number(usage.web_search_requests))
            } else {
                String::new()
            },
            if cost_state.has_unknown_model_cost {
                " (costs may be inaccurate due to usage of unknown models)".to_string()
            } else {
                String::new()
            },
            format_cost_for_display(usage.cost_usd, 4)
        );
        result.push('\n');
        // Pad the model name to 21 characters
        let padded_name = format!("{:<21}", format!("{}:", short_name));
        result.push_str(&padded_name);
        result.push_str(&usage_string.replace("  ", " "));
    }
    result
}

/// Format duration in human-readable format
fn format_duration(ms: u64) -> String {
    let seconds = ms / 1000;
    let minutes = seconds / 60;
    let hours = minutes / 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes % 60, seconds % 60)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds % 60)
    } else if seconds > 0 {
        format!("{}s", seconds)
    } else {
        format!("{}ms", ms)
    }
}

/// Format total cost for display
pub fn format_total_cost() -> String {
    let cost_state = get_global_cost_state();

    let cost_display = format!("Total cost:            ${:.4}", cost_state.total_cost_usd);

    let model_usage_display = format_model_usage();

    format!(
        "Total cost:            {}\nTotal duration (API):  {}\nTotal duration (wall): {}\nTotal code changes:    {} {} added, {} {}\n{}",
        cost_display,
        format_duration(cost_state.total_api_duration),
        format_duration(cost_state.last_duration.unwrap_or(0)),
        cost_state.total_lines_added,
        if cost_state.total_lines_added == 1 {
            "line"
        } else {
            "lines"
        },
        cost_state.total_lines_removed,
        if cost_state.total_lines_removed == 1 {
            "line"
        } else {
            "lines"
        },
        model_usage_display
    )
}

/// Global cost tracking state
#[derive(Debug, Clone, Default)]
pub struct GlobalCostState {
    pub total_cost_usd: f64,
    pub total_api_duration: u64,
    pub total_api_duration_without_retries: u64,
    pub total_tool_duration: u64,
    pub total_lines_added: u32,
    pub total_lines_removed: u32,
    pub last_duration: Option<u64>,
    pub total_input_tokens: u32,
    pub total_output_tokens: u32,
    pub total_cache_creation_input_tokens: u32,
    pub total_cache_read_input_tokens: u32,
    pub total_web_search_requests: u32,
    pub model_usage: std::collections::HashMap<String, ModelUsageInfo>,
    pub has_unknown_model_cost: bool,
    pub session_id: String,
    /// Per-turn tool metrics (TS: turnToolDurationMs, turnToolCount)
    pub turn_tool_duration_ms: u64,
    pub turn_tool_count: u32,
    /// Turn-level token budget tracking (TS: outputTokensAtTurnStart, currentTurnTokenBudget)
    pub output_tokens_at_turn_start: u64,
    pub current_turn_token_budget: Option<u64>,
    pub budget_continuation_count: u32,
}

/// Global cost state singleton - thread-safe, persisted across calls
static GLOBAL_COST_STATE: once_cell::sync::Lazy<std::sync::Mutex<GlobalCostState>> =
    once_cell::sync::Lazy::new(|| std::sync::Mutex::new(GlobalCostState::default()));

/// Initialize cost tracking for a new session
pub fn init_cost_state(session_id: &str) {
    let mut state = GLOBAL_COST_STATE.lock().unwrap();
    *state = GlobalCostState {
        session_id: session_id.to_string(),
        ..Default::default()
    };
}

/// Get the global cost state (singleton)
pub fn get_global_cost_state() -> GlobalCostState {
    GLOBAL_COST_STATE.lock().unwrap().clone()
}

/// Update the global cost state with a mutation closure
pub fn update_global_cost_state<F: FnOnce(&mut GlobalCostState)>(f: F) {
    let mut state = GLOBAL_COST_STATE.lock().unwrap();
    f(&mut state);
}

/// Add to total model usage
pub fn add_to_total_model_usage(
    cost: f64,
    input_tokens: u32,
    output_tokens: u32,
    cache_read_input_tokens: u32,
    cache_creation_input_tokens: u32,
    web_search_requests: u32,
    model: &str,
) -> ModelUsageInfo {
    update_global_cost_state(|cost_state| {
        let model_usage = cost_state
            .model_usage
            .entry(model.to_string())
            .or_insert_with(|| ModelUsageInfo {
                input_tokens: 0,
                output_tokens: 0,
                cache_read_input_tokens: 0,
                cache_creation_input_tokens: 0,
                web_search_requests: 0,
                cost_usd: 0.0,
                context_window: 0,
                max_output_tokens: 0,
            });

        model_usage.input_tokens += input_tokens;
        model_usage.output_tokens += output_tokens;
        model_usage.cache_read_input_tokens += cache_read_input_tokens;
        model_usage.cache_creation_input_tokens += cache_creation_input_tokens;
        model_usage.web_search_requests += web_search_requests;
        model_usage.cost_usd += cost;

        cost_state.total_cost_usd += cost;
        cost_state.total_input_tokens += input_tokens;
        cost_state.total_output_tokens += output_tokens;
        cost_state.total_cache_creation_input_tokens += cache_creation_input_tokens;
        cost_state.total_cache_read_input_tokens += cache_read_input_tokens;
        cost_state.total_web_search_requests += web_search_requests;
    });

    get_global_cost_state()
        .model_usage
        .get(model)
        .cloned()
        .unwrap_or_default()
}

/// Add to total session cost
pub fn add_to_total_session_cost(
    cost: f64,
    input_tokens: u32,
    output_tokens: u32,
    cache_read_input_tokens: u32,
    cache_creation_input_tokens: u32,
    web_search_requests: u32,
    model: &str,
) -> f64 {
    add_to_total_model_usage(
        cost,
        input_tokens,
        output_tokens,
        cache_read_input_tokens,
        cache_creation_input_tokens,
        web_search_requests,
        model,
    );

    cost
}

/// Reset per-turn metrics at the start of a new turn
pub fn reset_turn_metrics() {
    update_global_cost_state(|state| {
        state.turn_tool_duration_ms = 0;
        state.turn_tool_count = 0;
        state.output_tokens_at_turn_start = state.total_output_tokens as u64;
    });
}

/// Record tool execution duration for the current turn
pub fn record_turn_tool_duration(duration_ms: u64) {
    update_global_cost_state(|state| {
        state.turn_tool_duration_ms += duration_ms;
        state.turn_tool_count += 1;
    });
}

/// Get current turn metrics
pub fn get_turn_metrics() -> (u64, u32) {
    let state = get_global_cost_state();
    (state.turn_tool_duration_ms, state.turn_tool_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_costs_input() {
        let costs = COST_TIER_3_15;
        assert_eq!(costs.input_cost(1_000_000), 3.0);
        assert_eq!(costs.input_cost(500_000), 1.5);
    }

    #[test]
    fn test_model_costs_output() {
        let costs = COST_TIER_3_15;
        assert_eq!(costs.output_cost(1_000_000), 15.0);
    }

    #[test]
    fn test_token_usage_total() {
        let usage = TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            prompt_cache_write_tokens: 25,
            prompt_cache_read_tokens: 75,
        };
        assert_eq!(usage.total(), 250);
    }

    #[test]
    fn test_model_cost_registry() {
        let registry = ModelCostRegistry::new();

        let costs = registry.get("claude-sonnet-4-6");
        assert_eq!(costs.input_tokens, 3.0);

        let costs = registry.get("claude-haiku-4-5");
        assert_eq!(costs.input_tokens, 1.0);
    }

    #[test]
    fn test_model_cost_registry_unknown() {
        let registry = ModelCostRegistry::new();
        let costs = registry.get("unknown-model");
        assert_eq!(costs.input_tokens, COST_DEFAULT.input_tokens);
    }

    #[test]
    fn test_calculate_cost() {
        let usage = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 500_000,
            prompt_cache_write_tokens: 0,
            prompt_cache_read_tokens: 0,
        };

        let cost = calculate_cost("claude-sonnet-4-6", &usage);
        // $3 * 1 + $15 * 0.5 = $3 + $7.50 = $10.50
        assert!((cost - 10.5).abs() < 0.01);
    }

    #[test]
    fn test_format_cost() {
        assert_eq!(format_cost(0.001), "$0.0010");
        assert_eq!(format_cost(0.5), "$0.50");
        assert_eq!(format_cost(1.5), "$1.5000");
    }

    #[test]
    fn test_cost_summary() {
        let usage = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 500_000,
            prompt_cache_write_tokens: 100_000,
            prompt_cache_read_tokens: 200_000,
        };

        let summary = CostSummary::from_usage("claude-sonnet-4-6", &usage);

        // Input: 1M * $3/M = $3
        assert!((summary.input_cost - 3.0).abs() < 0.01);
        // Output: 500K * $15/M = $7.50
        assert!((summary.output_cost - 7.5).abs() < 0.01);
        // Cache write: 100K * $3.75/M = $0.375
        assert!((summary.cache_write_cost - 0.375).abs() < 0.01);
        // Cache read: 200K * $0.3/M = $0.06
        assert!((summary.cache_read_cost - 0.06).abs() < 0.01);
    }
}
