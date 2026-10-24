//! CLI Module
//!
//! Unified command-line interface for Fission binary analysis.
//! This module provides one-shot mode only (fission_cli binary).

pub mod oneshot;
pub mod output;
pub mod resources;

mod args;

pub use args::{
    LegacyInvocationKind, OneShotArgs, ParsedInvocation, ParsedOneShotArgs, ScriptCmd,
    ScriptInvocation, parse_hex_address, parse_oneshot_args,
};
pub use oneshot::{
    build_functions_response, build_protocol_callgraph, function_info_to_fn_entry, run_oneshot,
    to_disasm_response,
};
