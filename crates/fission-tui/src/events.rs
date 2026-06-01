//! Event handling: crossterm key events → App actions.

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use std::time::Duration;

/// Actions that the event loop can dispatch to the main loop.
#[derive(Debug)]
pub enum AppAction {
    /// User wants to quit.
    Quit,
    /// A regular printable character was typed.
    InsertChar(char),
    /// Backspace pressed.
    DeleteBack,
    /// Enter pressed — submit the current input.
    Submit,
    /// Arrow-up / scroll up.
    ScrollUp,
    /// Arrow-down / scroll down.
    ScrollDown,
    /// Scroll source pane up.
    ScrollSourceUp,
    /// Scroll source pane down.
    ScrollSourceDown,
    /// Toggle help overlay.
    ToggleHelp,
    /// No-op (e.g. unhandled key or tick timeout).
    Tick,
}

/// Poll for the next terminal event with a 50ms timeout.
/// Returns `Ok(None)` on timeout, `Ok(Some(action))` on a key event.
pub fn poll_event() -> std::io::Result<Option<AppAction>> {
    if !event::poll(Duration::from_millis(50))? {
        return Ok(Some(AppAction::Tick));
    }

    match event::read()? {
        Event::Key(KeyEvent { code, modifiers, .. }) => Ok(Some(map_key(code, modifiers))),
        _ => Ok(Some(AppAction::Tick)),
    }
}

fn map_key(code: KeyCode, modifiers: KeyModifiers) -> AppAction {
    match code {
        // Quit
        KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => AppAction::Quit,
        KeyCode::Char('q') if modifiers.is_empty() => AppAction::Quit,
        // Help
        KeyCode::Char('?') | KeyCode::F(1) => AppAction::ToggleHelp,
        // Submit
        KeyCode::Enter => AppAction::Submit,
        // Delete
        KeyCode::Backspace => AppAction::DeleteBack,
        // Scroll Source (left pane)
        KeyCode::PageUp => AppAction::ScrollSourceUp,
        KeyCode::PageDown => AppAction::ScrollSourceDown,
        KeyCode::Up if modifiers.contains(KeyModifiers::CONTROL) => AppAction::ScrollSourceUp,
        KeyCode::Down if modifiers.contains(KeyModifiers::CONTROL) => AppAction::ScrollSourceDown,
        // Scroll Chat (right pane)
        KeyCode::Up => AppAction::ScrollUp,
        KeyCode::Down => AppAction::ScrollDown,
        // Regular chars
        KeyCode::Char(c) if modifiers.is_empty() || modifiers == KeyModifiers::SHIFT => {
            AppAction::InsertChar(c)
        }
        _ => AppAction::Tick,
    }
}
