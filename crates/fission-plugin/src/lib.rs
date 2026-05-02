//! Plugin and runtime event subsystem for Fission.

#![allow(clippy::all)]

#[cfg(feature = "interactive_runtime")]
pub mod events;
#[cfg(feature = "interactive_runtime")]
pub mod plugin;
