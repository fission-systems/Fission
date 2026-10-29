//! Address-keyed CFG snapshots used by parity and regression fixtures.
//!
//! The production CFG and dominance facts live in `midend::structuring`. This
//! module deliberately contains only the small address-keyed DTO used by
//! external parity probes and the p-code CFG export that feeds it.

mod export;

pub use export::{AddressCfgSnapshot, AddressEdge};

/// Errors produced while exporting the parity snapshot.
#[derive(Debug, Clone)]
pub enum CfgError {
    /// No entry point found in the function.
    NoEntryPoint,
}

impl std::fmt::Display for CfgError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CfgError::NoEntryPoint => write!(f, "No entry point found in function"),
        }
    }
}

impl std::error::Error for CfgError {}

/// Result type for CFG snapshot export.
pub type CfgResult<T> = Result<T, CfgError>;
