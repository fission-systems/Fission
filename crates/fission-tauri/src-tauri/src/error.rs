//! Tauri-boundary error type.
//!
//! [`CmdError`] is the single error type returned by every `#[tauri::command]`
//! function.  It implements [`serde::Serialize`] so Tauri serialises it to a
//! JSON object on the JavaScript side:
//!
//! ```json
//! { "kind": "loader", "message": "Loader error: cannot open file" }
//! ```
//!
//! ## Mapping
//!
//! Inside the command implementations use [`FissionError`] (or `?` with the
//! `From` impl) for all fallible operations, then let the `?` coerce the error
//! to `CmdError` at the command boundary.
//!
//! ```rust,ignore
//! use crate::error::CmdResult;
//!
//! #[tauri::command]
//! pub async fn my_command() -> CmdResult<String> {
//!     let result = some_fission_api()?;  // FissionError → CmdError via From
//!     Ok(result)
//! }
//! ```

use fission_core::FissionError;

/// Serialisable error returned to the React frontend by every Tauri command.
///
/// - `kind`    — machine-readable category (e.g. `"loader"`, `"decompiler"`)
/// - `message` — human-readable description (forwarded from [`FissionError`])
#[derive(Debug, serde::Serialize)]
pub struct CmdError {
    /// Machine-readable error category.
    pub kind: &'static str,
    /// Human-readable message forwarded to the UI.
    pub message: String,
}

impl CmdError {
    /// Construct a generic error from any displayable value.
    ///
    /// Use this when no more specific `FissionError` variant applies
    /// (e.g. Tokio join errors, ad-hoc validation failures).
    pub fn other(message: impl std::fmt::Display) -> Self {
        Self {
            kind: "error",
            message: message.to_string(),
        }
    }
}

/// Allow `?` on `Result<T, String>` at the command boundary.
impl From<String> for CmdError {
    fn from(msg: String) -> Self {
        Self {
            kind: "error",
            message: msg,
        }
    }
}

impl From<FissionError> for CmdError {
    fn from(e: FissionError) -> Self {
        let kind = match &e {
            FissionError::Loader(_) => "loader",
            FissionError::Decompiler(_) => "decompiler",
            FissionError::Disassembler(_) => "disassembler",
            FissionError::Analysis(_) => "analysis",
            FissionError::Debug(_) => "debug",
            FissionError::Plugin(_) => "plugin",
            FissionError::Script(_) => "script",
            FissionError::Io(_) => "io",
            FissionError::Config(_) => "config",
            FissionError::Ui(_) => "ui",
            FissionError::Other(_) => "error",
        };
        Self {
            kind,
            message: e.to_string(),
        }
    }
}

/// Convenience alias used by every Tauri command handler.
pub type CmdResult<T> = Result<T, CmdError>;

impl std::fmt::Display for CmdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}

impl std::error::Error for CmdError {}
