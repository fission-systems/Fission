//! CLI Command Handlers
//!
//! Individual command implementations split by category.

pub mod load;
pub mod info;
pub mod analysis;
pub mod disasm;
pub mod decompile;
pub mod xrefs;
pub mod help;

// Re-export command functions
pub use load::cmd_load;
pub use info::cmd_info;
pub use analysis::{cmd_analyze, cmd_functions, cmd_sections, cmd_strings};
pub use disasm::cmd_disasm;
pub use decompile::cmd_decompile;
pub use xrefs::cmd_xrefs;
pub use help::{cmd_help, cmd_clear};
