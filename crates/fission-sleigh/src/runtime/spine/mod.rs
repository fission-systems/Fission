//! Ghidra-style ISA-independent SLEIGH runtime spine.
//!
//! This module owns the shared runtime concepts that map to Ghidra's
//! SleighParserContext, DecisionNode, ConstructState/ParserWalker, ConstructTpl,
//! and PcodeEmit. Processor-specific runtime policy must not live in side
//! modules; temporary compiled-table compatibility paths must be made visible
//! through telemetry and replaced by spec-derived template execution.

pub mod compiled_table;
pub mod construct;
pub mod context;
pub mod decision;
pub mod emitter;
pub mod language;
pub mod template;
pub mod walker;

pub use construct::{
    operand_size, BoundOperand, RuntimeConstructNode, RuntimeConstructState, RuntimeHandle,
};
pub use context::RuntimeInstructionContext;
pub use decision::{
    select_constructor, DecisionProbeEvaluator, RuntimeMatchTrace, RuntimeSelection,
};
pub use emitter::RuntimePcodeEmitter;
pub use language::{LanguageRuntime, ProcessorRuntimeProfile, RuntimeAttemptReport, RuntimeEndian};
pub use template::{RuntimeSemanticEmitter, RuntimeTemplateEvaluator};
pub use walker::RuntimeParserWalker;
