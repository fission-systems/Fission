#[derive(Debug, Clone)]
pub(super) struct CompiledTableEmitter<'c> {
    compiled: &'c CompiledFrontend,
    native: Option<&'c Arc<NativeBackend>>,
    /// Byte window for the current decode; `memory_window[0]` is at `memory_base`.
    memory_window: &'c [u8],
    memory_base: u64,
    emitter: RuntimePcodeEmitter,
    address: u64,
    built_operands: std::collections::BTreeSet<usize>,
    /// Exported varnodes produced by BUILD subconstructors. Ghidra templates
    /// reference these through negative handle indices in parent templates.
    exported_build_varnodes: std::collections::BTreeMap<i64, Varnode>,
    /// Index of the unique (temporary) address space, derived from `.sla` metadata.
    unique_space_index: u64,
    /// Mapping from space index to space reference, derived from `.sla` metadata.
    sla_spaces: std::collections::BTreeMap<u64, CompiledSpaceRef>,
    /// Label positions: `label_num` → emitter op count at the time the Label was seen.
    /// Used for `resolveRelatives()` post-processing.
    label_positions: std::collections::BTreeMap<u64, u32>,
    /// Pre-computed delay slot instruction length in bytes (first slot only).
    /// Used for `InstNext2 = inst_next + delay_slot_length`.
    delay_slot_length: Option<u32>,
    flow: FlowEmitOptions,
    /// Ghidra `PcodeEmit.build(construct, secnum)` — named-section pcode uses secnum ≥ 0.
    pcode_build_secnum: i32,
    in_delay_slot: bool,
    uniq_mask: u64,
}

#[derive(Debug, Clone)]
pub(super) struct DynamicMemoryTarget {
    space: Varnode,
    ptr: Varnode,
    temp: Varnode,
    size: u32,
}
