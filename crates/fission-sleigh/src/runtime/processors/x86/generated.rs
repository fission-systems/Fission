use anyhow::{anyhow, bail, Result};
use fission_pcode::arch::x86::{X86_EFLAGS_BASE, X86_REG_BASE};
use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};

use crate::compiler::{
    CompiledArithmeticOpcode, CompiledDecisionProbe, CompiledExecutableConstructor,
    CompiledFixedRegister, CompiledFrontend, CompiledHandleTemplate, CompiledOpcodeMatcher,
    CompiledOperandDecodeStep, CompiledOperandSpec, CompiledSemanticKind,
};
use crate::runtime::quirks::x86_fields::{
    candidate_bucket_keys, ensure_modrm, format_memory_operand, jcc_suffix, parse_modrm, read_sint,
    read_uint, register_name, ModRm, X86InstructionContext,
};
use crate::runtime::spine::{
    self, operand_size, BoundOperand, DecisionProbeEvaluator, RuntimeConstructState, RuntimeHandle,
    RuntimePcodeEmitter, RuntimeSelection, RuntimeSemanticEmitter, RuntimeTemplateEvaluator,
};
use crate::runtime::{
    DecodedFlowKind, DecodedInstruction, DecodedReference, DecodedReferenceKind,
    RuntimeSleighError, UNIQUE_SPACE_ID,
};

pub(crate) fn decode_and_lift(
    compiled: &CompiledFrontend,
    bytes: &[u8],
    address: u64,
) -> Result<(Vec<PcodeOp>, u64)> {
    let ctx = X86InstructionContext::parse(bytes, address)?;
    let selection =
        select_constructor(compiled, &ctx).ok_or_else(|| RuntimeSleighError::DecodeNoMatch {
            language: compiled.entry_id.clone(),
            address,
        })?;
    let decoded = bind_instruction(&ctx, selection)?;
    let mut emitter = GeneratedX86Emitter::new(address);
    RuntimeTemplateEvaluator::new(&mut emitter).emit(&decoded)?;
    Ok((emitter.finish(), decoded.length as u64))
}

pub(crate) fn decode_instruction(
    compiled: &CompiledFrontend,
    bytes: &[u8],
    address: u64,
) -> Result<DecodedInstruction> {
    let ctx = X86InstructionContext::parse(bytes, address)?;
    let selection =
        select_constructor(compiled, &ctx).ok_or_else(|| RuntimeSleighError::DecodeNoMatch {
            language: compiled.entry_id.clone(),
            address,
        })?;
    let constructor = selection.constructor;
    let decoded = bind_instruction(&ctx, selection)?;
    let length = decoded.length;
    let mnemonic = disasm_mnemonic(constructor, &decoded);
    let operands_text = decoded
        .operands
        .iter()
        .map(|operand| format_operand(operand))
        .collect::<Vec<_>>()
        .join(", ");
    let direct_target = decoded.operands.first().and_then(|operand| match operand {
        BoundOperand::Relative { target } => Some(*target),
        _ => None,
    });
    let flow_kind = flow_kind_for(decoded.semantic_kind);
    let references = decoded_references(address, length, flow_kind, &decoded.operands);
    Ok(DecodedInstruction {
        address,
        bytes: bytes.get(..length).unwrap_or(bytes).to_vec(),
        length,
        mnemonic,
        operands_text,
        flow_kind,
        direct_target,
        references,
    })
}

fn select_constructor<'a>(
    compiled: &'a CompiledFrontend,
    ctx: &X86InstructionContext<'_>,
) -> Option<RuntimeSelection<'a>> {
    let mut roots = vec![("global".to_string(), compiled.decision_tree.root_node_index)];
    if let Some(bucket_keys) = candidate_bucket_keys(ctx) {
        for bucket_key in bucket_keys {
            if let Some(bucket) = compiled
                .decision_tree
                .root_buckets
                .iter()
                .find(|bucket| bucket.key == bucket_key)
            {
                roots.push((bucket.key.clone(), bucket.node_index));
            }
        }
    }

    spine::select_constructor(
        compiled,
        roots,
        || X86DecisionProbeEvaluator::new(ctx),
        |constructor| constructor_matches(ctx, constructor),
    )
}

struct X86DecisionProbeEvaluator<'a, 'b> {
    ctx: &'a X86InstructionContext<'b>,
    cached_modrm: Option<ModRm>,
}

impl<'a, 'b> X86DecisionProbeEvaluator<'a, 'b> {
    fn new(ctx: &'a X86InstructionContext<'b>) -> Self {
        Self {
            ctx,
            cached_modrm: None,
        }
    }
}

impl DecisionProbeEvaluator for X86DecisionProbeEvaluator<'_, '_> {
    fn probe_value(&mut self, probe: CompiledDecisionProbe) -> Result<u8> {
        Ok(match probe {
            CompiledDecisionProbe::Terminal => 0,
            CompiledDecisionProbe::InstructionByte {
                offset,
                mask,
                shift,
            } => {
                let byte = *self
                    .ctx
                    .bytes
                    .get(self.ctx.cursor + usize::from(offset))
                    .ok_or_else(|| anyhow!("missing instruction byte probe at offset {offset}"))?;
                (byte & mask) >> shift
            }
            CompiledDecisionProbe::OperandSizeCode => self.ctx.operand_size_code,
            CompiledDecisionProbe::ModBits => {
                ensure_modrm(self.ctx, &mut self.cached_modrm)?.mod_bits
            }
            CompiledDecisionProbe::RegOpcode => ensure_modrm(self.ctx, &mut self.cached_modrm)?.reg,
        })
    }
}

fn constructor_matches(
    ctx: &X86InstructionContext<'_>,
    constructor: &CompiledExecutableConstructor,
) -> Result<()> {
    if !constructor.opsize_variants.is_empty()
        && !constructor
            .opsize_variants
            .iter()
            .any(|opsize| *opsize == ctx.operand_size_code)
    {
        bail!("opsize mismatch");
    }

    let opcode_len = opcode_len_from_matcher(&constructor.matcher);
    match &constructor.matcher {
        CompiledOpcodeMatcher::ExactBytes(bytes) => {
            if ctx.bytes.get(ctx.cursor..ctx.cursor + bytes.len()) != Some(bytes.as_slice()) {
                bail!("exact opcode mismatch");
            }
        }
        CompiledOpcodeMatcher::RowCc { prefix, row } => {
            if ctx.bytes.get(ctx.cursor..ctx.cursor + prefix.len()) != Some(prefix.as_slice()) {
                bail!("prefix mismatch");
            }
            let opcode = *ctx
                .bytes
                .get(ctx.cursor + prefix.len())
                .ok_or_else(|| anyhow!("missing row opcode"))?;
            if (opcode >> 4) != *row {
                bail!("row mismatch");
            }
        }
        CompiledOpcodeMatcher::RowPage { row, page } => {
            let opcode = *ctx
                .bytes
                .get(ctx.cursor)
                .ok_or_else(|| anyhow!("missing row/page opcode"))?;
            if (opcode >> 4) != *row || ((opcode >> 3) & 0x1) != *page {
                bail!("row/page mismatch");
            }
        }
    }

    let requires_modrm = constructor.mod_constraint.is_some()
        || !constructor.reg_opcode_values.is_empty()
        || constructor.operand_specs.iter().any(|spec| {
            matches!(
                spec,
                CompiledOperandSpec::ModRmRm { .. } | CompiledOperandSpec::ModRmReg { .. }
            )
        });
    if requires_modrm {
        let modrm = parse_modrm(ctx, ctx.cursor + opcode_len)?;
        if let Some(expected) = constructor.mod_constraint {
            if modrm.mod_bits != expected {
                bail!("mod mismatch");
            }
        }
        if !constructor.reg_opcode_values.is_empty()
            && !constructor.reg_opcode_values.contains(&modrm.reg)
        {
            bail!("reg_opcode mismatch");
        }
        if constructor.operand_specs.iter().any(|spec| {
            matches!(
                spec,
                CompiledOperandSpec::ModRmRm {
                    memory_only: true,
                    ..
                }
            )
        }) && modrm.mod_bits == 3
        {
            bail!("memory-only modrm mismatch");
        }
    }

    Ok(())
}

fn bind_instruction(
    ctx: &X86InstructionContext<'_>,
    selection: RuntimeSelection<'_>,
) -> Result<RuntimeConstructState> {
    constructor_matches(ctx, selection.constructor)?;
    X86ParserWalker::new(ctx, selection)?.walk()
}

struct X86ParserWalker<'a, 'b> {
    ctx: &'a X86InstructionContext<'b>,
    selection: RuntimeSelection<'a>,
    cursor: usize,
    modrm: Option<ModRm>,
    handles: Vec<Option<RuntimeHandle>>,
    walker: spine::RuntimeParserWalker,
}

impl<'a, 'b> X86ParserWalker<'a, 'b> {
    fn new(ctx: &'a X86InstructionContext<'b>, selection: RuntimeSelection<'a>) -> Result<Self> {
        let opcode_len = opcode_len_from_matcher(&selection.constructor.matcher);
        Ok(Self {
            ctx,
            cursor: ctx.cursor + opcode_len,
            modrm: None,
            handles: vec![None; selection.constructor.constructor_template.handles.len()],
            walker: spine::RuntimeParserWalker::new(ctx.cursor, opcode_len),
            selection,
        })
    }

    fn walk(mut self) -> Result<RuntimeConstructState> {
        let decode_steps = self
            .selection
            .constructor
            .constructor_template
            .decode_steps
            .clone();
        for step in decode_steps {
            match step {
                CompiledOperandDecodeStep::ConsumeModRm => {
                    self.ensure_modrm()?;
                }
                CompiledOperandDecodeStep::DecodeOperand { operand_index } => {
                    self.decode_operand(operand_index)?;
                }
            }
        }

        let mut handles = self
            .handles
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| anyhow!("incomplete handle decode"))?;
        handles.sort_by_key(|handle| handle.operand_index);
        let operands = handles
            .iter()
            .map(|handle| handle.value.clone())
            .collect::<Vec<_>>();

        let condition_code = match &self.selection.constructor.matcher {
            CompiledOpcodeMatcher::RowCc { prefix, .. } => {
                Some(self.ctx.bytes[self.ctx.cursor + prefix.len()] & 0x0f)
            }
            _ if matches!(
                self.selection.constructor.semantic_kind,
                CompiledSemanticKind::Setcc
            ) && matches!(
                self.selection.constructor.matcher,
                CompiledOpcodeMatcher::ExactBytes(_)
            ) =>
            {
                let opcode = self.ctx.bytes[self.ctx.cursor
                    + opcode_len_from_matcher(&self.selection.constructor.matcher)
                    - 1];
                Some(opcode & 0x0f)
            }
            _ => None,
        };

        Ok(RuntimeConstructState {
            semantic_kind: self.selection.constructor.semantic_kind,
            constructor_template: self.selection.constructor.constructor_template.clone(),
            construct_nodes: self.walker.into_nodes(),
            handles,
            operands,
            condition_code,
            length: self.cursor,
            match_trace: self.selection.trace,
        })
    }

    fn decode_operand(&mut self, operand_index: usize) -> Result<()> {
        if self
            .handles
            .get(operand_index)
            .is_some_and(|handle| handle.is_some())
        {
            return Ok(());
        }
        let template = self
            .selection
            .constructor
            .constructor_template
            .handles
            .get(operand_index)
            .ok_or_else(|| anyhow!("missing handle template {operand_index}"))?
            .clone();
        let operand_cursor_start = self.cursor;
        let value = self.bind_operand(&template)?;
        let handle_index = operand_index;
        self.walker.record_operand_node(
            operand_index,
            0,
            operand_cursor_start,
            self.cursor.saturating_sub(operand_cursor_start),
            handle_index,
        );
        self.handles[operand_index] = Some(RuntimeHandle {
            operand_index,
            spec: template.spec,
            value,
        });
        Ok(())
    }

    fn ensure_modrm(&mut self) -> Result<ModRm> {
        if self.modrm.is_none() {
            let decoded = parse_modrm(self.ctx, self.cursor)?;
            self.cursor += decoded.length;
            self.modrm = Some(decoded);
        }
        self.modrm.ok_or_else(|| anyhow!("failed to decode modrm"))
    }

    fn bind_operand(&mut self, template: &CompiledHandleTemplate) -> Result<BoundOperand> {
        match &template.spec {
            CompiledOperandSpec::ModRmRm { size, memory_only } => {
                let modrm = self.ensure_modrm()?;
                if modrm.mod_bits == 3 {
                    if *memory_only {
                        bail!("memory-only modrm operand cannot bind register");
                    }
                    Ok(BoundOperand::Register {
                        index: modrm.rm,
                        size: *size,
                    })
                } else {
                    Ok(BoundOperand::Memory {
                        base: modrm.base,
                        index: modrm.index,
                        scale: modrm.scale,
                        displacement: modrm.displacement,
                        rip_relative: modrm.rip_relative,
                        size: *size,
                    })
                }
            }
            CompiledOperandSpec::ModRmReg { size } => {
                let modrm = self.ensure_modrm()?;
                Ok(BoundOperand::Register {
                    index: modrm.reg,
                    size: *size,
                })
            }
            CompiledOperandSpec::OpcodeReg { size } => {
                let opcode_len = opcode_len_from_matcher(&self.selection.constructor.matcher);
                let opcode = *self
                    .ctx
                    .bytes
                    .get(self.ctx.cursor + opcode_len - 1)
                    .ok_or_else(|| anyhow!("missing opcode reg byte"))?;
                let reg = (opcode & 0x7) | ((self.ctx.rex.b as u8) << 3);
                Ok(BoundOperand::Register {
                    index: reg,
                    size: *size,
                })
            }
            CompiledOperandSpec::Immediate { size, signed } => {
                let value = read_uint(self.ctx.bytes, self.cursor, *size)?;
                self.cursor += *size as usize;
                Ok(BoundOperand::Immediate {
                    value,
                    encoded_size: *size,
                    signed: *signed,
                })
            }
            CompiledOperandSpec::Relative { size } => {
                let signed = read_sint(self.ctx.bytes, self.cursor, *size)?;
                self.cursor += *size as usize;
                let next_ip = self.ctx.address.wrapping_add(self.cursor as u64);
                Ok(BoundOperand::Relative {
                    target: next_ip.wrapping_add_signed(signed),
                })
            }
            CompiledOperandSpec::FixedRegister { reg, size } => {
                let index = match reg {
                    CompiledFixedRegister::Accumulator => 0,
                };
                Ok(BoundOperand::Register { index, size: *size })
            }
        }
    }
}

fn opcode_len_from_matcher(matcher: &CompiledOpcodeMatcher) -> usize {
    match matcher {
        CompiledOpcodeMatcher::ExactBytes(bytes) => bytes.len(),
        CompiledOpcodeMatcher::RowCc { prefix, .. } => prefix.len() + 1,
        CompiledOpcodeMatcher::RowPage { .. } => 1,
    }
}

fn flow_kind_for(kind: CompiledSemanticKind) -> DecodedFlowKind {
    match kind {
        CompiledSemanticKind::Call => DecodedFlowKind::Call,
        CompiledSemanticKind::Jmp => DecodedFlowKind::Jump,
        CompiledSemanticKind::Jcc => DecodedFlowKind::ConditionalJump,
        CompiledSemanticKind::Ret => DecodedFlowKind::Return,
        _ => DecodedFlowKind::None,
    }
}

fn disasm_mnemonic(
    constructor: &CompiledExecutableConstructor,
    state: &RuntimeConstructState,
) -> String {
    if constructor.mnemonic == "J^CC" {
        let suffix = state.condition_code.map(jcc_suffix).unwrap_or("cc");
        return format!("j{suffix}");
    }
    if constructor.mnemonic == "SET^CC" {
        let suffix = state.condition_code.map(jcc_suffix).unwrap_or("cc");
        return format!("set{suffix}");
    }
    constructor.mnemonic.to_ascii_lowercase()
}

fn format_operand(operand: &BoundOperand) -> String {
    match operand {
        BoundOperand::Register { index, size } => register_name(*index, *size).to_string(),
        BoundOperand::Immediate { value, .. } => format!("0x{value:x}"),
        BoundOperand::Relative { target } => format!("0x{target:x}"),
        BoundOperand::Memory {
            base,
            index,
            scale,
            displacement,
            rip_relative,
            ..
        } => format_memory_operand(*base, *index, *scale, *displacement, *rip_relative),
    }
}

fn decoded_references(
    address: u64,
    length: usize,
    flow_kind: DecodedFlowKind,
    operands: &[BoundOperand],
) -> Vec<DecodedReference> {
    let mut refs = Vec::new();
    for (operand_index, operand) in operands.iter().enumerate() {
        match operand {
            BoundOperand::Relative { target } => {
                let kind = match flow_kind {
                    DecodedFlowKind::Call => DecodedReferenceKind::CallTarget,
                    DecodedFlowKind::Jump | DecodedFlowKind::ConditionalJump => {
                        DecodedReferenceKind::BranchTarget
                    }
                    _ => continue,
                };
                refs.push(DecodedReference {
                    target: *target,
                    kind,
                    operand_index,
                });
            }
            BoundOperand::Memory {
                base,
                index,
                displacement,
                rip_relative,
                ..
            } => {
                let target = if *rip_relative {
                    Some(add_signed(
                        address.saturating_add(length as u64),
                        *displacement,
                    ))
                } else if *displacement > 0 {
                    Some(*displacement as u64)
                } else {
                    None
                };
                if let Some(target) = target {
                    let kind = if *rip_relative {
                        DecodedReferenceKind::RipRelativeAddress
                    } else if base.is_none() && index.is_none() {
                        DecodedReferenceKind::MemoryAddress
                    } else {
                        DecodedReferenceKind::MemoryAddress
                    };
                    refs.push(DecodedReference {
                        target,
                        kind,
                        operand_index,
                    });
                }
            }
            BoundOperand::Immediate { value, .. } if *value != 0 => {
                refs.push(DecodedReference {
                    target: *value,
                    kind: DecodedReferenceKind::ImmediateAddress,
                    operand_index,
                });
            }
            _ => {}
        }
    }
    refs
}

fn add_signed(base: u64, delta: i64) -> u64 {
    if delta >= 0 {
        base.saturating_add(delta as u64)
    } else {
        base.saturating_sub(delta.unsigned_abs())
    }
}

#[derive(Debug, Clone)]
struct GeneratedX86Emitter {
    address: u64,
    emitter: RuntimePcodeEmitter,
}

impl GeneratedX86Emitter {
    fn new(address: u64) -> Self {
        Self {
            address,
            emitter: RuntimePcodeEmitter::new(
                address,
                0xE400_0000_0000_0000u64.wrapping_add(address.wrapping_shl(6)),
            ),
        }
    }

    fn finish(self) -> Vec<PcodeOp> {
        self.emitter.finish()
    }
}

impl RuntimeSemanticEmitter for GeneratedX86Emitter {
    fn emit_return(&mut self) -> Result<()> {
        self.push(PcodeOpcode::Return, None, Vec::new(), "RET");
        Ok(())
    }

    fn emit_call(&mut self, state: &RuntimeConstructState) -> Result<()> {
        GeneratedX86Emitter::emit_call(self, state)
    }

    fn emit_jump(&mut self, state: &RuntimeConstructState) -> Result<()> {
        self.emit_jmp(state)
    }

    fn emit_conditional_jump(&mut self, state: &RuntimeConstructState) -> Result<()> {
        self.emit_jcc(state)
    }

    fn emit_move(&mut self, state: &RuntimeConstructState) -> Result<()> {
        self.emit_mov(state)
    }

    fn emit_lea(&mut self, state: &RuntimeConstructState) -> Result<()> {
        GeneratedX86Emitter::emit_lea(self, state)
    }

    fn emit_push(&mut self, state: &RuntimeConstructState) -> Result<()> {
        GeneratedX86Emitter::emit_push(self, state)
    }

    fn emit_pop(&mut self, state: &RuntimeConstructState) -> Result<()> {
        GeneratedX86Emitter::emit_pop(self, state)
    }

    fn emit_leave(&mut self) -> Result<()> {
        GeneratedX86Emitter::emit_leave(self)
    }

    fn emit_binary(
        &mut self,
        state: &RuntimeConstructState,
        opcode: CompiledArithmeticOpcode,
    ) -> Result<()> {
        match opcode {
            CompiledArithmeticOpcode::Add => self.emit_binary(state, PcodeOpcode::IntAdd, "ADD"),
            CompiledArithmeticOpcode::Sub => self.emit_binary(state, PcodeOpcode::IntSub, "SUB"),
            CompiledArithmeticOpcode::And => self.emit_binary(state, PcodeOpcode::IntAnd, "AND"),
            CompiledArithmeticOpcode::Or => self.emit_binary(state, PcodeOpcode::IntOr, "OR"),
            CompiledArithmeticOpcode::Xor => self.emit_binary(state, PcodeOpcode::IntXor, "XOR"),
            CompiledArithmeticOpcode::Mul => self.emit_binary(state, PcodeOpcode::IntMult, "IMUL"),
            CompiledArithmeticOpcode::Shl => self.emit_binary(state, PcodeOpcode::IntLeft, "SHL"),
            CompiledArithmeticOpcode::Shr => self.emit_binary(state, PcodeOpcode::IntRight, "SHR"),
            CompiledArithmeticOpcode::Sar => self.emit_binary(state, PcodeOpcode::IntSRight, "SAR"),
            CompiledArithmeticOpcode::Inc => self.emit_unary_delta(state, 1, "INC"),
            CompiledArithmeticOpcode::Dec => self.emit_unary_delta(state, -1, "DEC"),
        }
    }

    fn emit_compare(&mut self, state: &RuntimeConstructState, bitwise: bool) -> Result<()> {
        GeneratedX86Emitter::emit_compare(
            self,
            state,
            bitwise,
            if bitwise { "TEST" } else { "CMP" },
        )
    }

    fn emit_extend(&mut self, state: &RuntimeConstructState, signed: bool) -> Result<()> {
        let opcode = if signed {
            PcodeOpcode::IntSExt
        } else {
            PcodeOpcode::IntZExt
        };
        GeneratedX86Emitter::emit_extend(
            self,
            state,
            opcode,
            if signed { "MOVSX" } else { "MOVZX" },
        )
    }

    fn emit_setcc(&mut self, state: &RuntimeConstructState) -> Result<()> {
        GeneratedX86Emitter::emit_setcc(self, state)
    }

    fn emit_accumulator_extend(
        &mut self,
        state: &RuntimeConstructState,
        src_size: u32,
        dst_size: u32,
    ) -> Result<()> {
        self.emit_accumulator_extend(src_size, dst_size, state.semantic_kind.as_str())
    }
}

impl GeneratedX86Emitter {
    fn emit_call(&mut self, instruction: &RuntimeConstructState) -> Result<()> {
        let target = self.read_operand(&instruction.operands[0], 8, instruction.length)?;
        self.push(PcodeOpcode::Call, None, vec![target], "CALL");
        Ok(())
    }

    fn emit_jmp(&mut self, instruction: &RuntimeConstructState) -> Result<()> {
        let operand = &instruction.operands[0];
        match operand {
            BoundOperand::Relative { .. } => {
                let target = self.read_operand(operand, 8, instruction.length)?;
                self.push(PcodeOpcode::Branch, None, vec![target], "JMP");
            }
            _ => {
                let target = self.read_operand(operand, 8, instruction.length)?;
                self.push(PcodeOpcode::BranchInd, None, vec![target], "JMP");
            }
        }
        Ok(())
    }

    fn emit_jcc(&mut self, instruction: &RuntimeConstructState) -> Result<()> {
        let target = self.read_operand(&instruction.operands[0], 8, instruction.length)?;
        let cond = self.condition_varnode(
            instruction
                .condition_code
                .ok_or_else(|| anyhow!("missing jcc condition"))?,
        )?;
        self.push(PcodeOpcode::CBranch, None, vec![target, cond], "JCC");
        Ok(())
    }

    fn emit_mov(&mut self, instruction: &RuntimeConstructState) -> Result<()> {
        let size = operand_size(&instruction.operands[0]);
        let value = self.read_operand(&instruction.operands[1], size, instruction.length)?;
        self.write_operand(
            &instruction.operands[0],
            value,
            size,
            instruction.length,
            "MOV",
        )
    }

    fn emit_lea(&mut self, instruction: &RuntimeConstructState) -> Result<()> {
        let size = operand_size(&instruction.operands[0]).max(8);
        let BoundOperand::Memory { .. } = &instruction.operands[1] else {
            bail!("lea source must be memory");
        };
        let addr = self.effective_address(&instruction.operands[1], instruction.length)?;
        self.write_operand(
            &instruction.operands[0],
            addr,
            size,
            instruction.length,
            "LEA",
        )
    }

    fn emit_push(&mut self, instruction: &RuntimeConstructState) -> Result<()> {
        let value = self.read_operand(&instruction.operands[0], 8, instruction.length)?;
        let rsp = gpr(4, 8);
        let new_rsp = self.tmp(8);
        self.push(
            PcodeOpcode::IntSub,
            Some(new_rsp.clone()),
            vec![rsp.clone(), const_u64(8, 8)],
            "PUSH",
        );
        self.push(PcodeOpcode::Copy, Some(rsp), vec![new_rsp.clone()], "PUSH");
        self.push(
            PcodeOpcode::Store,
            None,
            vec![const_u64(0, 8), new_rsp, value],
            "PUSH",
        );
        Ok(())
    }

    fn emit_pop(&mut self, instruction: &RuntimeConstructState) -> Result<()> {
        let rsp = gpr(4, 8);
        let size = operand_size(&instruction.operands[0]).max(8);
        let value = self.tmp(size);
        self.push(
            PcodeOpcode::Load,
            Some(value.clone()),
            vec![const_u64(0, 8), rsp.clone()],
            "POP",
        );
        self.write_operand(
            &instruction.operands[0],
            value,
            size,
            instruction.length,
            "POP",
        )?;
        self.push(
            PcodeOpcode::IntAdd,
            Some(rsp.clone()),
            vec![rsp, const_u64(8, 8)],
            "POP",
        );
        Ok(())
    }

    fn emit_leave(&mut self) -> Result<()> {
        let rsp = gpr(4, 8);
        let rbp = gpr(5, 8);
        self.push(PcodeOpcode::Copy, Some(rsp.clone()), vec![rbp], "LEAVE");
        let value = self.tmp(8);
        self.push(
            PcodeOpcode::Load,
            Some(value.clone()),
            vec![const_u64(0, 8), rsp.clone()],
            "LEAVE",
        );
        self.push(PcodeOpcode::Copy, Some(gpr(5, 8)), vec![value], "LEAVE");
        self.push(
            PcodeOpcode::IntAdd,
            Some(rsp.clone()),
            vec![rsp, const_u64(8, 8)],
            "LEAVE",
        );
        Ok(())
    }

    fn emit_binary(
        &mut self,
        instruction: &RuntimeConstructState,
        opcode: PcodeOpcode,
        tag: &str,
    ) -> Result<()> {
        let size = operand_size(&instruction.operands[0]);
        let lhs = self.read_operand(&instruction.operands[0], size, instruction.length)?;
        let rhs = self.read_operand(&instruction.operands[1], size, instruction.length)?;
        let result = self.tmp(size);
        self.push(opcode, Some(result.clone()), vec![lhs, rhs], tag);
        self.write_operand(
            &instruction.operands[0],
            result.clone(),
            size,
            instruction.length,
            tag,
        )?;
        self.emit_basic_result_flags(result, size, tag);
        Ok(())
    }

    fn emit_unary_delta(
        &mut self,
        instruction: &RuntimeConstructState,
        delta: i64,
        tag: &str,
    ) -> Result<()> {
        let size = operand_size(&instruction.operands[0]);
        let lhs = self.read_operand(&instruction.operands[0], size, instruction.length)?;
        let result = self.tmp(size);
        let (opcode, rhs) = if delta >= 0 {
            (PcodeOpcode::IntAdd, const_u64(delta as u64, size))
        } else {
            (PcodeOpcode::IntSub, const_u64(delta.unsigned_abs(), size))
        };
        self.push(opcode, Some(result.clone()), vec![lhs, rhs], tag);
        self.write_operand(
            &instruction.operands[0],
            result.clone(),
            size,
            instruction.length,
            tag,
        )?;
        self.emit_basic_result_flags(result, size, tag);
        Ok(())
    }

    fn emit_compare(
        &mut self,
        instruction: &RuntimeConstructState,
        bitwise: bool,
        tag: &str,
    ) -> Result<()> {
        let size =
            operand_size(&instruction.operands[0]).max(operand_size(&instruction.operands[1]));
        let lhs = self.read_operand(&instruction.operands[0], size, instruction.length)?;
        let rhs = self.read_operand(&instruction.operands[1], size, instruction.length)?;
        let result = self.tmp(size);
        self.push(
            if bitwise {
                PcodeOpcode::IntAnd
            } else {
                PcodeOpcode::IntSub
            },
            Some(result.clone()),
            vec![lhs.clone(), rhs.clone()],
            tag,
        );
        self.emit_basic_result_flags(result, size, tag);
        let cf_value = if bitwise {
            const_u64(0, 1)
        } else {
            let cf = self.tmp(1);
            self.push(PcodeOpcode::IntLess, Some(cf.clone()), vec![lhs, rhs], tag);
            cf
        };
        self.push(PcodeOpcode::Copy, Some(flag(0)), vec![cf_value], tag);
        Ok(())
    }

    fn emit_extend(
        &mut self,
        instruction: &RuntimeConstructState,
        opcode: PcodeOpcode,
        tag: &str,
    ) -> Result<()> {
        let dst_size = operand_size(&instruction.operands[0]);
        let src_size = operand_size(&instruction.operands[1]);
        let src = self.read_operand(&instruction.operands[1], src_size, instruction.length)?;
        let out = self.tmp(dst_size);
        self.push(opcode, Some(out.clone()), vec![src], tag);
        self.write_operand(
            &instruction.operands[0],
            out,
            dst_size,
            instruction.length,
            tag,
        )
    }

    fn emit_setcc(&mut self, instruction: &RuntimeConstructState) -> Result<()> {
        let cond = self.condition_varnode(
            instruction
                .condition_code
                .ok_or_else(|| anyhow!("missing setcc condition"))?,
        )?;
        self.write_operand(
            &instruction.operands[0],
            cond,
            1,
            instruction.length,
            "SETCC",
        )
    }

    fn emit_accumulator_extend(&mut self, src_size: u32, dst_size: u32, tag: &str) -> Result<()> {
        self.push(
            PcodeOpcode::IntSExt,
            Some(gpr(0, dst_size)),
            vec![gpr(0, src_size)],
            tag,
        );
        Ok(())
    }

    fn condition_varnode(&mut self, condition_code: u8) -> Result<Varnode> {
        Ok(match condition_code {
            0x0 => flag(11),
            0x1 => self.bool_not(flag(11), "JNO_PRED"),
            0x2 => flag(0),
            0x3 => self.bool_not(flag(0), "JAE_PRED"),
            0x4 => flag(6),
            0x5 => self.bool_not(flag(6), "JNE_PRED"),
            0x6 => self.bool_or(flag(0), flag(6), "JBE_PRED"),
            0x7 => {
                let ncf = self.bool_not(flag(0), "JA_NCF");
                let nzf = self.bool_not(flag(6), "JA_NZF");
                self.bool_and(ncf, nzf, "JA_PRED")
            }
            0x8 => flag(7),
            0x9 => self.bool_not(flag(7), "JNS_PRED"),
            0xA => flag(2),
            0xB => self.bool_not(flag(2), "JNP_PRED"),
            0xC => self.bool_ne(flag(7), flag(11), "JL_PRED"),
            0xD => self.bool_eq(flag(7), flag(11), "JGE_PRED"),
            0xE => {
                let lt = self.bool_ne(flag(7), flag(11), "JLE_LT_CORE");
                self.bool_or(flag(6), lt, "JLE_PRED")
            }
            0xF => {
                let ge = self.bool_eq(flag(7), flag(11), "JG_GE_CORE");
                let nz = self.bool_not(flag(6), "JG_NZ");
                self.bool_and(ge, nz, "JG_PRED")
            }
            _ => bail!("unsupported condition code {condition_code}"),
        })
    }

    fn emit_basic_result_flags(&mut self, result: Varnode, size: u32, tag: &str) {
        let zf = self.tmp(1);
        self.push(
            PcodeOpcode::IntEqual,
            Some(zf.clone()),
            vec![result.clone(), const_u64(0, size)],
            tag,
        );
        self.push(PcodeOpcode::Copy, Some(flag(6)), vec![zf], tag);

        let shift = size.saturating_mul(8).saturating_sub(1);
        let sf = self.tmp(1);
        self.push(
            PcodeOpcode::IntRight,
            Some(sf.clone()),
            vec![result, const_u64(u64::from(shift), size)],
            tag,
        );
        self.push(PcodeOpcode::Copy, Some(flag(7)), vec![sf], tag);
    }

    fn read_operand(
        &mut self,
        operand: &BoundOperand,
        expected_size: u32,
        instruction_len: usize,
    ) -> Result<Varnode> {
        match operand {
            BoundOperand::Register { index, size } => Ok(gpr(
                u64::from(*index),
                (*size).max(expected_size.min(*size)),
            )),
            BoundOperand::Memory { .. } => {
                let addr = self.effective_address(operand, instruction_len)?;
                let out = self.tmp(expected_size);
                self.push(
                    PcodeOpcode::Load,
                    Some(out.clone()),
                    vec![const_u64(0, 8), addr],
                    "LOAD",
                );
                Ok(out)
            }
            BoundOperand::Immediate {
                value,
                encoded_size,
                signed,
            } => {
                let effective = if *signed && expected_size > *encoded_size {
                    sign_extend(*value, *encoded_size, expected_size)
                } else {
                    *value
                };
                Ok(const_u64(effective, expected_size))
            }
            BoundOperand::Relative { target } => Ok(const_u64(*target, 8)),
        }
    }

    fn write_operand(
        &mut self,
        operand: &BoundOperand,
        value: Varnode,
        _size: u32,
        instruction_len: usize,
        tag: &str,
    ) -> Result<()> {
        match operand {
            BoundOperand::Register { index, size } => {
                self.push(
                    PcodeOpcode::Copy,
                    Some(gpr(u64::from(*index), *size)),
                    vec![value],
                    tag,
                );
                Ok(())
            }
            BoundOperand::Memory { .. } => {
                let addr = self.effective_address(operand, instruction_len)?;
                self.push(
                    PcodeOpcode::Store,
                    None,
                    vec![const_u64(0, 8), addr, value],
                    tag,
                );
                Ok(())
            }
            _ => bail!("unsupported write operand"),
        }
    }

    fn effective_address(
        &mut self,
        operand: &BoundOperand,
        instruction_len: usize,
    ) -> Result<Varnode> {
        let BoundOperand::Memory {
            base,
            index,
            scale,
            displacement,
            rip_relative,
            ..
        } = operand
        else {
            bail!("effective_address requires memory operand");
        };

        let mut terms = Vec::new();
        if *rip_relative {
            let next_ip = self.address.wrapping_add(instruction_len as u64);
            terms.push(const_u64(next_ip.wrapping_add_signed(*displacement), 8));
        } else {
            if let Some(base) = base {
                terms.push(gpr(u64::from(*base), 8));
            }
            if let Some(index) = index {
                let idx = gpr(u64::from(*index), 8);
                if *scale > 1 {
                    let scaled = self.tmp(8);
                    self.push(
                        PcodeOpcode::IntMult,
                        Some(scaled.clone()),
                        vec![idx, const_u64(u64::from(*scale), 8)],
                        "EA_SCALE",
                    );
                    terms.push(scaled);
                } else {
                    terms.push(idx);
                }
            }
            if *displacement != 0 || terms.is_empty() {
                terms.push(const_u64(displacement.unsigned_abs(), 8));
                if *displacement < 0 && terms.len() >= 2 {
                    let rhs = terms.pop().unwrap();
                    let lhs = terms.pop().unwrap();
                    let tmp = self.tmp(8);
                    self.push(
                        PcodeOpcode::IntSub,
                        Some(tmp.clone()),
                        vec![lhs, rhs],
                        "EA_DISP",
                    );
                    terms.push(tmp);
                }
            }
        }

        let mut iter = terms.into_iter();
        let Some(mut acc) = iter.next() else {
            return Ok(const_u64(0, 8));
        };
        for term in iter {
            let next = self.tmp(8);
            self.push(
                PcodeOpcode::IntAdd,
                Some(next.clone()),
                vec![acc, term],
                "EA_ADD",
            );
            acc = next;
        }
        Ok(acc)
    }

    fn tmp(&mut self, size: u32) -> Varnode {
        self.emitter.tmp(UNIQUE_SPACE_ID, size)
    }

    fn bool_not(&mut self, input: Varnode, tag: &str) -> Varnode {
        let out = self.tmp(1);
        self.push(PcodeOpcode::BoolNegate, Some(out.clone()), vec![input], tag);
        out
    }

    fn bool_and(&mut self, lhs: Varnode, rhs: Varnode, tag: &str) -> Varnode {
        let out = self.tmp(1);
        self.push(PcodeOpcode::BoolAnd, Some(out.clone()), vec![lhs, rhs], tag);
        out
    }

    fn bool_or(&mut self, lhs: Varnode, rhs: Varnode, tag: &str) -> Varnode {
        let out = self.tmp(1);
        self.push(PcodeOpcode::BoolOr, Some(out.clone()), vec![lhs, rhs], tag);
        out
    }

    fn bool_eq(&mut self, lhs: Varnode, rhs: Varnode, tag: &str) -> Varnode {
        let out = self.tmp(1);
        self.push(
            PcodeOpcode::IntEqual,
            Some(out.clone()),
            vec![lhs, rhs],
            tag,
        );
        out
    }

    fn bool_ne(&mut self, lhs: Varnode, rhs: Varnode, tag: &str) -> Varnode {
        let out = self.tmp(1);
        self.push(
            PcodeOpcode::IntNotEqual,
            Some(out.clone()),
            vec![lhs, rhs],
            tag,
        );
        out
    }

    fn push(
        &mut self,
        opcode: PcodeOpcode,
        output: Option<Varnode>,
        inputs: Vec<Varnode>,
        mnemonic: &str,
    ) {
        self.emitter.push(opcode, output, inputs, mnemonic);
    }
}

fn sign_extend(value: u64, from_size: u32, to_size: u32) -> u64 {
    let bits = from_size * 8;
    let shift = 64 - bits;
    let signed = ((value << shift) as i64) >> shift;
    const_u64(signed as u64, to_size).constant_val as u64
}

fn const_u64(val: u64, size: u32) -> Varnode {
    let masked = if size >= 8 {
        val
    } else {
        let bits = size.saturating_mul(8);
        if bits == 0 {
            0
        } else {
            val & ((1u64 << bits) - 1)
        }
    };
    Varnode::constant(i64::from_ne_bytes(masked.to_ne_bytes()), size)
}

fn gpr(index: u64, size: u32) -> Varnode {
    Varnode {
        space_id: UNIQUE_SPACE_ID,
        offset: X86_REG_BASE + index * 8,
        size,
        is_constant: false,
        constant_val: 0,
    }
}

fn flag(bit: u64) -> Varnode {
    Varnode {
        space_id: UNIQUE_SPACE_ID,
        offset: X86_EFLAGS_BASE + bit,
        size: 1,
        is_constant: false,
        constant_val: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::compile_x86_64_frontend;

    #[test]
    fn generated_runtime_decodes_ret() {
        let compiled = compile_x86_64_frontend().expect("compile x86-64 frontend");
        let (ops, len) = decode_and_lift(&compiled, &[0xC3], 0x1000).expect("generated ret");
        assert_eq!(len, 1);
        assert_eq!(ops.last().map(|op| op.opcode), Some(PcodeOpcode::Return));
    }

    #[test]
    fn generated_runtime_decodes_mov_imm64() {
        let compiled = compile_x86_64_frontend().expect("compile x86-64 frontend");
        let bytes = [0x48, 0xB8, 0x34, 0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let (ops, len) = decode_and_lift(&compiled, &bytes, 0x1000).expect("generated mov");
        assert_eq!(len, bytes.len() as u64);
        assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::Copy));
    }

    #[test]
    fn generated_runtime_decodes_jcc_rel8() {
        let compiled = compile_x86_64_frontend().expect("compile x86-64 frontend");
        let (ops, len) = decode_and_lift(&compiled, &[0x75, 0x05], 0x1000).expect("generated jne");
        assert_eq!(len, 2);
        assert_eq!(ops.last().map(|op| op.opcode), Some(PcodeOpcode::CBranch));
    }

    #[test]
    fn generated_runtime_decodes_startup_store_mov_mem32_imm32() {
        let compiled = compile_x86_64_frontend().expect("compile x86-64 frontend");
        let bytes = [0xC7, 0x00, 0x01, 0x00, 0x00, 0x00];
        let (ops, len) =
            decode_and_lift(&compiled, &bytes, 0x1000).expect("generated mov [rax], imm32");
        assert_eq!(len, bytes.len() as u64);
        assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::Store));
    }

    #[test]
    fn generated_runtime_decodes_startup_sub_rsp_imm8() {
        let compiled = compile_x86_64_frontend().expect("compile x86-64 frontend");
        let bytes = [0x48, 0x83, 0xEC, 0x28];
        let (ops, len) =
            decode_and_lift(&compiled, &bytes, 0x1000).expect("generated sub rsp, imm8");
        assert_eq!(len, bytes.len() as u64);
        assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::IntSub));
        assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::Copy));
    }

    #[test]
    fn generated_runtime_decodes_startup_rip_relative_load() {
        let compiled = compile_x86_64_frontend().expect("compile x86-64 frontend");
        let bytes = [0x48, 0x8B, 0x05, 0x15, 0x30, 0x00, 0x00];
        let address = 0x1400_013e4;
        let (ops, len) =
            decode_and_lift(&compiled, &bytes, address).expect("generated rip-relative mov");
        assert_eq!(len, bytes.len() as u64);
        assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::Load));
        assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::Copy));
        let expected_target = address + bytes.len() as u64 + 0x3015;
        assert!(ops.iter().any(|op| {
            op.opcode == PcodeOpcode::Load
                && op
                    .inputs
                    .iter()
                    .any(|vn| vn.is_constant && vn.constant_val as u64 == expected_target)
        }));
    }

    #[test]
    fn generated_runtime_decodes_startup_call_rel32() {
        let compiled = compile_x86_64_frontend().expect("compile x86-64 frontend");
        let bytes = [0xE8, 0x1A, 0xFC, 0xFF, 0xFF];
        let (ops, len) =
            decode_and_lift(&compiled, &bytes, 0x1400_013ef).expect("generated call rel32");
        assert_eq!(len, bytes.len() as u64);
        assert_eq!(ops.last().map(|op| op.opcode), Some(PcodeOpcode::Call));
    }

    #[test]
    fn generated_runtime_records_decision_trace_for_startup_store() {
        let compiled = compile_x86_64_frontend().expect("compile x86-64 frontend");
        let ctx = X86InstructionContext::parse(&[0xC7, 0x00, 0x01, 0x00, 0x00, 0x00], 0x1000)
            .expect("decode context");
        let selection = select_constructor(&compiled, &ctx).expect("constructor selection");
        let state = bind_instruction(&ctx, selection).expect("bind instruction");
        assert_eq!(state.match_trace.root_bucket, "global");
        assert!(!state.match_trace.probes.is_empty());
        assert!(!state.construct_nodes.is_empty());
        assert!(state
            .handles
            .iter()
            .any(|handle| matches!(handle.spec, CompiledOperandSpec::ModRmRm { .. })));
    }
}
