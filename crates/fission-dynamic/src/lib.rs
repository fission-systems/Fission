//! Fission dynamic analysis and runtime engine.

#![warn(clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::multiple_crate_versions)]

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
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
pub mod plugin;
#[cfg(feature = "unpacker_runtime")]
#[allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub mod unpacker;
pub mod prelude;

pub use fission_core as core;
pub use fission_core::{config, constants, errors, logging, prelude as core_prelude, settings};
