//! Fission TUI — interactive AI chat interface.

pub mod app;
pub mod events;
pub mod ui;

mod tui_runner;

pub use tui_runner::run_tui;
