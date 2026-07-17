//! Integer arithmetic normalization: casts, flags, div/mod patterns, cleanup.

mod cast_wide;
mod cleanup_shifts;
mod concat_subpiece;
mod conditional_move;
mod div_mod;
mod double_precision;
mod flags_cond;
mod simplify_algebraic;
mod subfloat;
mod three_way;
mod util;

mod float_sign;
mod ignore_nan;
mod or_compare;

pub use cast_wide::{
    canonicalize_integer_expr, recognize_hi_lo_extract, recognize_wide_integer_recombine,
};
pub use cleanup_shifts::{
    cleanup_arithmetic_wrappers, collapse_zero_offset_cast, merge_consecutive_shifts,
    simplify_subpiece_chain,
};
pub use concat_subpiece::{
    recognize_concat_zext_or, recognize_dumpty_hump_cast, recognize_dumpty_hump_late,
    recognize_humpty_dumpty_or, recognize_piece2_zext_sext,
};
pub use conditional_move::apply_conditional_move_pass;
pub use div_mod::{
    recognize_compiler_runtime_division, recognize_magic_number_division,
    recognize_mod_div_power_of_two,
};
pub use double_precision::apply_double_precision_reconstruction_pass;
pub use flags_cond::{
    canonicalize_condition_expr, canonicalize_flag_intrinsics, normalize_boolean_logic,
};
pub use float_sign::apply_float_sign_pass;
pub use ignore_nan::apply_ignore_nan_pass;
pub use or_compare::apply_or_compare_pass;
pub use simplify_algebraic::{
    simplify_collect_mul_terms, simplify_distribute_common_factor, simplify_double_add,
    simplify_factor_common_mul, simplify_negated_const, simplify_nested_adds_subs,
    simplify_term_order_add,
};
pub use subfloat::apply_subfloat_flow_pass;
pub use three_way::apply_three_way_compare_pass;
