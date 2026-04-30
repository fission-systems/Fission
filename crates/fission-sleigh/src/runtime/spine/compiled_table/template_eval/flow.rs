/// Ghidra ConstTpl::getReal() V_OFFSET_PLUS case.
///
/// `plus` is value_real read from ATTR_PLUS in the SLA.
/// - Non-constant space: effective_offset + (plus & 0xFFFF)
/// - Constant space: effective_offset >> (8 * (plus >> 16))
pub(super) fn resolve_offset_plus_pub(handle: &RuntimeHandle, plus: u64) -> u64 {
    resolve_offset_plus(handle, plus)
}

fn resolve_offset_plus(handle: &RuntimeHandle, plus: u64) -> u64 {
    let effective_offset = if handle.fixed.offset_space.is_some() {
        handle.fixed.temp_offset
    } else {
        handle.fixed.offset_offset
    };
    let is_const_space = handle
        .fixed
        .space
        .as_ref()
        .map(|s| s.name == "const")
        .unwrap_or(false);
    if !is_const_space {
        effective_offset.wrapping_add(plus & 0xFFFF)
    } else {
        let shift_bytes = plus >> 16;
        let shift_bits = shift_bytes.saturating_mul(8);
        if shift_bits >= 64 {
            0
        } else {
            effective_offset >> shift_bits
        }
    }
}

pub(super) fn emit_pcode_for_state(
    compiled: &CompiledFrontend,
    native: Option<&Arc<NativeBackend>>,
    address: u64,
    memory_window: &[u8],
    memory_base: u64,
    decoded: &RuntimeConstructState,
    flow: FlowEmitOptions,
) -> Result<(Vec<PcodeOp>, RuntimeExecutionDetails)> {
    emit_pcode_for_state_with_bytes(
        compiled,
        native,
        address,
        memory_window,
        memory_base,
        decoded,
        flow,
    )
}

/// Options for pcode template emission (Ghidra `PcodeEmit` parity hooks).
#[derive(Debug, Clone, Default)]
pub struct FlowEmitOptions {
    /// Context register bits when binding cross-build / delay-slot instructions at another PC.
    pub instruction_context_register: u64,
    pub instruction_context_known_mask: u64,
    /// When set, Ghidra-style `ConstTpl` flowref / flowdest constants resolve from these.
    pub flow_ref_addr: Option<u64>,
    pub flow_ref_space_index: Option<u64>,
    pub flow_dest_addr: Option<u64>,
    pub flow_dest_space_index: Option<u64>,
    /// Ghidra `FlowOverride` — only `None` is fully supported; other variants fail closed until ported.
    pub flow_override: RuntimeFlowOverride,
}

/// Ghidra `PcodeEmit.flowOverride` (subset; extend as pcode replacement is ported).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RuntimeFlowOverride {
    #[default]
    None,
    Branch,
    Call,
    CallReturn,
    Return,
}

pub(super) fn emit_pcode_for_state_with_bytes(
    compiled: &CompiledFrontend,
    native: Option<&Arc<NativeBackend>>,
    address: u64,
    memory_window: &[u8],
    memory_base: u64,
    decoded: &RuntimeConstructState,
    flow: FlowEmitOptions,
) -> Result<(Vec<PcodeOp>, RuntimeExecutionDetails)> {
    let mut emitter = CompiledTableEmitter::new(
        compiled,
        native,
        address,
        memory_window,
        memory_base,
        flow,
    );
    // If the template uses InstNext2 (delay-slot architectures), pre-decode the
    // delay-slot instruction to get its actual length.
    if (uses_inst_next2(&decoded.constructor_template.ops)
        || uses_delay_slot_indirect(&decoded.constructor_template.ops))
        && !memory_window.is_empty()
    {
        emitter.precompute_delay_slot_length(decoded.length);
    }
    let details = RuntimeTemplateEvaluator::new(&mut emitter)
        .emit(&compiled.entry_id, decoded)
        .map_err(|err| template_emit_error(compiled, err))?;
    Ok((emitter.finish(), details))
}

fn ptrsub_named_section_index(op: &CompiledOpTpl) -> Result<usize> {
    let v1 = op
        .inputs
        .get(1)
        .ok_or_else(|| anyhow!("PTRSUB/CROSSBUILD missing section index input"))?;
    match v1 {
        CompiledVarnodeTpl::Varnode { offset, .. } => match offset.as_ref() {
            CompiledConstTpl::Real { value } => usize::try_from(*value)
                .map_err(|_| anyhow!("PTRSUB named section index does not fit usize")),
            _ => bail!("PTRSUB section index must be ConstTpl::Real"),
        },
        _ => bail!("PTRSUB section index must be a VarnodeTpl"),
    }
}

fn indirect_placeholder_delay_bytes(op: &CompiledOpTpl) -> Result<u32> {
    let v0 = op
        .inputs
        .first()
        .ok_or_else(|| anyhow!("INDIRECT delay-slot placeholder missing inputs"))?;
    match v0 {
        CompiledVarnodeTpl::Varnode { offset, .. } => match offset.as_ref() {
            CompiledConstTpl::Real { value } => u32::try_from(*value)
                .map_err(|_| anyhow!("INDIRECT delay byte count does not fit u32")),
            _ => bail!("INDIRECT delay size must be ConstTpl::Real (Ghidra walkTemplates)"),
        },
        CompiledVarnodeTpl::Const(CompiledConstTpl::Real { value }) => u32::try_from(*value)
            .map_err(|_| anyhow!("INDIRECT delay byte count does not fit u32")),
        _ => bail!("INDIRECT delay placeholder has unexpected varnode shape"),
    }
}

fn uses_delay_slot_indirect(ops: &[CompiledOpTpl]) -> bool {
    ops.iter()
        .any(|op| op.opcode == CompiledOpTplOpcode::DelaySlotIndirect)
}

/// Returns true if the template contains an InstNext2 constant, meaning this
/// is a delay-slot instruction and we need the delay slot's actual length.
fn uses_inst_next2(ops: &[CompiledOpTpl]) -> bool {
    ops.iter().any(|op| {
        let check_const = |c: &CompiledConstTpl| matches!(c, CompiledConstTpl::InstNext2);
        let check_varnode = |v: &CompiledVarnodeTpl| match v {
            CompiledVarnodeTpl::Varnode { offset, size, .. } => {
                check_const(offset) || check_const(size)
            }
            _ => false,
        };
        op.output.as_ref().map(check_varnode).unwrap_or(false)
            || op.inputs.iter().any(check_varnode)
    })
}

pub(super) fn template_emit_error(compiled: &CompiledFrontend, err: anyhow::Error) -> anyhow::Error {
    let msg = err.to_string();
    if msg.contains("HandleTpl")
        || msg.contains("ConstTpl")
        || msg.contains("unsupported")
        || msg.contains("compatibility varnode template")
    {
        RuntimeSleighError::UnsupportedPcodeTemplate {
            language: compiled.entry_id.clone(),
            reason: format!("emission_time_template_resolution_failed: {msg}"),
        }
        .into()
    } else {
        err
    }
}
