//! Fission Error Types
//!
//! Unified error handling for the entire codebase.
//! Provides a custom error enum and Result type alias.

use std::fmt;

/// Result type alias using FissionError
pub type Result<T> = std::result::Result<T, FissionError>;

/// Unified error type for Fission
#[derive(Debug)]
pub enum FissionError {
    /// Binary loading/parsing errors
    Loader(String),
    /// Decompilation errors
    Decompiler(String),
    /// Disassembly errors
    Disassembler(String),
    /// Analysis errors (xrefs, detection, etc.)
    Analysis(String),
    /// Debug/TTD errors
    Debug(String),
    /// Plugin system errors
    Plugin(String),
    /// Script execution errors
    Script(String),
    /// I/O errors
    Io(std::io::Error),
    /// Configuration errors
    Config(String),
    /// UI errors
    Ui(String),
    /// Generic/other errors
    Other(String),
}

impl fmt::Display for FissionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FissionError::Loader(msg) => write!(f, "Loader error: {}", msg),
            FissionError::Decompiler(msg) => write!(f, "Decompiler error: {}", msg),
            FissionError::Disassembler(msg) => write!(f, "Disassembler error: {}", msg),
            FissionError::Analysis(msg) => write!(f, "Analysis error: {}", msg),
            FissionError::Debug(msg) => write!(f, "Debug error: {}", msg),
            FissionError::Plugin(msg) => write!(f, "Plugin error: {}", msg),
            FissionError::Script(msg) => write!(f, "Script error: {}", msg),
            FissionError::Io(err) => write!(f, "I/O error: {}", err),
            FissionError::Config(msg) => write!(f, "Config error: {}", msg),
            FissionError::Ui(msg) => write!(f, "UI error: {}", msg),
            FissionError::Other(msg) => write!(f, "Error: {}", msg),
        }
    }
}

impl std::error::Error for FissionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            FissionError::Io(err) => Some(err),
            _ => None,
        }
    }
}

// Convenience constructors
impl FissionError {
    pub fn loader(msg: impl Into<String>) -> Self {
        FissionError::Loader(msg.into())
    }

    pub fn decompiler(msg: impl Into<String>) -> Self {
        FissionError::Decompiler(msg.into())
    }

    pub fn disassembler(msg: impl Into<String>) -> Self {
        FissionError::Disassembler(msg.into())
    }

    pub fn analysis(msg: impl Into<String>) -> Self {
        FissionError::Analysis(msg.into())
    }

    pub fn debug(msg: impl Into<String>) -> Self {
        FissionError::Debug(msg.into())
    }

    pub fn plugin(msg: impl Into<String>) -> Self {
        FissionError::Plugin(msg.into())
    }

    pub fn script(msg: impl Into<String>) -> Self {
        FissionError::Script(msg.into())
    }

    pub fn config(msg: impl Into<String>) -> Self {
        FissionError::Config(msg.into())
    }

    pub fn ui(msg: impl Into<String>) -> Self {
        FissionError::Ui(msg.into())
    }

    pub fn other(msg: impl Into<String>) -> Self {
        FissionError::Other(msg.into())
    }
}

// From implementations for common error types
impl From<std::io::Error> for FissionError {
    fn from(err: std::io::Error) -> Self {
        FissionError::Io(err)
    }
}

impl From<String> for FissionError {
    fn from(msg: String) -> Self {
        FissionError::Other(msg)
    }
}

impl From<&str> for FissionError {
    fn from(msg: &str) -> Self {
        FissionError::Other(msg.to_string())
    }
}

// Convert from anyhow::Error for gradual migration
impl From<anyhow::Error> for FissionError {
    fn from(err: anyhow::Error) -> Self {
        FissionError::Other(err.to_string())
    }
}

impl From<std::num::ParseIntError> for FissionError {
    fn from(err: std::num::ParseIntError) -> Self {
        FissionError::Other(err.to_string())
    }
}

// Allow implicit conversion from serde_json errors
impl From<serde_json::Error> for FissionError {
    fn from(err: serde_json::Error) -> Self {
        FissionError::Other(err.to_string())
    }
}

// Helper to convert to anyhow::Error (avoids orphan rule conflict)
impl FissionError {
    pub fn to_anyhow(self) -> anyhow::Error {
        anyhow::anyhow!("{}", self)
    }
}

/// Macro for creating errors quickly
///
/// # Examples
/// ```
/// use fission_core::errors::*;
/// use fission_core::err;
///
/// fn example() -> Result<()> {
///     Err(err!(loader, "Failed to parse PE header"))
/// }
/// ```
#[macro_export]
macro_rules! err {
    (loader, $($arg:tt)*) => {
        $crate::errors::FissionError::loader(format!($($arg)*))
    };
    (decompiler, $($arg:tt)*) => {
        $crate::errors::FissionError::decompiler(format!($($arg)*))
    };
    (disassembler, $($arg:tt)*) => {
        $crate::errors::FissionError::disassembler(format!($($arg)*))
    };
    (analysis, $($arg:tt)*) => {
        $crate::errors::FissionError::analysis(format!($($arg)*))
    };
    (debug, $($arg:tt)*) => {
        $crate::errors::FissionError::debug(format!($($arg)*))
    };
    (plugin, $($arg:tt)*) => {
        $crate::errors::FissionError::plugin(format!($($arg)*))
    };
    (script, $($arg:tt)*) => {
        $crate::errors::FissionError::script(format!($($arg)*))
    };
    (config, $($arg:tt)*) => {
        $crate::errors::FissionError::config(format!($($arg)*))
    };
    (ui, $($arg:tt)*) => {
        $crate::errors::FissionError::ui(format!($($arg)*))
    };
    ($($arg:tt)*) => {
        $crate::errors::FissionError::other(format!($($arg)*))
    };
}

/// Macro for early return with error
///
/// # Examples
/// ```
/// use fission_core::errors::*;
/// use fission_core::bail_if;
///
/// fn example(data: Option<&[u8]>) -> Result<()> {
///     let bytes = bail_if!(data.is_none(), loader, "No data provided");
///     Ok(())
/// }
/// ```
#[macro_export]
macro_rules! bail_if {
    ($cond:expr, $kind:ident, $($arg:tt)*) => {
        if $cond {
            return Err($crate::err!($kind, $($arg)*));
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = FissionError::loader("Failed to parse");
        assert!(err.to_string().contains("Loader error"));
    }

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: FissionError = io_err.into();
        assert!(matches!(err, FissionError::Io(_)));
    }

    #[test]
    fn test_result_alias() {
        fn example_fn() -> Result<i32> {
            Ok(42)
        }
        let Ok(value) = example_fn() else {
            panic!("example_fn should return Ok")
        };
        assert_eq!(value, 42);
    }
}
