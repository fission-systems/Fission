// ConstructTpl → p-code emission for the compiled table. Split into `template_eval/*.rs`
// fragments (single `template_eval` module scope via `include!`).
include!("template_eval/flow.rs");
include!("template_eval/relative_label.rs");
include!("template_eval/emitter_types.rs");
// Single `impl CompiledTableEmitter` file avoids `include!` delimiter edge cases across fragments.
include!("template_eval/emitter_impl.rs");
include!("template_eval/impl_executor_trait.rs");
