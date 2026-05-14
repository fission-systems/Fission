use super::*;

/// Ghidra ConstTpl::getReal() V_OFFSET_PLUS case.
///
/// `plus` is value_real read from ATTR_PLUS in the SLA.
/// - Non-constant space: effective_offset + (plus & 0xFFFF)
/// - Constant space: effective_offset >> (8 * (plus >> 16)), using Java long shift masking.
pub(super) fn resolve_offset_plus_pub(handle: &RuntimeHandle, plus: u64) -> Result<u64> {
    resolve_offset_plus(handle, plus)
}

fn resolve_offset_plus(handle: &RuntimeHandle, plus: u64) -> Result<u64> {
    let effective_offset = if handle.fixed.offset_space.is_some() {
        handle.fixed.temp_offset
    } else {
        handle.fixed.offset_offset
    };
    let is_const_space = handle
        .fixed
        .space
        .as_ref()
        .ok_or_else(|| anyhow!("offset_plus handle missing primary space metadata"))?
        .name
        == "const";
    if !is_const_space {
        Ok(effective_offset.wrapping_add(plus & 0xFFFF))
    } else {
        let shift_bits = u32::try_from(((plus >> 16).wrapping_mul(8)) & 0x3f)
            .expect("masked Java shift amount fits u32");
        Ok(effective_offset >> shift_bits)
    }
}

pub(super) fn reject_non_offset_handle_plus(plus: Option<u64>, role: &str) -> Result<()> {
    if let Some(plus) = plus {
        bail!("{role} non-offset_plus handle unexpectedly carried plus {plus}");
    }
    Ok(())
}

fn const_varnode(value: u64, size: u32) -> Varnode {
    Varnode::constant(u64_to_i64_bits(value), size)
}

fn space_id_const_varnode(space: &CompiledSpaceRef, role: &str) -> Result<Varnode> {
    let value = i64::try_from(space.index)
        .map_err(|_| anyhow!("{role} space id {} exceeds i64", space.index))?;
    Ok(Varnode::constant(value, 4))
}

fn build_operand_index_from_op(op: &CompiledOpTpl) -> Result<usize> {
    let input = op
        .inputs
        .first()
        .ok_or_else(|| anyhow!("BUILD template missing operand input"))?;
    let CompiledVarnodeTpl::Varnode { offset, .. } = input else {
        bail!("BUILD template operand input must be a VarnodeTpl");
    };
    let CompiledConstTpl::Real { value } = offset.as_ref() else {
        bail!("BUILD template operand offset must be a real operand index");
    };
    usize::try_from(*value).map_err(|_| anyhow!("BUILD operand index {value} exceeds usize"))
}

fn label_id_from_op_tpl(op: &CompiledOpTpl) -> Result<u64> {
    if op.output.is_some() || op.inputs.len() != 1 {
        bail!("LABEL template shape is unsupported");
    }
    let CompiledVarnodeTpl::Varnode { offset, .. } = &op.inputs[0] else {
        bail!("LABEL template input must be a constant varnode");
    };
    let CompiledConstTpl::Real { value } = offset.as_ref() else {
        bail!("LABEL template input offset must be a real label id");
    };
    Ok(*value)
}

fn required_const_tpl_u32(value: Option<u64>, role: &str) -> Result<u32> {
    let value = value.ok_or_else(|| anyhow!("{role} is missing"))?;
    u32::try_from(value).map_err(|_| anyhow!("{role} value {value} exceeds u32"))
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
    /// Ghidra `PcodeEmit` fallOffset: decoded instruction length used for `inst_next`.
    pub instruction_length: Option<u64>,
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
    let mut emitter =
        CompiledTableEmitter::new(compiled, native, address, memory_window, memory_base, flow);
    // If the template uses InstNext2 (delay-slot architectures), pre-decode the
    // delay-slot instruction to get its actual length.
    if (uses_inst_next2(&decoded.constructor_template.ops)
        || uses_delay_slot_indirect(&decoded.constructor_template.ops))
        && !memory_window.is_empty()
    {
        emitter.precompute_delay_slot_length(decoded.length)?;
    }
    let details = RuntimeTemplateEvaluator::new(&mut emitter)
        .emit(&compiled.entry_id, decoded)
        .map_err(|err| template_emit_error(compiled, err))?;
    Ok((emitter.finish()?, details))
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
        CompiledVarnodeTpl::HandleTpl(_) => {
            bail!("INDIRECT delay placeholder has unexpected HandleTpl shape")
        }
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
        op.output.as_ref().is_some_and(check_varnode) || op.inputs.iter().any(check_varnode)
    })
}

pub(super) fn template_emit_error(
    compiled: &CompiledFrontend,
    err: anyhow::Error,
) -> anyhow::Error {
    let msg = err.to_string();
    if msg.contains("HandleTpl") || msg.contains("ConstTpl") || msg.contains("unsupported") {
        RuntimeSleighError::UnsupportedPcodeTemplate {
            language: compiled.entry_id.clone(),
            reason: format!("emission_time_template_resolution_failed: {msg}"),
        }
        .into()
    } else {
        err
    }
}

/// Sentinel used to tag branch targets that reference pcode-internal relative labels.
/// Convention follows Ghidra: `-(label_num + 1)` as i64, stored as u64.
/// Any branch target constant with value > RELATIVE_LABEL_SENTINEL_THRESHOLD is a sentinel.
const RELATIVE_LABEL_SENTINEL_THRESHOLD: u64 = u64::MAX - 0x10000;

fn encode_relative_sentinel(label_num: u64) -> Result<u64> {
    let label_num = i64::try_from(label_num)
        .map_err(|_| anyhow!("relative label id {label_num} exceeds i64"))?;
    let sentinel = label_num
        .checked_add(1)
        .and_then(|value| value.checked_neg())
        .ok_or_else(|| anyhow!("relative label sentinel overflow"))?;
    Ok(i64_to_u64_bits(sentinel))
}

fn decode_relative_sentinel(sentinel: u64) -> Option<u64> {
    if sentinel > RELATIVE_LABEL_SENTINEL_THRESHOLD {
        let sentinel = u64_to_i64_bits(sentinel);
        let label_num = sentinel.checked_neg()?.checked_sub(1)?;
        Some(nonnegative_i64_to_u64(label_num)?)
    } else {
        None
    }
}

fn nonnegative_i64_to_u64(value: i64) -> Option<u64> {
    if value < 0 {
        None
    } else {
        Some(value.unsigned_abs())
    }
}

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

impl<'c> CompiledTableEmitter<'c> {
    fn new(
        compiled: &'c CompiledFrontend,
        native: Option<&'c Arc<NativeBackend>>,
        address: u64,
        memory_window: &'c [u8],
        memory_base: u64,
        flow: FlowEmitOptions,
    ) -> Self {
        let uniqbase = compiled.sla_uniqbase;
        Self {
            compiled,
            native,
            memory_window,
            memory_base,
            address,
            emitter: RuntimePcodeEmitter::new(address, uniqbase),
            built_operands: std::collections::BTreeSet::new(),
            exported_build_varnodes: std::collections::BTreeMap::new(),
            unique_space_index: compiled.sla_unique_space_index,
            sla_spaces: compiled.sla_spaces.clone(),
            label_positions: std::collections::BTreeMap::new(),
            delay_slot_length: None,
            flow,
            pcode_build_secnum: -1,
            in_delay_slot: false,
            uniq_mask: compiled.sla_uniqmask,
        }
    }

    /// Pre-compute the delay slot instruction length. Called from the emit wrapper
    /// when instruction bytes are available, so `resolve_const_value(InstNext2)` can
    /// return `inst_next + delay_slot_length` without needing `CompiledFrontend` again.
    fn precompute_delay_slot_length(&mut self, inst_length: usize) -> Result<()> {
        let inst_next_address = self
            .address
            .checked_add(
                u64::try_from(inst_length)
                    .map_err(|_| anyhow!("delay-slot instruction length exceeds u64"))?,
            )
            .ok_or_else(|| anyhow!("delay-slot InstNext address overflowed"))?;
        let inst_next_offset =
            inst_next_address
                .checked_sub(self.memory_base)
                .ok_or_else(|| {
                    anyhow!(
                    "delay-slot instruction at 0x{inst_next_address:x} precedes memory base 0x{:x}",
                    self.memory_base
                )
                })?;
        let inst_next_offset = usize::try_from(inst_next_offset)
            .map_err(|_| anyhow!("delay-slot memory offset exceeds usize"))?;
        let len = decode_instruction_length(
            self.compiled,
            self.native,
            self.memory_window,
            inst_next_address,
            inst_next_offset,
        )?;
        self.delay_slot_length = Some(len);
        Ok(())
    }

    fn truncate_to_pointer_size(space: &CompiledSpaceRef, offset: u64) -> Result<u64> {
        let bits = space
            .addr_size
            .checked_mul(8)
            .ok_or_else(|| anyhow!("address space {} pointer size overflowed", space.name))?;
        if space.addr_size == 0 || bits >= 64 {
            Ok(offset)
        } else {
            Ok(offset & ((1u64 << bits) - 1))
        }
    }

    fn crossbuild_flat_address(
        &mut self,
        tpl: &CompiledVarnodeTpl,
        state: &RuntimeConstructState,
    ) -> Result<(CompiledSpaceRef, u64)> {
        match tpl {
            CompiledVarnodeTpl::Varnode {
                space: space_tpl,
                offset: off_tpl,
                ..
            } => {
                let space = self.resolve_space_tpl(space_tpl, state)?;
                let offset = self.resolve_const_value(off_tpl, state)?;
                Ok((space, offset))
            }
            _ => bail!("CROSSBUILD address input must be a plain varnode template"),
        }
    }

    fn normalize_direct_control_target(&self, target: Varnode) -> Result<Varnode> {
        if !target.is_constant || decode_relative_sentinel(target.offset).is_some() {
            return Ok(target);
        }
        let size = if target.size == 0 {
            self.compiled.sla_default_cur_space_pointer_size()?
        } else {
            target.size
        };
        Ok(Varnode {
            space_id: self.compiled.sla_default_cur_space_index()?,
            offset: target.offset,
            size,
            is_constant: false,
            constant_val: 0,
        })
    }

    fn finish(self) -> Result<Vec<PcodeOp>> {
        let label_positions = self.label_positions;
        let mut ops = self.emitter.finish();
        // resolveRelatives: replace sentinel branch targets with actual relative offsets.
        // Follows Ghidra's PcodeCacher::resolveRelatives() convention.
        for i in 0..ops.len() {
            let opcode = ops[i].opcode;
            if !matches!(opcode, PcodeOpcode::Branch | PcodeOpcode::CBranch) {
                continue;
            }
            // Branch: input[0] is the target. CBranch: input[0] is target, input[1] is cond.
            if let Some(target_vn) = ops[i].inputs.first() {
                if !target_vn.is_constant {
                    continue;
                }
                let raw = i64_to_u64_bits(target_vn.constant_val);
                if let Some(label_num) = decode_relative_sentinel(raw) {
                    if let Some(&label_op_count) = label_positions.get(&label_num) {
                        // Relative offset = label_op_count - (branch_op_index + 1)
                        // Ghidra convention: positive = forward, negative = backward.
                        let branch_op =
                            i64::try_from(i).map_err(|_| anyhow!("branch op index exceeds i64"))?;
                        let label_pos = i64::try_from(label_op_count)
                            .map_err(|_| anyhow!("label op index exceeds i64"))?;
                        let relative = label_pos - branch_op;
                        ops[i].inputs[0] = Varnode::constant(relative, 8);
                    }
                }
            }
        }
        Ok(ops)
    }

    fn emit_op_template(
        &mut self,
        state: &RuntimeConstructState,
        op: &CompiledOpTpl,
    ) -> Result<()> {
        let mnemonic = op.opcode.as_str();
        if !matches!(self.flow.flow_override, RuntimeFlowOverride::None)
            && !self.in_delay_slot
            && matches!(
                op.opcode,
                CompiledOpTplOpcode::Branch
                    | CompiledOpTplOpcode::BranchInd
                    | CompiledOpTplOpcode::Call
                    | CompiledOpTplOpcode::CallInd
                    | CompiledOpTplOpcode::Return
            )
        {
            bail!(
                "FlowOverride {:?} pcode substitution is not supported yet",
                self.flow.flow_override
            );
        }
        match op.opcode {
            CompiledOpTplOpcode::Label => {
                // Record the current emitter op count as this label's position.
                // Labels themselves don't emit pcode ops; they are position markers.
                // Ghidra PcodeCompile::placeLabel encodes the label id in the
                // first input varnode's offset, with no output varnode.
                let label_num = label_id_from_op_tpl(op)?;
                // Use the emitter's actual op count so even recursively emitted ops
                // (via BUILD) are accounted for correctly.
                self.label_positions
                    .insert(label_num, self.emitter.op_count()?);
                Ok(())
            }
            CompiledOpTplOpcode::Return => {
                if op.output.is_some() || op.inputs.len() > 1 {
                    bail!("RETURN template shape is unsupported");
                }
                if let Some(target_tpl) = op.inputs.first() {
                    // Use size 0 (no constraint): address size varies by architecture.
                    let target = self.read_template_varnode(target_tpl, state, 0)?;
                    self.emitter.emit_return_target(target, mnemonic)
                } else {
                    self.emitter.emit_return(mnemonic)
                }
            }
            CompiledOpTplOpcode::Call => {
                let target_tpl = op
                    .inputs
                    .first()
                    .ok_or_else(|| anyhow!("CALL template requires one input"))?;
                // Use size 0: address size is architecture-dependent (4 for 32-bit, 8 for 64-bit).
                let target = self.read_template_varnode(target_tpl, state, 0)?;
                let target = self.normalize_direct_control_target(target)?;
                self.emitter.emit_call(target, mnemonic)
            }
            CompiledOpTplOpcode::CallInd => {
                let target_tpl = op
                    .inputs
                    .first()
                    .ok_or_else(|| anyhow!("CALLIND template requires one input"))?;
                let target = self.read_template_varnode(target_tpl, state, 0)?;
                self.emitter.emit_call_ind(target, mnemonic)
            }
            CompiledOpTplOpcode::Branch => {
                let target_tpl = op
                    .inputs
                    .first()
                    .ok_or_else(|| anyhow!("BRANCH template requires one input"))?;
                // Ghidra principle: BRANCH vs BRANCHIND is determined solely by the
                // SLA template opcode, NOT by the target's address space. A direct
                // jmp with a RAM-space absolute target is still a BRANCH.
                // Use size 0: address size is architecture-dependent.
                let target = self.read_template_varnode(target_tpl, state, 0)?;
                let target = self.normalize_direct_control_target(target)?;
                self.emitter.emit_branch(target, mnemonic)
            }
            CompiledOpTplOpcode::BranchInd => {
                let target_tpl = op
                    .inputs
                    .first()
                    .ok_or_else(|| anyhow!("BRANCHIND template requires one input"))?;
                let target = self.read_template_varnode(target_tpl, state, 0)?;
                self.emitter.emit_branch_ind(target, mnemonic)
            }
            CompiledOpTplOpcode::Copy => {
                let out_tpl = op
                    .output
                    .as_ref()
                    .ok_or_else(|| anyhow!("COPY template requires output"))?;
                let out_size = self.template_varnode_size(out_tpl, state)?;
                let input_tpl = op
                    .inputs
                    .first()
                    .ok_or_else(|| anyhow!("COPY template requires one input"))?;

                // Read input at its natural size (pass 0 = accept any size).
                // The SUBPIECE path below handles any size mismatch.
                let value = self.read_template_varnode(input_tpl, state, 0)?;

                if let Some(target) = self.dynamic_memory_target(out_tpl, state)? {
                    // Ghidra `PcodeEmit` materializes a dynamic output
                    // location as a temporary varnode first, then emits the
                    // backing STORE from that temporary. Do not fold the
                    // parent COPY away for register inputs: raw p-code parity
                    // depends on this two-step location generation.
                    self.emitter
                        .emit_copy(target.temp.clone(), value, mnemonic)?;
                    let store_value = target.temp;
                    if store_value.size != target.size {
                        bail!(
                            "dynamic memory STORE value size {} did not match target size {}",
                            store_value.size,
                            target.size
                        );
                    }
                    self.emitter
                        .emit_store(target.space, target.ptr, store_value, mnemonic)
                } else if value.size > out_size && out_size > 0 {
                    // Size mismatch: source is wider than destination.
                    // Emit SUBPIECE to truncate (matches Ghidra's behavior).
                    let out = self.template_write_target(out_tpl, state)?;
                    let trunc_const = Varnode::constant(0, 4);
                    self.emitter.append_checked(
                        fission_pcode::PcodeOpcode::SubPiece,
                        Some(out.clone()),
                        vec![value, trunc_const],
                        "SUBPIECE",
                    )?;
                    Ok(())
                } else {
                    self.write_template_target(out_tpl, value, state, mnemonic)
                }
            }
            CompiledOpTplOpcode::IntZExt
            | CompiledOpTplOpcode::IntSExt
            | CompiledOpTplOpcode::Int2Comp
            | CompiledOpTplOpcode::IntNegate
            | CompiledOpTplOpcode::BoolNegate
            | CompiledOpTplOpcode::PopCount
            | CompiledOpTplOpcode::LzCount
            | CompiledOpTplOpcode::Cast
            | CompiledOpTplOpcode::FloatNan
            | CompiledOpTplOpcode::FloatNeg
            | CompiledOpTplOpcode::FloatAbs
            | CompiledOpTplOpcode::FloatSqrt
            | CompiledOpTplOpcode::FloatInt2Float
            | CompiledOpTplOpcode::FloatFloat2Float
            | CompiledOpTplOpcode::FloatTrunc
            | CompiledOpTplOpcode::FloatCeil
            | CompiledOpTplOpcode::FloatFloor
            | CompiledOpTplOpcode::FloatRound => {
                let out_tpl = op
                    .output
                    .as_ref()
                    .ok_or_else(|| anyhow!("{} template requires output", mnemonic))?;
                let input_tpl = op
                    .inputs
                    .first()
                    .ok_or_else(|| anyhow!("{} template requires one input", mnemonic))?;
                let input = self.read_template_varnode(input_tpl, state, 0)?;
                let out = self.materialize_write_varnode(out_tpl, state, mnemonic)?;
                let opcode = self.unary_pcode_opcode(op.opcode)?;
                self.emitter
                    .emit_int_unop(opcode, out.clone(), input, mnemonic)?;
                self.commit_template_write_target(out_tpl, out, state, mnemonic)
            }
            CompiledOpTplOpcode::IntAdd
            | CompiledOpTplOpcode::IntSub
            | CompiledOpTplOpcode::IntCarry
            | CompiledOpTplOpcode::IntSCarry
            | CompiledOpTplOpcode::IntSBorrow
            | CompiledOpTplOpcode::IntAnd
            | CompiledOpTplOpcode::IntOr
            | CompiledOpTplOpcode::IntXor
            | CompiledOpTplOpcode::IntMult
            | CompiledOpTplOpcode::IntDiv
            | CompiledOpTplOpcode::IntSDiv
            | CompiledOpTplOpcode::IntRem
            | CompiledOpTplOpcode::IntSRem
            | CompiledOpTplOpcode::IntLeft
            | CompiledOpTplOpcode::IntRight
            | CompiledOpTplOpcode::IntSRight
            | CompiledOpTplOpcode::IntEqual
            | CompiledOpTplOpcode::IntNotEqual
            | CompiledOpTplOpcode::IntLess
            | CompiledOpTplOpcode::IntLessEqual
            | CompiledOpTplOpcode::IntSLess
            | CompiledOpTplOpcode::IntSLessEqual
            | CompiledOpTplOpcode::BoolXor
            | CompiledOpTplOpcode::BoolAnd
            | CompiledOpTplOpcode::BoolOr
            | CompiledOpTplOpcode::FloatEqual
            | CompiledOpTplOpcode::FloatNotEqual
            | CompiledOpTplOpcode::FloatLess
            | CompiledOpTplOpcode::FloatLessEqual
            | CompiledOpTplOpcode::FloatAdd
            | CompiledOpTplOpcode::FloatDiv
            | CompiledOpTplOpcode::FloatMult
            | CompiledOpTplOpcode::FloatSub => {
                let out_tpl = op
                    .output
                    .as_ref()
                    .ok_or_else(|| anyhow!("{} template requires output", mnemonic))?;
                if op.inputs.len() != 2 {
                    bail!("{mnemonic} template requires two inputs");
                }
                let mut lhs = self.read_template_varnode(&op.inputs[0], state, 0)?;
                let mut rhs = self.read_template_varnode(&op.inputs[1], state, 0)?;

                // Enforce P-code invariants: binary op inputs should generally match in size.
                // Ghidra's SLEIGH compiler implicitly folds constants to the required size,
                // but Fission bypasses subconstructors, so we must promote constants here.
                let is_shift_op = matches!(
                    op.opcode,
                    CompiledOpTplOpcode::IntLeft
                        | CompiledOpTplOpcode::IntRight
                        | CompiledOpTplOpcode::IntSRight
                );
                if lhs.is_constant && rhs.size > lhs.size {
                    lhs.size = rhs.size;
                } else if !is_shift_op && rhs.is_constant && lhs.size > rhs.size {
                    rhs.size = lhs.size;
                }

                let out = self.materialize_write_varnode(out_tpl, state, mnemonic)?;
                let opcode = self.binary_pcode_opcode(op.opcode)?;
                self.emitter
                    .emit_int_binop(opcode, out.clone(), lhs, rhs, mnemonic)?;
                self.commit_template_write_target(out_tpl, out, state, mnemonic)
            }
            CompiledOpTplOpcode::Load => {
                let out_tpl = op
                    .output
                    .as_ref()
                    .ok_or_else(|| anyhow!("LOAD template requires output"))?;
                if op.inputs.len() != 2 {
                    bail!("LOAD template requires two inputs");
                }
                let _out_size = self.template_varnode_size(out_tpl, state)?;
                // Space and pointer sizes are architecture-dependent (4 for 32-bit, 8 for 64-bit).
                let space = self.read_template_varnode(&op.inputs[0], state, 0)?;
                let ptr = self.read_template_varnode(&op.inputs[1], state, 0)?;
                let out = self.materialize_write_varnode(out_tpl, state, mnemonic)?;
                self.emitter.emit_load(out.clone(), space, ptr, mnemonic)?;
                self.commit_template_write_target(out_tpl, out, state, mnemonic)
            }
            CompiledOpTplOpcode::SegmentOp
            | CompiledOpTplOpcode::CPoolRef
            | CompiledOpTplOpcode::New
            | CompiledOpTplOpcode::Insert
            | CompiledOpTplOpcode::Extract => {
                let out_tpl = op
                    .output
                    .as_ref()
                    .ok_or_else(|| anyhow!("{} template requires output", mnemonic))?;
                let mut inputs = Vec::with_capacity(op.inputs.len());
                for input_tpl in &op.inputs {
                    inputs.push(self.read_template_varnode(input_tpl, state, 0)?);
                }
                let out = self.materialize_write_varnode(out_tpl, state, mnemonic)?;
                let opcode = self.dataflow_pcode_opcode(op.opcode)?;
                self.emitter
                    .append_checked(opcode, Some(out.clone()), inputs, mnemonic)?;
                self.commit_template_write_target(out_tpl, out, state, mnemonic)
            }
            CompiledOpTplOpcode::Store => {
                if op.output.is_some() || op.inputs.len() != 3 {
                    bail!("STORE template requires three inputs and no output");
                }
                // Space and pointer sizes are architecture-dependent (4 for 32-bit, 8 for 64-bit).
                let space = self.read_template_varnode(&op.inputs[0], state, 0)?;
                let ptr = self.read_template_varnode(&op.inputs[1], state, 0)?;
                let value_size = self.template_varnode_size(&op.inputs[2], state)?;
                let value = self.read_template_varnode(&op.inputs[2], state, value_size)?;
                self.emitter.emit_store(space, ptr, value, mnemonic)
            }
            CompiledOpTplOpcode::Piece
            | CompiledOpTplOpcode::Subpiece
            | CompiledOpTplOpcode::CBranch => {
                let out_tpl = op.output.as_ref();
                if matches!(op.opcode, CompiledOpTplOpcode::CBranch) {
                    if out_tpl.is_some() || op.inputs.len() != 2 {
                        bail!("CBRANCH template requires two inputs and no output");
                    }
                    // Target size is architecture-dependent (4 for 32-bit, 8 for 64-bit).
                    let target = self.read_template_varnode(&op.inputs[0], state, 0)?;
                    let target = self.normalize_direct_control_target(target)?;
                    let cond = self.read_template_varnode(&op.inputs[1], state, 1)?;
                    self.emitter.emit_cbranch(target, cond, mnemonic)
                } else {
                    let out_tpl =
                        out_tpl.ok_or_else(|| anyhow!("{} template requires output", mnemonic))?;
                    if op.inputs.len() != 2 {
                        bail!("{mnemonic} template requires two inputs");
                    }
                    let out_size = self.template_varnode_size(out_tpl, state)?;
                    let lhs_expected_size = if matches!(op.opcode, CompiledOpTplOpcode::Subpiece) {
                        0
                    } else {
                        out_size
                    };
                    let lhs =
                        self.read_template_varnode(&op.inputs[0], state, lhs_expected_size)?;
                    let rhs_size = self.template_varnode_size(&op.inputs[1], state)?;
                    let rhs = self.read_template_varnode(&op.inputs[1], state, rhs_size)?;
                    let out = self.materialize_write_varnode(out_tpl, state, mnemonic)?;
                    let opcode = self.binary_pcode_opcode(op.opcode)?;
                    self.emitter
                        .emit_int_binop(opcode, out.clone(), lhs, rhs, mnemonic)?;
                    self.commit_template_write_target(out_tpl, out, state, mnemonic)
                }
            }
            // Build: subtable inlining directive. Ghidra executes the matched
            // operand/sub-constructor template; it does not synthesize an
            // architecture-specific effective-address expression from the
            // already decoded display operand.
            CompiledOpTplOpcode::Build => {
                let idx = build_operand_index_from_op(op)?;
                if std::env::var("FISSION_BUILD_DEBUG").is_ok() {
                    let handle = state.handles.get(idx);
                    let has_sub = handle
                        .as_ref()
                        .and_then(|h| h.subtable_state.as_ref())
                        .is_some();
                    let template_src = handle
                        .as_ref()
                        .and_then(|h| h.subtable_state.as_ref())
                        .map(|s| format!("{:?}", s.constructor_template.template_source))
                        .unwrap_or_else(|| "None".to_string());
                    let ops_count = handle
                        .as_ref()
                        .and_then(|h| h.subtable_state.as_ref())
                        .map_or(0, |s| s.constructor_template.ops.len());
                    eprintln!(
                        "[BUILD] operand={idx} has_sub={has_sub} template_src={template_src} ops={ops_count} already_built={}",
                        self.built_operands.contains(&idx)
                    );
                }
                self.emit_build_operand(state, idx)?;
                Ok(())
            }
            // CallOther: user-defined pcodeop. Ghidra emits this as a real
            // CALLOTHER P-code op. Input[0] is the pcodeop index.
            CompiledOpTplOpcode::CallOther => {
                let mut inputs = Vec::new();
                for input in &op.inputs {
                    let size = self.template_varnode_size(input, state)?;
                    inputs.push(self.read_template_varnode(input, state, size)?);
                }
                let output = if let Some(ref out_ref) = op.output {
                    Some(self.materialize_write_varnode(out_ref, state, mnemonic)?)
                } else {
                    None
                };
                self.emitter.emit_callother(output, inputs, mnemonic)
            }
            CompiledOpTplOpcode::CrossBuild => {
                if self.pcode_build_secnum >= 0 {
                    bail!(
                        "CROSSBUILD (PTRSUB) while emitting named pcode section {} (Ghidra recursion guard)",
                        self.pcode_build_secnum
                    );
                }
                if op.inputs.len() < 2 {
                    bail!("CROSSBUILD (PTRSUB) requires two varnode inputs");
                }
                let (target_space, target_offset) =
                    self.crossbuild_flat_address(&op.inputs[0], state)?;
                let target_pc = Self::truncate_to_pointer_size(&target_space, target_offset)?;
                let section = ptrsub_named_section_index(op)?;
                let ctx_reg = self.flow.instruction_context_register;
                let ctx_mask = self.flow.instruction_context_known_mask;
                let cross_state = try_bind_runtime_state_at(
                    self.compiled,
                    self.native,
                    self.memory_window,
                    self.memory_base,
                    target_pc,
                    ctx_reg,
                    ctx_mask,
                )?;
                let unique_seed =
                    RuntimePcodeEmitter::unique_seed_for_address(self.uniq_mask, target_pc);
                let saved_emit = self.emitter.emit_context();
                let saved_labels = std::mem::take(&mut self.label_positions);
                let saved_built = std::mem::take(&mut self.built_operands);
                self.emitter.set_emit_context(target_pc, unique_seed);
                let emit_result = (|| -> Result<()> {
                    let Some(Some(named)) = cross_state.named_templates.get(section) else {
                        bail!(
                            "crossbuild: named template section {section} missing at target 0x{target_pc:x}"
                        );
                    };
                    for cop in &named.ops {
                        self.emit_op_template(&cross_state, cop)?;
                    }
                    Ok(())
                })();
                self.built_operands = saved_built;
                self.label_positions = saved_labels;
                self.emitter.set_emit_context(saved_emit.0, saved_emit.1);
                emit_result
            }
            CompiledOpTplOpcode::DelaySlotIndirect => {
                if self.in_delay_slot {
                    bail!(
                        "INDIRECT delay-slot recursion at pcode address 0x{:x}",
                        self.address
                    );
                }
                let delay_total = indirect_placeholder_delay_bytes(op)?;
                self.in_delay_slot = true;
                let ctx_reg = self.flow.instruction_context_register;
                let ctx_mask = self.flow.instruction_context_known_mask;
                let emit_result = (|| -> Result<()> {
                    let mut fall_offset = u64::try_from(state.length)
                        .map_err(|_| anyhow!("delay slot state length exceeds u64"))?;
                    let mut byte_count: u32 = 0;
                    while byte_count < delay_total {
                        let slot_pc = self
                            .address
                            .checked_add(fall_offset)
                            .ok_or_else(|| anyhow!("delay slot address overflow"))?;
                        let slot_state = try_bind_runtime_state_at(
                            self.compiled,
                            self.native,
                            self.memory_window,
                            self.memory_base,
                            slot_pc,
                            ctx_reg,
                            ctx_mask,
                        )?;
                        let slot_len = u32::try_from(slot_state.length)
                            .map_err(|_| anyhow!("delay slot length exceeds u32"))?;
                        if slot_len == 0 {
                            bail!("delay slot decode returned zero length at 0x{slot_pc:x}");
                        }
                        let unique_seed =
                            RuntimePcodeEmitter::unique_seed_for_address(self.uniq_mask, slot_pc);
                        let saved_emit = self.emitter.emit_context();
                        let saved_labels = std::mem::take(&mut self.label_positions);
                        let saved_built = std::mem::take(&mut self.built_operands);
                        self.emitter.set_emit_context(slot_pc, unique_seed);
                        let inner = (|| -> Result<()> {
                            for cop in &slot_state.constructor_template.ops {
                                self.emit_op_template(&slot_state, cop)?;
                            }
                            Ok(())
                        })();
                        self.built_operands = saved_built;
                        self.label_positions = saved_labels;
                        self.emitter.set_emit_context(saved_emit.0, saved_emit.1);
                        inner?;
                        fall_offset = fall_offset
                            .checked_add(u64::from(slot_len))
                            .ok_or_else(|| anyhow!("delay slot fallthrough offset overflow"))?;
                        byte_count = byte_count
                            .checked_add(slot_len)
                            .ok_or_else(|| anyhow!("delay slot byte count overflow"))?;
                    }
                    Ok(())
                })();
                self.in_delay_slot = false;
                emit_result
            }
            CompiledOpTplOpcode::Unsupported => bail!(
                "compiled op template {} is not executable in compiled-table cutover",
                mnemonic
            ),
        }
    }

    fn emit_build_operand(
        &mut self,
        state: &RuntimeConstructState,
        operand_index: usize,
    ) -> Result<()> {
        if !self.built_operands.insert(operand_index) {
            return Ok(());
        }
        let saved_sec = self.pcode_build_secnum;
        let result = (|| -> Result<()> {
            let handle = state
                .handles
                .get(operand_index)
                .ok_or_else(|| anyhow!("BUILD operand {operand_index} has no bound handle"))?;
            let Some(child) = handle.subtable_state.as_deref() else {
                return Ok(());
            };
            if child.constructor_template.template_source != CompiledTemplateSource::SpecDerived {
                bail!(
                    "BUILD operand {operand_index} is not backed by a SpecDerived subconstructor"
                );
            }
            if std::env::var("FISSION_BUILD_DEBUG").is_ok() {
                eprintln!(
                    "[emit_build_operand] operand={operand_index} mnemonic={} child_ops_count={}",
                    child.mnemonic,
                    child.constructor_template.ops.len()
                );
                for (i, child_op) in child.constructor_template.ops.iter().enumerate() {
                    eprintln!("  [child_op {}] {:?}", i, child_op.opcode);
                }
            }
            // Ghidra: each sub-constructor's BUILD scope is independent. Save and
            // restore `built_operands` so that child operand indices (e.g. 0 for the
            // base register within a memory-addressing subtable) are not mistakenly
            // treated as "already built" because the parent used the same numeric
            // index for a different operand (e.g. 0 for the memory operand itself).
            let saved_built = std::mem::take(&mut self.built_operands);
            if child.constructor_template.ops.is_empty() {
                // Ghidra PcodeEmit.build(): when the child template is empty (null in Ghidra),
                // fall back to the parent's named section indexed by the operand number.
                // Corresponds to: prototype.getNamedTempl(secnum) where secnum = operand_index.
                self.pcode_build_secnum = build_operand_section_index(operand_index)?;
                if let Some(Some(named_tpl)) = state.named_templates.get(operand_index) {
                    for named_op in &named_tpl.ops {
                        self.emit_op_template(child, named_op)?;
                    }
                }
            } else {
                self.pcode_build_secnum = -1;
                for child_op in &child.constructor_template.ops {
                    self.emit_op_template(child, child_op)?;
                }
            }
            self.built_operands = saved_built;
            if let Some(exported) = child.exported_handle.as_ref() {
                if let Ok(varnode) = varnode_from_fixed_handle(&exported.fixed) {
                    let handle_key = exported_build_handle_key(operand_index)?;
                    self.exported_build_varnodes.insert(handle_key, varnode);
                }
            }
            Ok(())
        })();
        self.pcode_build_secnum = saved_sec;
        result
    }

    fn template_varnode_size(
        &mut self,
        template: &CompiledVarnodeTpl,
        state: &RuntimeConstructState,
    ) -> Result<u32> {
        match template {
            CompiledVarnodeTpl::Varnode { size, .. } => {
                let value = self.resolve_const_value(size, state)?;
                u32::try_from(value).map_err(|_| anyhow!("VarnodeTpl size {value} exceeds u32"))
            }
            CompiledVarnodeTpl::HandleTpl(handle_tpl) => {
                let size = handle_tpl
                    .size
                    .as_ref()
                    .ok_or_else(|| anyhow!("HandleTpl size is missing"))?;
                let value = self.resolve_const_value(size, state)?;
                u32::try_from(value).map_err(|_| anyhow!("HandleTpl size {value} exceeds u32"))
            }
        }
    }

    fn read_template_varnode(
        &mut self,
        template: &CompiledVarnodeTpl,
        state: &RuntimeConstructState,
        expected_size: u32,
    ) -> Result<Varnode> {
        match template {
            CompiledVarnodeTpl::Varnode { .. } | CompiledVarnodeTpl::HandleTpl(_) => {
                // Ghidra: check isDynamic before resolving — emit LOAD for dynamic memory inputs.
                if let Some(loaded) = self.dynamic_memory_source(template, state)? {
                    if expected_size > 0 && loaded.size != expected_size {
                        bail!(
                            "dynamic memory source size {} did not match expected size {expected_size}",
                            loaded.size
                        );
                    }
                    return Ok(loaded);
                }
                let varnode = self.resolve_varnode_tpl(template, state)?;
                if expected_size > 0 && varnode.size != expected_size {
                    bail!(
                        "VarnodeTpl size {} did not match expected size {expected_size}",
                        varnode.size
                    );
                }
                Ok(varnode)
            }
        }
    }

    fn materialize_write_varnode(
        &mut self,
        template: &CompiledVarnodeTpl,
        state: &RuntimeConstructState,
        _mnemonic: &str,
    ) -> Result<Varnode> {
        self.template_write_target(template, state)
    }

    fn commit_template_write_target(
        &mut self,
        template: &CompiledVarnodeTpl,
        value: Varnode,
        state: &RuntimeConstructState,
        mnemonic: &str,
    ) -> Result<()> {
        let _ = (value, mnemonic);
        self.template_write_target(template, state).map(|_| ())
    }

    fn write_template_target(
        &mut self,
        template: &CompiledVarnodeTpl,
        value: Varnode,
        state: &RuntimeConstructState,
        mnemonic: &str,
    ) -> Result<()> {
        let out = self.template_write_target(template, state)?;
        self.emitter.emit_copy(out, value, mnemonic)
    }

    fn template_write_target(
        &mut self,
        template: &CompiledVarnodeTpl,
        state: &RuntimeConstructState,
    ) -> Result<Varnode> {
        match template {
            CompiledVarnodeTpl::Varnode { .. } | CompiledVarnodeTpl::HandleTpl(_) => {
                self.resolve_varnode_tpl(template, state)
            }
        }
    }

    fn dynamic_memory_target(
        &mut self,
        template: &CompiledVarnodeTpl,
        state: &RuntimeConstructState,
    ) -> Result<Option<DynamicMemoryTarget>> {
        let CompiledVarnodeTpl::Varnode {
            space,
            offset,
            size,
        } = template
        else {
            return Ok(None);
        };
        let Some(handle_index) =
            handle_selector_index_in_space(space, CompiledHandleSelector::Space)
        else {
            return Ok(None);
        };
        if !matches_handle_selector(offset, handle_index, CompiledHandleSelector::Offset) {
            return Ok(None);
        }
        let handle = state
            .handles
            .get(handle_index)
            .ok_or_else(|| anyhow!("handle {} is missing or unresolved", handle_index))?;
        if !handle
            .fixed
            .space
            .as_ref()
            .is_some_and(|space| space.index == 0x1b1 || space.name == "ram")
        {
            return Ok(None);
        }
        if handle.fixed.offset_space.is_none() {
            return Ok(None);
        }
        if !handle.fixed.fixable {
            bail!("dynamic memory handle {handle_index} is not fixable");
        }
        let space = handle
            .fixed
            .space
            .as_ref()
            .ok_or_else(|| anyhow!("dynamic memory handle {handle_index} missing space"))?;
        let offset_space =
            handle.fixed.offset_space.as_ref().ok_or_else(|| {
                anyhow!("dynamic memory handle {handle_index} missing offset space")
            })?;
        let temp_space =
            handle.fixed.temp_space.as_ref().ok_or_else(|| {
                anyhow!("dynamic memory handle {handle_index} missing temp space")
            })?;
        let target_size = u32::try_from(self.resolve_const_value(size, state)?)
            .map_err(|_| anyhow!("dynamic memory target size exceeds u32"))?;
        let ptr = Varnode {
            space_id: offset_space.index,
            offset: handle.fixed.offset_offset,
            size: handle.fixed.offset_size,
            is_constant: false,
            constant_val: 0,
        };
        Ok(Some(DynamicMemoryTarget {
            space: space_id_const_varnode(space, "dynamic memory target")?,
            ptr,
            temp: Varnode {
                space_id: temp_space.index,
                offset: handle.fixed.temp_offset,
                size: target_size,
                is_constant: false,
                constant_val: 0,
            },
            size: target_size,
        }))
    }

    /// Ghidra `VarnodeTpl.isDynamic(walker)` path for **input** varnodes.
    ///
    /// When the offset ConstTpl of a VarnodeTpl is a HANDLE reference (selector=Offset)
    /// and the resolved FixedHandle has a non-null `offset_space`, Ghidra's `PcodeEmit.dump()`
    /// emits a synthetic LOAD op to materialise the value before the parent op.
    ///
    /// Returns the temp varnode (LOAD output) if a LOAD was emitted; `None` otherwise.
    fn dynamic_memory_source(
        &mut self,
        template: &CompiledVarnodeTpl,
        state: &RuntimeConstructState,
    ) -> Result<Option<Varnode>> {
        // Only `Varnode { space, offset, size }` can be dynamic (matches Ghidra's VarnodeTpl).
        let CompiledVarnodeTpl::Varnode { offset, size, .. } = template else {
            return Ok(None);
        };

        // isDynamic condition 1: offset.getType() == ConstTpl.HANDLE (selector = Offset)
        let Some(handle_index) = handle_selector_index(offset, CompiledHandleSelector::Offset)
        else {
            return Ok(None);
        };

        let handle = state
            .handles
            .get(handle_index)
            .ok_or_else(|| anyhow!("handle {} is missing or unresolved", handle_index))?;

        // isDynamic condition 2: fixedHandle.offset_space != null
        if handle.fixed.offset_space.is_none() {
            return Ok(None);
        }

        let space = handle
            .fixed
            .space
            .as_ref()
            .ok_or_else(|| anyhow!("dynamic source handle {} missing space", handle_index))?;
        let offset_space = handle.fixed.offset_space.as_ref().ok_or_else(|| {
            anyhow!(
                "dynamic source handle {} missing offset_space",
                handle_index
            )
        })?;
        let temp_space =
            handle.fixed.temp_space.as_ref().ok_or_else(|| {
                anyhow!("dynamic source handle {} missing temp_space", handle_index)
            })?;

        let data_size = u32::try_from(self.resolve_const_value(size, state)?)
            .map_err(|_| anyhow!("dynamic memory source size exceeds u32"))?;

        // Ghidra: generateLocation → incache[i] = (temp_space, temp_offset, size)
        //         generatePointer  → dyncache[1] = (offset_space, offset_offset, offset_size)
        //         dump LOAD(ram_id, ptr) → temp
        let space_id = space_id_const_varnode(space, "dynamic memory source")?;
        let ptr = Varnode {
            space_id: offset_space.index,
            offset: handle.fixed.offset_offset,
            size: handle.fixed.offset_size,
            is_constant: false,
            constant_val: 0,
        };
        let temp = Varnode {
            space_id: temp_space.index,
            offset: handle.fixed.temp_offset,
            size: data_size,
            is_constant: false,
            constant_val: 0,
        };

        self.emitter
            .emit_load(temp.clone(), space_id, ptr, "LOAD")?;

        Ok(Some(temp))
    }

    fn resolve_varnode_tpl(
        &mut self,
        template: &CompiledVarnodeTpl,
        state: &RuntimeConstructState,
    ) -> Result<Varnode> {
        match template {
            CompiledVarnodeTpl::Varnode {
                space,
                offset,
                size,
            } => {
                if let Some(exported_vn) =
                    self.exported_build_varnode(space, offset, size, state)?
                {
                    return Ok(exported_vn);
                }
                let space = self.resolve_space_tpl(space, state)?;
                if (space.index == 0 || space.name == "const")
                    && handle_selector_index(offset, CompiledHandleSelector::Offset).is_some()
                {
                    let handle_index =
                        handle_selector_index(offset, CompiledHandleSelector::Offset)
                            .expect("checked above");
                    let handle = state.handles.get(handle_index).ok_or_else(|| {
                        anyhow!("handle {} is missing or unresolved", handle_index)
                    })?;
                    if let Some(offset_space) = &handle.fixed.offset_space {
                        let size = u32::try_from(self.resolve_const_value(size, state)?)
                            .map_err(|_| anyhow!("VarnodeTpl size exceeds u32"))?;
                        if offset_space.index == 0 || offset_space.name == "const" {
                            return Ok(const_varnode(handle.fixed.offset_offset, size));
                        }
                        return Ok(Varnode {
                            space_id: offset_space.index,
                            offset: handle.fixed.offset_offset,
                            size,
                            is_constant: false,
                            constant_val: 0,
                        });
                    }
                }
                let offset = self.resolve_const_value(offset, state)?;
                let size = u32::try_from(self.resolve_const_value(size, state)?)
                    .map_err(|_| anyhow!("VarnodeTpl size exceeds u32"))?;
                if space.index == 0 || space.name == "const" {
                    return Ok(const_varnode(offset, size));
                }
                Ok(Varnode {
                    space_id: space.index,
                    offset,
                    size,
                    is_constant: false,
                    constant_val: 0,
                })
            }
            CompiledVarnodeTpl::HandleTpl(handle_tpl) => {
                if let Some(ptr_space_tpl) = &handle_tpl.ptr_space {
                    let ptr_space = self.resolve_space_tpl(ptr_space_tpl, state)?;
                    let ptr_offset_tpl = handle_tpl
                        .ptr_offset
                        .as_ref()
                        .ok_or_else(|| anyhow!("HandleTpl missing ptr_offset"))?;
                    let ptr_offset = self.resolve_const_value(ptr_offset_tpl, state)?;
                    let ptr_size = handle_tpl
                        .ptr_size
                        .as_ref()
                        .map(|size| self.resolve_const_value(size, state))
                        .transpose()
                        .and_then(|size| required_const_tpl_u32(size, "HandleTpl ptr_size"))?;
                    if ptr_space.index == 0 || ptr_space.name == "const" {
                        return Ok(const_varnode(ptr_offset, ptr_size));
                    }
                    return Ok(Varnode {
                        space_id: ptr_space.index,
                        offset: ptr_offset,
                        size: ptr_size,
                        is_constant: false,
                        constant_val: 0,
                    });
                }
                let space = if let Some(space) = &handle_tpl.space {
                    self.resolve_space_tpl(space, state)?
                } else {
                    bail!("HandleTpl missing space")
                };
                let offset = if let Some(offset) = &handle_tpl.ptr_offset {
                    self.resolve_const_value(offset, state)?
                } else {
                    bail!("HandleTpl missing ptr_offset")
                };
                let size_tpl = handle_tpl
                    .size
                    .as_ref()
                    .ok_or_else(|| anyhow!("HandleTpl size is missing"))?;
                let size = u32::try_from(self.resolve_const_value(size_tpl, state)?)
                    .map_err(|_| anyhow!("HandleTpl size exceeds u32"))?;
                if space.index == 0 || space.name == "const" {
                    return Ok(const_varnode(offset, size));
                }
                Ok(Varnode {
                    space_id: space.index,
                    offset,
                    size,
                    is_constant: false,
                    constant_val: 0,
                })
            }
        }
    }

    fn exported_build_varnode(
        &mut self,
        space: &CompiledSpaceTpl,
        offset: &CompiledConstTpl,
        size: &CompiledConstTpl,
        state: &RuntimeConstructState,
    ) -> Result<Option<Varnode>> {
        let Some(handle_index) =
            negative_handle_selector_index_in_space(space, CompiledHandleSelector::Space)
        else {
            return Ok(None);
        };
        if !matches_negative_handle_selector(offset, handle_index, CompiledHandleSelector::Offset)
            || !matches_negative_handle_selector(size, handle_index, CompiledHandleSelector::Size)
        {
            return Ok(None);
        }
        let varnode = self
            .exported_build_varnodes
            .get(&handle_index)
            .cloned()
            .ok_or_else(|| anyhow!("exported BUILD handle {handle_index} is missing"))?;
        let _ = (size, state);
        Ok(Some(varnode))
    }

    fn resolve_space_tpl(
        &mut self,
        template: &CompiledSpaceTpl,
        state: &RuntimeConstructState,
    ) -> Result<CompiledSpaceRef> {
        match template {
            CompiledSpaceTpl::SpaceRef(space) => Ok(space.clone()),
            CompiledSpaceTpl::Const(const_tpl) => {
                let space_id = self.resolve_const_value(const_tpl, state)?;
                // Look up the space name from the SLA-derived space table instead
                // of using a hardcoded architecture-specific mapping.
                if let Some(space_ref) = self.sla_spaces.get(&space_id) {
                    return Ok(space_ref.clone());
                }
                bail!("SpaceTpl references unknown SLA space id {space_id}")
            }
        }
    }

    fn resolve_const_value(
        &mut self,
        template: &CompiledConstTpl,
        state: &RuntimeConstructState,
    ) -> Result<u64> {
        match template {
            CompiledConstTpl::Real { value } => Ok(*value),
            CompiledConstTpl::Integer { value, .. } if *value >= 0 => {
                u64::try_from(*value).map_err(|_| anyhow!("positive integer ConstTpl exceeds u64"))
            }
            CompiledConstTpl::Integer { value, .. } => Ok(i64_to_u64_bits(*value)),
            CompiledConstTpl::InstStart => Ok(self.address),
            CompiledConstTpl::InstNext => {
                let fall_offset = self.inst_next_fall_offset(state)?;
                self.address
                    .checked_add(fall_offset)
                    .ok_or_else(|| anyhow!("InstNext address overflowed"))
            }
            CompiledConstTpl::SpaceId(space) => Ok(space.index),
            CompiledConstTpl::Handle {
                handle_index,
                selector,
                plus,
            } => {
                let handle_index = checked_handle_index(*handle_index, "template")?;
                let handle = state
                    .handles
                    .get(handle_index)
                    .ok_or_else(|| anyhow!("handle {} is missing or unresolved", handle_index))?;

                if matches!(selector, CompiledHandleSelector::OffsetPlus) {
                    let plus = plus.ok_or_else(|| anyhow!("offset_plus handle is missing plus"))?;
                    return resolve_offset_plus(handle, plus);
                }

                reject_non_offset_handle_plus(*plus, "template")?;
                let val = self.resolve_fixed_handle_selector(handle, *selector)?;
                Ok(val)
            }
            CompiledConstTpl::Relative { value: label_num } => {
                // Emit a sentinel value; resolveRelatives() in finish() will replace it
                // with the actual relative op offset after all ops are emitted.
                // Uses Ghidra's convention: -(label_num + 1) so the value is in the
                // RELATIVE_LABEL_SENTINEL_THRESHOLD range (large unsigned values).
                encode_relative_sentinel(*label_num)
            }
            CompiledConstTpl::InstNext2 => {
                // Ghidra: inst_next2 = inst_next + delay_slot_instruction_length.
                // `inst_next` = address of the instruction after the current one.
                // `delay_slot_length` = actual length of the instruction in the delay slot,
                // pre-decoded in `precompute_delay_slot_length`.
                let inst_next = self
                    .address
                    .checked_add(
                        u64::try_from(state.length)
                            .map_err(|_| anyhow!("InstNext2 state length exceeds u64"))?,
                    )
                    .ok_or_else(|| anyhow!("InstNext2 base address overflowed"))?;
                let delay_len = self
                    .delay_slot_length
                    .ok_or_else(|| anyhow!("InstNext2 requires decoded delay-slot length"))?;
                let delay_len = u64::from(delay_len);
                inst_next
                    .checked_add(delay_len)
                    .ok_or_else(|| anyhow!("InstNext2 address overflowed"))
            }
            CompiledConstTpl::CurSpace => self.compiled.sla_default_cur_space_index(),
            CompiledConstTpl::CurSpaceSize => self
                .compiled
                .sla_default_cur_space_pointer_size()
                .map(u64::from),
            CompiledConstTpl::FlowRef => self
                .flow
                .flow_ref_addr
                .ok_or_else(|| anyhow!("ConstTpl FlowRef requires FlowEmitOptions.flow_ref_addr")),
            CompiledConstTpl::FlowRefSize => {
                let idx = self.flow.flow_ref_space_index.ok_or_else(|| {
                    anyhow!("ConstTpl FlowRefSize requires FlowEmitOptions.flow_ref_space_index")
                })?;
                let space = self.compiled.sla_spaces.get(&idx).ok_or_else(|| {
                    anyhow!("FlowRefSize space index {idx} missing from sla_spaces")
                })?;
                if space.addr_size == 0 {
                    bail!("FlowRefSize space {} has addr_size=0", space.name);
                }
                Ok(u64::from(space.addr_size))
            }
            CompiledConstTpl::FlowDest => self.flow.flow_dest_addr.ok_or_else(|| {
                anyhow!("ConstTpl FlowDest requires FlowEmitOptions.flow_dest_addr")
            }),
            CompiledConstTpl::FlowDestSize => {
                let idx = self.flow.flow_dest_space_index.ok_or_else(|| {
                    anyhow!("ConstTpl FlowDestSize requires FlowEmitOptions.flow_dest_space_index")
                })?;
                let space = self.compiled.sla_spaces.get(&idx).ok_or_else(|| {
                    anyhow!("FlowDestSize space index {idx} missing from sla_spaces")
                })?;
                if space.addr_size == 0 {
                    bail!("FlowDestSize space {} has addr_size=0", space.name);
                }
                Ok(u64::from(space.addr_size))
            }
        }
    }

    fn inst_next_fall_offset(&self, state: &RuntimeConstructState) -> Result<u64> {
        if state.length > 0 {
            return u64::try_from(state.length)
                .map_err(|_| anyhow!("InstNext state length exceeds u64"));
        }
        self.flow
            .instruction_length
            .ok_or_else(|| anyhow!("InstNext requires decoded instruction length"))
    }

    fn resolve_fixed_handle_selector(
        &self,
        handle: &RuntimeHandle,
        selector: CompiledHandleSelector,
    ) -> Result<u64> {
        match selector {
            CompiledHandleSelector::Space => {
                // Ghidra: ConstTpl.fixSpace() — when offset_space != null (dynamic),
                // returns temp_space rather than the primary space.
                if handle.fixed.offset_space.is_some() {
                    handle
                        .fixed
                        .temp_space
                        .as_ref()
                        .map(|s| s.index)
                        .ok_or_else(|| {
                            anyhow!("dynamic handle missing temp_space for Space selector")
                        })
                } else {
                    handle
                        .fixed
                        .space
                        .as_ref()
                        .map(|space| space.index)
                        .ok_or_else(|| anyhow!("fixed handle missing space"))
                }
            }
            CompiledHandleSelector::Offset => {
                // Ghidra: ConstTpl.fix() for V_OFFSET — when offset_space != null (dynamic),
                // returns temp_offset (the output temp of the implicit LOAD) rather than
                // offset_offset (the pointer location).
                if handle.fixed.offset_space.is_some() {
                    Ok(handle.fixed.temp_offset)
                } else {
                    Ok(handle.fixed.offset_offset)
                }
            }
            CompiledHandleSelector::Size => Ok(u64::from(handle.fixed.size)),
            CompiledHandleSelector::OffsetPlus => {
                unreachable!("OffsetPlus is handled before calling resolve_fixed_handle_selector")
            }
        }
    }

    fn unary_pcode_opcode(&self, opcode: CompiledOpTplOpcode) -> Result<PcodeOpcode> {
        Ok(match opcode {
            CompiledOpTplOpcode::IntZExt => PcodeOpcode::IntZExt,
            CompiledOpTplOpcode::IntSExt => PcodeOpcode::IntSExt,
            CompiledOpTplOpcode::Int2Comp => PcodeOpcode::Int2Comp,
            CompiledOpTplOpcode::IntNegate => PcodeOpcode::IntNegate,
            CompiledOpTplOpcode::BoolNegate => PcodeOpcode::BoolNegate,
            CompiledOpTplOpcode::PopCount => PcodeOpcode::PopCount,
            CompiledOpTplOpcode::LzCount => PcodeOpcode::LzCount,
            CompiledOpTplOpcode::Cast => PcodeOpcode::Cast,
            CompiledOpTplOpcode::FloatNan => PcodeOpcode::FloatNan,
            CompiledOpTplOpcode::FloatNeg => PcodeOpcode::FloatNeg,
            CompiledOpTplOpcode::FloatAbs => PcodeOpcode::FloatAbs,
            CompiledOpTplOpcode::FloatSqrt => PcodeOpcode::FloatSqrt,
            CompiledOpTplOpcode::FloatInt2Float => PcodeOpcode::FloatInt2Float,
            CompiledOpTplOpcode::FloatFloat2Float => PcodeOpcode::FloatFloat2Float,
            CompiledOpTplOpcode::FloatTrunc => PcodeOpcode::FloatTrunc,
            CompiledOpTplOpcode::FloatCeil => PcodeOpcode::FloatCeil,
            CompiledOpTplOpcode::FloatFloor => PcodeOpcode::FloatFloor,
            CompiledOpTplOpcode::FloatRound => PcodeOpcode::FloatRound,
            _ => bail!("unsupported unary compiled opcode {}", opcode.as_str()),
        })
    }

    fn binary_pcode_opcode(&self, opcode: CompiledOpTplOpcode) -> Result<PcodeOpcode> {
        Ok(match opcode {
            CompiledOpTplOpcode::IntAdd => PcodeOpcode::IntAdd,
            CompiledOpTplOpcode::IntSub => PcodeOpcode::IntSub,
            CompiledOpTplOpcode::IntCarry => PcodeOpcode::IntCarry,
            CompiledOpTplOpcode::IntSCarry => PcodeOpcode::IntSCarry,
            CompiledOpTplOpcode::IntSBorrow => PcodeOpcode::IntSBorrow,
            CompiledOpTplOpcode::IntAnd => PcodeOpcode::IntAnd,
            CompiledOpTplOpcode::IntOr => PcodeOpcode::IntOr,
            CompiledOpTplOpcode::IntXor => PcodeOpcode::IntXor,
            CompiledOpTplOpcode::IntMult => PcodeOpcode::IntMult,
            CompiledOpTplOpcode::IntDiv => PcodeOpcode::IntDiv,
            CompiledOpTplOpcode::IntSDiv => PcodeOpcode::IntSDiv,
            CompiledOpTplOpcode::IntRem => PcodeOpcode::IntRem,
            CompiledOpTplOpcode::IntSRem => PcodeOpcode::IntSRem,
            CompiledOpTplOpcode::IntLeft => PcodeOpcode::IntLeft,
            CompiledOpTplOpcode::IntRight => PcodeOpcode::IntRight,
            CompiledOpTplOpcode::IntSRight => PcodeOpcode::IntSRight,
            CompiledOpTplOpcode::IntEqual => PcodeOpcode::IntEqual,
            CompiledOpTplOpcode::IntNotEqual => PcodeOpcode::IntNotEqual,
            CompiledOpTplOpcode::IntLess => PcodeOpcode::IntLess,
            CompiledOpTplOpcode::IntLessEqual => PcodeOpcode::IntLessEqual,
            CompiledOpTplOpcode::IntSLess => PcodeOpcode::IntSLess,
            CompiledOpTplOpcode::IntSLessEqual => PcodeOpcode::IntSLessEqual,
            CompiledOpTplOpcode::BoolXor => PcodeOpcode::BoolXor,
            CompiledOpTplOpcode::BoolAnd => PcodeOpcode::BoolAnd,
            CompiledOpTplOpcode::BoolOr => PcodeOpcode::BoolOr,
            CompiledOpTplOpcode::FloatEqual => PcodeOpcode::FloatEqual,
            CompiledOpTplOpcode::FloatNotEqual => PcodeOpcode::FloatNotEqual,
            CompiledOpTplOpcode::FloatLess => PcodeOpcode::FloatLess,
            CompiledOpTplOpcode::FloatLessEqual => PcodeOpcode::FloatLessEqual,
            CompiledOpTplOpcode::FloatAdd => PcodeOpcode::FloatAdd,
            CompiledOpTplOpcode::FloatDiv => PcodeOpcode::FloatDiv,
            CompiledOpTplOpcode::FloatMult => PcodeOpcode::FloatMult,
            CompiledOpTplOpcode::FloatSub => PcodeOpcode::FloatSub,
            CompiledOpTplOpcode::Piece => PcodeOpcode::Piece,
            CompiledOpTplOpcode::Subpiece => PcodeOpcode::SubPiece,
            _ => bail!("unsupported binary compiled opcode {}", opcode.as_str()),
        })
    }

    fn dataflow_pcode_opcode(&self, opcode: CompiledOpTplOpcode) -> Result<PcodeOpcode> {
        Ok(match opcode {
            CompiledOpTplOpcode::SegmentOp => PcodeOpcode::SegmentOp,
            CompiledOpTplOpcode::CPoolRef => PcodeOpcode::CPoolRef,
            CompiledOpTplOpcode::New => PcodeOpcode::New,
            CompiledOpTplOpcode::Insert => PcodeOpcode::Insert,
            CompiledOpTplOpcode::Extract => PcodeOpcode::Extract,
            _ => bail!("unsupported dataflow compiled opcode {}", opcode.as_str()),
        })
    }
}

impl RuntimeTemplateExecutor for CompiledTableEmitter<'_> {
    fn emit_op_template(
        &mut self,
        state: &RuntimeConstructState,
        op: &CompiledOpTpl,
    ) -> Result<()> {
        CompiledTableEmitter::emit_op_template(self, state, op)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::{
        CompiledConstructorTemplate, CompiledDisplayTemplate, CompiledLanguageLayout,
    };
    use crate::runtime::spine::RuntimeMatchTrace;

    fn minimal_frontend_with_spaces(
        spaces: std::collections::BTreeMap<u64, CompiledSpaceRef>,
    ) -> CompiledFrontend {
        CompiledFrontend {
            arch: "test".to_string(),
            default_context: 0,
            default_context_known_mask: 0,
            entry_spec: "test.slaspec".to_string(),
            entry_id: "test".to_string(),
            include_manifest: Vec::new(),
            defines: Vec::new(),
            definitions: Vec::new(),
            macros: Vec::new(),
            constructors: Vec::new(),
            subtables: std::collections::BTreeMap::new(),
            language_layout: CompiledLanguageLayout {
                address_spaces: Vec::new(),
                registers: Vec::new(),
                token_fields: Vec::new(),
                context_fields: Vec::new(),
                subtables: Vec::new(),
                display_templates: Vec::new(),
            },
            construct_templates: Vec::new(),
            pcode_ops: Vec::new(),
            pattern_nodes: Vec::new(),
            sla_spaces: spaces,
            sla_unique_space_index: 0,
            sla_register_space_index: 0,
            sla_uniqbase: 0,
            sla_uniqmask: u64::MAX,
        }
    }

    fn minimal_construct_state() -> RuntimeConstructState {
        RuntimeConstructState {
            subtable_id: 0,
            constructor_id: 0,
            constructor_slot: 0,
            mnemonic: "test".to_string(),
            construct_tpl_kind: CompiledConstructTplKind::Generic,
            constructor_template: CompiledConstructorTemplate {
                handles: Vec::new(),
                decode_steps: Vec::new(),
                num_labels: 0,
                result: None,
                ops: Vec::new(),
                template_source: CompiledTemplateSource::SpecDerived,
            },
            named_templates: Vec::new(),
            context_commits: Vec::new(),
            display_template: CompiledDisplayTemplate {
                constructor_hash: 0,
                pieces: Vec::new(),
                first_whitespace: None,
                flowthru_operand_index: None,
                display: String::new(),
            },
            display_operands: Vec::new(),
            construct_nodes: Vec::new(),
            handles: Vec::new(),
            exported_handle: None,
            operands: Vec::new(),
            context_register: 0,
            context_known_mask: 0,
            absolute_offset: 0,
            relative_length: 0,
            length: 0,
            match_trace: RuntimeMatchTrace {
                root_bucket: "test".to_string(),
                probes: Vec::new(),
                leaf_constructor_indexes: Vec::new(),
                matched_leaf_pattern: None,
            },
        }
    }

    fn build_op_with_operand_index(value: u64) -> CompiledOpTpl {
        CompiledOpTpl {
            sla_raw_pcode_opcode: 0,
            opcode: CompiledOpTplOpcode::Build,
            output: None,
            inputs: vec![CompiledVarnodeTpl::Varnode {
                space: CompiledSpaceTpl::Const(Box::new(CompiledConstTpl::Real { value: 0 })),
                offset: Box::new(CompiledConstTpl::Real { value }),
                size: Box::new(CompiledConstTpl::Real { value: 0 }),
            }],
            label: None,
        }
    }

    fn test_handle(space_name: &str, offset: u64) -> RuntimeHandle {
        RuntimeHandle {
            operand_index: 0,
            spec: CompiledOperandSpec::ContextFieldExtraction {
                bit_offset: 0,
                bit_width: 1,
                sign_extend: false,
            },
            fixed: RuntimeFixedHandle {
                space: Some(CompiledSpaceRef {
                    name: space_name.to_string(),
                    index: 0,
                    word_size: 1,
                    addr_size: 8,
                }),
                size: 8,
                offset_offset: offset,
                ..RuntimeFixedHandle::default()
            },
            debug_value: None,
            subtable_state: None,
        }
    }

    #[test]
    fn offset_plus_constant_space_uses_ghidra_java_shift_masking() {
        let handle = test_handle("const", 0x8877_6655_4433_2211);

        assert_eq!(
            resolve_offset_plus(&handle, 1 << 16).unwrap(),
            0x0088_7766_5544_3322
        );
        assert_eq!(
            resolve_offset_plus(&handle, 8 << 16).unwrap(),
            0x8877_6655_4433_2211
        );
    }

    #[test]
    fn offset_plus_non_constant_space_preserves_ghidra_long_addition() {
        let handle = test_handle("ram", u64::MAX);

        assert_eq!(resolve_offset_plus(&handle, 1).unwrap(), 0);
    }

    #[test]
    fn offset_plus_rejects_missing_primary_space_metadata() {
        let mut handle = test_handle("ram", 0);
        handle.fixed.space = None;

        let err = resolve_offset_plus(&handle, 1)
            .expect_err("offset_plus requires decoded primary space metadata");

        assert!(err
            .to_string()
            .contains("offset_plus handle missing primary space metadata"));
    }

    #[test]
    fn non_offset_handle_plus_is_rejected_as_malformed_sla_contract() {
        let err = reject_non_offset_handle_plus(Some(1), "template")
            .expect_err("non-offset_plus ATTR_PLUS must fail closed");

        assert!(err
            .to_string()
            .contains("non-offset_plus handle unexpectedly carried plus"));
    }

    #[test]
    fn const_space_tpl_requires_decoded_sla_space_metadata() {
        let compiled = minimal_frontend_with_spaces(std::collections::BTreeMap::new());
        let mut emitter = CompiledTableEmitter::new(
            &compiled,
            None,
            0x1000,
            &[],
            0x1000,
            FlowEmitOptions::default(),
        );
        let state = minimal_construct_state();
        let err = emitter
            .resolve_space_tpl(
                &CompiledSpaceTpl::Const(Box::new(CompiledConstTpl::Real { value: 0 })),
                &state,
            )
            .expect_err("const space id must come from decoded SLA space metadata");

        assert!(err
            .to_string()
            .contains("SpaceTpl references unknown SLA space id 0"));
    }

    #[test]
    fn build_operand_index_fails_closed_on_malformed_templates() {
        assert_eq!(
            build_operand_index_from_op(&build_op_with_operand_index(3)).unwrap(),
            3
        );

        if usize::try_from(u64::MAX).is_err() {
            let err = build_operand_index_from_op(&build_op_with_operand_index(u64::MAX))
                .expect_err("oversized BUILD operand index must fail closed");
            assert!(err.to_string().contains("BUILD operand index"));
        }

        let mut missing_input = build_op_with_operand_index(0);
        missing_input.inputs.clear();
        let err = build_operand_index_from_op(&missing_input)
            .expect_err("missing BUILD operand input must fail closed");
        assert!(err
            .to_string()
            .contains("BUILD template missing operand input"));
    }

    #[test]
    fn relative_label_sentinel_decode_names_signed_conversion() {
        let label = 7;
        let sentinel = encode_relative_sentinel(label).expect("encode label sentinel");

        assert_eq!(decode_relative_sentinel(sentinel), Some(label));
        assert_eq!(nonnegative_i64_to_u64(0), Some(0));
        assert_eq!(nonnegative_i64_to_u64(i64::MAX), Some(i64::MAX as u64));
        assert_eq!(nonnegative_i64_to_u64(-1), None);
    }

    #[test]
    fn offset_plus_source_has_no_saturating_shift_fallback() {
        let source = include_str!("template_eval.rs");
        let saturating_shift_fallback =
            ["let shift_bits = shift_bytes.", "saturating", "_mul(8);"].concat();
        let dynamic_space_id_lossy_cast = ["space.index", "as", "i64"].join(" ");
        let build_index_ok_fallback = ["usize::try_from(*value)", ".ok()"].join("");
        let relative_label_ok_fallback = ["u64::try_from(label_num)", ".ok()"].join("");
        let missing_space_non_const_fallback =
            [".map(|s| s.name == \"const\")", ".unwrap_or(false)"].join("\n");
        let missing_const_space_materialization = [
            "name: \"const\".to_string()",
            "word_size: 0",
            "addr_size: 0",
        ]
        .join("\n");

        assert!(
            !source.contains(&saturating_shift_fallback),
            "constant-space offset_plus must mirror Ghidra Java long shift masking"
        );
        assert!(
            !source.contains(&missing_space_non_const_fallback),
            "offset_plus must require decoded primary space metadata instead of assuming non-constant space"
        );
        assert!(
            !source.contains(&dynamic_space_id_lossy_cast),
            "dynamic LOAD/STORE space-id constants must fail closed instead of truncating SLA space ids"
        );
        assert!(
            !source.contains(&build_index_ok_fallback),
            "BUILD operand index conversion must fail closed instead of skipping malformed templates"
        );
        assert!(
            !source.contains(&relative_label_ok_fallback),
            "relative label sentinel decode must name signed conversion instead of silently dropping it"
        );
        assert!(
            !source.contains(&missing_const_space_materialization),
            "SpaceTpl::Const must use decoded SLA const-space metadata instead of materializing it at runtime"
        );
    }

    #[test]
    fn dynamic_space_id_constant_rejects_oversized_metadata() {
        let space = CompiledSpaceRef {
            name: "oversized".to_string(),
            index: i64::MAX as u64 + 1,
            word_size: 1,
            addr_size: 8,
        };

        let err = space_id_const_varnode(&space, "test").expect_err("oversized space id");

        assert!(err.to_string().contains("space id"));
    }

    #[test]
    fn pointer_size_truncation_fails_on_metadata_overflow() {
        let space = CompiledSpaceRef {
            name: "oversized".to_string(),
            index: 1,
            word_size: 1,
            addr_size: u32::MAX,
        };

        let err = CompiledTableEmitter::truncate_to_pointer_size(&space, 0x1234)
            .expect_err("pointer-size overflow must fail closed");

        assert!(err
            .to_string()
            .contains("address space oversized pointer size overflowed"));
    }

    #[test]
    fn pointer_size_truncation_masks_known_widths() {
        let space = CompiledSpaceRef {
            name: "ram32".to_string(),
            index: 1,
            word_size: 1,
            addr_size: 4,
        };

        assert_eq!(
            CompiledTableEmitter::truncate_to_pointer_size(&space, 0x1_0000_1234).unwrap(),
            0x1234
        );
    }
}
