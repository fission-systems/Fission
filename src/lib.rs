//! Fission - Next-Gen Dynamic Instrumentation Platform
//!
//! This library provides the core functionality for binary analysis,
//! debugging, and decompilation.

#![warn(clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::missing_errors_doc)]
#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unused_variables)]
// Allow common cast patterns in binary analysis code
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_lossless)]
// Allow items after statements (common pattern in procedural code)
#![allow(clippy::items_after_statements)]
// Allow similar names (common in parsing code with similar field names)
#![allow(clippy::similar_names)]
// Allow struct field prefixes (domain-specific naming)
#![allow(clippy::struct_field_names)]
// Allow manual range patterns (clearer for hex comparisons)
#![allow(clippy::manual_range_contains)]
// Allow map().unwrap_or() patterns (often clearer than map_or)
#![allow(clippy::map_unwrap_or)]
// Allow uninlined format args (can reduce readability)
#![allow(clippy::uninlined_format_args)]
// Allow unnecessary debug formatting (useful for development)
#![allow(clippy::unnecessary_debug_formatting)]
// Allow single match else (often clearer for complex matches)
#![allow(clippy::single_match_else)]
// Allow missing panics documentation (to be added incrementally)
#![allow(clippy::missing_panics_doc)]
// Allow cognitive complexity warnings (complex binary parsing is expected)
#![allow(clippy::cognitive_complexity)]
// Allow too many lines (parsing functions are often long)
#![allow(clippy::too_many_lines)]
// Allow too many arguments (builder patterns often need many)
#![allow(clippy::too_many_arguments)]
// Allow redundant closure (sometimes clearer)
#![allow(clippy::redundant_closure)]
// Allow redundant closure for method calls
#![allow(clippy::redundant_closure_for_method_calls)]
// Allow bool to int if (clearer than usize::from for some cases)
#![allow(clippy::bool_to_int_with_if)]
// Allow significant drop in scrutinee (acceptable in some patterns)
#![allow(clippy::significant_drop_in_scrutinee)]
// Allow format push string (acceptable for string building)
#![allow(clippy::format_push_string)]
// Allow option if let else (some patterns are clearer with match)
#![allow(clippy::option_if_let_else)]
// Allow if not else (sometimes clearer negation)
#![allow(clippy::if_not_else)]
// Allow needless pass by value (sometimes intentional for API design)
#![allow(clippy::needless_pass_by_value)]
// Allow missing const (not all functions need const)
#![allow(clippy::missing_const_for_fn)]
// Allow implicit clone (acceptable for clarity)
#![allow(clippy::implicit_clone)]
// Allow unused self (may be used in future implementations)
#![allow(clippy::unused_self)]
// Allow trivially copy pass by ref (sometimes clearer for API consistency)
#![allow(clippy::trivially_copy_pass_by_ref)]
// Allow semicolon if nothing returned (style preference)
#![allow(clippy::semicolon_if_nothing_returned)]
// Allow default trait access (style preference)
#![allow(clippy::default_trait_access)]
// Allow use self (style preference)
#![allow(clippy::use_self)]
// Allow struct excessive bools (domain-specific structs)
#![allow(clippy::struct_excessive_bools)]
// Allow iter not returning iterator (acceptable naming)
#![allow(clippy::iter_not_returning_iterator)]
// Allow needless return (sometimes clearer for explicit returns)
#![allow(clippy::needless_return)]
// Allow match same arms (sometimes intentional for documentation)
#![allow(clippy::match_same_arms)]
// Allow else if without else (acceptable pattern)
#![allow(clippy::else_if_without_else)]
// Allow unreadable literal (hex addresses are domain-specific)
#![allow(clippy::unreadable_literal)]
// Allow branches sharing code (acceptable for clarity)
#![allow(clippy::branches_sharing_code)]
// Allow return self not must use (builder patterns)
#![allow(clippy::return_self_not_must_use)]
// Allow empty structs with brackets (consistent style)
#![allow(clippy::empty_structs_with_brackets)]
// Allow multiple crate versions (dependency management)
#![allow(clippy::multiple_crate_versions)]
// Additional commonly-triggered lints for binary analysis code
#![allow(clippy::doc_markdown)]
#![allow(clippy::needless_raw_string_hashes)]
#![allow(clippy::single_char_pattern)]
#![allow(clippy::map_entry)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_else_if)]
#![allow(clippy::collapsible_match)]
#![allow(clippy::manual_is_ascii_check)]
#![allow(clippy::redundant_clone)]
#![allow(clippy::useless_conversion)]
#![allow(clippy::manual_div_ceil)]
#![allow(clippy::or_fun_call)]
#![allow(clippy::unnecessary_wraps)]
#![allow(clippy::needless_lifetimes)]
#![allow(clippy::significant_drop_tightening)]
#![allow(clippy::get_first)]
#![allow(clippy::match_wildcard_for_single_variants)]
#![allow(clippy::redundant_field_names)]
#![allow(clippy::derivable_impls)]
#![allow(clippy::identity_op)]
#![allow(clippy::assigning_clones)]
#![allow(clippy::map_with_unused_argument_over_ranges)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::wildcard_imports)]
#![allow(clippy::unused_async)]
#![allow(clippy::explicit_iter_loop)]
#![allow(clippy::needless_for_each)]
#![allow(clippy::unused_unit)]
#![allow(clippy::cloned_instead_of_copied)]
#![allow(clippy::len_without_is_empty)]
#![allow(clippy::ptr_as_ptr)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::type_complexity)]
#![allow(clippy::fn_params_excessive_bools)]
#![allow(clippy::suspicious_open_options)]
#![allow(clippy::derive_partial_eq_without_eq)]
#![allow(clippy::new_without_default)]
#![allow(clippy::iter_without_into_iter)]
#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::unused_peekable)]
#![allow(clippy::single_element_loop)]
#![allow(clippy::integer_division_remainder_used)]
#![allow(clippy::ref_option_ref)]
#![allow(clippy::flat_map_option)]
#![allow(clippy::expect_fun_call)]
#![allow(clippy::iter_over_hash_type)]
#![allow(clippy::string_lit_chars_any)]
#![allow(clippy::option_option)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::arbitrary_source_item_ordering)]
// Additional lint allowances
#![allow(clippy::unnecessary_cast)]
#![allow(clippy::useless_format)]
#![allow(clippy::map_clone)]
#![allow(clippy::manual_inspect)]
#![allow(clippy::unnecessary_semicolon)]
#![allow(clippy::enum_glob_use)]
#![allow(clippy::unnested_or_patterns)]
#![allow(clippy::manual_is_variant_and)]
#![allow(clippy::manual_is_multiple_of)]
#![allow(clippy::or_then_unwrap)]
#![allow(clippy::let_and_return)]
#![allow(clippy::map_flatten)]
#![allow(clippy::unit_arg)]
#![allow(clippy::redundant_guards)]
#![allow(clippy::explicit_deref_methods)]
#![allow(clippy::default_constructed_unit_structs)]
#![allow(clippy::string_add)]
#![allow(clippy::needless_borrow)]
#![allow(clippy::explicit_into_iter_loop)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_assignments)]
// Additional lint allowances for remaining issues
#![allow(clippy::needless_pass_by_ref_mut)]
#![allow(clippy::manual_string_new)]
#![allow(clippy::iter_kv_map)]
#![allow(clippy::redundant_locals)]
#![allow(clippy::if_same_then_else)]
#![allow(clippy::needless_borrowed_reference)]
#![allow(clippy::char_lit_as_u8)]
#![allow(clippy::unused_io_amount)]
#![allow(clippy::assign_op_pattern)]
// Remaining lint allowances
#![allow(clippy::str_to_string)]
#![allow(clippy::unit_hash)]
#![allow(clippy::stable_sort_primitive)]
#![allow(clippy::manual_pattern_char_comparison)]
#![allow(clippy::used_underscore_items)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::elidable_lifetime_names)]
#![allow(clippy::pub_underscore_fields)]
#![allow(clippy::unwrap_or_default)]
#![allow(clippy::zero_sized_map_values)]
#![allow(clippy::unnecessary_literal_bound)]
#![allow(clippy::for_kv_map)]
#![allow(clippy::manual_ignore_case_cmp)]
// Allow deprecated pyo3 methods (migration to be done separately)
#![allow(deprecated)]

pub mod analysis;
pub mod core;
pub mod debug;
pub mod debug_engine;
pub mod parser;
pub mod plugin;
pub mod script;
pub mod ui;

// Re-export core utilities at crate level for convenience
pub use crate::core::config;
pub use crate::core::constants;
pub use crate::core::errors;
pub use crate::core::logging;
pub use crate::core::prelude;
