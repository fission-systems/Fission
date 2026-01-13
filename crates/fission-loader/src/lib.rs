//! Fission Loader - Binary format parsing and loading
//!
//! This crate provides functionality for loading and parsing various binary formats
//! including PE (Windows), ELF (Linux), and Mach-O (macOS).

#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]

pub mod detector;
pub mod loader;
pub mod prelude;

// Re-exports
pub use detector::{Confidence, Detection, DetectionResult, DetectionType, detect};
pub use loader::pe::detect_pe_is_64bit;
pub use loader::{FunctionInfo, LoadedBinary, SectionInfo};
