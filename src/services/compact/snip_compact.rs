pub fn snip_compact_if_needed<T>(messages: T, _options: Option<()>) -> SnipCompactResult<T> {
    SnipCompactResult {
        messages,
        changed: false,
        tokens_freed: 0,
    }
}

pub fn snip_compact_if_known<T>(messages: T) -> SnipCompactResult<T> {
    SnipCompactResult {
        messages,
        changed: false,
        tokens_freed: 0,
    }
}

#[derive(Debug, Clone)]
pub struct SnipCompactResult<T> {
    pub messages: T,
    pub changed: bool,
    pub tokens_freed: u32,
}
