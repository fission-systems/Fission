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
// Additional pedantic lints suppressed for fission-loader
#![allow(clippy::unreadable_literal)]
#![allow(clippy::unnested_or_patterns)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::return_self_not_must_use)]
#![allow(clippy::format_push_string)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::similar_names)]
#![allow(clippy::unnecessary_cast)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::wildcard_imports)]
#![allow(clippy::cloned_instead_of_copied)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::if_not_else)]
#![allow(clippy::single_char_pattern)]
#![allow(clippy::needless_borrows_for_generic_args)]
#![allow(clippy::unnecessary_wraps)]
#![allow(clippy::self_only_used_in_recursion)]
#![allow(clippy::pub_underscore_fields)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::question_mark)]
#![allow(clippy::needless_borrow)]
#![allow(clippy::items_after_statements)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::match_wildcard_for_single_variants)]
#![allow(clippy::unused_self)]
#![allow(clippy::doc_lazy_continuation)]
#![allow(clippy::elidable_lifetime_names)]
#![allow(clippy::used_underscore_binding)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::get_first)]
#![allow(clippy::assigning_clones)]

pub mod detector;
pub mod loader;
pub mod prelude;

// Re-exports
pub use detector::{Confidence, Detection, DetectionResult, DetectionType, detect};
pub use loader::pe::detect_pe_is_64bit;
pub use loader::{FunctionInfo, LoadedBinary, SectionInfo};
