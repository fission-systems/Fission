//! Fission logging and diagnostics utilities.
//!
//! The canonical tracing subscriber is assembled here so CLI, automation, and
//! worker entrypoints all share the same output, file logging, and span-aware
//! error context behavior.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Once, OnceLock};

use crate::config::LogConfig;
use crate::config::LogLevel as ConfigLogLevel;
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_error::{ErrorLayer, SpanTrace};
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::fmt::writer::{BoxMakeWriter, MakeWriterExt};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Registry, fmt, util::SubscriberInitExt};

pub use tracing::Level as LogLevel;

static TRACING_INIT: Once = Once::new();
static FILE_LOGGING_OVERRIDE: OnceLock<PathBuf> = OnceLock::new();
static LOG_GUARDS: OnceLock<Vec<WorkerGuard>> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoggingMode {
    ConsoleOnly,
    ConsoleAndFile,
    FileOnly,
}

impl LoggingMode {
    fn console_enabled(self) -> bool {
        matches!(self, Self::ConsoleOnly | Self::ConsoleAndFile)
    }

    fn file_enabled(self) -> bool {
        matches!(self, Self::ConsoleAndFile | Self::FileOnly)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoggingFormat {
    Compact,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoggingTargets {
    Hidden,
    Visible,
}

impl LoggingTargets {
    fn enabled(self) -> bool {
        matches!(self, Self::Visible)
    }
}

#[derive(Debug, Clone)]
pub struct LoggingOptions {
    pub level: String,
    pub mode: LoggingMode,
    pub format: LoggingFormat,
    pub file_path: Option<PathBuf>,
    pub targets: LoggingTargets,
    pub include_timestamp: bool,
    pub include_span_events: bool,
}

impl LoggingOptions {
    pub fn new(level: impl Into<String>) -> Self {
        Self {
            level: level.into(),
            mode: LoggingMode::ConsoleOnly,
            format: LoggingFormat::Compact,
            file_path: None,
            targets: LoggingTargets::Hidden,
            include_timestamp: true,
            include_span_events: false,
        }
    }

    pub fn from_config(config: &LogConfig) -> Self {
        let env_file_path = std::env::var("FISSION_LOG_FILE")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from);
        let file_path = env_file_path.or_else(|| {
            (config.file_enabled && !config.file_path.is_empty())
                .then(|| PathBuf::from(config.file_path.clone()))
        });
        let mode = match (config.console_enabled, file_path.is_some()) {
            (true, true) => LoggingMode::ConsoleAndFile,
            (true, false) => LoggingMode::ConsoleOnly,
            (false, true) => LoggingMode::FileOnly,
            (false, false) => LoggingMode::ConsoleOnly,
        };
        Self {
            level: config_log_level_to_directive(config.level).to_string(),
            mode,
            format: LoggingFormat::Compact,
            file_path,
            targets: if config.include_target {
                LoggingTargets::Visible
            } else {
                LoggingTargets::Hidden
            },
            include_timestamp: config.include_timestamp,
            include_span_events: false,
        }
    }

    pub fn with_file_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.file_path = Some(path.into());
        self.mode = match self.mode {
            LoggingMode::ConsoleOnly => LoggingMode::ConsoleAndFile,
            LoggingMode::ConsoleAndFile => LoggingMode::ConsoleAndFile,
            LoggingMode::FileOnly => LoggingMode::FileOnly,
        };
        self
    }
}

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

fn resolve_file_path(options: &LoggingOptions) -> Option<PathBuf> {
    options
        .file_path
        .clone()
        .or_else(|| FILE_LOGGING_OVERRIDE.get().cloned())
}

fn span_events(enabled: bool) -> FmtSpan {
    if enabled {
        FmtSpan::NEW | FmtSpan::CLOSE
    } else {
        FmtSpan::NONE
    }
}

fn build_file_writer(path: &Path) -> io::Result<(NonBlocking, WorkerGuard)> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir)?;
    let file_name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "log file path has no name"))?;
    let appender = tracing_appender::rolling::daily(dir, file_name.to_string_lossy().to_string());
    Ok(tracing_appender::non_blocking(appender))
}

fn build_make_writer(options: &LoggingOptions, guards: &mut Vec<WorkerGuard>) -> BoxMakeWriter {
    let file_writer = if options.mode.file_enabled() {
        resolve_file_path(options).and_then(|path| match build_file_writer(&path) {
            Ok((writer, guard)) => {
                guards.push(guard);
                Some(writer)
            }
            Err(error) => {
                eprintln!(
                    "[fission-core] file logging disabled: {} ({error})",
                    path.display()
                );
                None
            }
        })
    } else {
        None
    };

    match (options.mode.console_enabled(), file_writer) {
        (true, Some(file)) => BoxMakeWriter::new(std::io::stderr.and(file)),
        (true, None) => BoxMakeWriter::new(std::io::stderr),
        (false, Some(file)) => BoxMakeWriter::new(file),
        (false, None) => BoxMakeWriter::new(std::io::sink),
    }
}

/// Build an [`EnvFilter`] from `RUST_LOG` when set and parseable; otherwise `default_filter`
/// (e.g. `"warn"`, `"info"`).
pub fn env_filter_from_rust_log_or_default(default_filter: &str) -> EnvFilter {
    match std::env::var("RUST_LOG") {
        Ok(s) if !s.is_empty() => {
            EnvFilter::try_new(&s).unwrap_or_else(|_| EnvFilter::new(default_filter))
        }
        _ => EnvFilter::new(default_filter),
    }
}

/// Idempotent tracing subscriber setup (first successful init wins).
pub fn init_with_options(options: LoggingOptions) {
    TRACING_INIT.call_once(move || {
        let env_filter = env_filter_from_rust_log_or_default(&options.level);
        let mut guards = Vec::new();
        let writer = build_make_writer(&options, &mut guards);
        let _ = LOG_GUARDS.set(guards);
        let show_target = options.targets.enabled();
        let span_events = span_events(options.include_span_events);

        let registry = Registry::default().with(env_filter).with(ErrorLayer::default());
        let _ = match (options.format, options.include_timestamp) {
            (LoggingFormat::Compact, true) => registry
                .with(
                    fmt::layer()
                        .compact()
                        .with_ansi(!matches!(options.mode, LoggingMode::FileOnly))
                        .with_target(show_target)
                        .with_span_events(span_events)
                        .with_writer(writer),
                )
                .try_init(),
            (LoggingFormat::Compact, false) => registry
                .with(
                    fmt::layer()
                        .compact()
                        .with_ansi(!matches!(options.mode, LoggingMode::FileOnly))
                        .with_target(show_target)
                        .with_span_events(span_events)
                        .without_time()
                        .with_writer(writer),
                )
                .try_init(),
            (LoggingFormat::Json, true) => registry
                .with(
                    fmt::layer()
                        .json()
                        .with_ansi(false)
                        .with_target(show_target)
                        .with_span_events(span_events)
                        .with_writer(writer),
                )
                .try_init(),
            (LoggingFormat::Json, false) => registry
                .with(
                    fmt::layer()
                        .json()
                        .with_ansi(false)
                        .with_target(show_target)
                        .with_span_events(span_events)
                        .without_time()
                        .with_writer(writer),
                )
                .try_init(),
        };
    });
}

/// Idempotent tracing setup that honors `RUST_LOG` and uses compact console output.
pub fn try_init_tracing(default_filter: &str) {
    init_with_options(LoggingOptions::new(default_filter));
}

/// Initialize the logger with a minimum log level.
pub fn init(level: LogLevel) {
    try_init_tracing(tracing_level_to_directive(level));
}

/// Initialize logger from [`LogConfig`].
pub fn init_from_config(config: &LogConfig) {
    if let Some((key, value)) = config.get_cpp_log_file_env() {
        unsafe { std::env::set_var(key, value) };
    }
    init_with_options(LoggingOptions::from_config(config));
}

/// Initialize logger using global [`crate::CONFIG`].
pub fn init_from_global_config() {
    init_from_config(&crate::CONFIG.logging);
}

/// Configure a file sink before the shared subscriber is initialized.
pub fn enable_file_logging(path: &str) -> io::Result<()> {
    FILE_LOGGING_OVERRIDE
        .set(PathBuf::from(path))
        .map_err(|_| io::Error::new(io::ErrorKind::AlreadyExists, "file logging already configured"))
}

/// Dynamic post-init file logging is intentionally unsupported.
pub fn disable_file_logging() {}

/// Capture the current [`SpanTrace`] for boundary error rendering.
pub fn capture_span_trace() -> SpanTrace {
    SpanTrace::capture()
}

// Legacy wrappers kept for backward compatibility.
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
    fn env_filter_falls_back_when_rust_log_empty() {
        unsafe { std::env::set_var("RUST_LOG", "") };
        let filter = env_filter_from_rust_log_or_default("warn");
        assert_eq!(format!("{filter:?}"), format!("{:?}", EnvFilter::new("warn")));
        unsafe { std::env::remove_var("RUST_LOG") };
    }

    #[test]
    fn logging_init_is_idempotent() {
        init_with_options(LoggingOptions::new("warn"));
        init_with_options(LoggingOptions::new("info"));
    }

    #[test]
    fn capture_span_trace_is_available() {
        init_with_options(LoggingOptions::new("info"));
        let span = tracing::info_span!("logging_test_span");
        let _entered = span.enter();
        let trace = capture_span_trace();
        assert!(!format!("{trace:?}").is_empty());
    }
}
