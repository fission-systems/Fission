use super::*;

// Transitional shared-token cursor policy.
//
// MIGRATION DEBT: variable-length specs may need shared token cursor handling
// until the walker mirrors Ghidra's ConstructState offset/length tree directly.
//
// Ghidra does not enable this by architecture name. ParserWalker computes each
// operand offset from OperandSymbol.reloffset/offsetbase and each token field
// read uses the current ConstructState offset. Until Fission stores that full
// tree, this policy is enabled only when the compiled `.sla` metadata proves
// that sibling subtable operands read the same one-byte token selector.
//
// Do NOT add new subtable name entries to the detection lists below. Fix the
// underlying SLA-native byte-range accumulation instead.
#[derive(Debug, Clone, Copy)]
pub(super) struct CompiledTokenCursorPolicy {
    shared_token_cursor: bool,
}

impl CompiledTokenCursorPolicy {
    pub(super) fn for_frontend(compiled: &CompiledFrontend) -> Self {
        Self {
            shared_token_cursor: frontend_has_shared_one_byte_subtable_token_operands(compiled),
        }
    }

    pub(super) fn uses_shared_token_cursor(self) -> bool {
        self.shared_token_cursor
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SlaTokenByteSpan {
    start: i32,
    end: i32,
}

impl SlaTokenByteSpan {
    fn width(self) -> i32 {
        self.end.saturating_sub(self.start)
    }

    fn shifted(self, delta: i32) -> Option<Self> {
        Some(Self {
            start: self.start.checked_add(delta)?,
            end: self.end.checked_add(delta)?,
        })
    }

    fn union(self, other: Self) -> Self {
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    fn overlaps(self, other: Self) -> bool {
        self.start < other.end && other.start < self.end
    }
}

fn frontend_has_shared_one_byte_subtable_token_operands(compiled: &CompiledFrontend) -> bool {
    if compiled.sla_ram_address_size() <= 4 {
        return false;
    }

    if !frontend_has_instruction_forms_longer_than_four_bytes(compiled) {
        return false;
    }

    if frontend_has_variable_length_byte_token_layout(compiled) {
        return true;
    }

    compiled.subtables.values().any(|subtable| {
        subtable.constructors.iter().any(|constructor| {
            constructor_has_shared_one_byte_subtable_token_operands(compiled, constructor)
        })
    })
}

fn frontend_has_variable_length_byte_token_layout(compiled: &CompiledFrontend) -> bool {
    let one_byte_fields = compiled
        .language_layout
        .token_fields
        .iter()
        .filter(|field| field.bit_width == 8)
        .count();
    let has_eight_byte_payload = compiled
        .language_layout
        .token_fields
        .iter()
        .any(|field| field.bit_width >= 64);

    one_byte_fields >= 3 && has_eight_byte_payload
}

fn frontend_has_instruction_forms_longer_than_four_bytes(compiled: &CompiledFrontend) -> bool {
    compiled.subtables.values().any(|subtable| {
        subtable
            .constructors
            .iter()
            .any(|constructor| constructor.minimum_length > 4)
    })
}

fn constructor_has_shared_one_byte_subtable_token_operands(
    compiled: &CompiledFrontend,
    constructor: &CompiledExecutableConstructor,
) -> bool {
    if constructor.minimum_length > 2 {
        return false;
    }

    let mut spans = Vec::new();
    for handle in &constructor.constructor_template.handles {
        if !matches!(handle.spec, CompiledOperandSpec::SubtableEvaluation { .. }) {
            continue;
        }
        let Some(span) = operand_spec_primary_sla_token_span(compiled, &handle.spec, 0) else {
            continue;
        };
        if span.width() == 1 {
            spans.push(span);
        }
    }

    spans
        .iter()
        .enumerate()
        .any(|(index, lhs)| spans[index + 1..].iter().any(|rhs| lhs.overlaps(*rhs)))
}

fn operand_spec_primary_sla_token_span(
    compiled: &CompiledFrontend,
    spec: &CompiledOperandSpec,
    depth: usize,
) -> Option<SlaTokenByteSpan> {
    if depth > 8 {
        return None;
    }
    match spec {
        CompiledOperandSpec::SlaTokenField {
            byte_start,
            byte_end,
            reloffset,
            offsetbase: _,
            ..
        }
        | CompiledOperandSpec::SlaVarnodeList {
            byte_start,
            byte_end,
            reloffset,
            offsetbase: _,
            ..
        }
        | CompiledOperandSpec::SlaValueMap {
            byte_start,
            byte_end,
            reloffset,
            offsetbase: _,
            ..
        } => token_span_from_sla_field(*reloffset, *byte_start, *byte_end),
        CompiledOperandSpec::SlaPatternExpression {
            expr,
            reloffset,
            offsetbase: _,
        } => pattern_expression_primary_sla_token_span(*reloffset, expr),
        CompiledOperandSpec::SubtableEvaluation {
            table_name,
            reloffset,
            offsetbase: _,
        } => subtable_primary_sla_token_span(compiled, table_name, depth + 1)
            .and_then(|span| span.shifted(*reloffset)),
        _ => None,
    }
}

fn pattern_expression_primary_sla_token_span(
    reloffset: i32,
    expr: &CompiledPatternExpression,
) -> Option<SlaTokenByteSpan> {
    match expr {
        CompiledPatternExpression::TokenField {
            byte_start,
            byte_end,
            ..
        } => token_span_from_sla_field(reloffset, *byte_start, *byte_end),
        CompiledPatternExpression::Add(lhs, rhs)
        | CompiledPatternExpression::Sub(lhs, rhs)
        | CompiledPatternExpression::Mul(lhs, rhs)
        | CompiledPatternExpression::Div(lhs, rhs)
        | CompiledPatternExpression::LeftShift(lhs, rhs)
        | CompiledPatternExpression::RightShift(lhs, rhs)
        | CompiledPatternExpression::And(lhs, rhs)
        | CompiledPatternExpression::Or(lhs, rhs)
        | CompiledPatternExpression::Xor(lhs, rhs) => {
            let lhs = pattern_expression_primary_sla_token_span(reloffset, lhs);
            let rhs = pattern_expression_primary_sla_token_span(reloffset, rhs);
            match (lhs, rhs) {
                (Some(lhs), Some(rhs)) => Some(lhs.union(rhs)),
                (Some(span), None) | (None, Some(span)) => Some(span),
                (None, None) => None,
            }
        }
        CompiledPatternExpression::Negate(inner) | CompiledPatternExpression::Not(inner) => {
            pattern_expression_primary_sla_token_span(reloffset, inner)
        }
        _ => None,
    }
}

fn subtable_primary_sla_token_span(
    compiled: &CompiledFrontend,
    table_name: &str,
    depth: usize,
) -> Option<SlaTokenByteSpan> {
    if depth > 8 {
        return None;
    }
    let subtable = compiled.subtables.get(table_name)?;
    subtable
        .constructors
        .iter()
        .filter_map(|constructor| {
            constructor
                .constructor_template
                .handles
                .iter()
                .find_map(|handle| {
                    operand_spec_primary_sla_token_span(compiled, &handle.spec, depth + 1)
                })
        })
        .reduce(SlaTokenByteSpan::union)
}

fn token_span_from_sla_field(
    reloffset: i32,
    byte_start: u32,
    byte_end: u32,
) -> Option<SlaTokenByteSpan> {
    let start = i32::try_from(byte_start).ok()?;
    let end = byte_end
        .checked_add(1)
        .and_then(|end| i32::try_from(end).ok())?;
    Some(SlaTokenByteSpan {
        start: reloffset.checked_add(start)?,
        end: reloffset.checked_add(end)?,
    })
}

pub(super) fn opcode_len_from_context(ctx: &CompiledInstructionContext<'_>) -> Result<usize> {
    opcode_len_from_cursor(ctx, ctx.cursor)
}

pub(super) fn opcode_len_from_instruction_start(
    ctx: &CompiledInstructionContext<'_>,
) -> Result<usize> {
    opcode_len_from_cursor(ctx, ctx.instruction_cursor)
}

pub(super) fn opcode_cursor_from_context(ctx: &CompiledInstructionContext<'_>) -> usize {
    opcode_cursor_from_cursor(ctx, ctx.cursor)
}

pub(super) fn opcode_token_cursor_from_context(ctx: &CompiledInstructionContext<'_>) -> usize {
    let offset = opcode_cursor_from_context(ctx);
    let opcode = ctx.bytes.get(offset).copied().unwrap_or(0);
    let opcode_bytes: usize = if opcode == 0x0f {
        match ctx.bytes.get(offset + 1).copied() {
            Some(0x38 | 0x3a) => 3,
            Some(_) => 2,
            None => 1,
        }
    } else {
        1
    };
    offset + opcode_bytes.saturating_sub(1)
}

pub(super) fn opcode_cursor_from_cursor(
    ctx: &CompiledInstructionContext<'_>,
    cursor: usize,
) -> usize {
    let mut offset = cursor;
    while let Some(byte) = ctx.bytes.get(offset).copied() {
        if is_instruction_prefix_byte(byte) {
            offset += 1;
            continue;
        }
        break;
    }
    offset
}

pub(super) fn opcode_len_from_cursor(
    ctx: &CompiledInstructionContext<'_>,
    cursor: usize,
) -> Result<usize> {
    let offset = opcode_cursor_from_cursor(ctx, cursor);
    let opcode = *ctx
        .bytes
        .get(offset)
        .ok_or_else(|| anyhow!("missing opcode byte"))?;
    let opcode_bytes = if opcode == 0x0f {
        match ctx.bytes.get(offset + 1).copied() {
            Some(0x38 | 0x3a) => 3,
            Some(_) => 2,
            None => 1,
        }
    } else {
        1
    };
    Ok(offset.saturating_sub(cursor) + opcode_bytes)
}

pub(super) fn is_instruction_prefix_byte(byte: u8) -> bool {
    matches!(
        byte,
        0x26 | 0x2e | 0x36 | 0x3e | 0x64 | 0x65 | 0x66 | 0x67 | 0xf0 | 0xf2 | 0xf3
    ) || (0x40..=0x4f).contains(&byte)
}

pub(super) fn read_uint(bytes: &[u8], offset: usize, size: u32) -> Result<u64> {
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

pub(super) fn read_sint(bytes: &[u8], offset: usize, size: u32) -> Result<i64> {
    let value = read_uint(bytes, offset, size)?;
    let bits = size * 8;
    if bits == 64 {
        Ok(i64::from_ne_bytes(value.to_ne_bytes()))
    } else {
        let shift = 64 - bits;
        Ok(((value << shift) as i64) >> shift)
    }
}

pub(super) fn constructor_consumes_sequential_operand_bytes(
    compiled: &CompiledFrontend,
    constructor: &CompiledExecutableConstructor,
) -> bool {
    if CompiledTokenCursorPolicy::for_frontend(compiled).uses_shared_token_cursor()
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

pub(super) fn subtable_consumes_sequential_bytes(
    compiled: &CompiledFrontend,
    table_name: &str,
    depth: usize,
) -> bool {
    if shared_token_cursor_policy_zero_width_subtable(table_name) {
        return false;
    }
    if CompiledTokenCursorPolicy::for_frontend(compiled).uses_shared_token_cursor() {
        return true;
    }
    if depth > 8 {
        return false;
    }
    let Some(subtable) = compiled.subtables.get(table_name) else {
        return false;
    };
    subtable.constructors.iter().any(|constructor| {
        constructor_consumes_sequential_operand_bytes_with_depth(compiled, constructor, depth + 1)
    })
}

pub(super) fn constructor_consumes_sequential_operand_bytes_with_depth(
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

pub(super) fn operand_spec_consumes_sequential_bytes(
    compiled: &CompiledFrontend,
    spec: &CompiledOperandSpec,
    depth: usize,
) -> bool {
    match spec {
        CompiledOperandSpec::SlaValueMap { .. }
        | CompiledOperandSpec::Immediate { .. }
        | CompiledOperandSpec::Relative { .. } => true,
        CompiledOperandSpec::SubtableEvaluation { table_name, .. }
            if CompiledTokenCursorPolicy::for_frontend(compiled).uses_shared_token_cursor() =>
        {
            !shared_token_cursor_policy_zero_width_subtable(table_name)
        }
        CompiledOperandSpec::SubtableEvaluation { table_name, .. } => {
            subtable_consumes_sequential_bytes(compiled, table_name, depth + 1)
        }
        _ => false,
    }
}

pub(super) fn shared_token_cursor_policy_zero_width_subtable(table_name: &str) -> bool {
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
            | "cc"
    )
}

pub(super) fn shared_token_cursor_policy_register_subtable(table_name: &str) -> bool {
    matches!(table_name, "Reg8" | "Reg16" | "Reg32" | "Reg64")
}

pub(super) fn shared_token_cursor_policy_opcode_token_subtable(table_name: &str) -> bool {
    matches!(table_name, "cc")
}

pub(super) fn shared_token_cursor_policy_modrm_token_subtable(table_name: &str) -> bool {
    matches!(
        table_name,
        "Reg8"
            | "Reg16"
            | "Reg32"
            | "Reg64"
            | "Rmr8"
            | "Rmr16"
            | "Rmr32"
            | "Rmr64"
            | "CRmr8"
            | "CRmr16"
            | "CRmr32"
            | "check_Reg32_dest"
            | "check_Rmr32_dest"
            | "check_rm32_dest"
    )
}

pub(super) fn shared_token_cursor_policy_modrm_trailing_subtable(table_name: &str) -> bool {
    matches!(
        table_name,
        "simm8_16"
            | "simm8_32"
            | "simm8_64"
            | "simm16"
            | "simm16_32"
            | "simm16_64"
            | "simm32"
            | "simm32_64"
            | "pcRelSimm8"
            | "pcRelSimm16"
            | "pcRelSimm32"
            | "pcRelSimm64"
            | "rel8"
            | "rel16"
            | "rel32"
            | "rel64"
            | "imm8"
            | "imm16"
            | "imm32"
            | "imm64"
    )
}

pub(super) fn shared_token_cursor_policy_relative_trailing_subtable(table_name: &str) -> bool {
    table_name.starts_with("pcRelSimm") || table_name.starts_with("rel")
}

pub(super) fn shared_token_cursor_policy_shared_token_subtable(table_name: &str) -> bool {
    matches!(
        table_name,
        "addr16"
            | "addr32"
            | "addr64"
            | "Addr32_64"
            | "Base"
            | "Base64"
            | "Index"
            | "Index64"
            | "Rmr8"
            | "Rmr16"
            | "Rmr32"
            | "Rmr64"
            | "CRmr8"
            | "CRmr16"
            | "CRmr32"
            | "ss"
    )
}

pub(super) fn constructor_replaces_current(constructor: &CompiledExecutableConstructor) -> bool {
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

pub(super) fn read_sla_token_field(
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
        ctx, ctx.cursor, big_endian, sign_bit, bit_start, bit_end, byte_start, byte_end, shift,
    )
}

pub(super) fn read_sla_token_field_at(
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

pub(super) fn opcode_len_from_matcher(matcher: &CompiledPatternMatcher) -> usize {
    match matcher {
        CompiledPatternMatcher::ExactBytes(bytes) => bytes.len(),
        CompiledPatternMatcher::RowCc { prefix, .. } => prefix.len() + 1,
        CompiledPatternMatcher::RowPage { .. } => 1,
        CompiledPatternMatcher::BitConstraints(constraints) => constraints
            .iter()
            .filter_map(|c| match c {
                crate::compiler::PatternConstraint::Instruction { offset, .. } => {
                    Some(*offset as usize + 1)
                }
                _ => None,
            })
            .max()
            .unwrap_or(0),
    }
}

pub(super) fn zero_extend_bits(value: u64, bit: u32) -> u64 {
    if bit >= 64 {
        value
    } else {
        let mask = (1u64 << bit) - 1;
        value & mask
    }
}

pub(super) fn sign_extend_bits(value: u64, bit: u32) -> u64 {
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
