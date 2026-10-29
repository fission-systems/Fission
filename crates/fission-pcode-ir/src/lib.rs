//! Fission p-code IR leaf: the lifted p-code types, the p-code optimizer, the
//! block-level CFG facts derived from them, and the address-keyed CFG snapshot.
//!
//! This crate depends on no other workspace crate. `fission-pcode` re-exports it
//! under the historical paths (`fission_pcode::{Pcode*, Varnode, arch, cfg, ir}`),
//! so the split is invisible to dependents; the point of the boundary is that a
//! change to the midend or to a lower-layer crate does not rebuild this code.
//! See `docs/proposals/2026-10-29-pcode-crate-split.md`.

// Same lint posture as when this code lived in `fission-pcode`: the moved modules
// were written under these allows, and the split is not the place to change them.
#![allow(clippy::all)]
#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(unused_assignments)]
#![forbid(unsafe_code)]

pub mod arch;
pub mod block_cfg;
pub mod cfg;
pub mod pcode;
pub mod space;

pub use pcode::optimizer::{PcodeOptimizer, PcodeOptimizerConfig};
pub use pcode::*;
