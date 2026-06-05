//! Integer arithmetic normalization: casts, flags, div/mod patterns, cleanup.

mod cast_wide;
mod concat_subpiece;
mod cleanup_shifts;
mod div_mod;
mod flags_cond;
mod simplify_algebraic;
mod util;
mod double_precision;
mod three_way;
mod conditional_move;
mod subfloat;

mod or_compare;
mod float_sign;
mod ignore_nan;

pub(crate) use cast_wide::{
    canonicalize_integer_expr, recognize_hi_lo_extract, recognize_wide_integer_recombine,
};
pub(crate) use concat_subpiece::{
    recognize_concat_zext_or, recognize_dumpty_hump_cast, recognize_dumpty_hump_late,
    recognize_humpty_dumpty_or, recognize_piece2_zext_sext,
};
pub(crate) use cleanup_shifts::{
    cleanup_arithmetic_wrappers, collapse_zero_offset_cast, merge_consecutive_shifts,
    simplify_subpiece_chain,
};
pub(crate) use div_mod::{
    recognize_compiler_runtime_division, recognize_magic_number_division,
    recognize_mod_div_power_of_two,
};
pub(crate) use flags_cond::{
    canonicalize_condition_expr, canonicalize_flag_intrinsics, normalize_boolean_logic,
};
pub(crate) use simplify_algebraic::{
    simplify_double_add, simplify_factor_common_mul, simplify_negated_const,
    simplify_nested_adds_subs, simplify_collect_mul_terms, simplify_distribute_common_factor,
    simplify_term_order_add,
};
pub(crate) use double_precision::apply_double_precision_reconstruction_pass;
pub(crate) use three_way::apply_three_way_compare_pass;
pub(crate) use conditional_move::apply_conditional_move_pass;
pub(crate) use subfloat::apply_subfloat_flow_pass;
pub(crate) use or_compare::apply_or_compare_pass;
pub(crate) use float_sign::apply_float_sign_pass;
pub(crate) use ignore_nan::apply_ignore_nan_pass;


