//! DWARF Debug Information Parser
//!
//! Extracts type, function, parameter, and local variable information from
//! DWARF debug sections using the `gimli` crate.
//!
//! Supports ELF and Mach-O formats with `.debug_info`, `.debug_abbrev`,
//! `.debug_str`, `.debug_line`, and `.debug_ranges` sections.
//!
//! # Architecture
//!
//! - [`analyzer`] - Main coordinator (DwarfAnalyzer public API)
//! - [`types`] - Type information extraction (structs, classes, unions)
//! - [`functions`] - Function information extraction (params, locals)
//! - [`sections`] - Section data loading helper

pub mod analyzer;
mod functions;
mod sections;
mod types;

// Re-export public API
pub use analyzer::DwarfAnalyzer;
pub use types::{DwarfMemberInfo, DwarfTypeInfo};
