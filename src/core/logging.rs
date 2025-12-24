//! Fission Logging Utilities
//!
//! Provides level-based logging with optional file output.
//! Integrates with the UI log buffer for display.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use once_cell::sync::Lazy;

/// Log levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Trace = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
}

impl LogLevel {
    pub fn prefix(&self) -> &'static str {
        match self {
            LogLevel::Trace => "[TRACE]",
            LogLevel::Debug => "[DEBUG]",
            LogLevel::Info => "[*]",
            LogLevel::Warn => "[!]",
            LogLevel::Error => "[✗]",
        }
    }
    
    pub fn color_prefix(&self) -> &'static str {
        match self {
            LogLevel::Trace => "\x1b[90m[TRACE]\x1b[0m",
            LogLevel::Debug => "\x1b[36m[DEBUG]\x1b[0m",
            LogLevel::Info => "\x1b[32m[*]\x1b[0m",
            LogLevel::Warn => "\x1b[33m[!]\x1b[0m",
            LogLevel::Error => "\x1b[31m[✗]\x1b[0m",
        }
    }
}

/// Global logger state
struct LoggerState {
    level: LogLevel,
    file: Option<File>,
    file_path: Option<String>,
}

static LOGGER: Lazy<Mutex<LoggerState>> = Lazy::new(|| {
    Mutex::new(LoggerState {
        level: LogLevel::Info,
        file: None,
        file_path: None,
    })
});

/// Initialize the logger with a minimum log level
pub fn init(level: LogLevel) {
    if let Ok(mut state) = LOGGER.lock() {
        state.level = level;
    }
}

/// Enable file logging
pub fn enable_file_logging(path: &str) -> std::io::Result<()> {
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    
    if let Ok(mut state) = LOGGER.lock() {
        state.file = Some(file);
        state.file_path = Some(path.to_string());
    }
    Ok(())
}

/// Disable file logging
pub fn disable_file_logging() {
    if let Ok(mut state) = LOGGER.lock() {
        state.file = None;
        state.file_path = None;
    }
}

/// Get current log level
pub fn get_level() -> LogLevel {
    LOGGER.lock().map(|s| s.level).unwrap_or(LogLevel::Info)
}

/// Set log level
pub fn set_level(level: LogLevel) {
    if let Ok(mut state) = LOGGER.lock() {
        state.level = level;
    }
}

/// Internal log function
pub fn log(level: LogLevel, message: &str) {
    let state = match LOGGER.lock() {
        Ok(s) => s,
        Err(_) => return,
    };
    
    // Check log level
    if level < state.level {
        return;
    }
    
    // Format timestamp
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let formatted = format!("{} {}", level.prefix(), message);
    
    // Print to stderr (with colors)
    eprintln!("{} {}", level.color_prefix(), message);
    
    // Write to file (without colors)
    if let Some(ref file) = state.file {
        let mut file = file;
        let _ = writeln!(file, "[{}] {}", timestamp, formatted);
    }
}

/// Log at trace level
pub fn trace(message: &str) {
    log(LogLevel::Trace, message);
}

/// Log at debug level
pub fn debug(message: &str) {
    log(LogLevel::Debug, message);
}

/// Log at info level
pub fn info(message: &str) {
    log(LogLevel::Info, message);
}

/// Log at warn level
pub fn warn(message: &str) {
    log(LogLevel::Warn, message);
}

/// Log at error level
pub fn error(message: &str) {
    log(LogLevel::Error, message);
}

/// Convenience macros for logging
#[macro_export]
macro_rules! log_trace {
    ($($arg:tt)*) => {
        $crate::logging::trace(&format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => {
        $crate::logging::debug(&format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        $crate::logging::info(&format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        $crate::logging::warn(&format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        $crate::logging::error(&format!($($arg)*))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_levels() {
        assert!(LogLevel::Trace < LogLevel::Debug);
        assert!(LogLevel::Debug < LogLevel::Info);
        assert!(LogLevel::Info < LogLevel::Warn);
        assert!(LogLevel::Warn < LogLevel::Error);
    }

    #[test]
    fn test_prefix() {
        assert_eq!(LogLevel::Info.prefix(), "[*]");
        assert_eq!(LogLevel::Error.prefix(), "[✗]");
    }
}
