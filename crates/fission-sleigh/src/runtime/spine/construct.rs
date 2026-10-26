use super::RuntimeMatchTrace;
use crate::compiler::{
    CompiledDisjointPattern, CompiledExecutableConstructor, CompiledOperandSpec, CompiledSpaceRef,
};

#[derive(Clone)]
#[allow(dead_code)]
pub struct RuntimeConstructState {
    pub subtable_id: u32,
    pub constructor_id: u32,
    pub constructor_slot: usize,
    /// Selected immutable executable metadata. Instruction-specific bindings,
    /// context and lengths remain owned by this state.
    pub constructor: std::sync::Arc<CompiledExecutableConstructor>,
    pub construct_nodes: Vec<RuntimeConstructNode>,
    pub handles: Vec<RuntimeHandle>,
    pub exported_handle: Option<RuntimeHandle>,
    pub operands: Vec<BoundOperand>,
    /// Context register after this constructor's context changes have been applied.
    pub context_register: crate::packed_context::ContextWords,
    pub context_known_mask: crate::packed_context::ContextWords,
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
    pub replaced_wrapper_patterns: Vec<(usize, std::sync::Arc<CompiledDisjointPattern>)>,
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
    pub spec: std::sync::Arc<CompiledOperandSpec>,
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
        // `Arc<str>`, matching `CompiledResolvedVarnode.name`: this variant
        // is constructed on every decoded fixed-varnode operand (the
        // runtime decode hot path), so cloning it should be a refcount
        // bump, not a fresh allocation.
        name: std::sync::Arc<str>,
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

impl std::ops::Deref for RuntimeConstructState {
    type Target = CompiledExecutableConstructor;
    fn deref(&self) -> &Self::Target {
        &self.constructor
    }
}

// Keep diagnostics about the logical bound state, independently of ownership.
impl std::fmt::Debug for RuntimeConstructState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeConstructState")
            .field("subtable_id", &self.subtable_id)
            .field("constructor_id", &self.constructor_id)
            .field("constructor_slot", &self.constructor_slot)
            .field("mnemonic", &self.mnemonic)
            .field("construct_tpl_kind", &self.construct_tpl_kind)
            .field("constructor_template", &self.constructor_template)
            .field("named_templates", &self.named_templates)
            .field("context_commits", &self.context_commits)
            .field("display_template", &self.display_template)
            .field("display_operands", &self.display_operands)
            .field("construct_nodes", &self.construct_nodes)
            .field("handles", &self.handles)
            .field("exported_handle", &self.exported_handle)
            .field("operands", &self.operands)
            .field("context_register", &self.context_register)
            .field("context_known_mask", &self.context_known_mask)
            .field("absolute_offset", &self.absolute_offset)
            .field("relative_length", &self.relative_length)
            .field("length", &self.length)
            .field("match_trace", &self.match_trace)
            .field("replaced_wrapper_patterns", &self.replaced_wrapper_patterns)
            .finish()
    }
}

/// Complete synthetic metadata for tests that supply instruction bindings.
#[cfg(test)]
pub(super) fn test_constructor_metadata(
    mnemonic: String,
    construct_tpl_kind: crate::compiler::CompiledConstructTplKind,
    constructor_template: std::sync::Arc<crate::compiler::CompiledConstructorTemplate>,
    named_templates: Vec<Option<crate::compiler::CompiledConstructTpl>>,
    context_commits: Vec<crate::compiler::CompiledContextCommit>,
    display_template: std::sync::Arc<crate::compiler::CompiledDisplayTemplate>,
    display_operands: std::sync::Arc<Vec<crate::compiler::CompiledDisplayOperand>>,
) -> std::sync::Arc<CompiledExecutableConstructor> {
    CompiledExecutableConstructor {
        constructor_id: 0,
        sla_identity: None,
        sla_decode_status: crate::compiler::CompiledSlaDecodeStatus::Decoded,
        mnemonic,
        source: "test".into(),
        display: display_template.display.clone(),
        display_template,
        signature_hash: 0,
        minimum_length: 0,
        context_changes: Vec::new(),
        context_commits,
        matcher: crate::compiler::CompiledPatternMatcher::BitConstraints(Vec::new()),
        opsize_variants: Vec::new(),
        operand_specs: Vec::new(),
        display_operands,
        construct_tpl_kind,
        constructor_template,
        named_templates,
        runtime_ready: true,
        unsupported_template_kind: None,
    }
    .into()
}

#[cfg(test)]
mod constructor_ownership_tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn constructor_ownership_uses_exact_selected_metadata_and_outlives_frontend() {
        let mut compiled = crate::compiler::compile_x86_64_frontend().unwrap();
        let state = crate::runtime::spine::compiled_table::decode_instruction_raw_state(
            &compiled,
            &[0x90],
            0x1000,
        )
        .unwrap();
        let selected = compiled
            .subtables
            .values()
            .flat_map(|table| &table.constructors)
            .find(|constructor| Arc::ptr_eq(constructor, &state.constructor))
            .expect("bound metadata must be an existing selected constructor");
        let identity = selected.sla_identity.as_ref().unwrap();
        assert_eq!(state.subtable_id, identity.subtable_id);
        assert_eq!(state.constructor_id, identity.constructor_id);
        assert_eq!(state.constructor_slot, identity.constructor_slot);
        let expected = format!("{state:?}");
        // Removing the source owner does not invalidate a returned raw state.
        compiled.subtables.clear();
        drop(compiled);
        assert_eq!(format!("{state:?}"), expected);
        fn send_sync<T: Send + Sync>() {}
        send_sync::<RuntimeConstructState>();
    }

    #[test]
    fn constructor_ownership_clone_and_explicit_metadata_mutation_are_isolated() {
        let compiled = crate::compiler::compile_x86_64_frontend().unwrap();
        let state = crate::runtime::spine::compiled_table::decode_instruction_raw_state(
            &compiled,
            &[0x90],
            0x1000,
        )
        .unwrap();
        let before = Arc::strong_count(&state.constructor);
        let mut cloned = state.clone();
        assert!(Arc::ptr_eq(&state.constructor, &cloned.constructor));
        assert_eq!(Arc::strong_count(&state.constructor), before + 1);
        let original = format!("{state:?}");
        Arc::make_mut(&mut cloned.constructor)
            .mnemonic
            .push_str(" changed");
        assert!(!Arc::ptr_eq(&state.constructor, &cloned.constructor));
        assert_eq!(format!("{state:?}"), original);
        assert_eq!(Arc::strong_count(&state.constructor), before);
        assert_ne!(cloned.mnemonic, state.mnemonic);
        assert_eq!(cloned.context_register, state.context_register);
        assert_eq!(cloned.length, state.length);
    }
}
