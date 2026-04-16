// Source: ~/claudecode/openclaudecode/src/ink/events/input-event.ts

use super::event::Event;

/// Keyboard key representation with all modifier flags and special key indicators.
#[derive(Debug, Clone, Default)]
pub struct Key {
    pub up_arrow: bool,
    pub down_arrow: bool,
    pub left_arrow: bool,
    pub right_arrow: bool,
    pub page_down: bool,
    pub page_up: bool,
    pub wheel_up: bool,
    pub wheel_down: bool,
    pub home: bool,
    pub end: bool,
    pub return_key: bool,
    pub escape: bool,
    pub ctrl: bool,
    pub shift: bool,
    pub fn_key: bool,
    pub tab: bool,
    pub backspace: bool,
    pub delete: bool,
    pub meta: bool,
    pub super_key: bool,
}

impl Key {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if any navigation key (arrows, page up/down, home, end) is pressed.
    pub fn is_navigation(&self) -> bool {
        self.up_arrow
            || self.down_arrow
            || self.left_arrow
            || self.right_arrow
            || self.page_down
            || self.page_up
            || self.home
            || self.end
    }

    /// Returns true if any modifier key (ctrl, shift, alt/meta, super) is pressed.
    pub fn has_modifier(&self) -> bool {
        self.ctrl || self.shift || self.meta || self.super_key
    }
}

/// Input event containing parsed keypress information.
#[derive(Debug, Clone)]
pub struct InputEvent {
    pub keypress: ParsedKey,
    pub key: Key,
    pub input: String,
    base: Event,
}

impl InputEvent {
    pub fn new(keypress: ParsedKey) -> Self {
        let key = parse_key(&keypress);
        let input = compute_input(&keypress);

        Self {
            keypress,
            key,
            input,
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

/// A parsed keypress from terminal input.
#[derive(Debug, Clone)]
pub struct ParsedKey {
    pub kind: &'static str,
    pub name: Option<String>,
    pub fn_key: bool,
    pub ctrl: bool,
    pub meta: bool,
    pub shift: bool,
    pub option: bool,
    pub super_key: bool,
    pub sequence: Option<String>,
    pub raw: Option<String>,
    pub code: Option<String>,
    pub is_pasted: bool,
}

impl ParsedKey {
    pub fn new() -> Self {
        Self {
            kind: "key",
            name: None,
            fn_key: false,
            ctrl: false,
            meta: false,
            shift: false,
            option: false,
            super_key: false,
            sequence: None,
            raw: None,
            code: None,
            is_pasted: false,
        }
    }
}

impl Default for ParsedKey {
    fn default() -> Self {
        Self::new()
    }
}

/// Non-alphanumeric key names that should clear input.
const NON_ALPHANUMERIC_KEYS: &[&str] = &[
    "f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12",
    "up", "down", "left", "right", "clear", "end", "home",
    "insert", "delete", "pageup", "pagedown",
    "escape", "backspace", "wheelup", "wheeldown", "mouse",
];

fn parse_key(keypress: &ParsedKey) -> Key {
    let mut key = Key::new();

    if let Some(ref name) = keypress.name {
        key.up_arrow = name == "up";
        key.down_arrow = name == "down";
        key.left_arrow = name == "left";
        key.right_arrow = name == "right";
        key.page_down = name == "pagedown";
        key.page_up = name == "pageup";
        key.wheel_up = name == "wheelup";
        key.wheel_down = name == "wheeldown";
        key.home = name == "home";
        key.end = name == "end";
        key.return_key = name == "return";
        key.escape = name == "escape";
        key.fn_key = keypress.fn_key;
        key.ctrl = keypress.ctrl;
        key.shift = keypress.shift;
        key.tab = name == "tab";
        key.backspace = name == "backspace";
        key.delete = name == "delete";
        key.meta = keypress.meta || name == "escape" || keypress.option;
        key.super_key = keypress.super_key;
    }

    key
}

fn compute_input(keypress: &ParsedKey) -> String {
    // When ctrl is set, use key name for control characters
    if keypress.ctrl {
        if let Some(ref name) = keypress.name {
            if name == "space" {
                return " ".to_string();
            }
            // Control characters: ctrl+a through ctrl+z map to ASCII 1-26
            if name.len() == 1 {
                let c = name.chars().next().unwrap();
                if c.is_ascii_lowercase() {
                    return ((c as u8 - b'a' + 1) as char).to_string();
                }
            }
        }
    }

    // Handle sequence input
    if let Some(ref seq) = keypress.sequence {
        // Handle escape sequences
        if seq.starts_with('\u{1b}') {
            // Strip leading ESC
            return seq[1..].to_string();
        }
        return seq.clone();
    }

    String::new()
}