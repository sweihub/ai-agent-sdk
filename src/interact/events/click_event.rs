// Source: ~/claudecode/openclaudecode/src/ink/events/click-event.ts

use super::event::Event;

/// Mouse click event. Fired on left-button release without drag, only when
/// mouse tracking is enabled (i.e. inside <AlternateScreen>).
///
/// Bubbles from the deepest hit node up through parentNode. Call
/// stop_immediate_propagation() to prevent ancestors' on_click from firing.
#[derive(Debug, Clone)]
pub struct ClickEvent {
    /// 0-indexed screen column of the click
    pub col: u32,
    /// 0-indexed screen row of the click
    pub row: u32,
    /// Click column relative to the current handler's Box (col - box.x).
    /// Recomputed by dispatch_click before each handler fires, so an on_click
    /// on a container sees coords relative to that container, not to any
    /// child the click landed on.
    pub local_col: u32,
    /// Click row relative to the current handler's Box (row - box.y).
    pub local_row: u32,
    /// True if the clicked cell has no visible content (unwritten in the
    /// screen buffer — both packed words are 0). Handlers can check this to
    /// ignore clicks on blank space to the right of text, so accidental
    /// clicks on empty terminal space don't toggle state.
    pub cell_is_blank: bool,
    /// Base event for propagation control
    base: Event,
}

impl ClickEvent {
    pub fn new(col: u32, row: u32, cell_is_blank: bool) -> Self {
        Self {
            col,
            row,
            local_col: 0,
            local_row: 0,
            cell_is_blank,
            base: Event::new(),
        }
    }

    pub fn did_stop_immediate_propagation(&self) -> bool {
        self.base.did_stop_immediate_propagation()
    }

    pub fn stop_immediate_propagation(&self) {
        self.base.stop_immediate_propagation();
    }
}