//! Plugin and runtime event subsystem for Fission.

#![warn(clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "interactive_runtime")]
pub mod events;
#[cfg(feature = "interactive_runtime")]
pub mod plugin;
