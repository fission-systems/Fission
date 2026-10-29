//! Agent-first CLI contract, `--contract v1` (see
//! `docs/proposals/2026-10-26-agent-first-cli-contract.md`).
//!
//! The contract is opt-in: without `--contract v1` (or `FISSION_CONTRACT=v1`)
//! the CLI behaves exactly as before. In contract mode the binary supervises a
//! copy of itself, so every command gets the same envelope and exit codes
//! without each one having to return a typed payload.

mod errors;
mod schema;
mod supervisor;

pub use errors::{ERROR_CODES, ErrorKind, classify_error};
pub use schema::run_schema;
pub use supervisor::{emit_child_error, is_child, maybe_supervise};

/// Identifies the envelope version. Changes only on an incompatible envelope change.
pub const CONTRACT_ID: &str = "fission-cli/1";

/// Marks the re-executed child of a contract-mode run.
pub(crate) const CHILD_ENV: &str = "FISSION_CONTRACT_CHILD";

/// Prefix of the single machine-readable error line a child writes to stderr.
pub(crate) const ERROR_RECORD_PREFIX: &str = "@fission-error ";
