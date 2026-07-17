//! Midend **normalize** owner facade.
//!
//! # Extraction status (ADR 0012)
//!
//! Implementation still lives in [`fission_pcode::midend::normalize`]. This
//! crate exists so dependents can take a stable normalize-facing dependency
//! before the code move out of `fission-pcode`.
//!
//! Prefer this crate over reaching into `fission-pcode` for normalize-only
//! symbols as the surface expands.

#![doc = "See docs/adr/0012-midend-rename-and-crate-extraction.md"]

pub use fission_pcode::midend::normalize::{
    summarize_direct_tail_wrapper_from_ops, summarize_direct_tail_wrapper_from_pcode,
};

/// Owner module re-export (grows as normalize API is stabilized for extraction).
pub use fission_pcode::midend::normalize as owner;
