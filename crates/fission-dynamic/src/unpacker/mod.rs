//! # Unpacker - Runtime Memory Analysis & Reconstruction
//!
//! **This is NOT a debugger** - This module provides runtime analysis tools for:
//!
//! ## Core Capabilities
//!
//! 1. **Import Reconstruction** (`importer.rs`)
//!    - Scans process memory to rebuild Import Address Table (IAT)
//!    - Resolves function addresses back to module+function names
//!    - Critical for analyzing packed/obfuscated executables
//!
//! 2. **Process Dumping** (`dumper.rs`)
//!    - Dumps running process memory to executable file
//!    - Fixes PE headers and sections
//!    - Reconstructs import directory with proper RVAs
//!
//! 3. **PE Reconstruction** (`pe.rs`)
//!    - Reads PE structures from process memory
//!    - Validates and fixes corrupted headers
//!    - Handles both x86 and x64 formats
//!
//! ## Use Cases
//!
//! - **Unpacking**: Dump packed executables after they self-decrypt in memory
//! - **Malware Analysis**: Extract original binary from obfuscated samples
//! - **Forensics**: Reconstruct executables from memory dumps
//!
//! ## Architecture
//!
//! ```text
//! TitanEngine (main orchestrator)
//!    ├─> Attaches to process via debug APIs
//!    ├─> ImportReconstructor: Scans memory for IAT
//!    └─> Dumper: Writes fixed PE to disk
//! ```
//!
//! ## Example Usage
//!
//! ```rust,ignore
//! use fission::unpacker::TitanEngine;
//!
//! // Attach to packed process
//! let mut engine = TitanEngine::new();
//! engine.attach(pid)?;
//!
//! // Reconstruct imports
//! let imports = engine.reconstruct_imports()?;
//!
//! // Dump with fixed IAT
//! engine.dump_and_fix("unpacked.exe", &imports)?;
//! ```
//!
//! ## Platform Support
//!
//! - **Windows**: Full support (uses Debug APIs)
//! - **Linux/macOS**: Limited (no native implementation)
//!
//! ## Related Modules
//!
//! - `crate::debug` - Interactive debugger (breakpoints, stepping, etc.)
//! - `crate::analysis::loader` - Static file parsing
//! - `crate::parser` - Dynamic/static parsing bridge

pub mod breakpoint;
pub mod context;
#[cfg(feature = "unpacker_runtime")]
pub mod dumper;
pub mod engine;
#[cfg(feature = "unpacker_runtime")]
pub mod importer;
pub mod loader;
pub mod memory;
#[cfg(feature = "unpacker_runtime")]
pub mod pe;
pub mod types;

pub use engine::TitanEngine;
pub use loader::TitanLoader;
