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

use std::sync::Once;

use crate::config::LogConfig;
use crate::config::LogLevel as ConfigLogLevel;

pub use tracing::Level as LogLevel;

static TRACING_INIT: Once = Once::new();

fn tracing_level_to_directive(level: tracing::Level) -> &'static str {
    match level {
        tracing::Level::TRACE => "trace",
        tracing::Level::DEBUG => "debug",
        tracing::Level::INFO => "info",
        tracing::Level::WARN => "warn",
        tracing::Level::ERROR => "error",
    }
}

fn config_log_level_to_directive(level: ConfigLogLevel) -> &'static str {
    match level {
        ConfigLogLevel::Trace => "trace",
        ConfigLogLevel::Debug => "debug",
        ConfigLogLevel::Info => "info",
        ConfigLogLevel::Warn => "warn",
        ConfigLogLevel::Error => "error",
    }
}

/// Build an [`EnvFilter`] from `RUST_LOG` when set and parseable; otherwise `default_filter`
/// (e.g. `"warn"`, `"info"`).
pub fn env_filter_from_rust_log_or_default(default_filter: &str) -> tracing_subscriber::EnvFilter {
    match std::env::var("RUST_LOG") {
        Ok(s) if !s.is_empty() => tracing_subscriber::EnvFilter::try_new(&s).unwrap_or_else(|_| {
            tracing_subscriber::EnvFilter::new(default_filter)
        }),
        _ => tracing_subscriber::EnvFilter::new(default_filter),
    }
}

/// Idempotent `tracing` subscriber setup (first successful init wins).
///
/// Honors `RUST_LOG` when set and valid. Uses a compact console format without target paths.
pub fn try_init_tracing(default_filter: &str) {
    TRACING_INIT.call_once(|| {
        let env_filter = env_filter_from_rust_log_or_default(default_filter);
        let _ = tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_target(false)
            .try_init();
    });
}

/// Initialize the logger with a minimum log level
pub fn init(level: LogLevel) {
    try_init_tracing(tracing_level_to_directive(level));
}

/// Initialize logger from LogConfig
pub fn init_from_config(config: &LogConfig) {
    // Always apply C++ logger env hint even if the tracing subscriber was already initialized
    // elsewhere (e.g. CLI `try_init_tracing` in the same process).
    if let Some((key, value)) = config.get_cpp_log_file_env() {
        // SAFETY: We're setting an environment variable in a single-threaded init context.
        // The C++ logger will read this value once during its initialization.
        unsafe { std::env::set_var(key, value) };
    }

    let include_target = config.include_target;
    let include_timestamp = config.include_timestamp;
    let default_directive = config_log_level_to_directive(config.level);

    TRACING_INIT.call_once(move || {
        let env_filter = env_filter_from_rust_log_or_default(default_directive);
        let _ = if include_timestamp {
            tracing_subscriber::fmt()
                .with_env_filter(env_filter)
                .with_target(include_target)
                .try_init()
        } else {
            tracing_subscriber::fmt()
                .with_env_filter(env_filter)
                .with_target(include_target)
                .without_time()
                .try_init()
        };
    });
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
