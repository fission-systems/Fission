//! Fission Logging Utilities
//!
//! Provides level-based logging using the `tracing` ecosystem.
//!
//! ## Recommended usage
//!
//! Prefer `tracing` macros directly — they support lazy evaluation (format
//! strings are only evaluated when the level is enabled) and structured fields:
//!
//! ```rust,ignore
//! use tracing::{debug, error, info, warn};
//!
//! info!(addr = %addr, "function decompiled");           // structured field
//! warn!(error = %e, file = %path, "load failed");       // multiple fields
//! debug!("decompiler cache cleared");                   // simple message
//! ```
//!
//! Add `tracing = "0.1"` to the crate's `Cargo.toml` to use the macros
//! directly.
//!
//! ## Legacy usage (backward-compatible)
//!
//! The function wrappers below (`logging::info`, `logging::warn`, …) are kept
//! for backward compatibility.  They always format the message string **before**
//! calling into tracing, even when the log level is disabled — prefer the
//! macro style for hot paths.

use crate::config::LogConfig;

pub use tracing::Level as LogLevel;

/// Initialize the logger with a minimum log level
pub fn init(level: LogLevel) {
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(level)
        .with_target(false) // Don't print module path by default for cleaner output
        .finish();

    let _ = tracing::subscriber::set_global_default(subscriber);
}

/// Initialize logger from LogConfig
pub fn init_from_config(config: &LogConfig) {
    let level = config.level.to_tracing_level();

    // Build subscriber based on config
    // Note: Conditional time format requires different approach due to type differences
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(level)
        .with_target(config.include_target)
        .finish();

    let _ = tracing::subscriber::set_global_default(subscriber);

    // Set environment variable for C++ logger if file logging is enabled
    if let Some((key, value)) = config.get_cpp_log_file_env() {
        // SAFETY: We're setting an environment variable in a single-threaded init context.
        // The C++ logger will read this value once during its initialization.
        unsafe { std::env::set_var(key, value) };
    }
}

/// Initialize logger using global CONFIG
pub fn init_from_global_config() {
    init_from_config(&crate::CONFIG.logging);
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

// ── Legacy function wrappers ─────────────────────────────────────────────────
// These always evaluate the message string before calling tracing, even when
// the log level is disabled.  Prefer `tracing::{debug!, info!, warn!, error!}`
// macros in new code.

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_wrapper() {
        // Just verify the function wrappers compile and dispatch to tracing
        info("Test info log");
        warn(&format!("Test warn log {}", 123));
    }
}
