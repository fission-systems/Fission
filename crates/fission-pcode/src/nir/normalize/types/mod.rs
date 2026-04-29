//! Type inference and signature propagation passes.

mod callsite_type_prop;
mod entry_param_promotion;
mod interproc_sig_prop;
mod procedure_summary;
mod type_infer;
mod use_type_infer;
mod variadic_stack_region;

pub(crate) use callsite_type_prop::{apply_callsite_type_prop_pass, is_known_api_signature};
pub(crate) use entry_param_promotion::apply_entry_param_promotion_pass;
pub(crate) use interproc_sig_prop::apply_interproc_callsite_arity_pass;
pub use procedure_summary::{
    summarize_direct_tail_wrapper_from_ops, summarize_direct_tail_wrapper_from_pcode,
};
pub(crate) use procedure_summary::{summarize_wrapper_hir_function, summary_soundness_for_wrapper};
pub(crate) use type_infer::apply_type_inference_pass;
pub(crate) use use_type_infer::apply_use_driven_type_infer_pass;
pub(crate) use variadic_stack_region::apply_variadic_stack_region_pass;
