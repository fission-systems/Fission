//! Fission dynamic analysis and runtime engine.
//!
//! ## Runtime smoke API
//!
//! Use [`runtime_status()`] / [`DynamicRuntimeStatus`] for a stable, dependency-light probe that does not touch OS debugger APIs.
//!
//! ## Feature matrix (quick reference)
//!
//! | Build | Purpose |
//! |-------|---------|
//! | `--no-default-features` | Baseline: [`runtime_status()`], examples, unit tests (`default` features remain empty). |
//! | `--features interactive_runtime` | Full interactive stack (Tokio, plugin facade, OS helpers): **`nix` on Linux**, **`windows` on Windows**. Expect longer builds. |
//!
//! Canonical decompiler crates (`fission-pcode`, `fission-decompiler`, `fission-sleigh`) must not depend on this crate.

#![allow(clippy::all)]

mod runtime_status;
pub mod decode;

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
pub use fission_core as core;
pub use fission_core::{config, constants, errors, logging, prelude as core_prelude, settings};
pub use runtime_status::{DynamicRuntimeStatus, runtime_status};
