//! Midend **structuring** owner facade.
//!
//! # Extraction status (ADR 0012)
//!
//! Implementation still lives in [`fission_pcode::midend::structuring`]. This
//! crate establishes the future extraction dependency name. Most structuring
//! helpers remain `pub(crate)` inside `fission-pcode` until the code move;
//! the public owner module is re-exported so the graph can stabilize early.

#![doc = "See docs/adr/0012-midend-rename-and-crate-extraction.md"]

/// Owner module re-export (grows as structuring API is stabilized for extraction).
pub use fission_pcode::midend::structuring as owner;
