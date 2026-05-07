//! Ghidra-style ISA-independent SLEIGH runtime spine.
//!
//! This module owns the shared runtime concepts that map to Ghidra's
//! SleighParserContext, DecisionNode, ConstructState/ParserWalker, ConstructTpl,
//! and PcodeEmit. Processor-specific runtime policy must not live in side
//! modules; unsupported generated constructors must surface as typed runtime
//! errors rather than alternate semantic success paths.

pub mod compiled_table;
#[allow(unused_imports)] // Re-exported API (flow overrides, SLA audits).
pub use compiled_table::{
    audit_sla_template_features, FlowEmitOptions, RuntimeFlowOverride, SlaTemplateFeatureAudit,
};
pub mod construct;
pub mod context;
pub mod decision;
pub mod emitter;
pub mod language;
pub mod template;
pub mod walker;

pub use construct::{
    BoundOperand, RuntimeConstructNode, RuntimeConstructState, RuntimeFixedHandle, RuntimeHandle,
};
pub use context::RuntimeInstructionContext;
pub use decision::{
    select_constructor, DecisionProbeEvaluator, RuntimeMatchTrace, RuntimeSelection,
};
pub use emitter::RuntimePcodeEmitter;
pub use language::{LanguageRuntime, ProcessorRuntimeProfile, RuntimeAttemptReport, RuntimeEndian};
pub use template::{RuntimeTemplateEvaluator, RuntimeTemplateExecutor};
pub use walker::RuntimeParserWalker;
