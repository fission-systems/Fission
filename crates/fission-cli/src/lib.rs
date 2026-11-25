//! Fission CLI library.

#![allow(clippy::all)]
#![allow(unused_assignments)]

pub mod cli;
#[cfg(feature = "sentry")]
pub mod telemetry;

pub use fission_core as core;
pub use fission_static::analysis;
