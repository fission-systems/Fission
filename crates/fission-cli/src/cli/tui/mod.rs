//! TUI Module - Terminal User Interface
//!
//! A ratatui-based terminal interface for browsing and decompiling binaries.
//!
//! ## Structure
//! - `app.rs` - Application state and logic
//! - `ui.rs` - UI rendering components
//! - `events.rs` - Event handling

mod app;
mod events;
mod ui;

use std::io::{self, stdout};
use std::time::Duration;

use clap::Parser;
use crossterm::{
    ExecutableCommand, event,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use super::args::TuiArgs;
use fission_loader::loader::LoadedBinary;

pub use app::App;
pub use events::handle_events;
pub use ui::render_ui;

/// Entry point for TUI mode
pub fn run_tui() -> io::Result<()> {
    let cli = TuiArgs::parse();

    // Load binary
    let binary_data = std::fs::read(&cli.binary)?;
    let binary = LoadedBinary::from_bytes(
        binary_data.clone(),
        cli.binary.to_string_lossy().to_string(),
    )
    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

    let mut app = App::new(binary, binary_data);

    // Setup terminal
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    // Main loop
    loop {
        terminal.draw(|frame| render_ui(frame, &mut app))?;

        if event::poll(Duration::from_millis(100))? {
            handle_events(&mut app)?;
        }

        if app.should_quit() {
            break;
        }
    }

    // Cleanup
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}

/// Main entry point (for bin/fission_tui.rs binary)
pub fn main() -> io::Result<()> {
    run_tui()
}
