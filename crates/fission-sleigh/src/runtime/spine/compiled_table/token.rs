use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SlaTokenByteSpan {
    start: i32,
    end: i32,
}

impl SlaTokenByteSpan {
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
        CompiledOperandSpec::SlaVarnodeListExpression {
            expr,
            reloffset,
            offsetbase: _,
            ..
        }
        | CompiledOperandSpec::SlaValueMapExpression {
            expr,
            reloffset,
            offsetbase: _,
            ..
        } => pattern_expression_primary_sla_token_span(*reloffset, expr),
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

pub(super) fn read_uint(bytes: &[u8], offset: usize, size: u32) -> Result<u64> {
    let end = offset
        .checked_add(size as usize)
        .ok_or_else(|| anyhow!("immediate byte range overflow"))?;
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
    let bits = size
        .checked_mul(8)
        .ok_or_else(|| anyhow!("signed immediate bit width overflow"))?;
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
        CompiledOperandSpec::SlaTokenField { .. }
        | CompiledOperandSpec::SlaVarnodeList { .. }
        | CompiledOperandSpec::SlaValueMap { .. }
        | CompiledOperandSpec::SlaVarnodeListExpression { .. }
        | CompiledOperandSpec::SlaValueMapExpression { .. }
        | CompiledOperandSpec::SlaPatternExpression { .. }
            if operand_spec_primary_sla_token_span(compiled, spec, depth).is_some() =>
        {
            true
        }
        CompiledOperandSpec::SlaValueMap { .. } => true,
        CompiledOperandSpec::SubtableEvaluation { table_name, .. } => {
            subtable_consumes_sequential_bytes(compiled, table_name, depth + 1)
        }
        _ => false,
    }
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
    if byte_end < byte_start {
        bail!("tokenfield byte range is inverted: {byte_start}..={byte_end}");
    }
    if bit_end < bit_start {
        bail!("tokenfield bit range is inverted: {bit_start}..={bit_end}");
    }
    let size = byte_end
        .checked_sub(byte_start)
        .and_then(|value| value.checked_add(1))
        .ok_or_else(|| anyhow!("tokenfield byte range overflow: {byte_start}..={byte_end}"))?;
    let mut res = 0u64;
    for idx in 0..size {
        let off = if big_endian {
            byte_start + idx
        } else {
            byte_end
                .checked_sub(idx)
                .ok_or_else(|| anyhow!("tokenfield byte index underflow"))?
        } as usize;
        let absolute_off = base_cursor
            .checked_add(off)
            .ok_or_else(|| anyhow!("tokenfield absolute byte offset overflow"))?;
        let byte = *ctx
            .bytes
            .get(absolute_off)
            .ok_or_else(|| anyhow!("tokenfield byte {} out of range", off))?;
        res = (res << 8) | u64::from(byte);
    }
    let shifted = if shift >= 0 {
        res >> (shift as u32)
    } else {
        res << ((-shift) as u32)
    };
    let width = bit_end
        .checked_sub(bit_start)
        .and_then(|value| value.checked_add(1))
        .ok_or_else(|| anyhow!("tokenfield bit range overflow: {bit_start}..={bit_end}"))?;
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
