// Source: ~/claudecode/openclaudecode/src/ink/events/mod.ts

pub mod event;
pub mod emitter;
pub mod click_event;
pub mod input_event;
pub mod paste_event;
pub mod resize_event;
pub mod terminal_focus_event;

// Re-export types for convenience
pub use event::Event;
pub use emitter::EventEmitter;
pub use click_event::ClickEvent;
pub use input_event::{InputEvent, Key, ParsedKey};
pub use paste_event::PasteEvent;
pub use resize_event::ResizeEvent;
pub use terminal_focus_event::{TerminalFocusEvent, TerminalFocusEventType};