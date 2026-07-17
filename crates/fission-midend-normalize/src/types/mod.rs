//! Type inference and signature propagation passes.

mod callsite_type_prop;
mod constraint;
mod entry_param_promotion;
mod interproc_sig_prop;
mod procedure_summary;
mod type_infer;
mod use_type_infer;
mod variadic_stack_region;

pub use callsite_type_prop::apply_callsite_type_prop_pass;
pub use callsite_type_prop::is_known_api_signature;
pub use constraint::apply_type_constraint_propagation;
pub use entry_param_promotion::apply_entry_param_promotion_pass;
pub use interproc_sig_prop::apply_interproc_callsite_arity_pass;
pub use procedure_summary::{summarize_wrapper_hir_function, summary_soundness_for_wrapper};
pub use type_infer::apply_type_inference_pass;
pub use use_type_infer::apply_use_driven_type_infer_pass;
pub use variadic_stack_region::apply_variadic_stack_region_pass;
