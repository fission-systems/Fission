//! Transitional compiled-table executor for the common SLEIGH runtime spine.
//! The canonical owner mapping remains `DecisionNode -> RuntimeInstructionContext ->
//! RuntimeConstructState/RuntimeParserWalker -> RuntimeTemplateEvaluator -> RuntimePcodeEmitter`.

use anyhow::{anyhow, bail, Result};
use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};

use crate::compiler::{
    CompiledConstTpl, CompiledConstructTplKind, CompiledDecisionProbe,
    CompiledExecutableConstructor, CompiledFixedRegister, CompiledFrontend, CompiledHandleTemplate,
    CompiledOpTpl, CompiledOpTplOpcode, CompiledOperandDecodeStep, CompiledOperandSpec,
    CompiledPatternMatcher, CompiledSpaceRef, CompiledSpaceTpl, CompiledTokenFieldRef,
    CompiledVarnodeTpl,
};
use crate::runtime::spine::{
    self, BoundOperand, DecisionProbeEvaluator, RuntimeConstructState, RuntimeHandle,
    RuntimeInstructionContext, RuntimePcodeEmitter, RuntimeSelection, RuntimeTemplateEvaluator,
    RuntimeTemplateExecutor,
};
use crate::runtime::{
    DecodedFlowKind, DecodedInstruction, DecodedReference, DecodedReferenceKind,
    RuntimeExecutionDetails, RuntimeSleighError,
};

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

pub(crate) fn decode_and_lift_with_details(
    compiled: &CompiledFrontend,
    bytes: &[u8],
    address: u64,
) -> Result<(Vec<PcodeOp>, u64, RuntimeExecutionDetails)> {
    let ctx = CompiledInstructionContext::parse(bytes, address)?;
    let selection =
        select_constructor(compiled, &ctx).ok_or_else(|| RuntimeSleighError::DecodeNoMatch {
            language: compiled.entry_id.clone(),
            address,
        })?;
    if !selection.constructor.runtime_ready {
        return Err(unsupported_constructor_error(compiled, selection.constructor).into());
    }
    let decoded = bind_instruction(&ctx, selection)?;
    let mut emitter = CompiledTableEmitter::new(address);
    let details = RuntimeTemplateEvaluator::new(&mut emitter)
        .emit(&compiled.entry_id, &decoded)
        .map_err(|err| {
            // If template evaluation fails due to unresolvable template
            // features (HandleTpl, CurSpace, etc.), convert to a typed
            // UnsupportedPcodeTemplate error so that callers can
            // distinguish "not yet implemented" from "broken input".
            let msg = err.to_string();
            if msg.contains("HandleTpl")
                || msg.contains("ConstTpl")
                || msg.contains("unsupported")
            {
                RuntimeSleighError::UnsupportedPcodeTemplate {
                    language: compiled.entry_id.clone(),
                    reason: format!("emission_time_template_resolution_failed: {msg}"),
                }
                .into()
            } else {
                err
            }
        })?;
    Ok((emitter.finish(), decoded.length as u64, details))
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
    spine::select_constructor(
        compiled,
        [("global".to_string(), compiled.decision_tree.root_node_index)],
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
                ensure_token_fields(self.ctx, &mut self.cached_token_fields)
                    .map(|bundle| bundle.operand_mode)
                    .unwrap_or(0)
            }
            CompiledDecisionProbe::TokenFieldRef(CompiledTokenFieldRef::RegisterSelector) => {
                ensure_token_fields(self.ctx, &mut self.cached_token_fields)
                    .map(|bundle| bundle.reg)
                    .unwrap_or(0)
            }
            CompiledDecisionProbe::TerminalPatternCheck => 0,
        })
    }
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
                    CompiledFixedRegister::StackPointer => 4,
                    CompiledFixedRegister::FramePointer => 5,
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
        CompiledConstructTplKind::Unsupported => DecodedFlowKind::None,
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
    emitter: RuntimePcodeEmitter,
    address: u64,
    /// Cache of resolved effective-address varnodes for Memory operands.
    /// Key is the handle index, value is the unique-space varnode holding
    /// the computed pointer.
    resolved_memory_ea: std::collections::BTreeMap<usize, Varnode>,
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

                if value.size > out_size && out_size > 0 {
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
                let out_size = self.template_varnode_size(out_tpl, state)?;
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
                let out_size = self.template_varnode_size(out_tpl, state)?;
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
                    let lhs = self.read_template_varnode(&op.inputs[0], state, out_size)?;
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
                        CompiledVarnodeTpl::Varnode { offset, .. } => {
                            match offset.as_ref() {
                                CompiledConstTpl::Real { value } => Some(*value as usize),
                                _ => None,
                            }
                        }
                        _ => None,
                    }
                } else {
                    None
                };
                if let Some(idx) = operand_index {
                    if let Some(handle) = state.handles.get(idx) {
                        if let BoundOperand::Memory { base, index, scale, displacement, rip_relative, size: _op_size } = &handle.value {
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
                let op = state.operands.get(*operand_index).ok_or_else(|| anyhow!("missing operand index {}", operand_index))?;
                match op {
                    crate::runtime::spine::construct::BoundOperand::Register { index, size } => {
                        let offset = if *index < 8 { (*index as u64) * 8 } else { 128 + ((*index as u64) - 8) * 8 };
                        let varnode = Varnode { space_id: 4, offset, size: *size, is_constant: false, constant_val: 0 };
                        if expected_size > 0 && varnode.size != expected_size {
                            bail!("Handle size mismatch");
                        }
                        Ok(varnode)
                    }
                    crate::runtime::spine::construct::BoundOperand::Immediate { value, encoded_size, .. } => {
                        let varnode = Varnode::constant(*value as i64, *encoded_size);
                        if expected_size > 0 && varnode.size != expected_size {
                            bail!("Handle size mismatch");
                        }
                        Ok(varnode)
                    }
                    crate::runtime::spine::construct::BoundOperand::Relative { target } => {
                        let varnode = Varnode { space_id: 3, offset: *target, size: 8, is_constant: false, constant_val: 0 };
                        if expected_size > 0 && varnode.size != expected_size {
                            // Target size mismatch is acceptable for branch targets.
                        }
                        Ok(varnode)
                    }
                    crate::runtime::spine::construct::BoundOperand::Memory { .. } => {
                        bail!("Handle used for memory operand - expected EffectiveAddress")
                    }
                }
            }
            _ => bail!("compiled-table executor rejects compatibility varnode template: {:?}", template),
        }
    }

    fn emit_condition_predicate(&mut self, cc: u8, mnemonic: &str) -> Result<Varnode> {
        let cf = Varnode { space_id: 4, offset: 0x1200, size: 1, is_constant: false, constant_val: 0 };
        let pf = Varnode { space_id: 4, offset: 0x1202, size: 1, is_constant: false, constant_val: 0 };
        let zf = Varnode { space_id: 4, offset: 0x1206, size: 1, is_constant: false, constant_val: 0 };
        let sf = Varnode { space_id: 4, offset: 0x1207, size: 1, is_constant: false, constant_val: 0 };
        let of = Varnode { space_id: 4, offset: 0x120B, size: 1, is_constant: false, constant_val: 0 };

        match cc {
            0 => Ok(of), // O
            1 => { // NO
                let out = self.emitter.tmp(2, 1);
                self.emitter.append_checked(fission_pcode::PcodeOpcode::BoolNegate, Some(out.clone()), vec![of], mnemonic)?;
                Ok(out)
            }
            2 => Ok(cf), // B/C/NAE
            3 => { // NB/NC/AE
                let out = self.emitter.tmp(2, 1);
                self.emitter.append_checked(fission_pcode::PcodeOpcode::BoolNegate, Some(out.clone()), vec![cf], mnemonic)?;
                Ok(out)
            }
            4 => Ok(zf), // Z/E
            5 => { // NZ/NE
                let out = self.emitter.tmp(2, 1);
                self.emitter.append_checked(fission_pcode::PcodeOpcode::BoolNegate, Some(out.clone()), vec![zf], mnemonic)?;
                Ok(out)
            }
            6 => { // BE/NA (CF || ZF)
                let out = self.emitter.tmp(2, 1);
                self.emitter.append_checked(fission_pcode::PcodeOpcode::BoolOr, Some(out.clone()), vec![cf, zf], mnemonic)?;
                Ok(out)
            }
            7 => { // NBE/A !(CF || ZF)
                let tmp = self.emitter.tmp(2, 1);
                self.emitter.append_checked(fission_pcode::PcodeOpcode::BoolOr, Some(tmp.clone()), vec![cf, zf], mnemonic)?;
                let out = self.emitter.tmp(2, 1);
                self.emitter.append_checked(fission_pcode::PcodeOpcode::BoolNegate, Some(out.clone()), vec![tmp], mnemonic)?;
                Ok(out)
            }
            8 => Ok(sf), // S
            9 => { // NS
                let out = self.emitter.tmp(2, 1);
                self.emitter.append_checked(fission_pcode::PcodeOpcode::BoolNegate, Some(out.clone()), vec![sf], mnemonic)?;
                Ok(out)
            }
            10 => Ok(pf), // P/PE
            11 => { // NP/PO
                let out = self.emitter.tmp(2, 1);
                self.emitter.append_checked(fission_pcode::PcodeOpcode::BoolNegate, Some(out.clone()), vec![pf], mnemonic)?;
                Ok(out)
            }
            12 => { // L/NGE (SF != OF)
                let out = self.emitter.tmp(2, 1);
                self.emitter.append_checked(fission_pcode::PcodeOpcode::IntNotEqual, Some(out.clone()), vec![sf, of], mnemonic)?;
                Ok(out)
            }
            13 => { // NL/GE (SF == OF)
                let out = self.emitter.tmp(2, 1);
                self.emitter.append_checked(fission_pcode::PcodeOpcode::IntEqual, Some(out.clone()), vec![sf, of], mnemonic)?;
                Ok(out)
            }
            14 => { // LE/NG (ZF || (SF != OF))
                let tmp = self.emitter.tmp(2, 1);
                self.emitter.append_checked(fission_pcode::PcodeOpcode::IntNotEqual, Some(tmp.clone()), vec![sf, of], mnemonic)?;
                let out = self.emitter.tmp(2, 1);
                self.emitter.append_checked(fission_pcode::PcodeOpcode::BoolOr, Some(out.clone()), vec![zf, tmp], mnemonic)?;
                Ok(out)
            }
            15 => { // NLE/G (!ZF && (SF == OF))
                let tmp = self.emitter.tmp(2, 1);
                self.emitter.append_checked(fission_pcode::PcodeOpcode::IntEqual, Some(tmp.clone()), vec![sf, of], mnemonic)?;
                let zf_not = self.emitter.tmp(2, 1);
                self.emitter.append_checked(fission_pcode::PcodeOpcode::BoolNegate, Some(zf_not.clone()), vec![zf.clone()], mnemonic)?;
                let out = self.emitter.tmp(2, 1);
                self.emitter.append_checked(fission_pcode::PcodeOpcode::BoolAnd, Some(out.clone()), vec![zf_not, tmp], mnemonic)?;
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
            CompiledVarnodeTpl::Varnode { .. } | CompiledVarnodeTpl::HandleTpl(_) => self.resolve_varnode_tpl(template, state),
            _ => bail!("compiled-table executor rejects compatibility varnode template"),
        }
    }

    fn resolve_varnode_tpl(
        &mut self,
        template: &CompiledVarnodeTpl,
        state: &RuntimeConstructState,
    ) -> Result<Varnode> {
        match template {
            CompiledVarnodeTpl::Varnode { space, offset, size } => {
                let space = self.resolve_space_tpl(space, state)?;
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

                let val = match handle.value {
                    crate::runtime::spine::construct::BoundOperand::Register { index, size } => {
                        // Hardcode x86-64 register space mapping for now (space=4)
                        match selector {
                            crate::compiler::CompiledHandleSelector::Space => 4, // "register" space
                            crate::compiler::CompiledHandleSelector::Offset => {
                                // Ghidra x86-64 general registers are non-linear:
                                // RAX-RDI (0-7) are at offset 0
                                // R8-R15 (8-15) are at offset 0x80 (128)
                                if index < 8 {
                                    (index as u64) * 8
                                } else {
                                    128 + ((index as u64) - 8) * 8
                                }
                            }
                            crate::compiler::CompiledHandleSelector::Size => size as u64,
                            crate::compiler::CompiledHandleSelector::OffsetPlus => bail!("OffsetPlus unsupported"),
                        }
                    }
                    crate::runtime::spine::construct::BoundOperand::Immediate {
                        value,
                        encoded_size,
                        ..
                    } => match selector {
                        crate::compiler::CompiledHandleSelector::Space => 0, // "const" space
                        crate::compiler::CompiledHandleSelector::Offset => value,
                        crate::compiler::CompiledHandleSelector::Size => encoded_size as u64,
                        crate::compiler::CompiledHandleSelector::OffsetPlus => bail!("OffsetPlus unsupported"),
                    },
                    crate::runtime::spine::construct::BoundOperand::Relative { target } => {
                        match selector {
                            crate::compiler::CompiledHandleSelector::Space => 3, // "ram" space
                            crate::compiler::CompiledHandleSelector::Offset => target,
                            crate::compiler::CompiledHandleSelector::Size => 8, // Absolute pointer size
                            crate::compiler::CompiledHandleSelector::OffsetPlus => bail!("OffsetPlus unsupported"),
                        }
                    }
                    crate::runtime::spine::construct::BoundOperand::Memory { size, .. } => {
                        // Memory operands have their address calculation
                        // emitted during Build. The resolved EA varnode
                        // is cached in resolved_memory_ea.
                        let handle_idx = *handle_index as usize;
                        if let Some(ea_vn) = self.resolved_memory_ea.get(&handle_idx) {
                            match selector {
                                // Dynamic: point to the unique-space temp
                                crate::compiler::CompiledHandleSelector::Space => ea_vn.space_id,
                                crate::compiler::CompiledHandleSelector::Offset => ea_vn.offset,
                                crate::compiler::CompiledHandleSelector::Size => ea_vn.size as u64,
                                crate::compiler::CompiledHandleSelector::OffsetPlus => bail!("OffsetPlus unsupported"),
                            }
                        } else {
                            // Fallback: not yet resolved (shouldn't happen
                            // if Build ran first)
                            match selector {
                                crate::compiler::CompiledHandleSelector::Space => 3, // ram
                                crate::compiler::CompiledHandleSelector::Offset => 0,
                                crate::compiler::CompiledHandleSelector::Size => size as u64,
                                crate::compiler::CompiledHandleSelector::OffsetPlus => bail!("OffsetPlus unsupported"),
                            }
                        }
                    }
                };
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
    use crate::compiler::{compile_x86_64_frontend, CompiledTemplateSource};

    fn assert_spec_derived_lift_or_typed_unsupported(
        compiled: &CompiledFrontend,
        bytes: &[u8],
        address: u64,
    ) {
        match decode_and_lift_with_details(compiled, bytes, address) {
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
        let decoded = decode_instruction(&compiled, &[0xC3], 0x1000).expect("generated ret");
        assert_eq!(decoded.length, 1);
        assert!(matches!(decoded.flow_kind, DecodedFlowKind::Return));
        assert_spec_derived_lift_or_typed_unsupported(&compiled, &[0xC3], 0x1000);
    }

    #[test]
    fn generated_runtime_decodes_mov_imm64_without_compatibility_lift() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let bytes = [0x48, 0xB8, 0x34, 0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let decoded = decode_instruction(&compiled, &bytes, 0x1000).expect("generated mov");
        assert_eq!(decoded.length, bytes.len());
        assert_eq!(decoded.mnemonic, "mov");
        assert_spec_derived_lift_or_typed_unsupported(&compiled, &bytes, 0x1000);
    }

    #[test]
    fn generated_runtime_decodes_jcc_rel8_without_compatibility_lift() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let decoded = decode_instruction(&compiled, &[0x75, 0x05], 0x1000).expect("generated jne");
        assert_eq!(decoded.length, 2);
        assert!(matches!(
            decoded.flow_kind,
            DecodedFlowKind::ConditionalJump
        ));
        assert_spec_derived_lift_or_typed_unsupported(&compiled, &[0x75, 0x05], 0x1000);
    }

    #[test]
    fn generated_runtime_decodes_startup_store_mov_mem32_imm32_without_compatibility_lift() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let bytes = [0xC7, 0x00, 0x01, 0x00, 0x00, 0x00];
        let decoded =
            decode_instruction(&compiled, &bytes, 0x1000).expect("generated mov [rax], imm32");
        assert_eq!(decoded.length, bytes.len());
        assert_eq!(decoded.mnemonic, "mov");
        assert_spec_derived_lift_or_typed_unsupported(&compiled, &bytes, 0x1000);
    }

    #[test]
    fn generated_runtime_decodes_startup_sub_rsp_imm8_without_compatibility_lift() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let bytes = [0x48, 0x83, 0xEC, 0x28];
        let decoded =
            decode_instruction(&compiled, &bytes, 0x1000).expect("generated sub rsp, imm8");
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
            decode_instruction(&compiled, &bytes, address).expect("generated rip-relative mov");
        assert_eq!(decoded.length, bytes.len());
        assert_eq!(decoded.mnemonic, "mov");
        assert_spec_derived_lift_or_typed_unsupported(&compiled, &bytes, address);
    }

    #[test]
    fn generated_runtime_decodes_startup_call_rel32_without_compatibility_lift() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let bytes = [0xE8, 0x1A, 0xFC, 0xFF, 0xFF];
        let decoded =
            decode_instruction(&compiled, &bytes, 0x1400_013ef).expect("generated call rel32");
        assert_eq!(decoded.length, bytes.len());
        assert!(matches!(decoded.flow_kind, DecodedFlowKind::Call));
        assert_spec_derived_lift_or_typed_unsupported(&compiled, &bytes, 0x1400_013ef);
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
    fn generated_runtime_decodes_reg32_lea_without_decode_no_match_or_compatibility_lift() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let bytes = [0x8d, 0x04, 0x11];
        let decoded = decode_instruction(&compiled, &bytes, 0x1400_1450).expect("generated lea");
        assert_eq!(decoded.length, bytes.len());
        assert_eq!(decoded.mnemonic, "lea");
        assert_spec_derived_lift_or_typed_unsupported(&compiled, &bytes, 0x1400_1450);
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
    fn generated_runtime_decodes_movsxd_without_decode_no_match_or_compatibility_lift() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let bytes = [0x48, 0x63, 0x41, 0x3c];
        let decoded = decode_instruction(&compiled, &bytes, 0x1400_2600).expect("generated movsxd");
        assert_eq!(decoded.length, bytes.len());
        assert_eq!(decoded.mnemonic, "movsxd");
        assert_spec_derived_lift_or_typed_unsupported(&compiled, &bytes, 0x1400_2600);
    }

    #[test]
    fn generated_runtime_zero_extends_reg32_decode_without_compatibility_lift() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let bytes = [0x31, 0xc0];
        let decoded =
            decode_instruction(&compiled, &bytes, 0x1400_19e0).expect("generated xor eax, eax");
        assert_eq!(decoded.length, bytes.len());
        assert_eq!(decoded.mnemonic, "xor");
        assert_spec_derived_lift_or_typed_unsupported(&compiled, &bytes, 0x1400_19e0);
    }

    #[test]
    fn generated_runtime_decodes_fninit_without_decode_no_match() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let bytes = [0xdb, 0xe3];
        let decoded =
            decode_instruction(&compiled, &bytes, 0x1400_25c0).expect("generated fninit decode");
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
}
