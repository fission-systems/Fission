use anyhow::{anyhow, bail, Result};
use fission_pcode::arch::x86::{X86_EFLAGS_BASE, X86_REG_BASE};
use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};

use crate::compiler::{
    CompiledArithmeticOpcode, CompiledConstructorTemplate, CompiledDecisionProbe,
    CompiledExecutableConstructor, CompiledFixedRegister, CompiledFrontend, CompiledHandleTemplate,
    CompiledOpcodeMatcher, CompiledOperandDecodeStep, CompiledOperandSpec, CompiledSemanticKind,
    CompiledSemanticOp,
};

use super::{RuntimeSleighError, UNIQUE_SPACE_ID};

pub(super) fn decode_and_lift(
    compiled: &CompiledFrontend,
    bytes: &[u8],
    address: u64,
) -> Result<(Vec<PcodeOp>, u64)> {
    let ctx = RuntimeInstructionContext::parse(bytes, address)?;
    let selection = select_constructor(compiled, &ctx).ok_or_else(|| {
        RuntimeSleighError::DecodeNoMatch {
            language: compiled.entry_id.clone(),
            address,
        }
    })?;
    let decoded = bind_instruction(&ctx, selection)?;
    let mut emitter = GeneratedX86Emitter::new(address);
    RuntimeTemplateEvaluator::new(&mut emitter).emit(&decoded)?;
    Ok((emitter.finish(), decoded.length as u64))
}

#[derive(Debug, Clone, Copy)]
struct RexPrefix {
    w: bool,
    r: bool,
    x: bool,
    b: bool,
}

#[derive(Debug, Clone)]
struct RuntimeInstructionContext<'a> {
    bytes: &'a [u8],
    address: u64,
    cursor: usize,
    operand_size_code: u8,
    rex: RexPrefix,
}

impl<'a> RuntimeInstructionContext<'a> {
    fn parse(bytes: &'a [u8], address: u64) -> Result<Self> {
        if bytes.is_empty() {
            bail!("empty x86-64 decode buffer");
        }
        let mut cursor = 0usize;
        let mut operand_size_override = false;
        let mut rex = RexPrefix {
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
                    rex = RexPrefix {
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
        let operand_size_code = if rex.w {
            2
        } else if operand_size_override {
            0
        } else {
            1
        };
        Ok(Self {
            bytes,
            address,
            cursor,
            operand_size_code,
            rex,
        })
    }
}

#[derive(Debug, Clone)]
struct RuntimeSelection<'a> {
    constructor: &'a CompiledExecutableConstructor,
    trace: RuntimeMatchTrace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeMatchTrace {
    root_bucket: String,
    probes: Vec<RuntimeMatchProbe>,
    leaf_constructor_indexes: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeMatchProbe {
    probe: CompiledDecisionProbe,
    value: u8,
}

#[derive(Debug, Clone)]
struct RuntimeConstructState {
    semantic_kind: CompiledSemanticKind,
    constructor_template: CompiledConstructorTemplate,
    construct_nodes: Vec<RuntimeConstructNode>,
    handles: Vec<RuntimeHandle>,
    operands: Vec<BoundOperand>,
    condition_code: Option<u8>,
    length: usize,
    match_trace: RuntimeMatchTrace,
}

#[derive(Debug, Clone)]
struct RuntimeConstructNode {
    operand_index: Option<usize>,
    parent_index: Option<usize>,
    absolute_offset: usize,
    relative_length: usize,
    handle_index: Option<usize>,
}

#[derive(Debug, Clone)]
struct RuntimeHandle {
    operand_index: usize,
    spec: CompiledOperandSpec,
    value: BoundOperand,
}

#[derive(Debug, Clone)]
enum BoundOperand {
    Register {
        index: u8,
        size: u32,
    },
    Memory {
        base: Option<u8>,
        index: Option<u8>,
        scale: u8,
        displacement: i64,
        rip_relative: bool,
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

#[derive(Debug, Clone, Copy)]
struct ModRm {
    mod_bits: u8,
    reg: u8,
    rm: u8,
    base: Option<u8>,
    index: Option<u8>,
    scale: u8,
    displacement: i64,
    rip_relative: bool,
    length: usize,
}

fn select_constructor<'a>(
    compiled: &'a CompiledFrontend,
    ctx: &RuntimeInstructionContext<'_>,
) -> Option<RuntimeSelection<'a>> {
    if let Some(selection) = walk_decision_tree(
        compiled,
        ctx,
        compiled.decision_tree.root_node_index,
        "global",
    ) {
        return Some(selection);
    }
    for bucket_key in candidate_bucket_keys(ctx)? {
        let Some(bucket) = compiled
            .decision_tree
            .root_buckets
            .iter()
            .find(|bucket| bucket.key == bucket_key)
        else {
            continue;
        };
        if let Some(selection) = walk_decision_tree(compiled, ctx, bucket.node_index, &bucket.key) {
            return Some(selection);
        }
    }
    None
}

fn candidate_bucket_keys(ctx: &RuntimeInstructionContext<'_>) -> Option<Vec<String>> {
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

fn walk_decision_tree<'a>(
    compiled: &'a CompiledFrontend,
    ctx: &RuntimeInstructionContext<'_>,
    root_node_index: usize,
    root_bucket: &str,
) -> Option<RuntimeSelection<'a>> {
    let mut node_index = root_node_index;
    let mut cached_modrm = None;
    let mut trace = RuntimeMatchTrace {
        root_bucket: root_bucket.to_string(),
        probes: Vec::new(),
        leaf_constructor_indexes: Vec::new(),
    };

    loop {
        let node = compiled.decision_tree.nodes.get(node_index)?;
        match node.probe {
            CompiledDecisionProbe::Terminal => {
                trace.leaf_constructor_indexes = node.leaf_constructor_indexes.clone();
                for constructor_index in &node.leaf_constructor_indexes {
                    let constructor = compiled.executable_constructors.get(*constructor_index)?;
                    if !constructor.runtime_ready {
                        continue;
                    }
                    if constructor_matches(ctx, constructor).is_ok() {
                        return Some(RuntimeSelection { constructor, trace });
                    }
                }
                return None;
            }
            probe => {
                let value = probe_value(ctx, probe, &mut cached_modrm).ok()?;
                trace.probes.push(RuntimeMatchProbe { probe, value });
                let edge = node.branches.iter().find(|edge| edge.value == value)?;
                node_index = edge.next_node_index;
            }
        }
    }
}

fn probe_value(
    ctx: &RuntimeInstructionContext<'_>,
    probe: CompiledDecisionProbe,
    cached_modrm: &mut Option<ModRm>,
) -> Result<u8> {
    Ok(match probe {
        CompiledDecisionProbe::Terminal => 0,
        CompiledDecisionProbe::InstructionByte {
            offset,
            mask,
            shift,
        } => {
            let byte = *ctx
                .bytes
                .get(ctx.cursor + usize::from(offset))
                .ok_or_else(|| anyhow!("missing instruction byte probe at offset {offset}"))?;
            (byte & mask) >> shift
        }
        CompiledDecisionProbe::OperandSizeCode => ctx.operand_size_code,
        CompiledDecisionProbe::ModBits => ensure_modrm(ctx, cached_modrm)?.mod_bits,
        CompiledDecisionProbe::RegOpcode => ensure_modrm(ctx, cached_modrm)?.reg,
    })
}

fn ensure_modrm<'a>(
    ctx: &RuntimeInstructionContext<'_>,
    cached_modrm: &'a mut Option<ModRm>,
) -> Result<&'a ModRm> {
    if cached_modrm.is_none() {
        *cached_modrm = Some(parse_modrm(ctx, ctx.cursor + opcode_len_from_context(ctx)?)?);
    }
    cached_modrm
        .as_ref()
        .ok_or_else(|| anyhow!("missing cached modrm"))
}

fn opcode_len_from_context(ctx: &RuntimeInstructionContext<'_>) -> Result<usize> {
    let opcode = *ctx
        .bytes
        .get(ctx.cursor)
        .ok_or_else(|| anyhow!("missing opcode byte"))?;
    Ok(if opcode == 0x0f { 2 } else { 1 })
}

fn constructor_matches(
    ctx: &RuntimeInstructionContext<'_>,
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
    ctx: &RuntimeInstructionContext<'_>,
    selection: RuntimeSelection<'_>,
) -> Result<RuntimeConstructState> {
    constructor_matches(ctx, selection.constructor)?;
    RuntimeParserWalker::new(ctx, selection)?.walk()
}

struct RuntimeParserWalker<'a, 'b> {
    ctx: &'a RuntimeInstructionContext<'b>,
    selection: RuntimeSelection<'a>,
    cursor: usize,
    modrm: Option<ModRm>,
    handles: Vec<Option<RuntimeHandle>>,
    construct_nodes: Vec<RuntimeConstructNode>,
}

impl<'a, 'b> RuntimeParserWalker<'a, 'b> {
    fn new(
        ctx: &'a RuntimeInstructionContext<'b>,
        selection: RuntimeSelection<'a>,
    ) -> Result<Self> {
        let opcode_len = opcode_len_from_matcher(&selection.constructor.matcher);
        Ok(Self {
            ctx,
            cursor: ctx.cursor + opcode_len,
            modrm: None,
            handles: vec![None; selection.constructor.constructor_template.handles.len()],
            construct_nodes: vec![RuntimeConstructNode {
                operand_index: None,
                parent_index: None,
                absolute_offset: ctx.cursor,
                relative_length: opcode_len,
                handle_index: None,
            }],
            selection,
        })
    }

    fn walk(mut self) -> Result<RuntimeConstructState> {
        let decode_steps = self.selection.constructor.constructor_template.decode_steps.clone();
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
            CompiledOpcodeMatcher::RowCc { prefix, .. } => Some(
                self.ctx.bytes[self.ctx.cursor + prefix.len()] & 0x0f,
            ),
            _ if matches!(self.selection.constructor.semantic_kind, CompiledSemanticKind::Setcc)
                && matches!(
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
            construct_nodes: self.construct_nodes,
            handles,
            operands,
            condition_code,
            length: self.cursor,
            match_trace: self.selection.trace,
        })
    }

    fn decode_operand(&mut self, operand_index: usize) -> Result<()> {
        if self.handles.get(operand_index).is_some_and(|handle| handle.is_some()) {
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
        self.construct_nodes.push(RuntimeConstructNode {
            operand_index: Some(operand_index),
            parent_index: Some(0),
            absolute_offset: operand_cursor_start,
            relative_length: self.cursor.saturating_sub(operand_cursor_start),
            handle_index: Some(handle_index),
        });
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
        self.modrm
            .ok_or_else(|| anyhow!("failed to decode modrm"))
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
                Ok(BoundOperand::Register { index: reg, size: *size })
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

fn parse_modrm(ctx: &RuntimeInstructionContext<'_>, offset: usize) -> Result<ModRm> {
    let byte = *ctx
        .bytes
        .get(offset)
        .ok_or_else(|| anyhow!("missing modrm at {offset}"))?;
    let mod_bits = byte >> 6;
    let reg = ((byte >> 3) & 0x7) | ((ctx.rex.r as u8) << 3);
    let rm_low = byte & 0x7;
    let rm = rm_low | ((ctx.rex.b as u8) << 3);
    if mod_bits == 3 {
        return Ok(ModRm {
            mod_bits,
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
            index = Some(index_low | ((ctx.rex.x as u8) << 3));
        }
        if mod_bits == 0 && base_low == 5 {
            base = None;
            displacement = read_sint(ctx.bytes, offset + length, 4)?;
            length += 4;
        } else {
            base = Some(base_low | ((ctx.rex.b as u8) << 3));
        }
    } else if mod_bits == 0 && rm_low == 5 {
        base = None;
        rip_relative = true;
        displacement = read_sint(ctx.bytes, offset + length, 4)?;
        length += 4;
    }

    match mod_bits {
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

    Ok(ModRm {
        mod_bits,
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

#[derive(Debug, Clone)]
struct GeneratedX86Emitter {
    address: u64,
    seq: u32,
    next_tmp: u64,
    ops: Vec<PcodeOp>,
}

impl GeneratedX86Emitter {
    fn new(address: u64) -> Self {
        Self {
            address,
            seq: 0,
            next_tmp: 0xE400_0000_0000_0000u64.wrapping_add(address.wrapping_shl(6)),
            ops: Vec::new(),
        }
    }

    fn finish(self) -> Vec<PcodeOp> {
        self.ops
    }
}

struct RuntimeTemplateEvaluator<'a> {
    emitter: &'a mut GeneratedX86Emitter,
}

impl<'a> RuntimeTemplateEvaluator<'a> {
    fn new(emitter: &'a mut GeneratedX86Emitter) -> Self {
        Self { emitter }
    }

    fn emit(&mut self, state: &RuntimeConstructState) -> Result<()> {
        for op in &state.constructor_template.semantic_ops {
            match op {
                CompiledSemanticOp::Nop => {}
                CompiledSemanticOp::Return => {
                    self.emitter.push(PcodeOpcode::Return, None, Vec::new(), "RET");
                }
                CompiledSemanticOp::Call => self.emitter.emit_call(state)?,
                CompiledSemanticOp::Jump => self.emitter.emit_jmp(state)?,
                CompiledSemanticOp::ConditionalJump => self.emitter.emit_jcc(state)?,
                CompiledSemanticOp::Move => self.emitter.emit_mov(state)?,
                CompiledSemanticOp::Lea => self.emitter.emit_lea(state)?,
                CompiledSemanticOp::Push => self.emitter.emit_push(state)?,
                CompiledSemanticOp::Pop => self.emitter.emit_pop(state)?,
                CompiledSemanticOp::Leave => self.emitter.emit_leave()?,
                CompiledSemanticOp::Binary { opcode } => match opcode {
                    CompiledArithmeticOpcode::Add => {
                        self.emitter.emit_binary(state, PcodeOpcode::IntAdd, "ADD")?
                    }
                    CompiledArithmeticOpcode::Sub => {
                        self.emitter.emit_binary(state, PcodeOpcode::IntSub, "SUB")?
                    }
                    CompiledArithmeticOpcode::And => {
                        self.emitter.emit_binary(state, PcodeOpcode::IntAnd, "AND")?
                    }
                    CompiledArithmeticOpcode::Or => {
                        self.emitter.emit_binary(state, PcodeOpcode::IntOr, "OR")?
                    }
                    CompiledArithmeticOpcode::Xor => {
                        self.emitter.emit_binary(state, PcodeOpcode::IntXor, "XOR")?
                    }
                    CompiledArithmeticOpcode::Mul => {
                        self.emitter.emit_binary(state, PcodeOpcode::IntMult, "IMUL")?
                    }
                    CompiledArithmeticOpcode::Shl => {
                        self.emitter.emit_binary(state, PcodeOpcode::IntLeft, "SHL")?
                    }
                    CompiledArithmeticOpcode::Shr => {
                        self.emitter.emit_binary(state, PcodeOpcode::IntRight, "SHR")?
                    }
                    CompiledArithmeticOpcode::Sar => {
                        self.emitter.emit_binary(state, PcodeOpcode::IntSRight, "SAR")?
                    }
                    CompiledArithmeticOpcode::Inc => {
                        self.emitter.emit_unary_delta(state, 1, "INC")?
                    }
                    CompiledArithmeticOpcode::Dec => {
                        self.emitter.emit_unary_delta(state, -1, "DEC")?
                    }
                },
                CompiledSemanticOp::Compare { bitwise } => {
                    self.emitter
                        .emit_compare(state, *bitwise, if *bitwise { "TEST" } else { "CMP" })?
                }
                CompiledSemanticOp::Extend { signed } => {
                    let opcode = if *signed {
                        PcodeOpcode::IntSExt
                    } else {
                        PcodeOpcode::IntZExt
                    };
                    self.emitter
                        .emit_extend(state, opcode, if *signed { "MOVSX" } else { "MOVZX" })?;
                }
                CompiledSemanticOp::SetCc => self.emitter.emit_setcc(state)?,
                CompiledSemanticOp::AccumulatorExtend { src_size, dst_size } => {
                    self.emitter
                        .emit_accumulator_extend(*src_size, *dst_size, state.semantic_kind.as_str())?;
                }
            }
        }
        Ok(())
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
                self.push(
                    PcodeOpcode::Branch,
                    None,
                    vec![target],
                    "JMP",
                );
            }
            _ => {
                let target = self.read_operand(operand, 8, instruction.length)?;
                self.push(
                    PcodeOpcode::BranchInd,
                    None,
                    vec![target],
                    "JMP",
                );
            }
        }
        Ok(())
    }

    fn emit_jcc(&mut self, instruction: &RuntimeConstructState) -> Result<()> {
        let target = self.read_operand(&instruction.operands[0], 8, instruction.length)?;
        let cond = self.condition_varnode(instruction.condition_code.ok_or_else(|| anyhow!("missing jcc condition"))?)?;
        self.push(PcodeOpcode::CBranch, None, vec![target, cond], "JCC");
        Ok(())
    }

    fn emit_mov(&mut self, instruction: &RuntimeConstructState) -> Result<()> {
        let size = operand_size(&instruction.operands[0]);
        let value = self.read_operand(&instruction.operands[1], size, instruction.length)?;
        self.write_operand(&instruction.operands[0], value, size, instruction.length, "MOV")
    }

    fn emit_lea(&mut self, instruction: &RuntimeConstructState) -> Result<()> {
        let size = operand_size(&instruction.operands[0]).max(8);
        let BoundOperand::Memory { .. } = &instruction.operands[1] else {
            bail!("lea source must be memory");
        };
        let addr = self.effective_address(&instruction.operands[1], instruction.length)?;
        self.write_operand(&instruction.operands[0], addr, size, instruction.length, "LEA")
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
        self.write_operand(&instruction.operands[0], value, size, instruction.length, "POP")?;
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
        let size = operand_size(&instruction.operands[0]).max(operand_size(&instruction.operands[1]));
        let lhs = self.read_operand(&instruction.operands[0], size, instruction.length)?;
        let rhs = self.read_operand(&instruction.operands[1], size, instruction.length)?;
        let result = self.tmp(size);
        self.push(
            if bitwise { PcodeOpcode::IntAnd } else { PcodeOpcode::IntSub },
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
        let cond = self.condition_varnode(instruction.condition_code.ok_or_else(|| anyhow!("missing setcc condition"))?)?;
        self.write_operand(&instruction.operands[0], cond, 1, instruction.length, "SETCC")
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
            BoundOperand::Register { index, size } => Ok(gpr(u64::from(*index), (*size).max(expected_size.min(*size)))),
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
                self.push(PcodeOpcode::Copy, Some(gpr(u64::from(*index), *size)), vec![value], tag);
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

    fn effective_address(&mut self, operand: &BoundOperand, instruction_len: usize) -> Result<Varnode> {
        let BoundOperand::Memory {
            base,
            index,
            scale,
            displacement,
            rip_relative,
            ..
        } = operand else {
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
                    self.push(PcodeOpcode::IntSub, Some(tmp.clone()), vec![lhs, rhs], "EA_DISP");
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
            self.push(PcodeOpcode::IntAdd, Some(next.clone()), vec![acc, term], "EA_ADD");
            acc = next;
        }
        Ok(acc)
    }

    fn tmp(&mut self, size: u32) -> Varnode {
        let vn = Varnode {
            space_id: UNIQUE_SPACE_ID,
            offset: self.next_tmp,
            size,
            is_constant: false,
            constant_val: 0,
        };
        self.next_tmp = self.next_tmp.wrapping_add(8);
        vn
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
        self.push(PcodeOpcode::IntEqual, Some(out.clone()), vec![lhs, rhs], tag);
        out
    }

    fn bool_ne(&mut self, lhs: Varnode, rhs: Varnode, tag: &str) -> Varnode {
        let out = self.tmp(1);
        self.push(PcodeOpcode::IntNotEqual, Some(out.clone()), vec![lhs, rhs], tag);
        out
    }

    fn push(
        &mut self,
        opcode: PcodeOpcode,
        output: Option<Varnode>,
        inputs: Vec<Varnode>,
        mnemonic: &str,
    ) {
        self.ops.push(PcodeOp {
            seq_num: self.seq,
            opcode,
            address: self.address,
            output,
            inputs,
            asm_mnemonic: Some(mnemonic.to_string()),
        });
        self.seq = self.seq.saturating_add(1);
    }
}

fn operand_size(operand: &BoundOperand) -> u32 {
    match operand {
        BoundOperand::Register { size, .. } | BoundOperand::Memory { size, .. } => *size,
        BoundOperand::Immediate { encoded_size, .. } => *encoded_size,
        BoundOperand::Relative { .. } => 8,
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
        let (ops, len) = decode_and_lift(&compiled, &bytes, address)
            .expect("generated rip-relative mov");
        assert_eq!(len, bytes.len() as u64);
        assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::Load));
        assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::Copy));
        let expected_target = address + bytes.len() as u64 + 0x3015;
        assert!(ops.iter().any(|op| {
            op.opcode == PcodeOpcode::Load
                && op.inputs.iter().any(|vn| vn.is_constant && vn.constant_val as u64 == expected_target)
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
        let ctx = RuntimeInstructionContext::parse(&[0xC7, 0x00, 0x01, 0x00, 0x00, 0x00], 0x1000)
            .expect("decode context");
        let selection = select_constructor(&compiled, &ctx).expect("constructor selection");
        let state = bind_instruction(&ctx, selection).expect("bind instruction");
        assert_eq!(state.match_trace.root_bucket, "global");
        assert!(!state.match_trace.probes.is_empty());
        assert!(!state.construct_nodes.is_empty());
        assert!(
            state.handles.iter().any(|handle| matches!(handle.spec, CompiledOperandSpec::ModRmRm { .. }))
        );
    }
}
