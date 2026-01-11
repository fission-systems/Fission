//! RR (Record and Replay) Debugger Integration
//!
//! This module provides integration with Mozilla's RR debugger for Linux.
//! RR records program execution and allows deterministic replay with
//! forward and reverse execution.
//!
//! # Architecture
//!
//! ```text
//! Fission ←→ RRDebugger ←→ GDB/MI Protocol ←→ rr replay process
//! ```
//!
//! # Requirements
//!
//! - Linux only (RR uses Linux-specific features)
//! - RR must be installed (`rr record`, `rr replay`)
//! - GDB with Rust support recommended
//!
//! # Example
//!
//! ```ignore
//! use crate::debug::rr::RRDebugger;
//!
//! // Record a new trace
//! let trace = RRDebugger::record("/path/to/binary", &["arg1"])?;
//!
//! // Or replay an existing trace
//! let mut rr = RRDebugger::replay("/path/to/trace")?;
//!
//! // Navigate execution
//! rr.reverse_step()?;
//! rr.seek_to(100)?;
//! ```

mod gdb_mi;
mod trace;

pub use gdb_mi::GdbMiParser;
pub use trace::{RRDebugger, RRState, TraceInfo};
