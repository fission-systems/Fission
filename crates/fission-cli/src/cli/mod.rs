//! CLI Module
//!
//! Unified command-line interface for Fission binary analysis.
//! This module provides three CLI modes:
//! - One-shot: Single command execution (fission_cli binary)
//! - Interactive: REPL-style interface (fission --cli binary)
//! - TUI: Terminal UI with visual browsing (fission_tui binary - separate feature)

pub mod interactive;
pub mod oneshot;
pub mod output;

#[cfg(feature = "tui")]
pub mod tui;

mod args;

pub use args::{OneShotArgs, parse_hex_address};
pub use interactive::{CliRunArgs, run_cli_with_args};
pub use oneshot::run_oneshot;

#[cfg(feature = "tui")]
pub use tui::run_tui;
