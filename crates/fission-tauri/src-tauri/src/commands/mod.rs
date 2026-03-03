//! Fission Tauri command handlers — organized by domain.
//!
//! Commands are grouped into 5 major domains:
//! - **binary**: Binary loading, metadata, hex editing, listing
//! - **analysis**: Assembly, CFG, xrefs, annotations, function analysis
//! - **debugging**: Runtime debugging and time-travel debugging
//! - **workspace**: Project management, search, settings
//! - **extensions**: Plugins and developer tools

pub mod analysis;
pub mod binary;
pub mod debugging;
pub mod extensions;
pub mod workspace;

// Re-export all public commands for use in generate_handler![]
pub use analysis::*;
pub use binary::*;
pub use debugging::*;
pub use extensions::*;
pub use workspace::*;
