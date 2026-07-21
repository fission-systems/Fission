//! Decompiler-facing facts module.
//!
//! Fission decompiles through the Rust-only NIR/Rust-Sleigh pipeline
//! (see `fission-decompiler`). This module provides the safe, static-analysis
//! facts layer (`FactStore` and friends) consumed by that pipeline.

pub mod cache;
pub mod facts;

pub use facts::{FactProvenance, FactStore, FunctionFacts, NameFact, TypeFact, log_type_diag};
