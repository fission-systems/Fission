//! Fission dynamic analysis and runtime engine.

#![allow(clippy::all)]

#[cfg(feature = "interactive_runtime")]
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
pub mod app;
#[cfg(feature = "interactive_runtime")]
#[allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub mod debug;
#[cfg(feature = "interactive_runtime")]
pub use fission_plugin::plugin;
pub mod prelude;
#[cfg(feature = "unpacker_runtime")]
#[allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub mod unpacker;

pub use fission_core as core;
pub use fission_core::{config, constants, errors, logging, prelude as core_prelude, settings};
