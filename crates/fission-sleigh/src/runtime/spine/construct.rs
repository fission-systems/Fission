use super::RuntimeMatchTrace;
use crate::compiler::{
    CompiledConstructTpl, CompiledConstructTplKind, CompiledConstructorTemplate,
    CompiledContextCommit, CompiledDisjointPattern, CompiledDisplayOperand,
    CompiledDisplayTemplate, CompiledOperandSpec, CompiledSpaceRef,
};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RuntimeConstructState {
    pub subtable_id: u32,
    pub constructor_id: u32,
    pub constructor_slot: usize,
    pub mnemonic: String,
    pub construct_tpl_kind: CompiledConstructTplKind,
    pub constructor_template: CompiledConstructorTemplate,
    /// Named p-code sections (Ghidra's namedtempl). Each entry corresponds to
    /// a section number (ATTR_SECTION). Used by CROSSBUILD / sectioned constructors.
    pub named_templates: Vec<Option<CompiledConstructTpl>>,
    /// Deferred global context commits (Ghidra's `globalset` / `ContextCommit`).
    pub context_commits: Vec<CompiledContextCommit>,
    pub display_template: CompiledDisplayTemplate,
    pub display_operands: Vec<CompiledDisplayOperand>,
    pub construct_nodes: Vec<RuntimeConstructNode>,
    pub handles: Vec<RuntimeHandle>,
    pub exported_handle: Option<RuntimeHandle>,
    pub operands: Vec<BoundOperand>,
    /// Context register after this constructor's context changes have been applied.
    pub context_register: u64,
    pub context_known_mask: u64,
    /// Absolute byte offset of this constructor state from the instruction start.
    /// Mirrors Ghidra ConstructState.offset.
    pub absolute_offset: usize,
    /// Byte length of this constructor state relative to `absolute_offset`.
    /// Mirrors Ghidra ConstructState.length.
    pub relative_length: usize,
    /// Absolute byte end of the decoded instruction/subconstructor. Existing
    /// callers use this as the instruction length for the root state.
    pub length: usize,
    pub match_trace: RuntimeMatchTrace,
    /// Instruction-relative patterns from "replaces current" wrapper
    /// constructors discarded on the path to this state (e.g. an x86 legacy
    /// prefix byte's own constructor, which matches just that byte then
    /// hands off entirely to the constructor for the rest of the
    /// instruction -- see `constructor_replaces_current` in `walker.rs`).
    /// `(absolute_offset, pattern)`: the wrapper's own `ctx.cursor` at match
    /// time, paired with its `match_trace.matched_leaf_pattern`. Populated
    /// only by the "replaces current" path -- everything else's pattern
    /// info lives in `match_trace` directly. Consumed by
    /// `instruction_pattern_mask` (FID hashing) to recover pattern bits that
    /// would otherwise be silently lost when a wrapper is replaced.
    pub replaced_wrapper_patterns: Vec<(usize, CompiledDisjointPattern)>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RuntimeConstructNode {
    pub operand_index: Option<usize>,
    pub parent_index: Option<usize>,
    pub absolute_offset: usize,
    pub relative_length: usize,
    pub handle_index: Option<usize>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RuntimeHandle {
    pub operand_index: usize,
    pub spec: CompiledOperandSpec,
    pub fixed: RuntimeFixedHandle,
    pub debug_value: Option<BoundOperand>,
    pub subtable_state: Option<Box<RuntimeConstructState>>,
}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct RuntimeFixedHandle {
    pub space: Option<CompiledSpaceRef>,
    pub size: u32,
    pub offset_space: Option<CompiledSpaceRef>,
    pub offset_offset: u64,
    pub offset_size: u32,
    pub temp_space: Option<CompiledSpaceRef>,
    pub temp_offset: u64,
    pub fixable: bool,
}

#[derive(Debug, Clone)]
pub enum BoundOperand {
    Register {
        index: u8,
        size: u32,
    },
    NamedVarnode {
        name: String,
        display_index: Option<u32>,
        size: u32,
    },
    Memory {
        base: Option<u8>,
        index: Option<u8>,
        scale: u8,
        displacement: i64,
        rip_relative: bool,
        absolute: Option<u64>,
        size: u32,
    },
    Immediate {
        value: u64,
        encoded_size: u32,
        signed: bool,
    },
    Relative {
        target: u64,
    },
}
