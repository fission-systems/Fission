//! Transitional compiled-table executor for the common SLEIGH runtime spine.
//! The canonical owner mapping remains `DecisionNode -> RuntimeInstructionContext ->
//! RuntimeConstructState/RuntimeParserWalker -> RuntimeTemplateEvaluator -> RuntimePcodeEmitter`.

use anyhow::{anyhow, bail, Result};
use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};

use crate::compiler::{
    CompiledArithmeticOpcode, CompiledConstructTplKind, CompiledDecisionProbe,
    CompiledExecutableConstructor, CompiledFixedRegister, CompiledFrontend, CompiledHandleTemplate,
    CompiledOperandDecodeStep, CompiledOperandSpec, CompiledPatternMatcher, CompiledTokenFieldRef,
};
use crate::runtime::spine::{
    self, operand_size, BoundOperand, DecisionProbeEvaluator, RuntimeConstructState, RuntimeHandle,
    RuntimeInstructionContext, RuntimePcodeEmitter, RuntimeSelection, RuntimeSemanticEmitter,
    RuntimeTemplateEvaluator,
};
use crate::runtime::{
    DecodedFlowKind, DecodedInstruction, DecodedReference, DecodedReferenceKind,
    RuntimeSleighError, UNIQUE_SPACE_ID,
};

const SYNTHETIC_REGISTER_SPACE_ROOT: u64 = 0xA860_0000;
const SYNTHETIC_STATUS_SPACE_ROOT: u64 = 0xA86F_0000;

#[derive(Debug, Clone, Copy)]
struct InstructionExtensionState {
    w: bool,
    r: bool,
    x: bool,
    b: bool,
}

#[derive(Debug, Clone)]
struct CompiledInstructionContext<'a> {
    inner: RuntimeInstructionContext<'a>,
    extension_state: InstructionExtensionState,
}

impl<'a> std::ops::Deref for CompiledInstructionContext<'a> {
    type Target = RuntimeInstructionContext<'a>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a> CompiledInstructionContext<'a> {
    fn parse(bytes: &'a [u8], address: u64) -> Result<Self> {
        if bytes.is_empty() {
            bail!("empty compiled-table decode buffer");
        }
        let mut cursor = 0usize;
        let mut operand_size_override = false;
        let mut extension_state = InstructionExtensionState {
            w: false,
            r: false,
            x: false,
            b: false,
        };
        while cursor < bytes.len() {
            match bytes[cursor] {
                0x66 => {
                    operand_size_override = true;
                    cursor += 1;
                }
                0x67 | 0xF0 | 0xF2 | 0xF3 | 0x2E | 0x36 | 0x3E | 0x26 | 0x64 | 0x65 => {
                    cursor += 1;
                }
                value @ 0x40..=0x4F => {
                    extension_state = InstructionExtensionState {
                        w: value & 0x08 != 0,
                        r: value & 0x04 != 0,
                        x: value & 0x02 != 0,
                        b: value & 0x01 != 0,
                    };
                    cursor += 1;
                }
                _ => break,
            }
        }
        let instruction_width_profile = if extension_state.w {
            2
        } else if operand_size_override {
            0
        } else {
            1
        };
        Ok(Self {
            inner: RuntimeInstructionContext::new(
                bytes,
                address,
                cursor,
                instruction_width_profile,
            ),
            extension_state,
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct TokenFieldBundle {
    operand_mode: u8,
    reg: u8,
    rm: u8,
    base: Option<u8>,
    index: Option<u8>,
    scale: u8,
    displacement: i64,
    rip_relative: bool,
    length: usize,
}

pub(crate) fn decode_and_lift(
    compiled: &CompiledFrontend,
    bytes: &[u8],
    address: u64,
) -> Result<(Vec<PcodeOp>, u64)> {
    let ctx = CompiledInstructionContext::parse(bytes, address)?;
    let selection =
        select_constructor(compiled, &ctx).ok_or_else(|| RuntimeSleighError::DecodeNoMatch {
            language: compiled.entry_id.clone(),
            address,
        })?;
    let decoded = bind_instruction(&ctx, selection)?;
    let mut emitter = CompiledTableEmitter::new(address);
    RuntimeTemplateEvaluator::new(&mut emitter).emit(&decoded)?;
    Ok((emitter.finish(), decoded.length as u64))
}

pub(crate) fn decode_instruction(
    compiled: &CompiledFrontend,
    bytes: &[u8],
    address: u64,
) -> Result<DecodedInstruction> {
    let ctx = CompiledInstructionContext::parse(bytes, address)?;
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
    let flow_kind = flow_kind_for(decoded.construct_tpl_kind);
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
    ctx: &CompiledInstructionContext<'_>,
) -> Option<RuntimeSelection<'a>> {
    let mut roots = vec![("global".to_string(), compiled.decision_tree.root_node_index)];
    if let Some(bucket_keys) = decision_root_keys(ctx) {
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
        || CompiledDecisionProbeEvaluator::new(ctx),
        |constructor| constructor_matches(ctx, constructor),
    )
}

struct CompiledDecisionProbeEvaluator<'a, 'b> {
    ctx: &'a CompiledInstructionContext<'b>,
    cached_token_fields: Option<TokenFieldBundle>,
}

impl<'a, 'b> CompiledDecisionProbeEvaluator<'a, 'b> {
    fn new(ctx: &'a CompiledInstructionContext<'b>) -> Self {
        Self {
            ctx,
            cached_token_fields: None,
        }
    }
}

impl DecisionProbeEvaluator for CompiledDecisionProbeEvaluator<'_, '_> {
    fn probe_value(&mut self, probe: CompiledDecisionProbe) -> Result<u8> {
        Ok(match probe {
            CompiledDecisionProbe::Terminal => 0,
            CompiledDecisionProbe::InstructionBitSlice {
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
            CompiledDecisionProbe::ContextBitSlice { .. }
            | CompiledDecisionProbe::ContextFieldRef(_) => 0,
            CompiledDecisionProbe::TokenFieldRef(
                CompiledTokenFieldRef::InstructionWidthProfile,
            ) => self.ctx.instruction_width_profile,
            CompiledDecisionProbe::TokenFieldRef(CompiledTokenFieldRef::AddressingForm) => {
                ensure_token_fields(self.ctx, &mut self.cached_token_fields)?.operand_mode
            }
            CompiledDecisionProbe::TokenFieldRef(CompiledTokenFieldRef::RegisterSelector) => {
                ensure_token_fields(self.ctx, &mut self.cached_token_fields)?.reg
            }
            CompiledDecisionProbe::TerminalPatternCheck => 0,
        })
    }
}

fn decision_root_keys(ctx: &CompiledInstructionContext<'_>) -> Option<Vec<String>> {
    let first = *ctx.bytes.get(ctx.cursor)?;
    let mut keys = vec![format!("byte_{first:02x}")];
    if first == 0x0f {
        if let Some(second) = ctx.bytes.get(ctx.cursor + 1) {
            keys.push(format!("row_{}_after_0f", second >> 4));
        }
    }
    keys.push(format!("row_{}_page_{}", first >> 4, (first >> 3) & 0x1));
    keys.push(format!("row_{}", first >> 4));
    Some(keys)
}

fn ensure_token_fields<'a>(
    ctx: &CompiledInstructionContext<'_>,
    cached_token_fields: &'a mut Option<TokenFieldBundle>,
) -> Result<&'a TokenFieldBundle> {
    if cached_token_fields.is_none() {
        *cached_token_fields = Some(parse_token_fields(
            ctx,
            ctx.cursor + opcode_len_from_context(ctx)?,
        )?);
    }
    cached_token_fields
        .as_ref()
        .ok_or_else(|| anyhow!("missing cached token fields"))
}

fn opcode_len_from_context(ctx: &CompiledInstructionContext<'_>) -> Result<usize> {
    let opcode = *ctx
        .bytes
        .get(ctx.cursor)
        .ok_or_else(|| anyhow!("missing opcode byte"))?;
    Ok(if opcode == 0x0f { 2 } else { 1 })
}

fn parse_token_fields(
    ctx: &CompiledInstructionContext<'_>,
    offset: usize,
) -> Result<TokenFieldBundle> {
    let byte = *ctx
        .bytes
        .get(offset)
        .ok_or_else(|| anyhow!("missing token field bundle at {offset}"))?;
    let operand_mode = byte >> 6;
    let reg = ((byte >> 3) & 0x7) | ((ctx.extension_state.r as u8) << 3);
    let rm_low = byte & 0x7;
    let rm = rm_low | ((ctx.extension_state.b as u8) << 3);
    if operand_mode == 3 {
        return Ok(TokenFieldBundle {
            operand_mode,
            reg,
            rm,
            base: Some(rm),
            index: None,
            scale: 1,
            displacement: 0,
            rip_relative: false,
            length: 1,
        });
    }

    let mut length = 1usize;
    let mut displacement = 0i64;
    let mut rip_relative = false;
    let mut base = Some(rm);
    let mut index = None;
    let mut scale = 1u8;

    if rm_low == 4 {
        let sib = *ctx
            .bytes
            .get(offset + length)
            .ok_or_else(|| anyhow!("missing sib"))?;
        length += 1;
        scale = 1u8 << (sib >> 6);
        let index_low = (sib >> 3) & 0x7;
        let base_low = sib & 0x7;
        if index_low != 4 {
            index = Some(index_low | ((ctx.extension_state.x as u8) << 3));
        }
        if operand_mode == 0 && base_low == 5 {
            base = None;
            displacement = read_sint(ctx.bytes, offset + length, 4)?;
            length += 4;
        } else {
            base = Some(base_low | ((ctx.extension_state.b as u8) << 3));
        }
    } else if operand_mode == 0 && rm_low == 5 {
        base = None;
        rip_relative = true;
        displacement = read_sint(ctx.bytes, offset + length, 4)?;
        length += 4;
    }

    match operand_mode {
        1 => {
            displacement = displacement.wrapping_add(read_sint(ctx.bytes, offset + length, 1)?);
            length += 1;
        }
        2 => {
            displacement = displacement.wrapping_add(read_sint(ctx.bytes, offset + length, 4)?);
            length += 4;
        }
        _ => {}
    }

    Ok(TokenFieldBundle {
        operand_mode,
        reg,
        rm,
        base,
        index,
        scale,
        displacement,
        rip_relative,
        length,
    })
}

fn read_uint(bytes: &[u8], offset: usize, size: u32) -> Result<u64> {
    let end = offset + size as usize;
    let slice = bytes
        .get(offset..end)
        .ok_or_else(|| anyhow!("missing immediate bytes"))?;
    let mut value = 0u64;
    for (index, byte) in slice.iter().enumerate() {
        value |= u64::from(*byte) << (index * 8);
    }
    Ok(value)
}

fn read_sint(bytes: &[u8], offset: usize, size: u32) -> Result<i64> {
    let value = read_uint(bytes, offset, size)?;
    let bits = size * 8;
    if bits == 64 {
        Ok(i64::from_ne_bytes(value.to_ne_bytes()))
    } else {
        let shift = 64 - bits;
        Ok(((value << shift) as i64) >> shift)
    }
}

fn constructor_matches(
    ctx: &CompiledInstructionContext<'_>,
    constructor: &CompiledExecutableConstructor,
) -> Result<()> {
    if !constructor.opsize_variants.is_empty()
        && !constructor
            .opsize_variants
            .iter()
            .any(|opsize| *opsize == ctx.instruction_width_profile)
    {
        bail!("opsize mismatch");
    }

    let opcode_len = opcode_len_from_matcher(&constructor.matcher);
    match &constructor.matcher {
        CompiledPatternMatcher::ExactBytes(bytes) => {
            if ctx.bytes.get(ctx.cursor..ctx.cursor + bytes.len()) != Some(bytes.as_slice()) {
                bail!("exact opcode mismatch");
            }
        }
        CompiledPatternMatcher::RowCc { prefix, row } => {
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
        CompiledPatternMatcher::RowPage { row, page } => {
            let opcode = *ctx
                .bytes
                .get(ctx.cursor)
                .ok_or_else(|| anyhow!("missing row/page opcode"))?;
            if (opcode >> 4) != *row || ((opcode >> 3) & 0x1) != *page {
                bail!("row/page mismatch");
            }
        }
    }

    let requires_token_bundle = constructor.mod_constraint.is_some()
        || !constructor.operand_reg_values.is_empty()
        || constructor.operand_specs.iter().any(|spec| {
            matches!(
                spec,
                CompiledOperandSpec::TokenFieldRm { .. }
                    | CompiledOperandSpec::TokenFieldReg { .. }
            )
        });
    if requires_token_bundle {
        let token_fields = parse_token_fields(ctx, ctx.cursor + opcode_len)?;
        if let Some(expected) = constructor.mod_constraint {
            if token_fields.operand_mode != expected {
                bail!("mod mismatch");
            }
        }
        if !constructor.operand_reg_values.is_empty()
            && !constructor.operand_reg_values.contains(&token_fields.reg)
        {
            bail!("operand_reg mismatch");
        }
        if constructor.operand_specs.iter().any(|spec| {
            matches!(
                spec,
                CompiledOperandSpec::TokenFieldRm {
                    memory_only: true,
                    ..
                }
            )
        }) && token_fields.operand_mode == 3
        {
            bail!("memory-only token field mismatch");
        }
    }

    Ok(())
}

fn bind_instruction(
    ctx: &CompiledInstructionContext<'_>,
    selection: RuntimeSelection<'_>,
) -> Result<RuntimeConstructState> {
    constructor_matches(ctx, selection.constructor)?;
    CompiledParserWalker::new(ctx, selection)?.walk()
}

struct CompiledParserWalker<'a, 'b> {
    ctx: &'a CompiledInstructionContext<'b>,
    selection: RuntimeSelection<'a>,
    cursor: usize,
    token_fields: Option<TokenFieldBundle>,
    handles: Vec<Option<RuntimeHandle>>,
    walker: spine::RuntimeParserWalker,
}

impl<'a, 'b> CompiledParserWalker<'a, 'b> {
    fn new(
        ctx: &'a CompiledInstructionContext<'b>,
        selection: RuntimeSelection<'a>,
    ) -> Result<Self> {
        let opcode_len = opcode_len_from_matcher(&selection.constructor.matcher);
        Ok(Self {
            ctx,
            cursor: ctx.cursor + opcode_len,
            token_fields: None,
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
                CompiledOperandDecodeStep::ConsumeTokenFields => {
                    self.ensure_token_fields()?;
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
            CompiledPatternMatcher::RowCc { prefix, .. } => {
                Some(self.ctx.bytes[self.ctx.cursor + prefix.len()] & 0x0f)
            }
            _ if matches!(
                self.selection.constructor.construct_tpl_kind,
                CompiledConstructTplKind::Setcc
            ) && matches!(
                self.selection.constructor.matcher,
                CompiledPatternMatcher::ExactBytes(_)
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
            construct_tpl_kind: self.selection.constructor.construct_tpl_kind,
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

    fn ensure_token_fields(&mut self) -> Result<TokenFieldBundle> {
        if self.token_fields.is_none() {
            let decoded = parse_token_fields(self.ctx, self.cursor)?;
            self.cursor += decoded.length;
            self.token_fields = Some(decoded);
        }
        self.token_fields
            .ok_or_else(|| anyhow!("failed to decode token fields"))
    }

    fn bind_operand(&mut self, template: &CompiledHandleTemplate) -> Result<BoundOperand> {
        match &template.spec {
            CompiledOperandSpec::TokenFieldRm { size, memory_only } => {
                let token_fields = self.ensure_token_fields()?;
                if token_fields.operand_mode == 3 {
                    if *memory_only {
                        bail!("memory-only token field operand cannot bind register");
                    }
                    Ok(BoundOperand::Register {
                        index: token_fields.rm,
                        size: *size,
                    })
                } else {
                    Ok(BoundOperand::Memory {
                        base: token_fields.base,
                        index: token_fields.index,
                        scale: token_fields.scale,
                        displacement: token_fields.displacement,
                        rip_relative: token_fields.rip_relative,
                        size: *size,
                    })
                }
            }
            CompiledOperandSpec::TokenFieldReg { size } => {
                let token_fields = self.ensure_token_fields()?;
                Ok(BoundOperand::Register {
                    index: token_fields.reg,
                    size: *size,
                })
            }
            CompiledOperandSpec::OpcodeTokenReg { size } => {
                let opcode_len = opcode_len_from_matcher(&self.selection.constructor.matcher);
                let opcode = *self
                    .ctx
                    .bytes
                    .get(self.ctx.cursor + opcode_len - 1)
                    .ok_or_else(|| anyhow!("missing opcode reg byte"))?;
                let reg = (opcode & 0x7) | ((self.ctx.extension_state.b as u8) << 3);
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

fn opcode_len_from_matcher(matcher: &CompiledPatternMatcher) -> usize {
    match matcher {
        CompiledPatternMatcher::ExactBytes(bytes) => bytes.len(),
        CompiledPatternMatcher::RowCc { prefix, .. } => prefix.len() + 1,
        CompiledPatternMatcher::RowPage { .. } => 1,
    }
}

fn flow_kind_for(kind: CompiledConstructTplKind) -> DecodedFlowKind {
    match kind {
        CompiledConstructTplKind::Call => DecodedFlowKind::Call,
        CompiledConstructTplKind::Jmp => DecodedFlowKind::Jump,
        CompiledConstructTplKind::Jcc => DecodedFlowKind::ConditionalJump,
        CompiledConstructTplKind::Ret => DecodedFlowKind::Return,
        _ => DecodedFlowKind::None,
    }
}

fn disasm_mnemonic(
    constructor: &CompiledExecutableConstructor,
    _state: &RuntimeConstructState,
) -> String {
    // Final rendering must come from SLEIGH display templates. Until that
    // template IR is executable, keep this generic and avoid ISA-specific
    // condition/register naming tables in the runtime surface.
    constructor.mnemonic.replace('^', "").to_ascii_lowercase()
}

fn format_operand(operand: &BoundOperand) -> String {
    match operand {
        BoundOperand::Register { index, size } => format!("reg{size}_{index}"),
        BoundOperand::Immediate { value, .. } => format!("0x{value:x}"),
        BoundOperand::Relative { target } => format!("0x{target:x}"),
        BoundOperand::Memory {
            base,
            index,
            scale,
            displacement,
            rip_relative,
            ..
        } => {
            let base = base
                .map(|value| format!("reg8_{value}"))
                .unwrap_or_else(|| "none".to_string());
            let index = index
                .map(|value| format!("reg8_{value}"))
                .unwrap_or_else(|| "none".to_string());
            format!(
                "mem[base={base},index={index},scale={scale},disp={displacement},rip={rip_relative}]"
            )
        }
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
struct CompiledTableEmitter {
    address: u64,
    emitter: RuntimePcodeEmitter,
}

impl CompiledTableEmitter {
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

impl RuntimeSemanticEmitter for CompiledTableEmitter {
    fn emit_return(&mut self) -> Result<()> {
        self.emitter.emit_return("RET")
    }

    fn emit_call(&mut self, state: &RuntimeConstructState) -> Result<()> {
        CompiledTableEmitter::emit_call(self, state)
    }

    fn emit_jump(&mut self, state: &RuntimeConstructState) -> Result<()> {
        self.emit_jmp(state)
    }

    fn emit_conditional_jump(&mut self, state: &RuntimeConstructState) -> Result<()> {
        self.emit_jcc(state)
    }

    fn emit_copy_op(&mut self, state: &RuntimeConstructState) -> Result<()> {
        self.emit_mov(state)
    }

    fn emit_address_op(&mut self, state: &RuntimeConstructState) -> Result<()> {
        CompiledTableEmitter::emit_address_op(self, state)
    }

    fn emit_store_stack_op(&mut self, state: &RuntimeConstructState) -> Result<()> {
        CompiledTableEmitter::emit_store_stack_op(self, state)
    }

    fn emit_load_stack_op(&mut self, state: &RuntimeConstructState) -> Result<()> {
        CompiledTableEmitter::emit_load_stack_op(self, state)
    }

    fn emit_frame_teardown_op(&mut self) -> Result<()> {
        CompiledTableEmitter::emit_frame_teardown_op(self)
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
        CompiledTableEmitter::emit_compare(
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
        CompiledTableEmitter::emit_extend(
            self,
            state,
            opcode,
            if signed { "MOVSX" } else { "MOVZX" },
        )
    }

    fn emit_setcc(&mut self, state: &RuntimeConstructState) -> Result<()> {
        CompiledTableEmitter::emit_setcc(self, state)
    }

    fn emit_accumulator_extend(
        &mut self,
        state: &RuntimeConstructState,
        src_size: u32,
        dst_size: u32,
    ) -> Result<()> {
        self.emit_accumulator_extend(src_size, dst_size, state.construct_tpl_kind.as_str())
    }
}

impl CompiledTableEmitter {
    fn emit_call(&mut self, instruction: &RuntimeConstructState) -> Result<()> {
        let target = self.read_operand(&instruction.operands[0], 8, instruction.length)?;
        self.emitter.emit_call(target, "CALL")?;
        Ok(())
    }

    fn emit_jmp(&mut self, instruction: &RuntimeConstructState) -> Result<()> {
        let operand = &instruction.operands[0];
        match operand {
            BoundOperand::Relative { .. } => {
                let target = self.read_operand(operand, 8, instruction.length)?;
                self.emitter.emit_branch(target, "JMP")?;
            }
            _ => {
                let target = self.read_operand(operand, 8, instruction.length)?;
                self.emitter.emit_branch_ind(target, "JMP")?;
            }
        }
        Ok(())
    }

    fn emit_jcc(&mut self, instruction: &RuntimeConstructState) -> Result<()> {
        let target = self.read_operand(&instruction.operands[0], 8, instruction.length)?;
        let cond = self.status_predicate_varnode(
            instruction
                .condition_code
                .ok_or_else(|| anyhow!("missing jcc condition"))?,
        )?;
        self.emitter.emit_cbranch(target, cond, "JCC")?;
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

    fn emit_address_op(&mut self, instruction: &RuntimeConstructState) -> Result<()> {
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

    fn emit_store_stack_op(&mut self, instruction: &RuntimeConstructState) -> Result<()> {
        let value = self.read_operand(&instruction.operands[0], 8, instruction.length)?;
        let rsp = gpr(4, 8);
        let new_rsp = self.tmp(8);
        self.emitter.emit_int_binop(
            PcodeOpcode::IntSub,
            new_rsp.clone(),
            rsp.clone(),
            const_u64(8, 8),
            "PUSH",
        )?;
        self.emitter.emit_copy(rsp, new_rsp.clone(), "PUSH")?;
        self.emitter
            .emit_store(const_u64(0, 8), new_rsp, value, "PUSH")?;
        Ok(())
    }

    fn emit_load_stack_op(&mut self, instruction: &RuntimeConstructState) -> Result<()> {
        let rsp = gpr(4, 8);
        let size = operand_size(&instruction.operands[0]).max(8);
        let value = self.tmp(size);
        self.emitter
            .emit_load(value.clone(), const_u64(0, 8), rsp.clone(), "POP")?;
        self.write_operand(
            &instruction.operands[0],
            value,
            size,
            instruction.length,
            "POP",
        )?;
        self.emitter.emit_int_binop(
            PcodeOpcode::IntAdd,
            rsp.clone(),
            rsp,
            const_u64(8, 8),
            "POP",
        )?;
        Ok(())
    }

    fn emit_frame_teardown_op(&mut self) -> Result<()> {
        let rsp = gpr(4, 8);
        let rbp = gpr(5, 8);
        self.emitter.emit_copy(rsp.clone(), rbp, "LEAVE")?;
        let value = self.tmp(8);
        self.emitter
            .emit_load(value.clone(), const_u64(0, 8), rsp.clone(), "LEAVE")?;
        self.emitter.emit_copy(gpr(5, 8), value, "LEAVE")?;
        self.emitter.emit_int_binop(
            PcodeOpcode::IntAdd,
            rsp.clone(),
            rsp,
            const_u64(8, 8),
            "LEAVE",
        )?;
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
        self.emitter
            .emit_int_binop(opcode, result.clone(), lhs, rhs, tag)?;
        self.write_operand(
            &instruction.operands[0],
            result.clone(),
            size,
            instruction.length,
            tag,
        )?;
        self.emit_basic_result_flags(result, size, tag)?;
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
        self.emitter
            .emit_int_binop(opcode, result.clone(), lhs, rhs, tag)?;
        self.write_operand(
            &instruction.operands[0],
            result.clone(),
            size,
            instruction.length,
            tag,
        )?;
        self.emit_basic_result_flags(result, size, tag)?;
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
        self.emitter.emit_int_binop(
            if bitwise {
                PcodeOpcode::IntAnd
            } else {
                PcodeOpcode::IntSub
            },
            result.clone(),
            lhs.clone(),
            rhs.clone(),
            tag,
        )?;
        self.emit_basic_result_flags(result, size, tag)?;
        let cf_value = if bitwise {
            const_u64(0, 1)
        } else {
            let cf = self.tmp(1);
            self.emitter
                .emit_int_binop(PcodeOpcode::IntLess, cf.clone(), lhs, rhs, tag)?;
            cf
        };
        self.emitter.emit_copy(flag(0), cf_value, tag)?;
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
        self.emitter.emit_int_unop(opcode, out.clone(), src, tag)?;
        self.write_operand(
            &instruction.operands[0],
            out,
            dst_size,
            instruction.length,
            tag,
        )
    }

    fn emit_setcc(&mut self, instruction: &RuntimeConstructState) -> Result<()> {
        let cond = self.status_predicate_varnode(
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
        self.emitter.emit_int_unop(
            PcodeOpcode::IntSExt,
            gpr(0, dst_size),
            gpr(0, src_size),
            tag,
        )?;
        Ok(())
    }

    fn status_predicate_varnode(&mut self, condition_code: u8) -> Result<Varnode> {
        let value = match condition_code {
            0x0 => flag(11),
            0x1 => self.bool_not(flag(11), "JNO_PRED")?,
            0x2 => flag(0),
            0x3 => self.bool_not(flag(0), "JAE_PRED")?,
            0x4 => flag(6),
            0x5 => self.bool_not(flag(6), "JNE_PRED")?,
            0x6 => self.bool_or(flag(0), flag(6), "JBE_PRED")?,
            0x7 => {
                let ncf = self.bool_not(flag(0), "JA_NCF")?;
                let nzf = self.bool_not(flag(6), "JA_NZF")?;
                self.bool_and(ncf, nzf, "JA_PRED")?
            }
            0x8 => flag(7),
            0x9 => self.bool_not(flag(7), "JNS_PRED")?,
            0xA => flag(2),
            0xB => self.bool_not(flag(2), "JNP_PRED")?,
            0xC => self.bool_ne(flag(7), flag(11), "JL_PRED")?,
            0xD => self.bool_eq(flag(7), flag(11), "JGE_PRED")?,
            0xE => {
                let lt = self.bool_ne(flag(7), flag(11), "JLE_LT_CORE")?;
                self.bool_or(flag(6), lt, "JLE_PRED")?
            }
            0xF => {
                let ge = self.bool_eq(flag(7), flag(11), "JG_GE_CORE")?;
                let nz = self.bool_not(flag(6), "JG_NZ")?;
                self.bool_and(ge, nz, "JG_PRED")?
            }
            _ => bail!("unsupported condition code {condition_code}"),
        };
        Ok(value)
    }

    fn emit_basic_result_flags(&mut self, result: Varnode, size: u32, tag: &str) -> Result<()> {
        let zf = self.tmp(1);
        self.emitter.emit_int_binop(
            PcodeOpcode::IntEqual,
            zf.clone(),
            result.clone(),
            const_u64(0, size),
            tag,
        )?;
        self.emitter.emit_copy(flag(6), zf, tag)?;

        let shift = size.saturating_mul(8).saturating_sub(1);
        let sf = self.tmp(1);
        self.emitter.emit_int_binop(
            PcodeOpcode::IntRight,
            sf.clone(),
            result,
            const_u64(u64::from(shift), size),
            tag,
        )?;
        self.emitter.emit_copy(flag(7), sf, tag)?;
        Ok(())
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
                self.emitter
                    .emit_load(out.clone(), const_u64(0, 8), addr, "LOAD")?;
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
                let destination = gpr(u64::from(*index), *size);
                self.emitter.emit_copy(destination, value.clone(), tag)?;
                if *size == 4 {
                    let canonical = if value.size == 8 {
                        value
                    } else {
                        let extended = self.tmp(8);
                        self.emitter.emit_int_unop(
                            PcodeOpcode::IntZExt,
                            extended.clone(),
                            value,
                            tag,
                        )?;
                        extended
                    };
                    self.emitter
                        .emit_copy(gpr(u64::from(*index), 8), canonical, tag)?;
                }
                Ok(())
            }
            BoundOperand::Memory { .. } => {
                let addr = self.effective_address(operand, instruction_len)?;
                self.emitter.emit_store(const_u64(0, 8), addr, value, tag)?;
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
                    self.emitter.emit_int_binop(
                        PcodeOpcode::IntMult,
                        scaled.clone(),
                        idx,
                        const_u64(u64::from(*scale), 8),
                        "EA_SCALE",
                    )?;
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
                    self.emitter.emit_int_binop(
                        PcodeOpcode::IntSub,
                        tmp.clone(),
                        lhs,
                        rhs,
                        "EA_DISP",
                    )?;
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
            self.emitter
                .emit_int_binop(PcodeOpcode::IntAdd, next.clone(), acc, term, "EA_ADD")?;
            acc = next;
        }
        Ok(acc)
    }

    fn tmp(&mut self, size: u32) -> Varnode {
        self.emitter.tmp(UNIQUE_SPACE_ID, size)
    }

    fn bool_not(&mut self, input: Varnode, tag: &str) -> Result<Varnode> {
        let out = self.tmp(1);
        self.emitter
            .emit_int_unop(PcodeOpcode::BoolNegate, out.clone(), input, tag)?;
        Ok(out)
    }

    fn bool_and(&mut self, lhs: Varnode, rhs: Varnode, tag: &str) -> Result<Varnode> {
        let out = self.tmp(1);
        self.emitter
            .emit_int_binop(PcodeOpcode::BoolAnd, out.clone(), lhs, rhs, tag)?;
        Ok(out)
    }

    fn bool_or(&mut self, lhs: Varnode, rhs: Varnode, tag: &str) -> Result<Varnode> {
        let out = self.tmp(1);
        self.emitter
            .emit_int_binop(PcodeOpcode::BoolOr, out.clone(), lhs, rhs, tag)?;
        Ok(out)
    }

    fn bool_eq(&mut self, lhs: Varnode, rhs: Varnode, tag: &str) -> Result<Varnode> {
        let out = self.tmp(1);
        self.emitter
            .emit_int_binop(PcodeOpcode::IntEqual, out.clone(), lhs, rhs, tag)?;
        Ok(out)
    }

    fn bool_ne(&mut self, lhs: Varnode, rhs: Varnode, tag: &str) -> Result<Varnode> {
        let out = self.tmp(1);
        self.emitter
            .emit_int_binop(PcodeOpcode::IntNotEqual, out.clone(), lhs, rhs, tag)?;
        Ok(out)
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
        offset: SYNTHETIC_REGISTER_SPACE_ROOT + index * 8,
        size,
        is_constant: false,
        constant_val: 0,
    }
}

fn flag(bit: u64) -> Varnode {
    Varnode {
        space_id: UNIQUE_SPACE_ID,
        offset: SYNTHETIC_STATUS_SPACE_ROOT + bit,
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
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let (ops, len) = decode_and_lift(&compiled, &[0xC3], 0x1000).expect("generated ret");
        assert_eq!(len, 1);
        assert_eq!(ops.last().map(|op| op.opcode), Some(PcodeOpcode::Return));
    }

    #[test]
    fn generated_runtime_decodes_mov_imm64() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let bytes = [0x48, 0xB8, 0x34, 0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let (ops, len) = decode_and_lift(&compiled, &bytes, 0x1000).expect("generated mov");
        assert_eq!(len, bytes.len() as u64);
        assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::Copy));
    }

    #[test]
    fn generated_runtime_decodes_jcc_rel8() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let (ops, len) = decode_and_lift(&compiled, &[0x75, 0x05], 0x1000).expect("generated jne");
        assert_eq!(len, 2);
        assert_eq!(ops.last().map(|op| op.opcode), Some(PcodeOpcode::CBranch));
    }

    #[test]
    fn generated_runtime_decodes_startup_store_mov_mem32_imm32() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let bytes = [0xC7, 0x00, 0x01, 0x00, 0x00, 0x00];
        let (ops, len) =
            decode_and_lift(&compiled, &bytes, 0x1000).expect("generated mov [rax], imm32");
        assert_eq!(len, bytes.len() as u64);
        assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::Store));
    }

    #[test]
    fn generated_runtime_decodes_startup_sub_rsp_imm8() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let bytes = [0x48, 0x83, 0xEC, 0x28];
        let (ops, len) =
            decode_and_lift(&compiled, &bytes, 0x1000).expect("generated sub rsp, imm8");
        assert_eq!(len, bytes.len() as u64);
        assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::IntSub));
        assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::Copy));
    }

    #[test]
    fn generated_runtime_decodes_startup_rip_relative_load() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
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
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let bytes = [0xE8, 0x1A, 0xFC, 0xFF, 0xFF];
        let (ops, len) =
            decode_and_lift(&compiled, &bytes, 0x1400_013ef).expect("generated call rel32");
        assert_eq!(len, bytes.len() as u64);
        assert_eq!(ops.last().map(|op| op.opcode), Some(PcodeOpcode::Call));
    }

    #[test]
    fn generated_runtime_records_decision_trace_for_startup_store() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let ctx = CompiledInstructionContext::parse(&[0xC7, 0x00, 0x01, 0x00, 0x00, 0x00], 0x1000)
            .expect("decode context");
        let selection = select_constructor(&compiled, &ctx).expect("constructor selection");
        let state = bind_instruction(&ctx, selection).expect("bind instruction");
        assert_eq!(state.match_trace.root_bucket, "global");
        assert!(!state.match_trace.probes.is_empty());
        assert!(!state.construct_nodes.is_empty());
        assert!(state
            .handles
            .iter()
            .any(|handle| matches!(handle.spec, CompiledOperandSpec::TokenFieldRm { .. })));
    }

    #[test]
    fn generated_runtime_decodes_reg32_lea_without_decode_no_match() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let bytes = [0x8d, 0x04, 0x11];
        let (ops, len) = decode_and_lift(&compiled, &bytes, 0x1400_1450).expect("generated lea");
        assert_eq!(len, bytes.len() as u64);
        assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::IntAdd));
        assert!(ops.iter().any(|op| {
            op.opcode == PcodeOpcode::Copy && op.output.as_ref().is_some_and(|out| out.size == 8)
        }));
    }

    #[test]
    fn generated_runtime_decodes_rip_relative_mov32_without_decode_no_match() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let bytes = [0x8b, 0x05, 0x6a, 0x56, 0x00, 0x00];
        let decoded =
            decode_instruction(&compiled, &bytes, 0x1400_19c0).expect("generated mov rip-relative");
        assert_eq!(decoded.length, bytes.len());
        assert_eq!(decoded.mnemonic, "mov");
        assert!(matches!(
            decoded.references.first().map(|reference| reference.kind),
            Some(DecodedReferenceKind::RipRelativeAddress)
        ));
    }

    #[test]
    fn generated_runtime_decodes_movsxd_without_decode_no_match() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let bytes = [0x48, 0x63, 0x41, 0x3c];
        let (ops, len) = decode_and_lift(&compiled, &bytes, 0x1400_2600).expect("generated movsxd");
        assert_eq!(len, bytes.len() as u64);
        assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::IntSExt));
        assert!(ops.iter().any(|op| {
            op.opcode == PcodeOpcode::Copy && op.output.as_ref().is_some_and(|out| out.size == 8)
        }));
    }

    #[test]
    fn generated_runtime_zero_extends_reg32_writes_to_full_register() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let bytes = [0x31, 0xc0];
        let (ops, len) =
            decode_and_lift(&compiled, &bytes, 0x1400_19e0).expect("generated xor eax, eax");
        assert_eq!(len, bytes.len() as u64);
        assert!(ops.iter().any(|op| {
            op.opcode == PcodeOpcode::IntZExt && op.output.as_ref().is_some_and(|out| out.size == 8)
        }));
        assert!(ops.iter().any(|op| {
            op.opcode == PcodeOpcode::Copy && op.output.as_ref().is_some_and(|out| out.size == 8)
        }));
    }
}
