// Source: /data/home/swei/claudecode/openclaudecode/src/query/tokenBudget.ts
use regex::Regex;
use std::sync::LazyLock;
use std::time::Instant;

static SHORTHAND_START_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*\+(\d+(?:\.\d+)?)\s*(k|m|b)\b").unwrap());

static SHORTHAND_END_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\s\+(\d+(?:\.\d+)?)\s*(k|m|b)\s*[.!?]?\s*$").unwrap()
});

static VERBOSE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:use|spend)\s+(\d+(?:\.\d+)?)\s*(k|m|b)\s*tokens?\b").unwrap()
});

static VERBOSE_RE_G: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:use|spend)\s+(\d+(?:\.\d+)?)\s*(k|m|b)\s*tokens?\b").unwrap()
});

/// Threshold at which we stop continuing (90% of budget)
const COMPLETION_THRESHOLD: f64 = 0.9;
/// After this many continuations with low token production, trigger diminishing returns
const DIMINISHING_RETURNS_THRESHOLD: u64 = 3;
/// Tokens below which we consider a continuation "low production"
const LOW_PRODUCTION_TOKENS: u64 = 500;

fn parse_budget_match(value: &str, suffix: &str) -> f64 {
    let value: f64 = value.parse().unwrap_or(0.0);
    let multiplier = match suffix.to_lowercase().as_str() {
        "k" => 1_000.0,
        "m" => 1_000_000.0,
        "b" => 1_000_000_000.0,
        _ => 1.0,
    };
    value * multiplier
}

pub fn parse_token_budget(text: &str) -> Option<f64> {
    if let Some(caps) = SHORTHAND_START_RE.captures(text) {
        let value = caps.get(1).map(|m| m.as_str()).unwrap();
        let suffix = caps.get(2).map(|m| m.as_str()).unwrap();
        return Some(parse_budget_match(value, suffix));
    }

    if let Some(caps) = SHORTHAND_END_RE.captures(text) {
        let value = caps.get(1).map(|m| m.as_str()).unwrap();
        let suffix = caps.get(2).map(|m| m.as_str()).unwrap();
        return Some(parse_budget_match(value, suffix));
    }

    if let Some(caps) = VERBOSE_RE.captures(text) {
        let value = caps.get(1).map(|m| m.as_str()).unwrap();
        let suffix = caps.get(2).map(|m| m.as_str()).unwrap();
        return Some(parse_budget_match(value, suffix));
    }

    None
}

#[derive(Debug)]
pub struct BudgetPosition {
    pub start: usize,
    pub end: usize,
}

pub fn find_token_budget_positions(text: &str) -> Vec<BudgetPosition> {
    let mut positions = Vec::new();

    if let Some(m) = SHORTHAND_START_RE.find(text) {
        let offset = m.start() + m.as_str().len() - m.as_str().trim_start().len();
        positions.push(BudgetPosition {
            start: offset,
            end: m.end(),
        });
    }

    if let Some(m) = SHORTHAND_END_RE.find(text) {
        let end_start = m.start() + 1;
        let already_covered = positions
            .iter()
            .any(|p| end_start >= p.start && end_start < p.end);
        if !already_covered {
            positions.push(BudgetPosition {
                start: end_start,
                end: m.end(),
            });
        }
    }

    for m in VERBOSE_RE_G.find_iter(text) {
        positions.push(BudgetPosition {
            start: m.start(),
            end: m.end(),
        });
    }

    positions
}

pub fn get_budget_continuation_message(pct: f64, turn_tokens: u64, budget: f64) -> String {
    format!(
        "Stopped at {pct}% of token target ({turn_tokens} / {budget}). Keep working \u{2014} do not summarize."
    )
}

/// Tracker state for a single query loop's token budget.
/// One tracker per query loop iteration, created when TOKEN_BUDGET is active.
#[derive(Debug)]
pub struct BudgetTracker {
    /// How many times we've continued the loop with a nudge message
    pub continuation_count: u64,
    /// Tokens produced since the last check (delta)
    pub last_delta_tokens: u64,
    /// Global turn tokens at last check
    pub last_global_turn_tokens: u64,
    /// When the tracker was created
    pub started_at: Instant,
}

impl BudgetTracker {
    pub fn new() -> Self {
        Self {
            continuation_count: 0,
            last_delta_tokens: 0,
            last_global_turn_tokens: 0,
            started_at: Instant::now(),
        }
    }
}

impl Default for BudgetTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Completion event emitted when token budget causes the loop to stop.
#[derive(Debug, Clone)]
pub struct TokenBudgetCompletion {
    /// Percentage of budget consumed (0.0-1.0+)
    pub pct: f64,
    /// Total turn tokens consumed
    pub tokens: u64,
    /// Budget target
    pub budget: f64,
    /// How many continuations were performed
    pub continuation_count: u64,
    /// Duration in milliseconds since tracker started
    pub duration_ms: u64,
    /// Whether diminishing returns triggered the stop
    pub diminishing_returns: bool,
}

/// Decision from `check_token_budget()`.
#[derive(Debug)]
pub enum TokenBudgetDecision {
    /// Continue the loop with a nudge message injected.
    Continue { nudge_message: String },
    /// Stop the loop. Optionally emits a completion event for telemetry.
    Stop { completion: Option<TokenBudgetCompletion> },
}

/// Check whether the token budget allows the query loop to continue.
pub fn check_token_budget(
    tracker: &mut BudgetTracker,
    _agent_id: Option<&str>,
    budget: Option<f64>,
    turn_tokens: u64,
) -> TokenBudgetDecision {
    let budget = match budget {
        Some(b) if b > 0.0 => b,
        _ => return TokenBudgetDecision::Stop { completion: None },
    };

    if _agent_id.is_some() {
        return TokenBudgetDecision::Stop { completion: None };
    }

    let current_delta = if turn_tokens >= tracker.last_global_turn_tokens {
        turn_tokens - tracker.last_global_turn_tokens
    } else {
        turn_tokens
    };

    let diminishing_returns = tracker.continuation_count >= DIMINISHING_RETURNS_THRESHOLD
        && current_delta < LOW_PRODUCTION_TOKENS
        && tracker.last_delta_tokens < LOW_PRODUCTION_TOKENS;

    let pct = if budget > 0.0 {
        (turn_tokens as f64 / budget)
    } else {
        1.0
    };

    if pct < COMPLETION_THRESHOLD && !diminishing_returns {
        tracker.continuation_count += 1;
        tracker.last_delta_tokens = current_delta;
        tracker.last_global_turn_tokens = turn_tokens;
        return TokenBudgetDecision::Continue {
            nudge_message: get_budget_continuation_message((pct * 100.0) as u64 as f64, turn_tokens, budget),
        };
    }

    let completion = if tracker.continuation_count > 0 || diminishing_returns {
        Some(TokenBudgetCompletion {
            pct,
            tokens: turn_tokens,
            budget,
            continuation_count: tracker.continuation_count,
            duration_ms: tracker.started_at.elapsed().as_millis() as u64,
            diminishing_returns,
        })
    } else {
        None
    };

    TokenBudgetDecision::Stop { completion }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_token_budget_shorthand_start() {
        assert_eq!(parse_token_budget("+500k"), Some(500_000.0));
        assert_eq!(parse_token_budget("+2m"), Some(2_000_000.0));
        assert_eq!(parse_token_budget("+1.5b"), Some(1_500_000_000.0));
    }

    #[test]
    fn test_parse_token_budget_shorthand_end() {
        assert_eq!(parse_token_budget("I want +500k."), Some(500_000.0));
    }

    #[test]
    fn test_parse_token_budget_verbose() {
        assert_eq!(parse_token_budget("use 2M tokens"), Some(2_000_000.0));
        assert_eq!(parse_token_budget("spend 500k tokens"), Some(500_000.0));
    }

    #[test]
    fn test_parse_token_budget_none() {
        assert!(parse_token_budget("hello world").is_none());
    }

    #[test]
    fn test_find_positions() {
        let positions = find_token_budget_positions("+500k");
        assert!(!positions.is_empty());
    }

    #[test]
    fn test_budget_continuation_message() {
        let msg = get_budget_continuation_message(80.0, 160000, 200000.0);
        assert!(msg.contains("80%"));
        assert!(msg.contains("160000"));
        assert!(msg.contains("200000"));
    }

    #[test]
    fn test_budget_tracker_new() {
        let t = BudgetTracker::new();
        assert_eq!(t.continuation_count, 0);
        assert_eq!(t.last_delta_tokens, 0);
    }

    #[test]
    fn test_check_no_budget() {
        let mut t = BudgetTracker::new();
        let d = check_token_budget(&mut t, None, None, 100);
        assert!(matches!(d, TokenBudgetDecision::Stop { completion: None }));

        let d2 = check_token_budget(&mut t, None, Some(0.0), 100);
        assert!(matches!(d2, TokenBudgetDecision::Stop { completion: None }));
    }

    #[test]
    fn test_check_subagent_skips_budget() {
        let mut t = BudgetTracker::new();
        let d = check_token_budget(&mut t, Some("sub1"), Some(5_000.0), 100);
        assert!(matches!(d, TokenBudgetDecision::Stop { completion: None }));
    }

    #[test]
    fn test_check_continue_under_threshold() {
        let mut t = BudgetTracker::new();
        let d = check_token_budget(&mut t, None, Some(5_000.0), 100);
        match d {
            TokenBudgetDecision::Continue { nudge_message } => {
                assert!(nudge_message.contains("Keep working"));
            }
            other => panic!("Expected Continue, got {:?}", other),
        }
        assert_eq!(t.continuation_count, 1);
    }

    #[test]
    fn test_check_stop_over_threshold() {
        let mut t = BudgetTracker::new();
        let d = check_token_budget(&mut t, None, Some(5_000.0), 5_000);
        match d {
            TokenBudgetDecision::Stop { completion } => {
                assert!(completion.is_none());
            }
            other => panic!("Expected Stop, got {:?}", other),
        }
    }

    #[test]
    fn test_check_continuation_then_stop() {
        let mut t = BudgetTracker::new();
        let d = check_token_budget(&mut t, None, Some(5_000.0), 100);
        assert!(matches!(d, TokenBudgetDecision::Continue { .. }));

        let d = check_token_budget(&mut t, None, Some(5_000.0), 4_800);
        match d {
            TokenBudgetDecision::Stop { completion } => {
                let c = completion.expect("should have completion");
                assert!(c.pct >= 0.9);
                assert_eq!(c.continuation_count, 1);
                assert!(!c.diminishing_returns);
            }
            other => panic!("Expected Stop, got {:?}", other),
        }
    }

    #[test]
    fn test_check_diminishing_returns() {
        let mut t = BudgetTracker::new();
        for _ in 0..3 {
            let tokens = t.last_global_turn_tokens + 100;
            let d = check_token_budget(&mut t, None, Some(10_000.0), tokens);
            assert!(matches!(d, TokenBudgetDecision::Continue { .. }));
        }
        let tokens = t.last_global_turn_tokens + 100;
        let d = check_token_budget(&mut t, None, Some(10_000.0), tokens);
        match d {
            TokenBudgetDecision::Stop { completion } => {
                let c = completion.expect("should have completion");
                assert!(c.diminishing_returns);
            }
            TokenBudgetDecision::Continue { .. } => {
                panic!("Expected Stop due to diminishing returns");
            }
        }
    }
}
