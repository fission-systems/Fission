//! Event handling

use std::io;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};

use super::app::App;

/// Handle keyboard events
pub fn handle_events(app: &mut App) -> io::Result<()> {
    if let Event::Key(key) = event::read()? {
        if key.kind == KeyEventKind::Press {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => app.quit(),
                KeyCode::Down | KeyCode::Char('j') => app.select_next(),
                KeyCode::Up | KeyCode::Char('k') => app.select_previous(),
                KeyCode::Enter => app.decompile_selected(),
                KeyCode::PageDown => app.page_down(),
                KeyCode::PageUp => app.page_up(),
                KeyCode::Char('d') => app.scroll_down(),
                KeyCode::Char('u') => app.scroll_up(),
                _ => {}
            }
        }
    }
    Ok(())
}
