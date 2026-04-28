// Source: /data/home/swei/claudecode/openclaudecode/src/context.ts
use crate::constants::env::ai;
use once_cell::sync::Lazy;
use regex::Regex;

pub const MODEL_CONTEXT_WINDOW_DEFAULT: u64 = 200_000;
pub const COMPACT_MAX_OUTPUT_TOKENS: u64 = 20_000;

const MAX_OUTPUT_TOKENS_DEFAULT: u64 = 32_000;
const MAX_OUTPUT_TOKENS_UPPER_LIMIT: u64 = 64_000;

pub const CAPPED_DEFAULT_MAX_TOKENS: u64 = 8_000;
pub const ESCALATED_MAX_TOKENS: u64 = 64_000;

static DISABLE_1M_CONTEXT_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\[1m\]").unwrap());

fn is_env_truthy(value: &str) -> bool {
    value == "1" || value.to_lowercase() == "true" || value.to_lowercase() == "yes"
}

fn is_1m_context_disabled() -> bool {
    std::env::var(ai::CODE_DISABLE_1M_CONTEXT)
        .map(|v| is_env_truthy(&v))
        .unwrap_or(false)
}

fn has_1m_context(model: &str) -> bool {
    if is_1m_context_disabled() {
        return false;
    }
    DISABLE_1M_CONTEXT_REGEX.is_match(model)
}

fn get_user_type() -> String {
    std::env::var(ai::USER_TYPE).unwrap_or_default()
}

pub fn model_supports_1m(model: &str) -> bool {
    if is_1m_context_disabled() {
        return false;
    }
    let canonical = get_canonical_name(model);
    canonical.contains("claude-sonnet-4") || canonical.contains("opus-4-6")
}

fn get_canonical_name(model: &str) -> String {
    let m = model.to_lowercase();
    if m.contains("sonnet-4-20250514") || m.contains("sonnet-4-6") {
        return "claude-sonnet-4-6".to_string();
    }
    if m.contains("sonnet-4-20250507") || m.contains("sonnet-4-5") {
        return "claude-sonnet-4-5".to_string();
    }
    if m.contains("sonnet-4") {
        return "claude-sonnet-4".to_string();
    }
    if m.contains("opus-4-20250514") || m.contains("opus-4-6") {
        return "claude-opus-4-6".to_string();
    }
    if m.contains("opus-4-20250501") || m.contains("opus-4-5") {
        return "claude-opus-4-5".to_string();
    }
    if m.contains("opus-4-2") || m.contains("opus-4-1") {
        return "claude-opus-4-1".to_string();
    }
    if m.contains("opus-4") {
        return "claude-opus-4".to_string();
    }
    if m.contains("haiku-4") {
        return "claude-haiku-4".to_string();
    }
    if m.contains("3-7-sonnet") {
        return "claude-3-7-sonnet".to_string();
    }
    if m.contains("3-5-sonnet") || m.contains("sonnet-3-5") {
        return "claude-3-5-sonnet".to_string();
    }
    if m.contains("3-5-haiku") || m.contains("haiku-3-5") {
        return "claude-3-5-haiku".to_string();
    }
    if m.contains("3-opus") || m.contains("opus-3") {
        return "claude-3-opus".to_string();
    }
    if m.contains("3-sonnet") || m.contains("sonnet-3") {
        return "claude-3-sonnet".to_string();
    }
    if m.contains("3-haiku") || m.contains("haiku-3") {
        return "claude-3-haiku".to_string();
    }
    m
}

fn get_model_capability(model: &str) -> Option<ModelCapability> {
    None
}

#[derive(Debug, Clone)]
pub struct ModelCapability {
    pub max_input_tokens: Option<u64>,
    pub max_tokens: Option<u64>,
}

const CONTEXT_1M_BETA_HEADER: &str = "context-1m-2025-08-07";

pub fn get_context_window_for_model(model: &str, betas: Option<&[String]>) -> u64 {
    if get_user_type() == "ant" {
        if let Ok(override_val) = std::env::var(ai::CODE_MAX_CONTEXT_TOKENS) {
            if let Ok(override_num) = override_val.parse::<u64>() {
                if override_num > 0 {
                    return override_num;
                }
            }
        }
    }

    if has_1m_context(model) {
        return 1_000_000;
    }

    if let Some(cap) = get_model_capability(model) {
        if let Some(max_input) = cap.max_input_tokens {
            if max_input >= 100_000 {
                if max_input > MODEL_CONTEXT_WINDOW_DEFAULT && is_1m_context_disabled() {
                    return MODEL_CONTEXT_WINDOW_DEFAULT;
                }
                return max_input;
            }
        }
    }

    if let Some(betas_arr) = betas {
        if betas_arr.iter().any(|b| b == CONTEXT_1M_BETA_HEADER) && model_supports_1m(model) {
            return 1_000_000;
        }
    }

    if get_sonnet_1m_exp_treatment_enabled(model) {
        return 1_000_000;
    }

    MODEL_CONTEXT_WINDOW_DEFAULT
}

fn get_global_config() -> GlobalConfig {
    GlobalConfig::default()
}

#[derive(Debug, Default)]
struct GlobalConfig {
    client_data_cache: Option<std::collections::HashMap<String, String>>,
}

fn get_sonnet_1m_exp_treatment_enabled(model: &str) -> bool {
    if is_1m_context_disabled() {
        return false;
    }
    if has_1m_context(model) {
        return false;
    }
    let canonical = get_canonical_name(model);
    if !canonical.contains("sonnet-4-6") {
        return false;
    }
    let config = get_global_config();
    config
        .client_data_cache
        .as_ref()
        .map(|c| {
            c.get("coral_reef_sonnet")
                .map(|v| v == "true")
                .unwrap_or(false)
        })
        .unwrap_or(false)
}

pub fn calculate_context_percentages(
    current_usage: Option<&ContextUsage>,
    context_window_size: u64,
) -> ContextPercentages {
    let usage = match current_usage {
        Some(u) => u,
        None => {
            return ContextPercentages {
                used: None,
                remaining: None,
            }
        }
    };

    let total_input_tokens =
        usage.input_tokens + usage.cache_creation_input_tokens + usage.cache_read_input_tokens;

    let used_percentage =
        ((total_input_tokens as f64 / context_window_size as f64) * 100.0).round() as u64;
    let clamped_used = used_percentage.min(100).max(0);

    ContextPercentages {
        used: Some(clamped_used),
        remaining: Some(100 - clamped_used),
    }
}

#[derive(Debug, Clone)]
pub struct ContextUsage {
    pub input_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
}

#[derive(Debug, Clone)]
pub struct ContextPercentages {
    pub used: Option<u64>,
    pub remaining: Option<u64>,
}

pub fn get_model_max_output_tokens(model: &str) -> MaxOutputTokens {
    let mut default_tokens = MAX_OUTPUT_TOKENS_DEFAULT;
    let mut upper_limit = MAX_OUTPUT_TOKENS_UPPER_LIMIT;

    let m = get_canonical_name(model);

    if m.contains("opus-4-6") {
        default_tokens = 64_000;
        upper_limit = 128_000;
    } else if m.contains("sonnet-4-6") {
        default_tokens = 32_000;
        upper_limit = 128_000;
    } else if m.contains("opus-4-5") || m.contains("sonnet-4") || m.contains("haiku-4") {
        default_tokens = 32_000;
        upper_limit = 64_000;
    } else if m.contains("opus-4-1") || m.contains("opus-4") {
        default_tokens = 32_000;
        upper_limit = 32_000;
    } else if m.contains("claude-3-opus") {
        default_tokens = 4_096;
        upper_limit = 4_096;
    } else if m.contains("claude-3-sonnet") {
        default_tokens = 8_192;
        upper_limit = 8_192;
    } else if m.contains("claude-3-haiku") {
        default_tokens = 4_096;
        upper_limit = 4_096;
    } else if m.contains("3-5-sonnet") || m.contains("3-5-haiku") {
        default_tokens = 8_192;
        upper_limit = 8_192;
    } else if m.contains("3-7-sonnet") {
        default_tokens = 32_000;
        upper_limit = 64_000;
    }

    if let Some(cap) = get_model_capability(model) {
        if let Some(max_tokens) = cap.max_tokens {
            if max_tokens >= 4_096 {
                upper_limit = max_tokens;
                default_tokens = default_tokens.min(upper_limit);
            }
        }
    }

    MaxOutputTokens {
        default: default_tokens,
        upper_limit,
    }
}

#[derive(Debug, Clone)]
pub struct MaxOutputTokens {
    pub default: u64,
    pub upper_limit: u64,
}

pub fn get_max_thinking_tokens_for_model(model: &str) -> u64 {
    get_model_max_output_tokens(model).upper_limit - 1
}

/// Feature flag: slot-reservation cap for max_tokens.
/// Always enabled (matching TS feature('tengu_otk_slot_v1'), all features enabled).
fn is_max_tokens_cap_enabled() -> bool {
    true
}

/// Resolve the effective max_tokens for a given model.
///
/// TS source: src/services/api/claude.ts `getMaxOutputTokensForModel`
///
/// 1. Get model-specific { default, upperLimit } from `get_model_max_output_tokens`.
/// 2. Apply slot-reservation cap: default = min(default, CAPPED_DEFAULT_MAX_TOKENS).
/// 3. Apply env var override `AI_CODE_MAX_OUTPUT_TOKENS`, bounded to [default, upperLimit].
/// 4. Return effective value.
pub fn get_max_output_tokens_for_model(model: &str) -> u64 {
    use crate::constants::env::ai_code::MAX_OUTPUT_TOKENS as ENV_MAX_OUTPUT_TOKENS;
    use crate::utils::env_validation::validate_bounded_int_env_var;

    let max_output = get_model_max_output_tokens(model);

    let default_tokens = if is_max_tokens_cap_enabled() {
        max_output.default.min(CAPPED_DEFAULT_MAX_TOKENS)
    } else {
        max_output.default
    };

    let env_value = std::env::var(ENV_MAX_OUTPUT_TOKENS).ok();
    let result = validate_bounded_int_env_var(
        ENV_MAX_OUTPUT_TOKENS,
        env_value.as_deref(),
        default_tokens as i64,
        max_output.upper_limit as i64,
    );
    result.effective as u64
}
