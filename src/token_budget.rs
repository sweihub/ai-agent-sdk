// Source: /data/home/swei/claudecode/openclaudecode/src/query/tokenBudget.ts
use regex::Regex;
use std::time::Instant;

const SHORTHAND_START_RE: &str = r"^\s*\+(\d+(?:\.\d+)?)\s*(k|m|b)\b";
const SHORTHAND_END_RE: &str = r"\s\+(\d+(?:\.\d+)?)\s*(k|m|b)\s*[.!?]?\s*$";
const VERBOSE_RE: &str = r"\b(?:use|spend)\s+(\d+(?:\.\d+)?)\s*(k|m|b)\s*tokens?\b";

const MULTIPLIERS: &[(&str, u64); 3] = &[("k", 1_000), ("m", 1_000_000), ("b", 1_000_000_000)];

/// Threshold at which we stop continuing (90% of budget)
const COMPLETION_THRESHOLD: f64 = 0.9;
/// After this many continuations with low token production, trigger diminishing returns
const DIMINISHING_RETURNS_THRESHOLD: u64 = 3;
/// Tokens below which we consider a continuation "low production"
const LOW_PRODUCTION_TOKENS: u64 = 500;

fn get_multiplier(suffix: &str) -> u64 {
    for (c, m) in MULTIPLIERS {
        if c.eq_ignore_ascii_case(suffix) {
            return *m;
        }
    }
    1
}

fn parse_budget_match(value: &str, suffix: &str) -> u64 {
    let parsed: f64 = value.parse().unwrap_or(0.0);
    (parsed * get_multiplier(suffix) as f64) as u64
}

pub fn parse_token_budget(text: &str) -> Option<u64> {
    let re_start = Regex::new(SHORTHAND_START_RE).unwrap();
    if let Some(caps) = re_start.captures(text) {
        return Some(parse_budget_match(&caps[1], &caps[2]));
    }

    let re_end = Regex::new(SHORTHAND_END_RE).unwrap();
    if let Some(caps) = re_end.captures(text) {
        return Some(parse_budget_match(&caps[1], &caps[2]));
    }

    let re_verbose = Regex::new(VERBOSE_RE).unwrap();
    if let Some(caps) = re_verbose.captures(text) {
        return Some(parse_budget_match(&caps[1], &caps[2]));
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

    let re_start = Regex::new(SHORTHAND_START_RE).unwrap();
    if let Some(m) = re_start.find(text) {
        let offset = m.start() + m.as_str().len() - m.as_str().trim_start().len();
        positions.push(BudgetPosition {
            start: offset,
            end: m.end(),
        });
    }

    let re_end = Regex::new(SHORTHAND_END_RE).unwrap();
    if let Some(m) = re_end.find(text) {
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

    let re_verbose_g = Regex::new(&format!("{}g", VERBOSE_RE)).unwrap();
    for m in re_verbose_g.find_iter(text) {
        positions.push(BudgetPosition {
            start: m.start(),
            end: m.end(),
        });
    }

    positions
}

pub fn get_budget_continuation_message(pct_display: u64, turn_tokens: u64, budget: u64) -> String {
    format!(
        "Stopped at {}% of token target ({} / {}). Keep working — do not summarize.",
        pct_display, turn_tokens, budget
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
    pub budget: u64,
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
///
/// * `tracker` — mutable BudgetTracker shared across checks
/// * `_agent_id` — if Some, we're in a subagent context; budget check is skipped
/// * `budget` — the token budget target, or None if not set
/// * `turn_tokens` — total tokens consumed in the current turn
///
/// When the budget is under 90%, a nudge message is returned and the loop continues.
/// Once over 90% or diminishing returns are detected, the loop stops gracefully.
pub fn check_token_budget(
    tracker: &mut BudgetTracker,
    _agent_id: Option<&str>,
    budget: Option<u64>,
    turn_tokens: u64,
) -> TokenBudgetDecision {
    let budget = match budget {
        Some(b) if b > 0 => b,
        _ => return TokenBudgetDecision::Stop { completion: None },
    };

    // Subagents don't enforce token budget
    if _agent_id.is_some() {
        return TokenBudgetDecision::Stop { completion: None };
    }

    let current_delta = if turn_tokens >= tracker.last_global_turn_tokens {
        turn_tokens - tracker.last_global_turn_tokens
    } else {
        turn_tokens
    };

    // Check diminishing returns: 3+ continuations, both current and previous delta low
    let diminishing_returns = tracker.continuation_count >= DIMINISHING_RETURNS_THRESHOLD
        && current_delta < LOW_PRODUCTION_TOKENS
        && tracker.last_delta_tokens < LOW_PRODUCTION_TOKENS;

    let pct = if budget > 0 {
        (turn_tokens as f64 / budget as f64)
    } else {
        1.0
    };
    let pct_display = (pct * 100.0) as u64;

    // If under 90% and not diminishing returns, continue with nudge
    if pct < COMPLETION_THRESHOLD && !diminishing_returns {
        tracker.continuation_count += 1;
        tracker.last_delta_tokens = current_delta;
        tracker.last_global_turn_tokens = turn_tokens;
        return TokenBudgetDecision::Continue {
            nudge_message: get_budget_continuation_message(pct_display, turn_tokens, budget),
        };
    }

    // Stop: we've either exceeded 90% or hit diminishing returns
    // If we've already continued at least once, emit a completion event
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
        assert_eq!(parse_token_budget("+5k tokens"), Some(5_000));
        assert_eq!(parse_token_budget("+2m"), Some(2_000_000));
        assert_eq!(parse_token_budget("+1.5b"), Some(1_500_000_000));
    }

    #[test]
    fn test_parse_token_budget_shorthand_end() {
        assert_eq!(parse_token_budget("do this +3k"), Some(3_000));
        assert_eq!(parse_token_budget("keep going +1m!"), Some(1_000_000));
    }

    #[test]
    fn test_parse_token_budget_verbose() {
        assert_eq!(parse_token_budget("use 4k tokens"), Some(4_000));
        assert_eq!(parse_token_budget("spend 2m tokens"), Some(2_000_000));
    }

    #[test]
    fn test_parse_token_budget_none() {
        assert_eq!(parse_token_budget("hello world"), None);
        assert_eq!(parse_token_budget(""), None);
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

        let d2 = check_token_budget(&mut t, None, Some(0), 100);
        assert!(matches!(d2, TokenBudgetDecision::Stop { completion: None }));
    }

    #[test]
    fn test_check_subagent_skips_budget() {
        let mut t = BudgetTracker::new();
        let d = check_token_budget(&mut t, Some("sub1"), Some(5_000), 100);
        assert!(matches!(d, TokenBudgetDecision::Stop { completion: None }));
    }

    #[test]
    fn test_check_continue_under_threshold() {
        let mut t = BudgetTracker::new();
        // 100 / 5000 = 2%, well under 90%
        let d = check_token_budget(&mut t, None, Some(5_000), 100);
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
        // 5000 / 5000 = 100%, over 90%
        let d = check_token_budget(&mut t, None, Some(5_000), 5_000);
        match d {
            TokenBudgetDecision::Stop { completion } => {
                // First check, no continuations yet -> no completion event
                assert!(completion.is_none());
            }
            other => panic!("Expected Stop, got {:?}", other),
        }
    }

    #[test]
    fn test_check_continuation_then_stop() {
        let mut t = BudgetTracker::new();
        // First: under 90% -> continue
        let d = check_token_budget(&mut t, None, Some(5_000), 100);
        assert!(matches!(d, TokenBudgetDecision::Continue { .. }));

        // Second: over 90% -> stop with completion
        let d = check_token_budget(&mut t, None, Some(5_000), 4_800);
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
        // Simulate 3 continuations with low deltas
        for i in 0..3 {
            let tokens = t.last_global_turn_tokens + 100;
            let d = check_token_budget(&mut t, None, Some(10_000), tokens);
            assert!(matches!(d, TokenBudgetDecision::Continue { .. }), "iteration {} should continue", i);
        }
        // 4th continuation with low delta -> diminishing returns
        let tokens = t.last_global_turn_tokens + 100;
        let d = check_token_budget(&mut t, None, Some(10_000), tokens);
        match d {
            TokenBudgetDecision::Stop { completion } => {
                let c = completion.expect("should have completion");
                assert!(c.diminishing_returns);
            }
            TokenBudgetDecision::Continue { nudge_message } => {
                // Still under 90% but diminishing returns triggered after 3 low deltas
                // Actually the logic: diminishing returns = count >= 3 && both deltas < 500
                // At this point count=3, current_delta=100, last_delta=100 -> diminishing returns
                // But this was returned as Continue because it checked before diminishing returns
                // Let me check: the 4th call sets count=3 (after 3 continuations)
                // The 5th call should trigger diminishing returns
                // Actually let's count: after 3 calls, continuation_count=3
                // On the 4th call: diminishing_returns = 3 >= 3 && 100 < 500 && 100 < 500 = true
                // So pct (400/10000 = 4%) < 0.9 && !diminishing_returns(false) = false
                // -> Stop path
                panic!("Expected Stop due to diminishing returns, got Continue: {}", nudge_message);
            }
        }
    }

    #[test]
    fn test_budget_continuation_message() {
        let msg = get_budget_continuation_message(75, 3750, 5000);
        assert!(msg.contains("75%"));
        assert!(msg.contains("3750"));
        assert!(msg.contains("5000"));
        assert!(msg.contains("Keep working"));
    }
}
