pub(super) struct CompiledParserWalker<'a, 'b> {
    compiled: &'a CompiledFrontend,
    strategy: RuntimeDecodeStrategy<'a>,
    ctx: &'a CompiledInstructionContext<'b>,
    selection: RuntimeSelection<'a>,
    minimum_length: usize,
    context_register: u64,
    context_known_mask: u64,
    cursor: usize,
    handles: Vec<Option<RuntimeHandle>>,
    walker: spine::RuntimeParserWalker,
}

pub(super) struct OperandBinding {
    debug_value: Option<BoundOperand>,
    subtable_state: Option<RuntimeConstructState>,
    fixed: Option<RuntimeFixedHandle>,
}

impl OperandBinding {
    fn debug_only(value: BoundOperand) -> Self {
        Self {
            debug_value: Some(value),
            subtable_state: None,
            fixed: None,
        }
    }

    fn with_fixed(debug_value: Option<BoundOperand>, fixed: RuntimeFixedHandle) -> Self {
        Self {
            debug_value,
            subtable_state: None,
            fixed: Some(fixed),
        }
    }
}
