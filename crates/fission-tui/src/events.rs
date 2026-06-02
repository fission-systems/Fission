//! Event handling: crossterm key events → App actions.

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
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
    /// Toggle help overlay.
    ToggleHelp,
    /// Toggle provider menu (Ctrl+P).
    ToggleProviderMenu,
    /// Toggle model menu (Ctrl+O).
    ToggleModelMenu,
    /// Cycle to the next provider (Ctrl+Right)
    CycleProviderNext,
    /// Cycle to the prev provider (Ctrl+Left)
    CycleProviderPrev,
    /// Toggle Agent Mode (Tab)
    ToggleMode,
    /// Escape pressed (to close menus/overlays).
    Escape,
    /// Move cursor left
    CursorLeft,
    /// Move cursor right
    CursorRight,
    /// Move cursor up (multiline or history)
    CursorUp,
    /// Move cursor down (multiline or history)
    CursorDown,
    /// Resize event from terminal.
    Resize(u16, u16),
    /// No-op (e.g. unhandled key or tick timeout).
    Tick,
}

/// Poll for the next terminal event with a 50ms timeout.
/// Returns `Ok(None)` on timeout, `Ok(Some(action))` on a key event.
pub fn poll_event() -> std::io::Result<Option<AppAction>> {
    if !event::poll(Duration::from_millis(100))? {
        return Ok(Some(AppAction::Tick));
    }

    match event::read()? {
        Event::Key(key_event) if key_event.kind == crossterm::event::KeyEventKind::Press
            || key_event.kind == crossterm::event::KeyEventKind::Repeat =>
        {
            Ok(Some(map_key(key_event.code, key_event.modifiers)))
        }
        Event::Key(_) => Ok(Some(AppAction::Tick)), // ignore Release events
        Event::Resize(w, h) => Ok(Some(AppAction::Resize(w, h))),
        _ => Ok(Some(AppAction::Tick)),
    }
}

fn map_key(code: KeyCode, modifiers: KeyModifiers) -> AppAction {
    match code {
        // Quit
        KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => AppAction::Quit,
        // Toggle Provider Menu
        KeyCode::Char('p') if modifiers.contains(KeyModifiers::CONTROL) => AppAction::ToggleProviderMenu,
        // Toggle Model Menu
        KeyCode::Char('o') if modifiers.contains(KeyModifiers::CONTROL) => AppAction::ToggleModelMenu,
        // Close menus
        KeyCode::Esc => AppAction::Escape,
        // Toggle Mode
        KeyCode::Tab => AppAction::ToggleMode,
        // Help
        KeyCode::Char('?') | KeyCode::F(1) => AppAction::ToggleHelp,
        // Submit or Newline
        KeyCode::Enter if modifiers.contains(KeyModifiers::SHIFT) => AppAction::InsertChar('\n'),
        KeyCode::Enter => AppAction::Submit,
        // Delete
        KeyCode::Backspace => AppAction::DeleteBack,
        // Scroll / Cursor Navigation
        KeyCode::PageUp => AppAction::ScrollUp,
        KeyCode::PageDown => AppAction::ScrollDown,
        KeyCode::Up => AppAction::CursorUp,
        KeyCode::Down => AppAction::CursorDown,
        // Cycle Providers
        KeyCode::Right if modifiers.contains(KeyModifiers::CONTROL) => AppAction::CycleProviderNext,
        KeyCode::Left if modifiers.contains(KeyModifiers::CONTROL) => AppAction::CycleProviderPrev,
        // Cursor movement
        KeyCode::Left if modifiers.is_empty() => AppAction::CursorLeft,
        KeyCode::Right if modifiers.is_empty() => AppAction::CursorRight,
        // Regular chars
        KeyCode::Char(c) if modifiers.is_empty() || modifiers == KeyModifiers::SHIFT => {
            AppAction::InsertChar(c)
        }
        _ => AppAction::Tick,
    }
}
