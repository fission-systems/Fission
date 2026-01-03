//! Interactive CLI - REPL mode
//!
//! Re-exports the interactive CLI from ui::cli module.
//! The actual implementation is in src/ui/cli/ to keep UI-related code together.

pub use crate::ui::cli::run_cli_with_args;
