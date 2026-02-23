//! Fission Tauri command handlers — organized by domain.
//!
//! Each sub-module groups related `#[tauri::command]` functions. The public
//! commands are re-exported here so `lib.rs` can continue to reference them
//! as `commands::open_file`, etc.

pub mod analysis;
pub mod annotations;
pub mod assembly;
pub mod binary;
pub mod cfg;
pub mod debug;
pub mod hex;
pub mod listing;
pub mod metadata;
pub mod plugins;
pub mod project;
pub mod search;
pub mod settings;
pub mod ttd;
pub mod xrefs;

// Re-export all public commands for use in generate_handler![]
pub use analysis::*;
pub use annotations::*;
pub use assembly::*;
pub use binary::*;
pub use cfg::*;
pub use debug::*;
pub use hex::*;
pub use listing::*;
pub use metadata::*;
pub use plugins::*;
pub use project::*;
pub use search::*;
pub use settings::*;
pub use ttd::*;
pub use xrefs::*;
