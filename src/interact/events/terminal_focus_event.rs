// Source: ~/claudecode/openclaudecode/src/ink/events/terminal-focus-event.ts

use super::event::Event;

/// Event fired when the terminal window gains or loses focus.
///
/// Uses DECSET 1004 focus reporting - the terminal sends:
/// - CSI I (\x1b[I) when the terminal gains focus
/// - CSI O (\x1b[O) when the terminal loses focus
#[derive(Debug, Clone, PartialEq)]
pub enum TerminalFocusEventType {
    TerminalFocus,
    TerminalBlur,
}

impl TerminalFocusEventType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "terminalfocus" => Some(TerminalFocusEventType::TerminalFocus),
            "terminalblur" => Some(TerminalFocusEventType::TerminalBlur),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            TerminalFocusEventType::TerminalFocus => "terminalfocus",
            TerminalFocusEventType::TerminalBlur => "terminalblur",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TerminalFocusEvent {
    pub event_type: TerminalFocusEventType,
    base: Event,
}

impl TerminalFocusEvent {
    pub fn new(event_type: TerminalFocusEventType) -> Self {
        Self {
            event_type,
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