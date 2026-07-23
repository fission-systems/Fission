//! CLI Module
//!
//! Unified command-line interface for Fission binary analysis.
//! This module provides one-shot mode only (fission_cli binary).

pub mod ai;
pub mod oneshot;
pub mod output;
pub mod resources;
pub mod serve;

mod args;

pub use args::{
    AiInvocation, LegacyInvocationKind, OneShotArgs, ParsedInvocation, ParsedOneShotArgs,
    ScriptCmd, ScriptInvocation, parse_hex_address, parse_oneshot_args,
};
pub use oneshot::run_oneshot;
