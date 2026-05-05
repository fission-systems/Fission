//! Plugin and runtime event subsystem for Fission.

#![allow(clippy::all)]

pub mod contracts;

#[cfg(feature = "interactive_runtime")]
pub mod events;
#[cfg(feature = "interactive_runtime")]
pub mod plugin;
