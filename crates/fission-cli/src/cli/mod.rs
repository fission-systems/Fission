//! CLI Module
//!
//! Unified command-line interface for Fission binary analysis.
//! This module provides one-shot mode only (fission_cli binary).

pub mod oneshot;
pub mod output;

mod args;

pub use args::{LegacyInvocationKind, OneShotArgs, parse_hex_address, parse_oneshot_args};
pub use oneshot::run_oneshot;
