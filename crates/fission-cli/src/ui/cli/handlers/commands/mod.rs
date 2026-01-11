//! CLI Command Handlers
//!
//! Individual command implementations split by category.

pub mod analysis;
pub mod decompile;
pub mod disasm;
pub mod graph;
pub mod help;
pub mod info;
pub mod load;
pub mod string_xrefs;
pub mod xrefs;

// Re-export command functions
pub use analysis::{cmd_analyze, cmd_functions, cmd_sections, cmd_strings};
pub use decompile::cmd_decompile;
pub use disasm::cmd_disasm;
pub use graph::cmd_graph;
pub use help::{cmd_clear, cmd_help};
pub use info::cmd_info;
pub use load::cmd_load;
pub use string_xrefs::cmd_string_xrefs;
pub use xrefs::cmd_xrefs;

pub mod rr;
pub use rr::{cmd_rr_record, cmd_rr_replay};
