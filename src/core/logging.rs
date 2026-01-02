//! Fission Logging Utilities
//!
//! Provides level-based logging using `tracing` ecosystem.
//! Integrates with stdout and file output.
//!
//! Note: This module wraps `tracing` macros in functions to maintain
//! backward compatibility with the existing codebase which uses
//! `logging::info(&format!(...))` patterns.

use once_cell::sync::Lazy;
use std::sync::Mutex;

pub use tracing::Level as LogLevel;
pub use tracing::{
    debug as _debug, error as _error, info as _info, trace as _trace, warn as _warn,
};

/// Initialize the logger with a minimum log level
pub fn init(level: LogLevel) {
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(level)
        .with_target(false) // Don't print module path by default for cleaner output
        .finish();

    let _ = tracing::subscriber::set_global_default(subscriber);
}

pub fn enable_file_logging(_path: &str) -> std::io::Result<()> {
    // Tracing doesn't easily support adding file output *after* init without more complex setup (ReloadLayer).
    // For this step, we'll log a warning that dynamic file logging is limited.
    warn("Dynamic file logging enabling is not fully implemented in tracing migration yet.");
    Ok(())
}

pub fn disable_file_logging() {
    // No-op
}

// Function wrappers for tracing macros
// This allows `logging::info(&format!(...))` to work without changing call sites to macros.

#[track_caller]
pub fn trace(message: &str) {
    tracing::trace!("{}", message);
}

#[track_caller]
pub fn debug(message: &str) {
    tracing::debug!("{}", message);
}

#[track_caller]
pub fn info(message: &str) {
    tracing::info!("{}", message);
}

#[track_caller]
pub fn warn(message: &str) {
    tracing::warn!("{}", message);
}

#[track_caller]
pub fn error(message: &str) {
    tracing::error!("{}", message);
}

/// Convenience macros for logging (optional, if we want to support macro style too)
#[macro_export]
macro_rules! log_trace {
    ($($arg:tt)*) => { tracing::trace!($($arg)*) };
}

#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => { tracing::debug!($($arg)*) };
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => { tracing::info!($($arg)*) };
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => { tracing::warn!($($arg)*) };
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => { tracing::error!($($arg)*) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_wrapper() {
        // Just verify it compiles and runs
        info("Test info log");
        warn(&format!("Test warn log {}", 123));
    }
}
