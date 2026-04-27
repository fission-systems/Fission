//! Transitional compiled-table executor for the common SLEIGH runtime spine.
//! The canonical owner mapping remains `DecisionNode -> RuntimeInstructionContext ->
//! RuntimeConstructState/RuntimeParserWalker -> RuntimeTemplateEvaluator -> RuntimePcodeEmitter`.

use anyhow::{anyhow, bail, Result};
use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};

use crate::compiler::{
    CompiledConstTpl, CompiledConstructTplKind, CompiledDecisionProbe,
    CompiledDisjointPattern, CompiledExecutableConstructor, CompiledFixedRegister, CompiledFrontend, CompiledHandleSelector,
    CompiledHandleTemplate, CompiledHandleTpl, CompiledOpTpl, CompiledOpTplOpcode, CompiledOperandDecodeStep,
    CompiledOperandSpec, CompiledPatternBlock, CompiledPatternExpression, CompiledPatternMatcher, CompiledSpaceRef,
    CompiledSpaceTpl, CompiledTokenFieldRef, CompiledVarnodeTpl,
    CompiledTemplateSource,
};
use crate::runtime::spine::{
    self, BoundOperand, DecisionProbeEvaluator, RuntimeConstructState, RuntimeFixedHandle,
    RuntimeHandle, RuntimeInstructionContext, RuntimePcodeEmitter, RuntimeSelection,
    RuntimeTemplateEvaluator, RuntimeTemplateExecutor,
};
use crate::runtime::{
    DecodedFlowKind, DecodedInstruction, DecodedReference, DecodedReferenceKind,
    RuntimeExecutionDetails, RuntimeSleighError,
};

#[derive(Debug, Clone)]
struct CompiledInstructionContext<'a> {
    inner: RuntimeInstructionContext<'a>,
    context_register: u64,
    context_known_mask: u64,
}

impl<'a> std::ops::Deref for CompiledInstructionContext<'a> {
    type Target = RuntimeInstructionContext<'a>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a> std::ops::DerefMut for CompiledInstructionContext<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<'a> CompiledInstructionContext<'a> {
    fn parse(bytes: &'a [u8], address: u64) -> Result<Self> {
        if bytes.is_empty() {
            bail!("empty compiled-table decode buffer");
        }
        let cursor = 0usize;
        let instruction_width_profile = 1;
        Ok(Self {
            inner: RuntimeInstructionContext::new(
                bytes,
                address,
                cursor,
                instruction_width_profile,
            ),
            context_register: 0,
            context_known_mask: 0,
        })
    }
}

fn packed_context_word(context_register: u64, index: u32) -> Result<u32> {
    match index {
        0 => Ok(context_register as u32),
        1 => Ok((context_register >> 32) as u32),
        _ => bail!("packed context word index {index} is out of range"),
    }
}

fn set_packed_context_word(
    context_register: &mut u64,
    index: u32,
    value: u32,
    mask: u32,
) -> Result<()> {
    let shift = match index {
        0 => 0,
        1 => 32,
        _ => bail!("packed context word index {index} is out of range"),
    };
    let shifted_mask = u64::from(mask) << shift;
    let shifted_value = u64::from(value & mask) << shift;
    *context_register &= !shifted_mask;
    *context_register |= shifted_value;
    Ok(())
}

fn set_packed_context_bits(
    context_register: &mut u64,
    startbit: u32,
    bitsize: u32,
    value: u64,
) -> Result<()> {
    if bitsize == 0 {
        return Ok(());
    }
    if bitsize > 64 {
        bail!("packed context bit write must be 1..=64 bits, got {bitsize}");
    }

    let mut remaining = bitsize;
    let mut word_index = startbit / 32;
    let mut bit_offset = startbit % 32;
    while remaining > 0 {
        let chunk_bits = remaining.min(32 - bit_offset);
        let chunk_mask = if chunk_bits >= 32 {
            u32::MAX
        } else {
            (1u32 << chunk_bits) - 1
        };
        let word_shift = 32 - chunk_bits - bit_offset;
        let value_shift = remaining - chunk_bits;
        let chunk_value = ((value >> value_shift) as u32) & chunk_mask;
        set_packed_context_word(
            context_register,
            word_index,
            chunk_value << word_shift,
            chunk_mask << word_shift,
        )?;
        remaining -= chunk_bits;
        word_index += 1;
        bit_offset = 0;
    }
    Ok(())
}

fn packed_context_bytes(context_register: u64, bytestart: u32, bytesize: u32) -> Result<u32> {
    if bytesize == 0 || bytesize > 4 {
        bail!("packed context byte read must be 1..=4 bytes, got {bytesize}");
    }
    let mut intstart = bytestart / 4;
    let mut res = packed_context_word(context_register, intstart)?;
    let byte_offset = bytestart % 4;
    let mut unused_bytes = 4 - bytesize;
    res <<= byte_offset * 8;
    res >>= unused_bytes * 8;
    let remaining = bytesize as i32 - 4 + byte_offset as i32;
    if remaining > 0 {
        intstart += 1;
        let mut res2 = packed_context_word(context_register, intstart)?;
        unused_bytes = 4 - remaining as u32;
        res2 >>= unused_bytes * 8;
        res |= res2;
    }
    Ok(res)
}

fn packed_context_bits(context_register: u64, startbit: u32, bitsize: u32) -> Result<u32> {
    if bitsize == 0 {
        return Ok(0);
    }
    if bitsize > 32 {
        bail!("packed context bit read must be 1..=32 bits, got {bitsize}");
    }
    let mut intstart = startbit / 32;
    let mut res = packed_context_word(context_register, intstart)?;
    let bit_offset = startbit % 32;
    let mut unused_bits = 32 - bitsize;
    res <<= bit_offset;
    res >>= unused_bits;
    let remaining = bitsize as i32 - 32 + bit_offset as i32;
    if remaining > 0 {
        intstart += 1;
        let mut res2 = packed_context_word(context_register, intstart)?;
        unused_bits = 32 - remaining as u32;
        res2 >>= unused_bits;
        res |= res2;
    }
    Ok(res)
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

use std::sync::Arc;
use crate::runtime::native::NativeBackend;

pub(crate) fn decode_and_lift_with_details(
    compiled: &CompiledFrontend,
    native: Option<&Arc<NativeBackend>>,
    bytes: &[u8],
    address: u64,
) -> Result<(Vec<PcodeOp>, u64, RuntimeExecutionDetails)> {
    let mut ctx = CompiledInstructionContext::parse(bytes, address)?;
    ctx.context_register = compiled.default_context;
    ctx.context_known_mask = compiled.default_context_known_mask;

    let native = native.filter(|_| native_backend_allowed(compiled, "instruction", &ctx));
    let candidates = candidate_selections(compiled, native, &ctx, address)?;
    let mut first_error: Option<anyhow::Error> = None;

    for selection in candidates {
        if !selection.constructor.runtime_ready {
            let err = unsupported_constructor_error(compiled, selection.constructor);
            if first_error.is_none() {
                first_error = Some(err.into());
            }
            continue;
        }

        let decoded = match bind_instruction(compiled, native, &ctx, selection) {
            Ok(decoded) => decoded,
            Err(err) => {
                if first_error.is_none() {
                    first_error = Some(err);
                }
                continue;
            }
        };

        match emit_pcode_for_state(compiled, address, &decoded) {
            Ok((ops, details)) => return Ok((ops, decoded.length as u64, details)),
            Err(err) => {
                if first_error.is_none() {
                    first_error = Some(err);
                }
            }
        }
    }

    Err(first_error.unwrap_or_else(|| RuntimeSleighError::DecodeNoMatch {
        language: compiled.entry_id.clone(),
        address,
    }
    .into()))
}

pub(crate) fn decode_instruction(
    compiled: &CompiledFrontend,
    native: Option<&Arc<NativeBackend>>,
    bytes: &[u8],
    address: u64,
) -> Result<DecodedInstruction> {
    let mut ctx = CompiledInstructionContext::parse(bytes, address)?;
    ctx.context_register = compiled.default_context;
    ctx.context_known_mask = compiled.default_context_known_mask;

    let native = native.filter(|_| native_backend_allowed(compiled, "instruction", &ctx));
    let candidates = candidate_selections(compiled, native, &ctx, address)?;
    let mut fallback_state = None;
    let mut first_error: Option<anyhow::Error> = None;

    for selection in candidates {
        if std::env::var_os("FISSION_TRACE_AARCH64_RESELECT").is_some() {
            eprintln!(
                "[decode-instruction] ctor={} mnemonic={} source={} ctx=0x{:016x} known=0x{:016x}",
                selection.constructor_index,
                selection.constructor.mnemonic,
                selection.constructor.source,
                ctx.context_register,
                ctx.context_known_mask,
            );
        }
        if !selection.constructor.runtime_ready {
            let err = unsupported_constructor_error(compiled, selection.constructor);
            if first_error.is_none() {
                first_error = Some(err.into());
            }
            continue;
        }

        let decoded = match bind_instruction(compiled, native, &ctx, selection.clone()) {
            Ok(decoded) => decoded,
            Err(err) => {
                if first_error.is_none() {
                    first_error = Some(err);
                }
                continue;
            }
        };

        match emit_pcode_for_state(compiled, address, &decoded) {
            Ok(_) => {
                return decoded_instruction_from_state(address, bytes, decoded);
            }
            Err(err) => {
                if fallback_state.is_none() {
                    fallback_state = Some(decoded);
                }
                if first_error.is_none() {
                    first_error = Some(err);
                }
            }
        }
    }

    if let Some(decoded) = fallback_state {
        return decoded_instruction_from_state(address, bytes, decoded);
    }

    return Err(first_error.unwrap_or_else(|| RuntimeSleighError::DecodeNoMatch {
        language: compiled.entry_id.clone(),
        address,
    }
    .into()));
}

fn decoded_instruction_from_state(
    address: u64,
    bytes: &[u8],
    decoded: RuntimeConstructState,
) -> Result<DecodedInstruction> {
    let length = decoded.length;
    let (mnemonic, operands_text) = render_instruction_display(&decoded);
    let direct_target = decoded.operands.first().and_then(|operand| match operand {
        BoundOperand::Relative { target } => Some(*target),
        _ => None,
    });
    let flow_kind = flow_kind_for_state(&decoded);
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

fn candidate_selections<'a>(
    compiled: &'a CompiledFrontend,
    native: Option<&'a Arc<NativeBackend>>,
    ctx: &CompiledInstructionContext<'_>,
    address: u64,
) -> Result<Vec<RuntimeSelection<'a>>> {
    let instruction_table = compiled
        .subtables
        .get("instruction")
        .expect("missing 'instruction' subtable");
    let primary = if let Some(native) = native {
        let constructor_index = native
            .decode_match("instruction", ctx.bytes, ctx.context_register)?
            .ok_or_else(|| RuntimeSleighError::DecodeNoMatch {
                language: compiled.entry_id.clone(),
                address,
            })?;
        let constructor = instruction_table
            .constructors
            .get(constructor_index)
            .ok_or_else(|| anyhow!("invalid constructor index returned by backend"))?;
        RuntimeSelection {
            constructor,
            constructor_index,
            trace: spine::RuntimeMatchTrace {
                root_bucket: "native".to_string(),
                probes: Vec::new(),
                leaf_constructor_indexes: vec![constructor_index],
                matched_leaf_pattern: None,
            },
        }
    } else {
        select_constructor(compiled, "instruction", ctx).ok_or_else(|| {
            RuntimeSleighError::DecodeNoMatch {
                language: compiled.entry_id.clone(),
                address,
            }
        })?
    };

    Ok(vec![primary])
}

fn emit_pcode_for_state(
    compiled: &CompiledFrontend,
    address: u64,
    decoded: &RuntimeConstructState,
) -> Result<(Vec<PcodeOp>, RuntimeExecutionDetails)> {
    let mut emitter = CompiledTableEmitter::new(address);
    let details = RuntimeTemplateEvaluator::new(&mut emitter)
        .emit(&compiled.entry_id, decoded)
        .map_err(|err| template_emit_error(compiled, err))?;
    Ok((emitter.finish(), details))
}

fn template_emit_error(compiled: &CompiledFrontend, err: anyhow::Error) -> anyhow::Error {
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

fn select_constructor<'a>(
    compiled: &'a CompiledFrontend,
    table_name: &str,
    ctx: &CompiledInstructionContext<'_>,
) -> Option<RuntimeSelection<'a>> {
    let subtable = compiled.subtables.get(table_name)?;
    spine::select_constructor(
        compiled,
        [(table_name.to_string(), subtable.decision_tree.root_node_index)],
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
    fn probe_values(&mut self, probe: CompiledDecisionProbe) -> Result<Vec<u8>> {
        Ok(match probe {
            CompiledDecisionProbe::Terminal => vec![0],
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
                vec![(byte & mask) >> shift]
            }
            CompiledDecisionProbe::ContextBitSlice {
                offset,
                mask,
                shift,
            } => {
                possible_context_probe_values(
                    self.ctx.context_register,
                    self.ctx.context_known_mask,
                    u32::from(offset),
                    8,
                )?
                .into_iter()
                .map(|value| ((value & u64::from(mask)) >> shift) as u8)
                .collect()
            }
            CompiledDecisionProbe::ContextFieldRef(_) => vec![0],
            CompiledDecisionProbe::TokenFieldRef(
                CompiledTokenFieldRef::InstructionWidthProfile,
            ) => vec![self.ctx.instruction_width_profile],
            CompiledDecisionProbe::TokenFieldRef(CompiledTokenFieldRef::AddressingForm) => {
                vec![ensure_token_fields(self.ctx, &mut self.cached_token_fields)
                    .map(|bundle| bundle.operand_mode)
                    .unwrap_or(0)]
            }
            CompiledDecisionProbe::TokenFieldRef(CompiledTokenFieldRef::RegisterSelector) => {
                vec![ensure_token_fields(self.ctx, &mut self.cached_token_fields)
                    .map(|bundle| bundle.reg)
                    .unwrap_or(0)]
            }
            CompiledDecisionProbe::SlaInstructionBits { start_bit, bit_size } => {
                let byte_offset = start_bit / 8;
                let bit_offset = start_bit % 8;
                let byte_cnt = (bit_offset + bit_size + 7) / 8;
                if self.ctx.cursor + byte_offset as usize >= self.ctx.bytes.len() {
                    bail!("instruction bit read out of range at bit {start_bit}");
                }
                let mut word = 0u64;
                for i in 0..byte_cnt {
                    word <<= 8;
                    word |= u64::from(
                        self.ctx
                            .bytes
                            .get(self.ctx.cursor + byte_offset as usize + i as usize)
                            .copied()
                            .unwrap_or(0),
                    );
                }
                let shift = (8 * byte_cnt) - bit_offset - bit_size;
                vec![((word >> shift) & ((1u64 << bit_size) - 1)) as u8]
            }
            CompiledDecisionProbe::SlaContextBits { start_bit, bit_size } => {
                vec![packed_context_bits(self.ctx.context_register, start_bit, bit_size)? as u8]
            }
            CompiledDecisionProbe::TerminalPatternCheck => vec![0],
        })
    }

    fn instruction_bytes(&self, offset: i32, size: u32) -> Result<u32> {
        if size == 0 || size > 4 {
            bail!("instruction byte read must be 1..=4 bytes, got {size}");
        }
        let start = self
            .ctx
            .cursor
            .checked_add_signed(offset as isize)
            .ok_or_else(|| anyhow!("instruction byte read underflow at offset {offset}"))?;
        if start == self.ctx.cursor && start >= self.ctx.bytes.len() {
            bail!("instruction byte read out of range at offset {offset}");
        }
        let mut word = 0u32;
        for i in 0..size as usize {
            word <<= 8;
            word |= u32::from(self.ctx.bytes.get(start + i).copied().unwrap_or(0));
        }
        Ok(word)
    }

    fn context_bytes(&self, offset: i32, size: u32) -> Result<u32> {
        if offset < 0 {
            bail!("context byte read underflow at offset {offset}");
        }
        packed_context_bytes(self.ctx.context_register, offset as u32, size)
    }
}

fn possible_context_probe_values(
    context_register: u64,
    context_known_mask: u64,
    start_bit: u32,
    bit_size: u32,
) -> Result<Vec<u64>> {
    if bit_size == 0 {
        return Ok(vec![0]);
    }
    if bit_size > 16 {
        bail!("context probe width {bit_size} is too large to enumerate");
    }
    let field_mask = if bit_size == 64 {
        u64::MAX
    } else {
        (1u64 << bit_size) - 1
    };
    let known = u64::from(packed_context_bits(context_known_mask, start_bit, bit_size)?);
    let known_value = u64::from(packed_context_bits(context_register, start_bit, bit_size)?) & known;
    let unknown_positions = (0..bit_size)
        .filter(|bit| ((known >> bit) & 1) == 0)
        .collect::<Vec<_>>();
    let combinations = 1usize << unknown_positions.len();
    let mut values = Vec::with_capacity(combinations);
    for combo in 0..combinations {
        let mut value = known_value;
        for (combo_bit, position) in unknown_positions.iter().enumerate() {
            if ((combo >> combo_bit) & 1) != 0 {
                value |= 1u64 << position;
            }
        }
        values.push(value);
    }
    values.sort_unstable();
    values.dedup();
    Ok(values)
}

fn native_backend_allowed(
    compiled: &CompiledFrontend,
    table_name: &str,
    ctx: &CompiledInstructionContext<'_>,
) -> bool {
    if is_x86_compat_language(compiled) {
        return true;
    }
    let Some(subtable) = compiled.subtables.get(table_name) else {
        return false;
    };
    !subtable
        .decision_tree
        .nodes
        .iter()
        .any(|node| match node.probe {
            CompiledDecisionProbe::ContextBitSlice { offset, mask, .. } => {
                let relevant_mask = u64::from(mask) << offset;
                (ctx.context_known_mask & relevant_mask) != relevant_mask
            }
            CompiledDecisionProbe::SlaContextBits { .. } => true,
            _ => false,
        })
}

fn unsupported_constructor_error(
    compiled: &CompiledFrontend,
    constructor: &CompiledExecutableConstructor,
) -> RuntimeSleighError {
    RuntimeSleighError::UnsupportedPcodeTemplate {
        language: compiled.entry_id.clone(),
        reason: constructor
            .unsupported_template_kind
            .clone()
            .unwrap_or_else(|| "unsupported_constructor_template".to_string()),
    }
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
    let reg = ((byte >> 3) & 0x7) | ((false as u8) << 3);
    let rm_low = byte & 0x7;
    let rm = rm_low | ((false as u8) << 3);
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
            index = Some(index_low | ((false as u8) << 3));
        }
        if operand_mode == 0 && base_low == 5 {
            base = None;
            displacement = read_sint(ctx.bytes, offset + length, 4)?;
            length += 4;
        } else {
            base = Some(base_low | ((false as u8) << 3));
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
        CompiledPatternMatcher::BitConstraints(constraints) => {
            for constraint in constraints {
                match constraint {
                    crate::compiler::PatternConstraint::Instruction { offset, mask, value } => {
                        let mut inst_val = 0u64;
                        for i in 0..8 {
                            if let Some(byte) = ctx.bytes.get(ctx.cursor + *offset as usize + i) {
                                inst_val |= u64::from(*byte) << (i * 8);
                            }
                        }
                        if (inst_val & mask) != *value {
                            bail!("instruction bit constraint mismatch");
                        }
                    }
                    crate::compiler::PatternConstraint::Context { offset, mask, value } => {
                        let val = (ctx.context_register >> offset) & mask;
                        if val != *value {
                            bail!("context bit constraint mismatch");
                        }
                    }
                }
            }
        }
    }

    let requires_token_bundle = constructor.mod_constraint.is_some()
        || !constructor.operand_reg_values.is_empty()
        || constructor.operand_specs.iter().any(|spec| {
            matches!(
                spec,
                CompiledOperandSpec::TokenFieldExtraction { .. }
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
        if false && token_fields.operand_mode == 3
        {
            bail!("memory-only token field mismatch");
        }
    }

    Ok(())
}

fn bind_instruction<'a>(
    compiled: &'a CompiledFrontend,
    native: Option<&'a Arc<NativeBackend>>,
    ctx: &CompiledInstructionContext<'_>,
    selection: RuntimeSelection<'a>,
) -> Result<RuntimeConstructState> {
    constructor_matches(ctx, selection.constructor)?;
    CompiledParserWalker::new(compiled, native, ctx, selection)?.walk()
}

struct CompiledParserWalker<'a, 'b> {
    compiled: &'a CompiledFrontend,
    native: Option<&'a Arc<NativeBackend>>,
    ctx: &'a CompiledInstructionContext<'b>,
    selection: RuntimeSelection<'a>,
    minimum_length: usize,
    context_register: u64,
    context_known_mask: u64,
    cursor: usize,
    token_fields: Option<TokenFieldBundle>,
    handles: Vec<Option<RuntimeHandle>>,
    walker: spine::RuntimeParserWalker,
}

struct OperandBinding {
    value: BoundOperand,
    subtable_state: Option<RuntimeConstructState>,
    fixed: Option<RuntimeFixedHandle>,
}

impl OperandBinding {
    fn plain(value: BoundOperand) -> Self {
        Self {
            value,
            subtable_state: None,
            fixed: None,
        }
    }

    fn with_fixed(value: BoundOperand, fixed: RuntimeFixedHandle) -> Self {
        Self {
            value,
            subtable_state: None,
            fixed: Some(fixed),
        }
    }
}

fn constructor_consumes_sequential_operand_bytes(
    compiled: &CompiledFrontend,
    constructor: &CompiledExecutableConstructor,
) -> bool {
    if is_x86_compat_language(compiled)
        && constructor
            .constructor_template
            .handles
            .iter()
            .any(|handle| matches!(handle.spec, CompiledOperandSpec::SubtableEvaluation { .. }))
    {
        return true;
    }
    constructor
        .constructor_template
        .handles
        .iter()
        .any(|handle| operand_spec_consumes_sequential_bytes(compiled, &handle.spec, 0))
}

fn subtable_consumes_sequential_bytes(
    compiled: &CompiledFrontend,
    table_name: &str,
    depth: usize,
) -> bool {
    if is_x86_zero_width_subtable(table_name) {
        return false;
    }
    if is_x86_compat_language(compiled) {
        return true;
    }
    if depth > 8 {
        return false;
    }
    let Some(subtable) = compiled.subtables.get(table_name) else {
        return false;
    };
    subtable
        .constructors
        .iter()
        .any(|constructor| {
            constructor_consumes_sequential_operand_bytes_with_depth(
                compiled,
                constructor,
                depth + 1,
            )
        })
}

fn constructor_consumes_sequential_operand_bytes_with_depth(
    compiled: &CompiledFrontend,
    constructor: &CompiledExecutableConstructor,
    depth: usize,
) -> bool {
    constructor
        .constructor_template
        .handles
        .iter()
        .any(|handle| operand_spec_consumes_sequential_bytes(compiled, &handle.spec, depth))
}

fn operand_spec_consumes_sequential_bytes(
    compiled: &CompiledFrontend,
    spec: &CompiledOperandSpec,
    depth: usize,
) -> bool {
    match spec {
        CompiledOperandSpec::TokenFieldExtraction { .. }
        | CompiledOperandSpec::Immediate { .. }
        | CompiledOperandSpec::Relative { .. } => true,
        CompiledOperandSpec::SubtableEvaluation { table_name }
            if is_x86_compat_language(compiled) =>
        {
            !is_x86_zero_width_subtable(table_name)
        }
        CompiledOperandSpec::SubtableEvaluation { table_name } => {
            subtable_consumes_sequential_bytes(compiled, table_name, depth + 1)
        }
        _ => false,
    }
}

fn is_x86_compat_language(compiled: &CompiledFrontend) -> bool {
    compiled.arch.eq_ignore_ascii_case("x86") || compiled.entry_id.starts_with("x86")
}

fn is_x86_zero_width_subtable(table_name: &str) -> bool {
    matches!(
        table_name,
        "xrelease"
            | "xacq_xrel_prefx"
            | "lockx"
            | "unlock"
            | "segWide"
            | "Reg8"
            | "Reg16"
            | "Reg32"
            | "Reg64"
            | "check_Reg32_dest"
            | "check_Rmr32_dest"
            | "check_rm32_dest"
            | "check_EAX_dest"
    )
}

fn is_x86_register_subtable(table_name: &str) -> bool {
    matches!(table_name, "Reg8" | "Reg16" | "Reg32" | "Reg64")
}

fn constructor_replaces_current(constructor: &CompiledExecutableConstructor) -> bool {
    constructor
        .constructor_template
        .decode_steps
        .iter()
        .any(|step| {
            matches!(
                step,
                CompiledOperandDecodeStep::DescendSubtable {
                    replace_current: true,
                    ..
                }
            )
        })
}

impl<'a, 'b> CompiledParserWalker<'a, 'b> {
    fn new(
        compiled: &'a CompiledFrontend,
        native: Option<&'a Arc<NativeBackend>>,
        ctx: &'a CompiledInstructionContext<'b>,
        selection: RuntimeSelection<'a>,
    ) -> Result<Self> {
        let opcode_len = if is_x86_compat_language(compiled)
            && constructor_replaces_current(selection.constructor)
        {
            0
        } else if selection.constructor.constructor_template.template_source
            == CompiledTemplateSource::SpecDerived
        {
            if selection.trace.root_bucket == "instruction"
                && constructor_consumes_sequential_operand_bytes(compiled, selection.constructor)
            {
                opcode_len_from_context(ctx)?
            } else {
                0
            }
        } else {
            opcode_len_from_matcher(&selection.constructor.matcher)
        };
        let minimum_length = selection.constructor.minimum_length as usize;
        let handles = vec![None; selection.constructor.constructor_template.handles.len()];
        Ok(Self {
            compiled,
            native,
            ctx,
            selection,
            minimum_length,
            context_register: ctx.context_register,
            context_known_mask: ctx.context_known_mask,
            cursor: ctx.cursor + opcode_len,
            token_fields: None,
            handles,
            walker: spine::RuntimeParserWalker::new(ctx.cursor, opcode_len),
        })
    }

    fn walk(mut self) -> Result<RuntimeConstructState> {
        for change in self.selection.constructor.context_changes.clone() {
            self.apply_context_change(&change)?;
        }

        let decode_steps = self
            .selection
            .constructor
            .constructor_template
            .decode_steps
            .clone();
        let x86_replace_current_wrapper = is_x86_compat_language(self.compiled)
            && decode_steps.iter().any(|step| {
                matches!(
                    step,
                    CompiledOperandDecodeStep::DescendSubtable {
                        replace_current: true,
                        ..
                    }
                )
            });
        for step in decode_steps {
            match step {
                CompiledOperandDecodeStep::ConsumeTokenFields => {
                    if !x86_replace_current_wrapper {
                        self.ensure_token_fields()?;
                    }
                }
                CompiledOperandDecodeStep::DecodeOperand { operand_index } => {
                    self.decode_operand(operand_index)?;
                }
                CompiledOperandDecodeStep::DescendSubtable {
                    table_name,
                    replace_current,
                } => {
                    let sub_state = self.decode_subtable(&table_name)?;
                    if replace_current {
                        return Ok(sub_state);
                    }
                }
            }
        }

        let mut handles = std::mem::take(&mut self.handles)
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| anyhow!("incomplete handle decode"))?;
        handles.sort_by_key(|handle| handle.operand_index);
        let exported_handle = self.materialize_export_handle(&handles)?;
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
            mnemonic: self.selection.constructor.mnemonic.clone(),
            construct_tpl_kind: self.selection.constructor.construct_tpl_kind,
            constructor_template: self.selection.constructor.constructor_template.clone(),
            display_template: self.selection.constructor.display_template.clone(),
            display_operands: self.selection.constructor.display_operands.clone(),
            construct_nodes: self.walker.into_nodes(),
            handles,
            exported_handle,
            operands,
            condition_code,
            length: self.cursor.max(self.ctx.cursor + self.minimum_length),
            match_trace: self.selection.trace,
        })
    }

    fn materialize_export_handle(&mut self, handles: &[RuntimeHandle]) -> Result<Option<RuntimeHandle>> {
        let Some(export_tpl) = self
            .selection
            .constructor
            .constructor_template
            .export
            .clone()
        else {
            return Ok(None);
        };
        let fixed = self.fixed_handle_from_handle_tpl(&export_tpl, handles)?;
        let value = bound_operand_from_fixed_handle(&fixed)?;
        Ok(Some(RuntimeHandle {
            operand_index: usize::MAX,
            spec: CompiledOperandSpec::SubtableEvaluation {
                table_name: self.selection.constructor.source.clone(),
            },
            value,
            fixed,
            subtable_state: None,
        }))
    }

    fn fixed_handle_from_handle_tpl(
        &mut self,
        handle_tpl: &CompiledHandleTpl,
        handles: &[RuntimeHandle],
    ) -> Result<RuntimeFixedHandle> {
        let space = handle_tpl
            .space
            .as_ref()
            .map(|space| self.resolve_export_space_tpl(space, handles))
            .transpose()?;
        let size = handle_tpl
            .size
            .as_ref()
            .map(|value| self.resolve_export_const_tpl(value, handles))
            .transpose()?
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or(0);
        let offset_space = handle_tpl
            .ptr_space
            .as_ref()
            .map(|space| self.resolve_export_space_tpl(space, handles))
            .transpose()?;
        let offset_offset = handle_tpl
            .ptr_offset
            .as_ref()
            .map(|value| self.resolve_export_const_tpl(value, handles))
            .transpose()?
            .unwrap_or(0);
        let offset_size = handle_tpl
            .ptr_size
            .as_ref()
            .map(|value| self.resolve_export_const_tpl(value, handles))
            .transpose()?
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or(0);
        let temp_space = handle_tpl
            .temp_space
            .as_ref()
            .map(|space| self.resolve_export_space_tpl(space, handles))
            .transpose()?;
        let temp_offset = handle_tpl
            .temp_offset
            .as_ref()
            .map(|value| self.resolve_export_const_tpl(value, handles))
            .transpose()?
            .unwrap_or(0);
        let fixable = space.is_some()
            && (offset_space.is_none()
                || (offset_size != 0 && temp_space.is_some()));
        Ok(RuntimeFixedHandle {
            space,
            size,
            offset_space,
            offset_offset,
            offset_size,
            temp_space,
            temp_offset,
            fixable,
        })
    }

    fn resolve_export_space_tpl(
        &mut self,
        space: &CompiledSpaceTpl,
        handles: &[RuntimeHandle],
    ) -> Result<CompiledSpaceRef> {
        match space {
            CompiledSpaceTpl::SpaceRef(space) => Ok(space.clone()),
            CompiledSpaceTpl::Const(value) => {
                let index = self.resolve_export_const_tpl(value, handles)?;
                let name = match index {
                    0 => "const",
                    2 => "unique",
                    3 => "ram",
                    4 => "register",
                    _ => "unknown",
                };
                Ok(CompiledSpaceRef {
                    name: name.to_string(),
                    index,
                })
            }
        }
    }

    fn resolve_export_const_tpl(
        &mut self,
        value: &CompiledConstTpl,
        handles: &[RuntimeHandle],
    ) -> Result<u64> {
        match value {
            CompiledConstTpl::Real { value } => Ok(*value),
            CompiledConstTpl::Integer { value, .. } if *value >= 0 => Ok(*value as u64),
            CompiledConstTpl::Integer { value, .. } => {
                Ok((*value as i128 as u128 & u64::MAX as u128) as u64)
            }
            CompiledConstTpl::SpaceId(space) => Ok(space.index),
            CompiledConstTpl::Handle {
                handle_index,
                selector,
                plus,
            } => {
                let handle = handles
                    .get(*handle_index as usize)
                    .ok_or_else(|| anyhow!("export handle {} is missing", handle_index))?;
                let value = match selector {
                    CompiledHandleSelector::Space => handle
                        .fixed
                        .space
                        .as_ref()
                        .map(|space| space.index)
                        .ok_or_else(|| anyhow!("export fixed handle missing space"))?,
                    CompiledHandleSelector::Offset => handle.fixed.offset_offset,
                    CompiledHandleSelector::Size => u64::from(handle.fixed.size),
                    CompiledHandleSelector::OffsetPlus => bail!("export OffsetPlus unsupported"),
                };
                Ok(value.wrapping_add(plus.unwrap_or(0)))
            }
            CompiledConstTpl::InstStart => Ok(self.ctx.address),
            CompiledConstTpl::InstNext => Ok(self.ctx.address.saturating_add(self.minimum_length as u64)),
            other => bail!("export ConstTpl {:?} is unsupported", other),
        }
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
        let binding = self.bind_operand(&template)?;
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
            fixed: binding
                .fixed
                .unwrap_or_else(|| fixed_handle_for_bound_operand(&binding.value)),
            value: binding.value,
            subtable_state: binding.subtable_state.map(Box::new),
        });
        Ok(())
    }

    fn ensure_token_fields(&mut self) -> Result<TokenFieldBundle> {
        if self.token_fields.is_none() {
            let token_offset = if self.selection.constructor.constructor_template.template_source
                == CompiledTemplateSource::SpecDerived
                && self.selection.trace.root_bucket == "instruction"
            {
                self.ctx.cursor + opcode_len_from_context(self.ctx)?
            } else {
                self.cursor
            };
            let decoded = parse_token_fields(self.ctx, token_offset)?;
            self.cursor = self.cursor.max(token_offset + decoded.length);
            self.token_fields = Some(decoded);
        }
        self.token_fields
            .ok_or_else(|| anyhow!("failed to decode token fields"))
    }

    fn bind_operand(&mut self, template: &CompiledHandleTemplate) -> Result<OperandBinding> {
        match &template.spec {
            CompiledOperandSpec::TokenFieldExtraction { bit_offset, bit_width, sign_extend } => {
                let token_fields = self.ensure_token_fields()?;
                if token_fields.operand_mode == 3 {
                    Ok(OperandBinding::plain(BoundOperand::Register {
                        index: token_fields.rm,
                        size: *bit_width / 8,
                    }))
                } else {
                    Ok(OperandBinding::plain(BoundOperand::Memory {
                        base: token_fields.base,
                        index: token_fields.index,
                        scale: token_fields.scale,
                        displacement: token_fields.displacement,
                        rip_relative: token_fields.rip_relative,
                        absolute: token_fields.rip_relative.then(|| {
                            self.ctx
                                .address
                                .wrapping_add(self.cursor as u64)
                                .wrapping_add_signed(token_fields.displacement)
                        }),
                        size: *bit_width / 8,
                    }))
                }
            }
            CompiledOperandSpec::SlaTokenField {
                big_endian,
                sign_bit,
                bit_start,
                bit_end,
                byte_start,
                byte_end,
                shift,
            } => {
                let token_base = if is_x86_compat_language(self.compiled) {
                    self.cursor
                } else {
                    self.ctx.cursor
                };
                let value = read_sla_token_field_at(
                    self.ctx,
                    token_base,
                    *big_endian,
                    *sign_bit,
                    *bit_start,
                    *bit_end,
                    *byte_start,
                    *byte_end,
                    *shift,
                )?;
                if is_x86_compat_language(self.compiled) {
                    self.cursor = self
                        .cursor
                        .max(token_base + ((*byte_end - *byte_start) + 1) as usize);
                }
                Ok(OperandBinding::plain(BoundOperand::Immediate {
                    value,
                    encoded_size: ((*byte_end - *byte_start) + 1).max(1),
                    signed: *sign_bit,
                }))
            }
            CompiledOperandSpec::SlaVarnodeList {
                big_endian,
                sign_bit: _,
                bit_start,
                bit_end,
                byte_start,
                byte_end,
                shift,
                entries,
            } => {
                let token_base = if is_x86_compat_language(self.compiled) {
                    self.cursor
                } else {
                    self.ctx.cursor
                };
                let selector = read_sla_token_field_at(
                    self.ctx,
                    token_base,
                    *big_endian,
                    false,
                    *bit_start,
                    *bit_end,
                    *byte_start,
                    *byte_end,
                    *shift,
                )?;
                if is_x86_compat_language(self.compiled) {
                    self.cursor = self
                        .cursor
                        .max(token_base + ((*byte_end - *byte_start) + 1) as usize);
                }
                let entry = entries.get(selector as usize).ok_or_else(|| {
                    anyhow!(
                        "varnode list selector {} out of range for {} entries",
                        selector,
                        entries.len()
                    )
                })?;
                Ok(OperandBinding::with_fixed(
                    BoundOperand::NamedVarnode {
                        name: entry.name.clone(),
                        display_index: Some(selector as u32),
                        size: entry.size,
                    },
                    fixed_handle_from_resolved_varnode(entry),
                ))
            }
            CompiledOperandSpec::SlaFixedVarnode { varnode } => Ok(OperandBinding::with_fixed(
                BoundOperand::NamedVarnode {
                    name: varnode.name.clone(),
                    display_index: None,
                    size: varnode.size,
                },
                fixed_handle_from_resolved_varnode(varnode),
            )),
            CompiledOperandSpec::SlaPatternExpression { expr } => {
                let value = if is_x86_compat_language(self.compiled) {
                    if let CompiledPatternExpression::TokenField {
                        big_endian,
                        sign_bit,
                        bit_start,
                        bit_end,
                        byte_start,
                        byte_end,
                        shift,
                    } = expr
                    {
                        let token_base = self.cursor;
                        let value = read_sla_token_field_at(
                            self.ctx,
                            token_base,
                            *big_endian,
                            *sign_bit,
                            *bit_start,
                            *bit_end,
                            *byte_start,
                            *byte_end,
                            *shift,
                        )? as i64;
                        self.cursor = self
                            .cursor
                            .max(token_base + ((*byte_end - *byte_start) + 1) as usize);
                        value
                    } else {
                        self.eval_pattern_expression(expr)?
                    }
                } else {
                    self.eval_pattern_expression(expr)?
                };
                Ok(OperandBinding::plain(BoundOperand::Immediate {
                    value: value as u64,
                    encoded_size: 0,
                    signed: value < 0,
                }))
            }
            CompiledOperandSpec::ContextFieldExtraction { bit_offset, bit_width, sign_extend } => {
                let val = u64::from(packed_context_bits(
                    self.context_register,
                    *bit_offset,
                    *bit_width,
                )?);
                let value = if *sign_extend {
                    let shift = 64 - bit_width;
                    ((val << shift) as i64 >> shift) as u64
                } else {
                    val
                };
                Ok(OperandBinding::plain(BoundOperand::Immediate {
                    value,
                    encoded_size: (*bit_width / 8).max(1),
                    signed: *sign_extend,
                }))
            }
            CompiledOperandSpec::SubtableEvaluation { table_name } => {
                let cursor_start = self.cursor;
                let sub_state = self.decode_subtable(table_name)?;
                if is_x86_zero_width_subtable(table_name) {
                    self.cursor = cursor_start;
                } else if self.selection.constructor.constructor_template.template_source
                    == CompiledTemplateSource::SpecDerived
                    && !subtable_consumes_sequential_bytes(self.compiled, table_name, 0)
                {
                    self.minimum_length = self
                        .minimum_length
                        .max(sub_state.length.saturating_sub(self.ctx.cursor));
                    self.cursor = cursor_start;
                } else {
                    let mut next_cursor = sub_state.length;
                    if next_cursor <= cursor_start && is_x86_compat_language(self.compiled) {
                        next_cursor = cursor_start.saturating_add(1);
                    }
                    self.cursor = self.cursor.max(next_cursor);
                }
                // Return the exported handle from the sub-constructor. Some
                // x86 subtables are zero-op BUILD checks/prefix hooks and do
                // not export a value; keep those as handle placeholders only,
                // never as synthetic p-code.
                let exported = match sub_state.exported_handle.as_ref() {
                    Some(exported) => exported,
                    None => {
                        if let Some(binding) =
                            self.fallback_binding_for_no_export_subtable(table_name, cursor_start)?
                        {
                            return Ok(OperandBinding {
                                value: binding.value,
                                fixed: binding.fixed,
                                subtable_state: Some(sub_state),
                            });
                        }
                        bail!("subtable {table_name} did not export a handle");
                    }
                };
                Ok(OperandBinding {
                    value: exported.value.clone(),
                    fixed: Some(exported.fixed.clone()),
                    subtable_state: Some(sub_state),
                })
            }
            CompiledOperandSpec::Immediate { size, signed } => {
                let value = read_uint(self.ctx.bytes, self.cursor, *size)?;
                self.cursor += *size as usize;
                Ok(OperandBinding::plain(BoundOperand::Immediate {
                    value,
                    encoded_size: *size,
                    signed: *signed,
                }))
            }
            CompiledOperandSpec::Relative { size } => {
                let signed = read_sint(self.ctx.bytes, self.cursor, *size)?;
                self.cursor += *size as usize;
                let next_ip = self.ctx.address.wrapping_add(self.cursor as u64);
                Ok(OperandBinding::plain(BoundOperand::Relative {
                    target: next_ip.wrapping_add_signed(signed),
                }))
            }
            CompiledOperandSpec::FixedRegister { reg, size } => {
                let index = match reg {
                    CompiledFixedRegister::Accumulator => 0,
                    CompiledFixedRegister::StackPointer => 4,
                    CompiledFixedRegister::FramePointer => 5,
                };
                Ok(OperandBinding::plain(BoundOperand::Register { index, size: *size }))
            }
        }
    }

    fn fallback_binding_for_no_export_subtable(
        &mut self,
        table_name: &str,
        cursor_start: usize,
    ) -> Result<Option<OperandBinding>> {
        if !is_x86_compat_language(self.compiled) {
            return Ok(None);
        }

        if is_x86_zero_width_subtable(table_name) {
            self.cursor = cursor_start;
            let value = BoundOperand::Immediate {
                value: 0,
                encoded_size: 0,
                signed: false,
            };
            return Ok(Some(OperandBinding::with_fixed(
                value.clone(),
                fixed_handle_for_bound_operand(&value),
            )));
        }

        let relative_size = table_name
            .strip_prefix("pcRelSimm")
            .or_else(|| table_name.strip_prefix("rel"))
            .and_then(|suffix| suffix.parse::<u32>().ok())
            .map(|bits| (bits / 8).max(1));
        let Some(size) = relative_size else {
            return Ok(None);
        };
        let signed = read_sint(self.ctx.bytes, cursor_start, size)?;
        self.cursor = self.cursor.max(cursor_start + size as usize);
        let next_ip = self.ctx.address.wrapping_add(self.cursor as u64);
        let target = next_ip.wrapping_add_signed(signed);
        let value = BoundOperand::Relative { target };
        Ok(Some(OperandBinding::with_fixed(
            value.clone(),
            fixed_handle_for_bound_operand(&value),
        )))
    }

    fn apply_context_change(&mut self, change: &crate::compiler::CompiledContextOp) -> Result<()> {
        if let Some(expr) = &change.expr {
            let saved_cursor = self.cursor;
            let raw = self.eval_pattern_expression(expr)? as u32;
            self.cursor = saved_cursor;
            let value = if change.shift >= 0 {
                raw << (change.shift as u32)
            } else {
                raw >> ((-change.shift) as u32)
            };
            set_packed_context_word(
                &mut self.context_register,
                change.word_index,
                value,
                change.mask as u32,
            )?;
            set_packed_context_word(
                &mut self.context_known_mask,
                change.word_index,
                change.mask as u32,
                change.mask as u32,
            )?;
            if std::env::var_os("FISSION_TRACE_AARCH64_RESELECT").is_some() {
                eprintln!(
                    "[context-change expr] word={} mask=0x{:08x} value=0x{:08x} ctx=0x{:016x} known=0x{:016x}",
                    change.word_index,
                    change.mask as u32,
                    value,
                    self.context_register,
                    self.context_known_mask,
                );
            }
            Ok(())
        } else {
            let field_mask = if change.bit_width >= 64 {
                u64::MAX
            } else {
                (1u64 << change.bit_width) - 1
            };
            let masked_value = change.value & field_mask;
            set_packed_context_bits(
                &mut self.context_register,
                change.bit_offset as u32,
                change.bit_width as u32,
                masked_value,
            )?;
            set_packed_context_bits(
                &mut self.context_known_mask,
                change.bit_offset as u32,
                change.bit_width as u32,
                field_mask,
            )?;
            if std::env::var_os("FISSION_TRACE_AARCH64_RESELECT").is_some() {
                eprintln!(
                    "[context-change bits] start={} width={} value=0x{:x} ctx=0x{:016x} known=0x{:016x}",
                    change.bit_offset,
                    change.bit_width,
                    masked_value,
                    self.context_register,
                    self.context_known_mask,
                );
            }
            Ok(())
        }
    }

    fn eval_pattern_expression(&mut self, expr: &CompiledPatternExpression) -> Result<i64> {
        match expr {
            CompiledPatternExpression::Constant(value) => Ok(*value),
            CompiledPatternExpression::TokenField {
                big_endian,
                sign_bit,
                bit_start,
                bit_end,
                byte_start,
                byte_end,
                shift,
            } => Ok(read_sla_token_field(
                self.ctx,
                *big_endian,
                *sign_bit,
                *bit_start,
                *bit_end,
                *byte_start,
                *byte_end,
                *shift,
            )? as i64),
            CompiledPatternExpression::ContextField {
                sign_bit,
                bit_start,
                bit_end,
                byte_start: _,
                byte_end: _,
                shift: _,
            } => {
                let bit_width = bit_end
                    .checked_sub(*bit_start)
                    .and_then(|value| value.checked_add(1))
                    .ok_or_else(|| anyhow!("invalid context field {}..{}", bit_start, bit_end))?;
                let raw = u64::from(packed_context_bits(
                    self.context_register,
                    *bit_start,
                    bit_width,
                )?);
                if *sign_bit {
                    let shift = 64 - bit_width;
                    Ok(((raw << shift) as i64) >> shift)
                } else {
                    Ok(raw as i64)
                }
            }
            CompiledPatternExpression::OperandValue { index } => {
                self.decode_operand(*index)?;
                let handle = self
                    .handles
                    .get(*index)
                    .and_then(|value| value.as_ref())
                    .ok_or_else(|| anyhow!("operand {} was not decoded for pattern expression", index))?;
                match handle.value.clone() {
                    BoundOperand::Immediate { value, .. } => Ok(value as i64),
                    BoundOperand::Relative { target } => Ok(target as i64),
                    BoundOperand::Register { index, .. } => Ok(i64::from(index)),
                    BoundOperand::NamedVarnode {
                        display_index: Some(index),
                        ..
                    } => Ok(i64::from(index)),
                    BoundOperand::NamedVarnode { name, .. } => {
                        bail!("operand {name} has no numeric selector value")
                    }
                    BoundOperand::Memory { absolute, displacement, .. } => {
                        Ok(absolute.unwrap_or(displacement as u64) as i64)
                    }
                }
            }
            CompiledPatternExpression::Add(lhs, rhs) => {
                Ok(self.eval_pattern_expression(lhs)? + self.eval_pattern_expression(rhs)?)
            }
            CompiledPatternExpression::Sub(lhs, rhs) => {
                Ok(self.eval_pattern_expression(lhs)? - self.eval_pattern_expression(rhs)?)
            }
            CompiledPatternExpression::Mul(lhs, rhs) => {
                Ok(self.eval_pattern_expression(lhs)? * self.eval_pattern_expression(rhs)?)
            }
            CompiledPatternExpression::Div(lhs, rhs) => {
                let rhs = self.eval_pattern_expression(rhs)?;
                if rhs == 0 {
                    bail!("pattern expression divide by zero");
                }
                Ok(self.eval_pattern_expression(lhs)? / rhs)
            }
            CompiledPatternExpression::LeftShift(lhs, rhs) => Ok(
                self.eval_pattern_expression(lhs)?
                    << (self.eval_pattern_expression(rhs)? as u32),
            ),
            CompiledPatternExpression::RightShift(lhs, rhs) => {
                let lhs = self.eval_pattern_expression(lhs)? as u64;
                Ok((lhs >> (self.eval_pattern_expression(rhs)? as u32)) as i64)
            }
            CompiledPatternExpression::And(lhs, rhs) => {
                Ok(self.eval_pattern_expression(lhs)? & self.eval_pattern_expression(rhs)?)
            }
            CompiledPatternExpression::Or(lhs, rhs) => {
                Ok(self.eval_pattern_expression(lhs)? | self.eval_pattern_expression(rhs)?)
            }
            CompiledPatternExpression::Xor(lhs, rhs) => {
                Ok(self.eval_pattern_expression(lhs)? ^ self.eval_pattern_expression(rhs)?)
            }
            CompiledPatternExpression::Negate(inner) => Ok(-self.eval_pattern_expression(inner)?),
            CompiledPatternExpression::Not(inner) => Ok(!self.eval_pattern_expression(inner)?),
        }
    }

    fn decode_subtable(&self, table_name: &str) -> Result<RuntimeConstructState> {
        let mut sub_ctx = (*self.ctx).clone();
        let consumed_instruction_bytes = if is_x86_compat_language(self.compiled) {
            0
        } else {
            self.selection
                .trace
                .matched_leaf_pattern
                .as_ref()
                .map(disjoint_pattern_instruction_byte_len)
                .unwrap_or(0)
        };
        sub_ctx.cursor = if is_x86_compat_language(self.compiled)
            && constructor_replaces_current(self.selection.constructor)
            && table_name == "instruction"
        {
            self.ctx.cursor + opcode_len_from_context(self.ctx).unwrap_or(0)
        } else if is_x86_register_subtable(table_name) {
            self.ctx.cursor + opcode_len_from_context(self.ctx).unwrap_or(0)
        } else if self.selection.constructor.context_changes.is_empty()
            || consumed_instruction_bytes == 0
        {
            self.cursor
        } else {
            self.ctx.cursor + consumed_instruction_bytes.max(self.cursor.saturating_sub(self.ctx.cursor))
        };
        sub_ctx.context_register = self.context_register;
        sub_ctx.context_known_mask = self.context_known_mask;
        if std::env::var_os("FISSION_TRACE_AARCH64_RESELECT").is_some() {
            eprintln!(
                "[decode-subtable] table={} cursor=0x{:x} ctx=0x{:016x} known=0x{:016x}",
                table_name,
                sub_ctx.cursor,
                sub_ctx.context_register,
                sub_ctx.context_known_mask,
            );
        }

        let selection = if let Some(native) = self
            .native
            .filter(|_| native_backend_allowed(self.compiled, table_name, &sub_ctx))
        {
            let constructor_index = native
                .decode_match(table_name, self.ctx.bytes, sub_ctx.context_register)?
                .ok_or_else(|| {
                    anyhow!(
                        "DecodeNoMatch in subtable {table_name} at 0x{:x}",
                        sub_ctx.address.wrapping_add(sub_ctx.cursor as u64)
                    )
                })?;
            let subtable = self
                .compiled
                .subtables
                .get(table_name)
                .ok_or_else(|| anyhow!("missing subtable {table_name}"))?;
            let constructor = subtable
                .constructors
                .get(constructor_index)
                .ok_or_else(|| anyhow!("invalid constructor index {constructor_index} in subtable {table_name}"))?;
            RuntimeSelection {
                constructor,
                constructor_index,
                trace: spine::RuntimeMatchTrace {
                    root_bucket: format!("native:{}", table_name),
                    probes: Vec::new(),
                    leaf_constructor_indexes: vec![constructor_index],
                    matched_leaf_pattern: None,
                },
            }
        } else {
            select_constructor(self.compiled, table_name, &sub_ctx).ok_or_else(|| {
                anyhow!(
                    "DecodeNoMatch in subtable {table_name} at 0x{:x}",
                    sub_ctx.address.wrapping_add(sub_ctx.cursor as u64)
                )
            })?
        };
        if std::env::var_os("FISSION_TRACE_AARCH64_RESELECT").is_some() {
            eprintln!(
                "[decode-subtable selection] table={} ctor={} mnemonic={} source={}",
                table_name,
                selection.constructor_index,
                selection.constructor.mnemonic,
                selection.constructor.source,
            );
        }

        bind_instruction(self.compiled, self.native, &sub_ctx, selection)
    }
}

fn read_sla_token_field(
    ctx: &CompiledInstructionContext<'_>,
    big_endian: bool,
    sign_bit: bool,
    bit_start: u32,
    bit_end: u32,
    byte_start: u32,
    byte_end: u32,
    shift: i32,
) -> Result<u64> {
    read_sla_token_field_at(
        ctx,
        ctx.cursor,
        big_endian,
        sign_bit,
        bit_start,
        bit_end,
        byte_start,
        byte_end,
        shift,
    )
}

fn read_sla_token_field_at(
    ctx: &CompiledInstructionContext<'_>,
    base_cursor: usize,
    big_endian: bool,
    sign_bit: bool,
    bit_start: u32,
    bit_end: u32,
    byte_start: u32,
    byte_end: u32,
    shift: i32,
) -> Result<u64> {
    let size = byte_end.saturating_sub(byte_start) + 1;
    let mut res = 0u64;
    for idx in 0..size {
        let off = if big_endian {
            byte_start + idx
        } else {
            byte_end.saturating_sub(idx)
        } as usize;
        let byte = *ctx
            .bytes
            .get(base_cursor + off)
            .ok_or_else(|| anyhow!("tokenfield byte {} out of range", off))?;
        res = (res << 8) | u64::from(byte);
    }
    let shifted = if shift >= 0 {
        res >> (shift as u32)
    } else {
        res << ((-shift) as u32)
    };
    let width = bit_end.saturating_sub(bit_start) + 1;
    Ok(if sign_bit {
        sign_extend_bits(shifted, width)
    } else {
        zero_extend_bits(shifted, width)
    })
}

fn opcode_len_from_matcher(matcher: &CompiledPatternMatcher) -> usize {
    match matcher {
        CompiledPatternMatcher::ExactBytes(bytes) => bytes.len(),
        CompiledPatternMatcher::RowCc { prefix, .. } => prefix.len() + 1,
        CompiledPatternMatcher::RowPage { .. } => 1,
        CompiledPatternMatcher::BitConstraints(constraints) => constraints
            .iter()
            .filter_map(|c| match c {
                crate::compiler::PatternConstraint::Instruction { offset, .. } => Some(*offset as usize + 1),
                _ => None,
            })
            .max()
            .unwrap_or(0),
    }
}

fn flow_kind_for(kind: CompiledConstructTplKind) -> DecodedFlowKind {
    match kind {
        CompiledConstructTplKind::Unsupported => DecodedFlowKind::None,
        CompiledConstructTplKind::Call => DecodedFlowKind::Call,
        CompiledConstructTplKind::Jmp => DecodedFlowKind::Jump,
        CompiledConstructTplKind::Jcc => DecodedFlowKind::ConditionalJump,
        CompiledConstructTplKind::Ret => DecodedFlowKind::Return,
        _ => DecodedFlowKind::None,
    }
}

fn flow_kind_for_state(state: &RuntimeConstructState) -> DecodedFlowKind {
    if state
        .constructor_template
        .op_templates
        .iter()
        .any(|op| matches!(op.opcode, CompiledOpTplOpcode::Return))
    {
        return DecodedFlowKind::Return;
    }
    if state
        .constructor_template
        .op_templates
        .iter()
        .any(|op| matches!(op.opcode, CompiledOpTplOpcode::Call))
    {
        return DecodedFlowKind::Call;
    }
    flow_kind_for(state.construct_tpl_kind)
}

fn zero_extend_bits(value: u64, bit: u32) -> u64 {
    if bit >= 64 {
        value
    } else {
        let mask = (1u64 << bit) - 1;
        value & mask
    }
}

fn sign_extend_bits(value: u64, bit: u32) -> u64 {
    if bit == 0 || bit >= 64 {
        return value;
    }
    let sign_mask = 1u64 << (bit - 1);
    let value = zero_extend_bits(value, bit);
    if value & sign_mask != 0 {
        value | (!0u64 << bit)
    } else {
        value
    }
}

fn disasm_mnemonic(state: &RuntimeConstructState) -> String {
    // Final rendering must come from SLEIGH display templates. Until that
    // template IR is executable, keep condition-code rendering isolated to
    // the display-only Jcc NativeFission holdout. This must not affect p-code
    // template execution.
    if matches!(state.construct_tpl_kind, CompiledConstructTplKind::Jcc) {
        if let Some(cc) = state.condition_code {
            if let Some(mnemonic) = jcc_mnemonic(cc) {
                return mnemonic.to_string();
            }
        }
    }
    state.mnemonic.replace('^', "").to_ascii_lowercase()
}

fn render_instruction_display(state: &RuntimeConstructState) -> (String, String) {
    if state.display_template.pieces.is_empty() {
        return (
            disasm_mnemonic(state),
            state
                .operands
                .iter()
                .map(format_operand)
                .collect::<Vec<_>>()
                .join(", "),
        );
    }

    let (mnemonic, body) = render_display_template_parts(state);
    let mnemonic = if mnemonic.is_empty() {
        disasm_mnemonic(state)
    } else {
        mnemonic.replace('^', "").to_ascii_lowercase()
    };
    (mnemonic, body)
}

fn render_display_template_parts(state: &RuntimeConstructState) -> (String, String) {
    if let Some(flow_index) = state.display_template.flowthru_operand_index {
        if let Some(child) = state
            .handles
            .get(flow_index)
            .and_then(|handle| handle.subtable_state.as_deref())
        {
            return render_display_template_parts(child);
        }
    }

    let split = state
        .display_template
        .first_whitespace
        .unwrap_or(state.display_template.pieces.len());
    let mnemonic = render_display_pieces(state, &state.display_template.pieces[..split]);
    let body = if state.display_template.first_whitespace.is_some() && split < state.display_template.pieces.len() {
        render_display_pieces(state, &state.display_template.pieces[split + 1..])
    } else {
        String::new()
    };
    (mnemonic, body)
}

fn render_display_pieces(
    state: &RuntimeConstructState,
    pieces: &[crate::compiler::CompiledDisplayPiece],
) -> String {
    let mut rendered = String::new();
    for piece in pieces {
        match piece {
            crate::compiler::CompiledDisplayPiece::Literal(literal) => rendered.push_str(literal),
            crate::compiler::CompiledDisplayPiece::OperandRef(index) => {
                rendered.push_str(&render_operand_display(state, *index));
            }
        }
    }
    rendered
}

fn render_operand_display(state: &RuntimeConstructState, operand_index: usize) -> String {
    let Some(handle) = state.handles.get(operand_index) else {
        return String::new();
    };
    if let Some(child) = handle.subtable_state.as_deref() {
        let (mnemonic, body) = render_display_template_parts(child);
        return if body.is_empty() {
            mnemonic
        } else {
            format!("{mnemonic} {body}")
        };
    }
    let display_kind = state
        .display_operands
        .get(operand_index)
        .map(|operand| &operand.kind);
    format_operand_with_display_kind(&handle.value, display_kind)
}

fn jcc_mnemonic(cc: u8) -> Option<&'static str> {
    Some(match cc {
        0 => "jo",
        1 => "jno",
        2 => "jb",
        3 => "jnb",
        4 => "jz",
        5 => "jnz",
        6 => "jbe",
        7 => "ja",
        8 => "js",
        9 => "jns",
        10 => "jp",
        11 => "jnp",
        12 => "jl",
        13 => "jge",
        14 => "jle",
        15 => "jg",
        _ => return None,
    })
}

fn format_operand(operand: &BoundOperand) -> String {
    format_operand_with_display_kind(operand, None)
}

fn format_operand_with_display_kind(
    operand: &BoundOperand,
    display_kind: Option<&crate::compiler::CompiledDisplayOperandKind>,
) -> String {
    if let Some(kind) = display_kind {
        match kind {
            crate::compiler::CompiledDisplayOperandKind::NameTable(names)
            | crate::compiler::CompiledDisplayOperandKind::VarnodeList(names) => {
                if let Some(index) = operand_display_index(operand) {
                    if let Some(name) = names.get(index) {
                        return name.clone();
                    }
                }
            }
            crate::compiler::CompiledDisplayOperandKind::ValueMap(values) => {
                if let Some(index) = operand_display_index(operand) {
                    if let Some(value) = values.get(index) {
                        return format_signed_hex(*value);
                    }
                }
            }
            crate::compiler::CompiledDisplayOperandKind::ValueHex => {
                if let Some(value) = operand_display_value(operand) {
                    return format_signed_hex(value);
                }
            }
            crate::compiler::CompiledDisplayOperandKind::Generic
            | crate::compiler::CompiledDisplayOperandKind::Subtable => {}
        }
    }

    match operand {
        BoundOperand::Register { index, size } => format!("reg{size}_{index}"),
        BoundOperand::NamedVarnode { name, .. } => name.clone(),
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

fn operand_display_index(operand: &BoundOperand) -> Option<usize> {
    match operand {
        BoundOperand::Immediate { value, .. } => Some(*value as usize),
        BoundOperand::Register { index, .. } => Some(*index as usize),
        BoundOperand::NamedVarnode { display_index, .. } => display_index.map(|idx| idx as usize),
        BoundOperand::Relative { target } => Some(*target as usize),
        BoundOperand::Memory { absolute, displacement, .. } => {
            absolute.map(|value| value as usize).or_else(|| usize::try_from(*displacement).ok())
        }
    }
}

fn operand_display_value(operand: &BoundOperand) -> Option<i64> {
    match operand {
        BoundOperand::Immediate { value, .. } => Some(*value as i64),
        BoundOperand::Register { index, .. } => Some(i64::from(*index)),
        BoundOperand::NamedVarnode { display_index, .. } => display_index.map(i64::from),
        BoundOperand::Relative { target } => Some(*target as i64),
        BoundOperand::Memory { absolute, displacement, .. } => {
            absolute.map(|value| value as i64).or(Some(*displacement))
        }
    }
}

fn format_signed_hex(value: i64) -> String {
    if value >= 0 {
        format!("0x{:x}", value as u64)
    } else {
        format!("-0x{:x}", value.unsigned_abs())
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

fn disjoint_pattern_instruction_byte_len(pattern: &CompiledDisjointPattern) -> usize {
    match pattern {
        CompiledDisjointPattern::Instruction(block) => pattern_block_byte_len(block),
        CompiledDisjointPattern::Context(_) => 0,
        CompiledDisjointPattern::Combine { instruction, .. } => {
            pattern_block_byte_len(instruction)
        }
    }
}

fn pattern_block_byte_len(block: &CompiledPatternBlock) -> usize {
    if block.nonzero_size <= 0 {
        return 0;
    }
    block
        .offset
        .max(0)
        .saturating_add(block.nonzero_size)
        .try_into()
        .unwrap_or(usize::MAX)
}

fn compiled_space(name: &str, index: u64) -> CompiledSpaceRef {
    CompiledSpaceRef {
        name: name.to_string(),
        index,
    }
}

fn register_offset(index: u8) -> u64 {
    if index < 8 {
        (index as u64) * 8
    } else {
        128 + ((index as u64) - 8) * 8
    }
}

fn fixed_handle_for_bound_operand(value: &BoundOperand) -> RuntimeFixedHandle {
    match value {
        BoundOperand::Register { index, size } => RuntimeFixedHandle {
            space: Some(compiled_space("register", 4)),
            size: *size,
            offset_space: None,
            offset_offset: register_offset(*index),
            offset_size: *size,
            temp_space: None,
            temp_offset: 0,
            fixable: true,
        },
        BoundOperand::NamedVarnode { size, .. } => RuntimeFixedHandle {
            space: None,
            size: *size,
            offset_space: None,
            offset_offset: 0,
            offset_size: *size,
            temp_space: None,
            temp_offset: 0,
            fixable: false,
        },
        BoundOperand::Immediate {
            value,
            encoded_size,
            ..
        } => RuntimeFixedHandle {
            space: Some(compiled_space("const", 0)),
            size: *encoded_size,
            offset_space: None,
            offset_offset: *value,
            offset_size: *encoded_size,
            temp_space: None,
            temp_offset: 0,
            fixable: true,
        },
        BoundOperand::Relative { target } => RuntimeFixedHandle {
            space: Some(compiled_space("ram", 3)),
            size: 8,
            offset_space: None,
            offset_offset: *target,
            offset_size: 8,
            temp_space: None,
            temp_offset: 0,
            fixable: true,
        },
        BoundOperand::Memory {
            base,
            index,
            displacement,
            rip_relative,
            size,
            ..
        } if base.is_some() && index.is_none() && *displacement == 0 && !*rip_relative => {
            RuntimeFixedHandle {
                // SpaceId constants in STORE/LOAD use Ghidra's address-space
                // id, while actual pointer varnodes stay in the register space.
                space: Some(compiled_space("ram", 0x1b1)),
                size: *size,
                offset_space: Some(compiled_space("register", 4)),
                offset_offset: register_offset(base.expect("checked above")),
                offset_size: 8,
                temp_space: Some(compiled_space("unique", 2)),
                temp_offset: 0xd400,
                fixable: true,
            }
        }
        BoundOperand::Memory {
            rip_relative: true,
            absolute: Some(absolute),
            size,
            ..
        } => RuntimeFixedHandle {
            space: Some(compiled_space("ram", 3)),
            size: *size,
            offset_space: None,
            offset_offset: *absolute,
            offset_size: *size,
            temp_space: None,
            temp_offset: 0,
            fixable: true,
        },
        BoundOperand::Memory { size, .. } => RuntimeFixedHandle {
            space: Some(compiled_space("ram", 0x1b1)),
            size: *size,
            offset_space: None,
            offset_offset: 0,
            offset_size: 0,
            temp_space: None,
            temp_offset: 0,
            fixable: false,
        },
    }
}

fn fixed_handle_from_resolved_varnode(
    varnode: &crate::compiler::CompiledResolvedVarnode,
) -> RuntimeFixedHandle {
    RuntimeFixedHandle {
        space: Some(varnode.space.clone()),
        size: varnode.size,
        offset_space: None,
        offset_offset: varnode.offset,
        offset_size: varnode.size,
        temp_space: None,
        temp_offset: 0,
        fixable: true,
    }
}

fn bound_operand_from_fixed_handle(handle: &RuntimeFixedHandle) -> Result<BoundOperand> {
    let space = handle
        .space
        .as_ref()
        .ok_or_else(|| anyhow!("exported fixed handle missing space"))?;
    if space.index == 0 || space.name == "const" {
        return Ok(BoundOperand::Immediate {
            value: handle.offset_offset,
            encoded_size: handle.size.max(1),
            signed: false,
        });
    }
    if space.name == "register" || space.index == 4 {
        return Ok(BoundOperand::NamedVarnode {
            name: format!("register_{:x}", handle.offset_offset),
            display_index: None,
            size: handle.size,
        });
    }
    Ok(BoundOperand::Memory {
        base: None,
        index: None,
        scale: 1,
        displacement: handle.offset_offset as i64,
        rip_relative: false,
        absolute: Some(handle.offset_offset),
        size: handle.size,
    })
}

fn varnode_from_fixed_handle(handle: &RuntimeFixedHandle) -> Result<Varnode> {
    if handle.offset_space.is_some() {
        bail!("dynamic fixed handle cannot materialize into a direct varnode");
    }
    let space = handle
        .space
        .as_ref()
        .ok_or_else(|| anyhow!("fixed handle missing space"))?;
    let size = if handle.size > 0 {
        handle.size
    } else {
        handle.offset_size
    };
    if space.name == "const" {
        Ok(Varnode::constant(handle.offset_offset as i64, size))
    } else {
        Ok(Varnode {
            space_id: space.index,
            offset: handle.offset_offset,
            size,
            is_constant: false,
            constant_val: 0,
        })
    }
}

fn handle_selector_index_in_space(
    space: &CompiledSpaceTpl,
    selector: CompiledHandleSelector,
) -> Option<usize> {
    let CompiledSpaceTpl::Const(const_tpl) = space else {
        return None;
    };
    handle_selector_index(const_tpl, selector)
}

fn negative_handle_selector_index_in_space(
    space: &CompiledSpaceTpl,
    selector: CompiledHandleSelector,
) -> Option<i64> {
    let CompiledSpaceTpl::Const(const_tpl) = space else {
        return None;
    };
    let CompiledConstTpl::Handle {
        handle_index,
        selector: actual_selector,
        plus,
    } = const_tpl.as_ref()
    else {
        return None;
    };
    if *actual_selector == selector && plus.is_none() && *handle_index < 0 {
        Some(*handle_index)
    } else {
        None
    }
}

fn matches_handle_selector(
    const_tpl: &CompiledConstTpl,
    handle_index: usize,
    selector: CompiledHandleSelector,
) -> bool {
    handle_selector_index(const_tpl, selector).is_some_and(|idx| idx == handle_index)
}

fn handle_selector_index(
    const_tpl: &CompiledConstTpl,
    expected_selector: CompiledHandleSelector,
) -> Option<usize> {
    let CompiledConstTpl::Handle {
        handle_index,
        selector,
        plus,
    } = const_tpl
    else {
        return None;
    };
    if *selector != expected_selector || plus.is_some() || *handle_index < 0 {
        return None;
    }
    Some(*handle_index as usize)
}

fn matches_negative_handle_selector(
    const_tpl: &CompiledConstTpl,
    handle_index: i64,
    expected_selector: CompiledHandleSelector,
) -> bool {
    let CompiledConstTpl::Handle {
        handle_index: actual_handle_index,
        selector,
        plus,
    } = const_tpl
    else {
        return false;
    };
    *actual_handle_index == handle_index && *selector == expected_selector && plus.is_none()
}

#[derive(Debug, Clone)]
struct CompiledTableEmitter {
    emitter: RuntimePcodeEmitter,
    address: u64,
    /// Cache of resolved effective-address varnodes for Memory operands.
    /// Key is the handle index, value is the unique-space varnode holding
    /// the computed pointer.
    resolved_memory_ea: std::collections::BTreeMap<usize, Varnode>,
    /// Exported varnodes produced by BUILD subconstructors. Ghidra templates
    /// reference these through negative handle indices in parent templates.
    exported_build_varnodes: std::collections::BTreeMap<i64, Varnode>,
}

#[derive(Debug, Clone)]
struct DynamicMemoryTarget {
    space: Varnode,
    ptr: Varnode,
    temp: Varnode,
    size: u32,
}

impl CompiledTableEmitter {
    fn new(address: u64) -> Self {
        // Use Ghidra-compatible unique offset base: 0x9300 is the first
        // canonical unique temp used by the x86 Addr sub-constructors.
        // Ghidra computes: (addr & uniquemask) << 8, but for the SLA
        // template temps these are static offsets baked into the spec.
        // We start at 0x9300 to match Ghidra's allocation pattern.
        Self {
            address,
            emitter: RuntimePcodeEmitter::new(address, 0x9300),
            resolved_memory_ea: std::collections::BTreeMap::new(),
            exported_build_varnodes: std::collections::BTreeMap::new(),
        }
    }

    fn finish(self) -> Vec<PcodeOp> {
        self.emitter.finish()
    }

    fn emit_op_template(
        &mut self,
        state: &RuntimeConstructState,
        op: &CompiledOpTpl,
    ) -> Result<()> {
        let mnemonic = op.opcode.as_str();
        match op.opcode {
            CompiledOpTplOpcode::Label => Ok(()),
            CompiledOpTplOpcode::Return => {
                if op.output.is_some() || op.inputs.len() > 1 {
                    bail!("RETURN template shape is unsupported");
                }
                if let Some(target_tpl) = op.inputs.first() {
                    let target = self.read_template_varnode(target_tpl, state, 8)?;
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
                let target = self.read_template_varnode(target_tpl, state, 8)?;
                self.emitter.emit_call(target, mnemonic)
            }
            CompiledOpTplOpcode::Branch => {
                let target_tpl = op
                    .inputs
                    .first()
                    .ok_or_else(|| anyhow!("BRANCH template requires one input"))?;
                let target = self.read_template_varnode(target_tpl, state, 8)?;
                let is_indirect = matches!(target_tpl, CompiledVarnodeTpl::Handle { operand_index }
                    if !matches!(state.operands.get(*operand_index), Some(BoundOperand::Relative { .. })));
                if is_indirect {
                    self.emitter.emit_branch_ind(target, mnemonic)
                } else {
                    self.emitter.emit_branch(target, mnemonic)
                }
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
                    let store_value = if value.is_constant {
                        self.emitter
                            .emit_copy(target.temp.clone(), value, mnemonic)?;
                        target.temp
                    } else {
                        value
                    };
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
                    // In x86-64, writing to a 32-bit register implicitly
                    // zero-extends to 64 bits. Emit INT_ZEXT if the output
                    // is a 4-byte register (Ghidra does this explicitly).
                    if out.size == 4 && out.space_id == 4 {
                        let extended = Varnode {
                            size: 8,
                            ..out.clone()
                        };
                        self.emitter.append_checked(
                            fission_pcode::PcodeOpcode::IntZExt,
                            Some(extended),
                            vec![out],
                            "INT_ZEXT",
                        )?;
                    }
                    Ok(())
                } else {
                    self.write_template_target(out_tpl, value, state, mnemonic)
                }
            }
            CompiledOpTplOpcode::IntZExt
            | CompiledOpTplOpcode::IntSExt
            | CompiledOpTplOpcode::BoolNegate
            | CompiledOpTplOpcode::PopCount => {
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
            | CompiledOpTplOpcode::IntLeft
            | CompiledOpTplOpcode::IntRight
            | CompiledOpTplOpcode::IntSRight
            | CompiledOpTplOpcode::IntEqual
            | CompiledOpTplOpcode::IntNotEqual
            | CompiledOpTplOpcode::IntLess
            | CompiledOpTplOpcode::IntSLess
            | CompiledOpTplOpcode::BoolAnd
            | CompiledOpTplOpcode::BoolOr => {
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
                if lhs.is_constant && rhs.size > lhs.size {
                    lhs.size = rhs.size;
                } else if rhs.is_constant && lhs.size > rhs.size {
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
                let space = self.read_template_varnode(&op.inputs[0], state, 8)?;
                let ptr = self.read_template_varnode(&op.inputs[1], state, 8)?;
                let out = self.materialize_write_varnode(out_tpl, state, mnemonic)?;
                self.emitter.emit_load(out.clone(), space, ptr, mnemonic)?;
                self.commit_template_write_target(out_tpl, out, state, mnemonic)
            }
            CompiledOpTplOpcode::Store => {
                if op.output.is_some() || op.inputs.len() != 3 {
                    bail!("STORE template requires three inputs and no output");
                }
                let space = self.read_template_varnode(&op.inputs[0], state, 8)?;
                let ptr = self.read_template_varnode(&op.inputs[1], state, 8)?;
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
                    let target = self.read_template_varnode(&op.inputs[0], state, 8)?;
                    let cond = self.read_template_varnode(&op.inputs[1], state, 1)?;
                    self.emitter.emit_cbranch(target, cond, mnemonic)
                } else {
                    let out_tpl =
                        out_tpl.ok_or_else(|| anyhow!("{} template requires output", mnemonic))?;
                    if op.inputs.len() != 2 {
                        bail!("{mnemonic} template requires two inputs");
                    }
                    let out_size = self.template_varnode_size(out_tpl, state)?;
                    let lhs_expected_size = if matches!(op.opcode, CompiledOpTplOpcode::Subpiece)
                    {
                        0
                    } else {
                        out_size
                    };
                    let lhs = self.read_template_varnode(&op.inputs[0], state, lhs_expected_size)?;
                    let rhs_size = self.template_varnode_size(&op.inputs[1], state)?;
                    let rhs = self.read_template_varnode(&op.inputs[1], state, rhs_size)?;
                    let out = self.materialize_write_varnode(out_tpl, state, mnemonic)?;
                    let opcode = self.binary_pcode_opcode(op.opcode)?;
                    self.emitter
                        .emit_int_binop(opcode, out.clone(), lhs, rhs, mnemonic)?;
                    self.commit_template_write_target(out_tpl, out, state, mnemonic)
                }
            }
            // Build: subtable inlining directive. In Ghidra, this recursively
            // emits the P-code from a matched sub-constructor's template.
            // For Memory operands, we emit the address calculation P-code
            // here (INT_MULT for scale, INT_ADD for base+index+disp) and
            // cache the result varnode so that the parent COPY/LOAD/STORE
            // template can reference it via handle selectors.
            CompiledOpTplOpcode::Build => {
                let operand_index = if let Some(input_tpl) = op.inputs.first() {
                    match input_tpl {
                        CompiledVarnodeTpl::Varnode { offset, .. } => match offset.as_ref() {
                            CompiledConstTpl::Real { value } => Some(*value as usize),
                            _ => None,
                        },
                        _ => None,
                    }
                } else {
                    None
                };
                if let Some(idx) = operand_index {
                    if idx == 0
                        && state.condition_code.is_some()
                        && matches!(state.construct_tpl_kind, CompiledConstructTplKind::Jcc)
                    {
                        let predicate = self.emit_condition_predicate(
                            state.condition_code.expect("checked above"),
                            "cc",
                        )?;
                        self.exported_build_varnodes.insert(-1, predicate);
                    }
                    if let Some(handle) = state.handles.get(idx) {
                        if let BoundOperand::Memory {
                            base,
                            index,
                            scale,
                            displacement,
                            rip_relative,
                            size: _op_size,
                            ..
                        } = &handle.value
                        {
                            if base.is_none()
                                && index.is_none()
                                && *displacement == 0
                                && !*rip_relative
                            {
                                if let Some(offset_space) = &handle.fixed.offset_space {
                                    self.resolved_memory_ea.insert(
                                        idx,
                                        Varnode {
                                            space_id: offset_space.index,
                                            offset: handle.fixed.offset_offset,
                                            size: handle.fixed.offset_size.max(8),
                                            is_constant: false,
                                            constant_val: 0,
                                        },
                                    );
                                    return Ok(());
                                }
                            }
                            // Emit address calculation P-code matching Ghidra's
                            // Addr sub-constructor output.
                            // Helper for Ghidra x86-64 register mapping
                            let reg_offset = |reg: u8| -> u64 {
                                if reg < 8 {
                                    (reg as u64) * 8
                                } else {
                                    128 + ((reg as u64) - 8) * 8
                                }
                            };

                            let mut tmp: Option<Varnode> = None;

                            // Step 1: index * scale
                            if let Some(idx_reg) = index {
                                let idx_vn = Varnode {
                                    space_id: 4, // register space
                                    offset: reg_offset(*idx_reg),
                                    size: 8,
                                    is_constant: false,
                                    constant_val: 0,
                                };
                                // Ghidra always emits INT_MULT even when scale==1
                                let scale_vn = Varnode::constant(*scale as i64, 8);
                                let out_tmp = self.emitter.tmp(2, 8);
                                self.emitter.emit_int_binop(
                                    fission_pcode::PcodeOpcode::IntMult,
                                    out_tmp.clone(),
                                    idx_vn,
                                    scale_vn,
                                    "INT_MULT",
                                )?;
                                tmp = Some(out_tmp);
                            }

                            // Step 2: base + scaled_index
                            let base_vn = if let Some(base_reg) = base {
                                Some(Varnode {
                                    space_id: 4,
                                    offset: reg_offset(*base_reg),
                                    size: 8,
                                    is_constant: false,
                                    constant_val: 0,
                                })
                            } else {
                                None
                            };

                            if let Some(b_vn) = base_vn {
                                if let Some(t_vn) = tmp {
                                    let out_tmp = self.emitter.tmp(2, 8);
                                    self.emitter.emit_int_binop(
                                        fission_pcode::PcodeOpcode::IntAdd,
                                        out_tmp.clone(),
                                        b_vn,
                                        t_vn,
                                        "INT_ADD",
                                    )?;
                                    tmp = Some(out_tmp);
                                } else {
                                    tmp = Some(b_vn);
                                }
                            }

                            // Step 3: + displacement (or RIP + displacement)
                            if *rip_relative {
                                // Ghidra folds RIP + displacement into a direct static RAM reference
                                let next_ip = self.address.saturating_add(state.length as u64);
                                let folded_addr = (next_ip as i64).wrapping_add(*displacement);
                                tmp = Some(Varnode {
                                    space_id: 3, // ram space
                                    offset: folded_addr as u64,
                                    size: 8,
                                    is_constant: false,
                                    constant_val: 0,
                                });
                            } else if *displacement != 0 || tmp.is_none() {
                                let disp_vn = Varnode::constant(*displacement, 8);
                                if let Some(t_vn) = tmp {
                                    let out_tmp = self.emitter.tmp(2, 8);
                                    self.emitter.emit_int_binop(
                                        fission_pcode::PcodeOpcode::IntAdd,
                                        out_tmp.clone(),
                                        t_vn,
                                        disp_vn,
                                        "INT_ADD",
                                    )?;
                                    tmp = Some(out_tmp);
                                } else {
                                    tmp = Some(disp_vn);
                                }
                            }

                            if let Some(ea_vn) = tmp {
                                self.resolved_memory_ea.insert(idx, ea_vn);
                            }
                        }
                    }
                }
                Ok(())
            }
            // CallOther: user-defined pcodeop. Ghidra emits this as a real
            // CALLOTHER P-code op. Input[0] is the pcodeop index.
            CompiledOpTplOpcode::CallOther => {
                let mut inputs = Vec::new();
                for input in &op.inputs {
                    let size = self.template_varnode_size(input, state).unwrap_or(8);
                    inputs.push(self.read_template_varnode(input, state, size)?);
                }
                let output = if let Some(ref out_ref) = op.output {
                    Some(self.materialize_write_varnode(out_ref, state, mnemonic)?)
                } else {
                    None
                };
                self.emitter.emit_callother(output, inputs, mnemonic)
            }
            CompiledOpTplOpcode::Unsupported => bail!(
                "compiled op template {} is not executable in compiled-table cutover",
                mnemonic
            ),
        }
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
                if let Some(size) = &handle_tpl.size {
                    let value = self.resolve_const_value(size, state)?;
                    u32::try_from(value).map_err(|_| anyhow!("HandleTpl size {value} exceeds u32"))
                } else {
                    Ok(0)
                }
            }
            _ => bail!("compiled-table executor rejects compatibility varnode template"),
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
                let varnode = self.resolve_varnode_tpl(template, state)?;
                if expected_size > 0 && varnode.size != expected_size {
                    bail!(
                        "VarnodeTpl size {} did not match expected size {expected_size}",
                        varnode.size
                    );
                }
                Ok(varnode)
            }
            CompiledVarnodeTpl::ConditionPredicate => {
                if let Some(cc) = state.condition_code {
                    let out = self.emit_condition_predicate(cc, "Jcc")?;
                    if expected_size > 0 && out.size != expected_size {
                        bail!("ConditionPredicate size mismatch");
                    }
                    Ok(out)
                } else {
                    bail!("ConditionPredicate used but condition_code is missing")
                }
            }
            CompiledVarnodeTpl::Handle { operand_index } => {
                let handle = state
                    .handles
                    .get(*operand_index)
                    .ok_or_else(|| anyhow!("missing operand index {}", operand_index))?;
                if matches!(
                    handle.value,
                    crate::runtime::spine::construct::BoundOperand::Memory { .. }
                ) {
                    bail!("Handle used for memory operand - expected EffectiveAddress");
                }
                let varnode = varnode_from_fixed_handle(&handle.fixed)?;
                if expected_size > 0
                    && varnode.size != expected_size
                    && !matches!(
                        handle.value,
                        crate::runtime::spine::construct::BoundOperand::Relative { .. }
                    )
                {
                    bail!("Handle size mismatch");
                }
                Ok(varnode)
            }
            _ => bail!(
                "compiled-table executor rejects compatibility varnode template: {:?}",
                template
            ),
        }
    }

    fn emit_condition_predicate(&mut self, cc: u8, mnemonic: &str) -> Result<Varnode> {
        let cf = Varnode {
            space_id: 4,
            offset: 0x0200,
            size: 1,
            is_constant: false,
            constant_val: 0,
        };
        let pf = Varnode {
            space_id: 4,
            offset: 0x0202,
            size: 1,
            is_constant: false,
            constant_val: 0,
        };
        let zf = Varnode {
            space_id: 4,
            offset: 0x0206,
            size: 1,
            is_constant: false,
            constant_val: 0,
        };
        let sf = Varnode {
            space_id: 4,
            offset: 0x0207,
            size: 1,
            is_constant: false,
            constant_val: 0,
        };
        let of = Varnode {
            space_id: 4,
            offset: 0x020B,
            size: 1,
            is_constant: false,
            constant_val: 0,
        };

        match cc {
            0 => Ok(of), // O
            1 => {
                // NO
                let out = self.emitter.tmp(2, 1);
                self.emitter.append_checked(
                    fission_pcode::PcodeOpcode::BoolNegate,
                    Some(out.clone()),
                    vec![of],
                    mnemonic,
                )?;
                Ok(out)
            }
            2 => Ok(cf), // B/C/NAE
            3 => {
                // NB/NC/AE
                let out = self.emitter.tmp(2, 1);
                self.emitter.append_checked(
                    fission_pcode::PcodeOpcode::BoolNegate,
                    Some(out.clone()),
                    vec![cf],
                    mnemonic,
                )?;
                Ok(out)
            }
            4 => Ok(zf), // Z/E
            5 => {
                // NZ/NE
                let out = self.emitter.tmp(2, 1);
                self.emitter.append_checked(
                    fission_pcode::PcodeOpcode::BoolNegate,
                    Some(out.clone()),
                    vec![zf],
                    mnemonic,
                )?;
                Ok(out)
            }
            6 => {
                // BE/NA (CF || ZF)
                let out = self.emitter.tmp(2, 1);
                self.emitter.append_checked(
                    fission_pcode::PcodeOpcode::BoolOr,
                    Some(out.clone()),
                    vec![cf, zf],
                    mnemonic,
                )?;
                Ok(out)
            }
            7 => {
                // NBE/A !(CF || ZF)
                let tmp = self.emitter.tmp(2, 1);
                self.emitter.append_checked(
                    fission_pcode::PcodeOpcode::BoolOr,
                    Some(tmp.clone()),
                    vec![cf, zf],
                    mnemonic,
                )?;
                let out = self.emitter.tmp(2, 1);
                self.emitter.append_checked(
                    fission_pcode::PcodeOpcode::BoolNegate,
                    Some(out.clone()),
                    vec![tmp],
                    mnemonic,
                )?;
                Ok(out)
            }
            8 => Ok(sf), // S
            9 => {
                // NS
                let out = self.emitter.tmp(2, 1);
                self.emitter.append_checked(
                    fission_pcode::PcodeOpcode::BoolNegate,
                    Some(out.clone()),
                    vec![sf],
                    mnemonic,
                )?;
                Ok(out)
            }
            10 => Ok(pf), // P/PE
            11 => {
                // NP/PO
                let out = self.emitter.tmp(2, 1);
                self.emitter.append_checked(
                    fission_pcode::PcodeOpcode::BoolNegate,
                    Some(out.clone()),
                    vec![pf],
                    mnemonic,
                )?;
                Ok(out)
            }
            12 => {
                // L/NGE (SF != OF)
                let out = self.emitter.tmp(2, 1);
                self.emitter.append_checked(
                    fission_pcode::PcodeOpcode::IntNotEqual,
                    Some(out.clone()),
                    vec![of.clone(), sf.clone()],
                    mnemonic,
                )?;
                Ok(out)
            }
            13 => {
                // NL/GE (SF == OF)
                let out = self.emitter.tmp(2, 1);
                self.emitter.append_checked(
                    fission_pcode::PcodeOpcode::IntEqual,
                    Some(out.clone()),
                    vec![of.clone(), sf.clone()],
                    mnemonic,
                )?;
                Ok(out)
            }
            14 => {
                // LE/NG (ZF || (SF != OF))
                let tmp = self.emitter.tmp(2, 1);
                self.emitter.append_checked(
                    fission_pcode::PcodeOpcode::IntNotEqual,
                    Some(tmp.clone()),
                    vec![of.clone(), sf.clone()],
                    mnemonic,
                )?;
                let out = self.emitter.tmp(2, 1);
                self.emitter.append_checked(
                    fission_pcode::PcodeOpcode::BoolOr,
                    Some(out.clone()),
                    vec![zf.clone(), tmp],
                    mnemonic,
                )?;
                Ok(out)
            }
            15 => {
                // NLE/G (!ZF && (SF == OF))
                let tmp = self.emitter.tmp(2, 1);
                self.emitter.append_checked(
                    fission_pcode::PcodeOpcode::IntEqual,
                    Some(tmp.clone()),
                    vec![of, sf],
                    mnemonic,
                )?;
                let zf_not = self.emitter.tmp(2, 1);
                self.emitter.append_checked(
                    fission_pcode::PcodeOpcode::BoolNegate,
                    Some(zf_not.clone()),
                    vec![zf.clone()],
                    mnemonic,
                )?;
                let out = self.emitter.tmp(2, 1);
                self.emitter.append_checked(
                    fission_pcode::PcodeOpcode::BoolAnd,
                    Some(out.clone()),
                    vec![zf_not, tmp],
                    mnemonic,
                )?;
                Ok(out)
            }
            _ => bail!("invalid condition code {}", cc),
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
            _ => bail!("compiled-table executor rejects compatibility varnode template"),
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
        if !matches!(handle.value, BoundOperand::Memory { .. }) {
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
        let ptr = self
            .resolved_memory_ea
            .get(&handle_index)
            .cloned()
            .unwrap_or(Varnode {
                space_id: offset_space.index,
                offset: handle.fixed.offset_offset,
                size: handle.fixed.offset_size,
                is_constant: false,
                constant_val: 0,
            });
        Ok(Some(DynamicMemoryTarget {
            space: Varnode::constant(space.index as i64, 4),
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
                if let Some(ea_vn) = self.resolved_memory_ea_varnode(space, offset) {
                    return Ok(ea_vn);
                }
                let space = self.resolve_space_tpl(space, state)?;
                if (space.index == 0 || space.name == "const")
                    && handle_selector_index(offset, CompiledHandleSelector::Offset).is_some()
                {
                    let handle_index = handle_selector_index(offset, CompiledHandleSelector::Offset)
                        .expect("checked above");
                    let handle = state
                        .handles
                        .get(handle_index)
                        .ok_or_else(|| anyhow!("handle {} is missing or unresolved", handle_index))?;
                    if let Some(offset_space) = &handle.fixed.offset_space {
                        let size = u32::try_from(self.resolve_const_value(size, state)?)
                            .map_err(|_| anyhow!("VarnodeTpl size exceeds u32"))?;
                        if offset_space.index == 0 || offset_space.name == "const" {
                            let value = i64::try_from(handle.fixed.offset_offset).map_err(|_| {
                                anyhow!(
                                    "constant handle offset {} exceeds i64",
                                    handle.fixed.offset_offset
                                )
                            })?;
                            return Ok(Varnode::constant(value, size));
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
                    let value = i64::try_from(offset)
                        .map_err(|_| anyhow!("constant VarnodeTpl offset {offset} exceeds i64"))?;
                    return Ok(Varnode::constant(value, size));
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
                    let ptr_offset = handle_tpl
                        .ptr_offset
                        .as_ref()
                        .map(|offset| self.resolve_const_value(offset, state))
                        .transpose()?
                        .unwrap_or(0);
                    let ptr_size = handle_tpl
                        .ptr_size
                        .as_ref()
                        .map(|size| self.resolve_const_value(size, state))
                        .transpose()?
                        .and_then(|size| u32::try_from(size).ok())
                        .unwrap_or(0);
                    if ptr_space.index == 0 || ptr_space.name == "const" {
                        let value = i64::try_from(ptr_offset).map_err(|_| {
                            anyhow!("constant HandleTpl ptr_offset {ptr_offset} exceeds i64")
                        })?;
                        return Ok(Varnode::constant(value, ptr_size));
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
                let size = if let Some(size) = &handle_tpl.size {
                    u32::try_from(self.resolve_const_value(size, state)?)
                        .map_err(|_| anyhow!("HandleTpl size exceeds u32"))?
                } else {
                    0
                };
                if space.index == 0 || space.name == "const" {
                    let value = i64::try_from(offset)
                        .map_err(|_| anyhow!("constant HandleTpl offset {offset} exceeds i64"))?;
                    return Ok(Varnode::constant(value, size));
                }
                Ok(Varnode {
                    space_id: space.index,
                    offset,
                    size,
                    is_constant: false,
                    constant_val: 0,
                })
            }
            _ => bail!("expected Ghidra VarnodeTpl or HandleTpl"),
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

    fn resolved_memory_ea_varnode(
        &self,
        space: &CompiledSpaceTpl,
        offset: &CompiledConstTpl,
    ) -> Option<Varnode> {
        let space_handle = handle_selector_index_in_space(space, CompiledHandleSelector::Space)?;
        if !matches_handle_selector(offset, space_handle, CompiledHandleSelector::Offset) {
            return None;
        }
        self.resolved_memory_ea.get(&space_handle).cloned()
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
                let name = match space_id {
                    0 => "const",
                    2 => "unique",
                    3 => "ram",
                    4 => "register",
                    _ => "unknown",
                };
                Ok(CompiledSpaceRef {
                    name: name.to_string(),
                    index: space_id,
                })
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
            CompiledConstTpl::Integer { value, .. } if *value >= 0 => Ok(*value as u64),
            CompiledConstTpl::Integer { value, .. } => {
                Ok((*value as i128 as u128 & u64::MAX as u128) as u64)
            }
            CompiledConstTpl::InstStart => Ok(self.address),
            CompiledConstTpl::InstNext => Ok(self.address.saturating_add(state.length as u64)),
            CompiledConstTpl::SpaceId(space) => Ok(space.index),
            CompiledConstTpl::Handle {
                handle_index,
                selector,
                plus,
            } => {
                let handle = state
                    .handles
                    .get(*handle_index as usize)
                    .ok_or_else(|| anyhow!("handle {} is missing or unresolved", handle_index))?;

                let val = self.resolve_fixed_handle_selector(handle, *selector)?;
                if let Some(plus) = plus {
                    Ok(val.wrapping_add(*plus))
                } else {
                    Ok(val)
                }
            }
            CompiledConstTpl::Relative { .. } | CompiledConstTpl::RelativeAddress => {
                bail!("ConstTpl relative label resolution is unsupported")
            }
            CompiledConstTpl::InstNext2 => bail!("ConstTpl inst_next2 resolution is unsupported"),
            CompiledConstTpl::CurSpace => bail!("ConstTpl curspace resolution is unsupported"),
            CompiledConstTpl::CurSpaceSize => {
                bail!("ConstTpl curspace_size resolution is unsupported")
            }
            CompiledConstTpl::FlowRef => bail!("ConstTpl flowref resolution is unsupported"),
            CompiledConstTpl::FlowRefSize => {
                bail!("ConstTpl flowref_size resolution is unsupported")
            }
            CompiledConstTpl::FlowDest => bail!("ConstTpl flowdest resolution is unsupported"),
            CompiledConstTpl::FlowDestSize => {
                bail!("ConstTpl flowdest_size resolution is unsupported")
            }
        }
    }

    fn resolve_fixed_handle_selector(
        &self,
        handle: &RuntimeHandle,
        selector: CompiledHandleSelector,
    ) -> Result<u64> {
        match selector {
            CompiledHandleSelector::Space => handle
                .fixed
                .space
                .as_ref()
                .map(|space| space.index)
                .ok_or_else(|| anyhow!("fixed handle missing space")),
            CompiledHandleSelector::Offset => {
                if handle.fixed.offset_space.is_some() {
                    if let Some(ea_vn) = self.resolved_memory_ea.get(&handle.operand_index) {
                        Ok(ea_vn.offset)
                    } else {
                        Ok(handle.fixed.offset_offset)
                    }
                } else {
                    Ok(handle.fixed.offset_offset)
                }
            }
            CompiledHandleSelector::Size => Ok(handle.fixed.size as u64),
            CompiledHandleSelector::OffsetPlus => bail!("OffsetPlus unsupported"),
        }
    }

    fn unary_pcode_opcode(&self, opcode: CompiledOpTplOpcode) -> Result<PcodeOpcode> {
        Ok(match opcode {
            CompiledOpTplOpcode::IntZExt => PcodeOpcode::IntZExt,
            CompiledOpTplOpcode::IntSExt => PcodeOpcode::IntSExt,
            CompiledOpTplOpcode::BoolNegate => PcodeOpcode::BoolNegate,
            CompiledOpTplOpcode::PopCount => PcodeOpcode::PopCount,
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
            CompiledOpTplOpcode::IntLeft => PcodeOpcode::IntLeft,
            CompiledOpTplOpcode::IntRight => PcodeOpcode::IntRight,
            CompiledOpTplOpcode::IntSRight => PcodeOpcode::IntSRight,
            CompiledOpTplOpcode::IntEqual => PcodeOpcode::IntEqual,
            CompiledOpTplOpcode::IntNotEqual => PcodeOpcode::IntNotEqual,
            CompiledOpTplOpcode::IntLess => PcodeOpcode::IntLess,
            CompiledOpTplOpcode::IntSLess => PcodeOpcode::IntSLess,
            CompiledOpTplOpcode::BoolAnd => PcodeOpcode::BoolAnd,
            CompiledOpTplOpcode::BoolOr => PcodeOpcode::BoolOr,
            CompiledOpTplOpcode::Piece => PcodeOpcode::Piece,
            CompiledOpTplOpcode::Subpiece => PcodeOpcode::SubPiece,
            _ => bail!("unsupported binary compiled opcode {}", opcode.as_str()),
        })
    }
}

impl RuntimeTemplateExecutor for CompiledTableEmitter {
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
        compile_frontend_for_entry_spec, compile_x86_64_frontend, CompiledTemplateSource,
    };
    use std::path::PathBuf;

    fn assert_spec_derived_lift_or_typed_unsupported(
        compiled: &CompiledFrontend,
        bytes: &[u8],
        address: u64,
    ) {
        match decode_and_lift_with_details(compiled, None, bytes, address) {
            Ok((ops, length, details)) => {
                assert_eq!(length as usize, bytes.len());
                assert!(
                    !details.compat_emitter_used,
                    "raw p-code path must not use compatibility emitter"
                );
                assert!(
                    details.template_source == Some(CompiledTemplateSource::SpecDerived)
                        || details.template_source == Some(CompiledTemplateSource::NativeFission),
                    "expected SpecDerived or NativeFission, got {:?}",
                    details.template_source
                );
                assert!(!ops.is_empty(), "spec-derived template emitted no p-code");
            }
            Err(err) => {
                let rendered = err.to_string();
                assert!(
                    rendered.contains("UnsupportedPcodeTemplate"),
                    "unsupported raw p-code must be typed: {rendered}"
                );
                assert!(
                    !rendered.contains("compatibility_lowered_template_not_canonical"),
                    "x86-64 generated rows should now resolve to .sla templates: {rendered}"
                );
            }
        }
    }

    #[test]
    fn generated_runtime_decodes_ret_with_spec_derived_lift() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let decoded = decode_instruction(&compiled, None, &[0xC3], 0x1000).expect("generated ret");
        assert_eq!(decoded.length, 1);
        assert!(matches!(decoded.flow_kind, DecodedFlowKind::Return));
        assert_spec_derived_lift_or_typed_unsupported(&compiled, &[0xC3], 0x1000);
    }

    #[test]
    fn generated_runtime_decodes_mov_imm64_without_compatibility_lift() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let bytes = [0x48, 0xB8, 0x34, 0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let decoded = decode_instruction(&compiled, None, &bytes, 0x1000).expect("generated mov");
        assert_eq!(decoded.length, bytes.len());
        assert_eq!(decoded.mnemonic, "mov");
        assert_spec_derived_lift_or_typed_unsupported(&compiled, &bytes, 0x1000);
    }

    #[test]
    fn generated_runtime_decodes_jcc_rel8_without_compatibility_lift() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let decoded = decode_instruction(&compiled, None, &[0x75, 0x05], 0x1000).expect("generated jne");
        assert_eq!(decoded.length, 2);
        assert_eq!(decoded.mnemonic, "jnz");
        assert!(matches!(
            decoded.flow_kind,
            DecodedFlowKind::ConditionalJump
        ));
        assert_spec_derived_lift_or_typed_unsupported(&compiled, &[0x75, 0x05], 0x1000);
    }

    #[test]
    fn generated_runtime_renders_jle_condition_mnemonic_display_only() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let decoded = decode_instruction(&compiled, None, &[0x7e, 0x05], 0x1000).expect("generated jle");
        assert_eq!(decoded.length, 2);
        assert_eq!(decoded.mnemonic, "jle");
        assert!(matches!(
            decoded.flow_kind,
            DecodedFlowKind::ConditionalJump
        ));
        assert_spec_derived_lift_or_typed_unsupported(&compiled, &[0x7e, 0x05], 0x1000);
    }

    #[test]
    fn generated_runtime_decodes_startup_store_mov_mem32_imm32_without_compatibility_lift() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let bytes = [0xC7, 0x00, 0x01, 0x00, 0x00, 0x00];
        let decoded =
            decode_instruction(&compiled, None, &bytes, 0x1000).expect("generated mov [rax], imm32");
        assert_eq!(decoded.length, bytes.len());
        assert_eq!(decoded.mnemonic, "mov");
        let (ops, length, details) =
            decode_and_lift_with_details(&compiled, None, &bytes, 0x1000).expect("lift mov [rax], imm32");
        assert_eq!(length as usize, bytes.len());
        assert_eq!(
            details.template_source,
            Some(CompiledTemplateSource::SpecDerived)
        );
        assert!(!details.compat_emitter_used);
        assert_eq!(ops.len(), 2);
        assert_eq!(ops[0].opcode, PcodeOpcode::Copy);
        assert_eq!(ops[1].opcode, PcodeOpcode::Store);
        assert_eq!(ops[1].inputs[1].space_id, 4);
        assert_eq!(ops[1].inputs[1].offset, 0);
        assert_eq!(ops[1].inputs[1].size, 8);
    }

    #[test]
    fn generated_runtime_decodes_startup_sub_rsp_imm8_without_compatibility_lift() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let bytes = [0x48, 0x83, 0xEC, 0x28];
        let decoded =
            decode_instruction(&compiled, None, &bytes, 0x1000).expect("generated sub rsp, imm8");
        assert_eq!(decoded.length, bytes.len());
        assert_eq!(decoded.mnemonic, "sub");
        assert_spec_derived_lift_or_typed_unsupported(&compiled, &bytes, 0x1000);
    }

    #[test]
    fn generated_runtime_decodes_startup_rip_relative_load_without_compatibility_lift() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let bytes = [0x48, 0x8B, 0x05, 0x15, 0x30, 0x00, 0x00];
        let address = 0x1400_013e4;
        let decoded =
            decode_instruction(&compiled, None, &bytes, address).expect("generated rip-relative mov");
        assert_eq!(decoded.length, bytes.len());
        assert_eq!(decoded.mnemonic, "mov");
        assert_spec_derived_lift_or_typed_unsupported(&compiled, &bytes, address);
    }

    #[test]
    fn generated_runtime_decodes_startup_call_rel32_without_compatibility_lift() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let bytes = [0xE8, 0x1A, 0xFC, 0xFF, 0xFF];
        let decoded =
            decode_instruction(&compiled, None, &bytes, 0x1400_013ef).expect("generated call rel32");
        assert_eq!(decoded.length, bytes.len());
        assert!(matches!(decoded.flow_kind, DecodedFlowKind::Call));
        assert_spec_derived_lift_or_typed_unsupported(&compiled, &bytes, 0x1400_013ef);
    }

    #[test]
    fn generated_runtime_records_decision_trace_for_startup_store() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let ctx = CompiledInstructionContext::parse(&[0xC7, 0x00, 0x01, 0x00, 0x00, 0x00], 0x1000)
            .expect("decode context");
        let selection = select_constructor(&compiled, "instruction", &ctx).expect("constructor selection");
        let state = bind_instruction(&compiled, None, &ctx, selection).expect("bind instruction");
        assert_eq!(state.match_trace.root_bucket, "global");
        assert!(!state.match_trace.probes.is_empty());
        assert!(!state.construct_nodes.is_empty());
        assert!(state
            .handles
            .iter()
            .any(|handle| matches!(handle.spec, CompiledOperandSpec::TokenFieldExtraction { .. })));
    }

    #[test]
    fn generated_runtime_decodes_reg32_lea_without_decode_no_match_or_compatibility_lift() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let bytes = [0x8d, 0x04, 0x11];
        let decoded = decode_instruction(&compiled, None, &bytes, 0x1400_1450).expect("generated lea");
        assert_eq!(decoded.length, bytes.len());
        assert_eq!(decoded.mnemonic, "lea");
        assert_spec_derived_lift_or_typed_unsupported(&compiled, &bytes, 0x1400_1450);
    }

    #[test]
    fn generated_runtime_decodes_rip_relative_mov32_without_decode_no_match() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let bytes = [0x8b, 0x05, 0x6a, 0x56, 0x00, 0x00];
        let decoded =
            decode_instruction(&compiled, None, &bytes, 0x1400_19c0).expect("generated mov rip-relative");
        assert_eq!(decoded.length, bytes.len());
        assert_eq!(decoded.mnemonic, "mov");
        assert!(matches!(
            decoded.references.first().map(|reference| reference.kind),
            Some(DecodedReferenceKind::RipRelativeAddress)
        ));
    }

    #[test]
    fn generated_runtime_decodes_movsxd_without_decode_no_match_or_compatibility_lift() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let bytes = [0x48, 0x63, 0x41, 0x3c];
        let decoded = decode_instruction(&compiled, None, &bytes, 0x1400_2600).expect("generated movsxd");
        assert_eq!(decoded.length, bytes.len());
        assert_eq!(decoded.mnemonic, "movsxd");
        assert_spec_derived_lift_or_typed_unsupported(&compiled, &bytes, 0x1400_2600);
    }

    #[test]
    fn generated_runtime_zero_extends_reg32_decode_without_compatibility_lift() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let bytes = [0x31, 0xc0];
        let decoded =
            decode_instruction(&compiled, None, &bytes, 0x1400_19e0).expect("generated xor eax, eax");
        assert_eq!(decoded.length, bytes.len());
        assert_eq!(decoded.mnemonic, "xor");
        assert_spec_derived_lift_or_typed_unsupported(&compiled, &bytes, 0x1400_19e0);
    }

    #[test]
    fn generated_runtime_decodes_fninit_without_decode_no_match() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let bytes = [0xdb, 0xe3];
        let decoded =
            decode_instruction(&compiled, None, &bytes, 0x1400_25c0).expect("generated fninit decode");
        assert_eq!(decoded.length, bytes.len());
        assert_eq!(decoded.mnemonic, "fninit");
        assert!(matches!(decoded.flow_kind, DecodedFlowKind::None));
    }

    #[test]
    fn generated_runtime_lifts_fninit_without_compatibility_emitter() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let bytes = [0xdb, 0xe3];
        assert_spec_derived_lift_or_typed_unsupported(&compiled, &bytes, 0x1400_25c0);
    }

    #[test]
    fn generated_runtime_rejects_or_lifts_cmp_templates_without_compatibility() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let bytes = [0x83, 0xf9, 0x01];
        assert_spec_derived_lift_or_typed_unsupported(&compiled, &bytes, 0x1400_1485);
    }

    #[test]
    fn generated_runtime_rejects_or_lifts_push_templates_without_compatibility() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let bytes = [0x41, 0x57];
        assert_spec_derived_lift_or_typed_unsupported(&compiled, &bytes, 0x1400_1470);
    }

    #[test]
    fn generated_runtime_rejects_or_lifts_lea_templates_without_compatibility() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let bytes = [0x8d, 0x04, 0x11];
        assert_spec_derived_lift_or_typed_unsupported(&compiled, &bytes, 0x1400_1450);
    }

    #[test]
    fn packed_context_word_write_matches_ghidra_bit_numbering() {
        let mut context = 0u64;
        set_packed_context_word(&mut context, 0, 1u32 << 31, 1u32 << 31)
            .expect("set context word");
        assert_eq!(packed_context_bits(context, 0, 1).expect("bit 0"), 1);
        assert_eq!(packed_context_bits(context, 31, 1).expect("bit 31"), 0);
    }

    #[test]
    fn packed_context_bit_write_matches_ghidra_bit_numbering() {
        let mut context = 0u64;
        set_packed_context_bits(&mut context, 0, 1, 1).expect("set bit 0");
        assert_eq!(packed_context_bits(context, 0, 1).expect("bit 0"), 1);
        assert_eq!(packed_context_bits(context, 31, 1).expect("bit 31"), 0);

        set_packed_context_bits(&mut context, 31, 2, 0b11).expect("set cross-word bits");
        assert_eq!(packed_context_bits(context, 31, 2).expect("cross-word bits"), 0b11);
    }

    #[test]
    fn packed_context_bit_reads_cross_word_boundaries_like_ghidra() {
        let mut context = 0u64;
        set_packed_context_word(&mut context, 0, 0x0000_0001, 0x0000_0001)
            .expect("set low word");
        set_packed_context_word(&mut context, 1, 0x8000_0000, 0x8000_0000)
            .expect("set high word");
        assert_eq!(packed_context_bits(context, 31, 2).expect("cross-word bits"), 0b11);
    }

    #[test]
    fn generated_runtime_decodes_aarch64_smoke_without_constructor_loop() {
        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("repo root")
            .to_path_buf();
        let aarch64_spec =
            repo_root.join("crates/fission-sleigh/specs/languages/AARCH64/AARCH64.slaspec");
        let compiled = compile_frontend_for_entry_spec(&aarch64_spec).expect("compile aarch64");
        let bytes = [0x0c, 0x10, 0x8e, 0xd2];
        let decoded = decode_instruction(&compiled, None, &bytes, 0x100000).expect("decode aarch64");
        assert_eq!(decoded.length, bytes.len());
        assert!(!decoded.mnemonic.is_empty(), "expected resolved aarch64 mnemonic");
        assert_ne!(decoded.mnemonic, "udf", "expected terminal verification to avoid udf fallback");
    }
}
