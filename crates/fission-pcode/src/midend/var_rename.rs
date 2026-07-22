//! Re-export DIR-side variable renaming helpers from midend-core -- only
//! `builder` uses this module, never `render` (confirmed: no
//! `var_rename::` reference anywhere under `src/render/`).
pub use fission_midend_core::util_dir::var_rename::*;
