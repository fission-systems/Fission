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
    /// Delete pressed.
    Delete,
    /// Enter pressed — submit the current input.
    Submit,
    /// Arrow-up / scroll up with mouse (col, row) coordinates.
    ScrollUp(u16, u16),
    /// Arrow-down / scroll down with mouse (col, row) coordinates.
    ScrollDown(u16, u16),
    /// Toggle horizontal/vertical split in code explorer.
    ToggleSplitDirection,
    /// Toggle help overlay.
    ToggleHelp,
    /// Toggle provider menu (Ctrl+P).
    ToggleProviderMenu,
    /// Toggle model menu (Ctrl+O).
    ToggleModelMenu,
    /// Toggle history menu (Ctrl+H).
    ToggleHistoryMenu,
    /// Cycle to the next provider (Ctrl+Right)
    CycleProviderNext,
    /// Cycle to the prev provider (Ctrl+Left)
    CycleProviderPrev,
    /// Toggle Agent Mode (Tab)
    ToggleMode,
    /// Toggle view between Chat and Code Explorer (Ctrl+Tab / F2).
    ToggleViewMode,
    /// Toggle focused panel within Code Explorer (Tab when in code explorer).
    TogglePanel,
    /// Cycle UI pane focus (Tab/ShiftTab outside of explorer)
    CycleFocus,
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
        Event::Key(key_event)
            if key_event.kind == crossterm::event::KeyEventKind::Press
                || key_event.kind == crossterm::event::KeyEventKind::Repeat =>
        {
            Ok(Some(map_key(key_event.code, key_event.modifiers)))
        }
        Event::Key(_) => Ok(Some(AppAction::Tick)), // ignore Release events
        Event::Resize(w, h) => Ok(Some(AppAction::Resize(w, h))),
        Event::Mouse(mouse_event) => match mouse_event.kind {
            crossterm::event::MouseEventKind::ScrollUp => Ok(Some(AppAction::ScrollUp(
                mouse_event.column,
                mouse_event.row,
            ))),
            crossterm::event::MouseEventKind::ScrollDown => Ok(Some(AppAction::ScrollDown(
                mouse_event.column,
                mouse_event.row,
            ))),
            _ => Ok(Some(AppAction::Tick)),
        },
        _ => Ok(Some(AppAction::Tick)),
    }
}

fn map_key(code: KeyCode, modifiers: KeyModifiers) -> AppAction {
    match code {
        // Quit
        KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => AppAction::Quit,
        // Toggle Provider Menu
        KeyCode::Char('p') if modifiers.contains(KeyModifiers::CONTROL) => {
            AppAction::ToggleProviderMenu
        }
        // Toggle Model Menu (Ctrl+O or Ctrl+M)
        KeyCode::Char('o') if modifiers.contains(KeyModifiers::CONTROL) => {
            AppAction::ToggleModelMenu
        }
        KeyCode::Char('m') if modifiers.contains(KeyModifiers::CONTROL) => {
            AppAction::ToggleModelMenu
        }
        // Toggle History Menu
        KeyCode::Char('h') if modifiers.contains(KeyModifiers::CONTROL) => {
            AppAction::ToggleHistoryMenu
        }
        // Close menus
        KeyCode::Esc => AppAction::Escape,
        // Toggle View Mode (Ctrl+Tab sends BackTab on some terminals; F2 is the explicit binding)
        KeyCode::F(2) => AppAction::ToggleViewMode,
        KeyCode::BackTab if modifiers.contains(KeyModifiers::CONTROL) => AppAction::ToggleViewMode,
        // Toggle Layout Split direction in code explorer
        KeyCode::F(3) => AppAction::ToggleSplitDirection,
        // Tab — context-sensitive
        KeyCode::Tab | KeyCode::BackTab => AppAction::CycleFocus,
        // Help
        KeyCode::Char('?') | KeyCode::F(1) => AppAction::ToggleHelp,
        // Submit or Newline
        KeyCode::Enter if modifiers.contains(KeyModifiers::SHIFT) => AppAction::InsertChar('\n'),
        KeyCode::Enter => AppAction::Submit,
        // Delete
        KeyCode::Backspace => AppAction::DeleteBack,
        KeyCode::Delete => AppAction::Delete,
        // Scroll / Cursor Navigation
        KeyCode::PageUp => AppAction::ScrollUp(0, 0),
        KeyCode::PageDown => AppAction::ScrollDown(0, 0),
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
