//! App module - shared application state and commands
//!
//! Provides common state and command processing used by both CLI and GUI.

mod state;
mod commands;

pub use state::*;
pub use commands::*;
