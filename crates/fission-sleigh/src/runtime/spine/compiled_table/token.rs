use super::handles::u64_to_i64_bits;
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SlaTokenByteSpan {
    start: i32,
    end: i32,
}

impl SlaTokenByteSpan {
    fn shifted(self, delta: i32) -> Result<Self> {
        Ok(Self {
            start: self
                .start
                .checked_add(delta)
                .ok_or_else(|| anyhow!("SLA token span start overflowed"))?,
            end: self
                .end
                .checked_add(delta)
                .ok_or_else(|| anyhow!("SLA token span end overflowed"))?,
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
) -> Result<Option<SlaTokenByteSpan>> {
    if depth > 8 {
        bail!("SLA token span recursion limit exceeded");
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
        } => token_span_from_sla_field(*reloffset, *byte_start, *byte_end).map(Some),
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
        } => subtable_primary_sla_token_span(compiled, table_name, depth + 1)?
            .map(|span| span.shifted(*reloffset))
            .transpose(),
        _ => Ok(None),
    }
}

fn pattern_expression_primary_sla_token_span(
    reloffset: i32,
    expr: &CompiledPatternExpression,
) -> Result<Option<SlaTokenByteSpan>> {
    match expr {
        CompiledPatternExpression::TokenField {
            byte_start,
            byte_end,
            ..
        } => token_span_from_sla_field(reloffset, *byte_start, *byte_end).map(Some),
        CompiledPatternExpression::Add(lhs, rhs)
        | CompiledPatternExpression::Sub(lhs, rhs)
        | CompiledPatternExpression::Mul(lhs, rhs)
        | CompiledPatternExpression::Div(lhs, rhs)
        | CompiledPatternExpression::LeftShift(lhs, rhs)
        | CompiledPatternExpression::RightShift(lhs, rhs)
        | CompiledPatternExpression::And(lhs, rhs)
        | CompiledPatternExpression::Or(lhs, rhs)
        | CompiledPatternExpression::Xor(lhs, rhs) => {
            let lhs = pattern_expression_primary_sla_token_span(reloffset, lhs)?;
            let rhs = pattern_expression_primary_sla_token_span(reloffset, rhs)?;
            Ok(match (lhs, rhs) {
                (Some(lhs), Some(rhs)) => Some(lhs.union(rhs)),
                (Some(span), None) | (None, Some(span)) => Some(span),
                (None, None) => None,
            })
        }
        CompiledPatternExpression::Negate(inner) | CompiledPatternExpression::Not(inner) => {
            pattern_expression_primary_sla_token_span(reloffset, inner)
        }
        _ => Ok(None),
    }
}

fn subtable_primary_sla_token_span(
    compiled: &CompiledFrontend,
    table_name: &str,
    depth: usize,
) -> Result<Option<SlaTokenByteSpan>> {
    if depth > 8 {
        bail!("SLA token span recursion limit exceeded");
    }
    let subtable = compiled
        .subtables
        .get(table_name)
        .ok_or_else(|| anyhow!("missing subtable {table_name} for SLA token span"))?;
    let mut span: Option<SlaTokenByteSpan> = None;
    for constructor in &subtable.constructors {
        for handle in &constructor.constructor_template.handles {
            if let Some(handle_span) =
                operand_spec_primary_sla_token_span(compiled, &handle.spec, depth + 1)?
            {
                span = Some(match span {
                    Some(current) => current.union(handle_span),
                    None => handle_span,
                });
                break;
            }
        }
    }
    Ok(span)
}

fn token_span_from_sla_field(
    reloffset: i32,
    byte_start: u32,
    byte_end: u32,
) -> Result<SlaTokenByteSpan> {
    let start =
        i32::try_from(byte_start).map_err(|_| anyhow!("SLA token byte_start exceeds i32"))?;
    let end = byte_end
        .checked_add(1)
        .ok_or_else(|| anyhow!("SLA token byte_end overflowed"))?;
    let end = i32::try_from(end).map_err(|_| anyhow!("SLA token byte_end exceeds i32"))?;
    Ok(SlaTokenByteSpan {
        start: reloffset
            .checked_add(start)
            .ok_or_else(|| anyhow!("SLA token span start overflowed"))?,
        end: reloffset
            .checked_add(end)
            .ok_or_else(|| anyhow!("SLA token span end overflowed"))?,
    })
}

pub(super) fn read_uint(bytes: &[u8], offset: usize, size: u32) -> Result<u64> {
    ensure_u64_byte_width(size, "immediate")?;
    let size = usize::try_from(size).map_err(|_| anyhow!("immediate byte width exceeds usize"))?;
    let end = offset
        .checked_add(size)
        .ok_or_else(|| anyhow!("immediate byte range overflow"))?;
    let slice = bytes
        .get(offset..end)
        .ok_or_else(|| anyhow!("missing immediate bytes"))?;
    let mut value = 0u64;
    for (index, byte) in slice.iter().enumerate() {
        let shift = byte_shift(index, "immediate")?;
        let shifted = u64::from(*byte)
            .checked_shl(shift)
            .ok_or_else(|| anyhow!("immediate byte shift {shift} exceeds u64 width"))?;
        value |= shifted;
    }
    Ok(value)
}

pub(super) fn read_sint(bytes: &[u8], offset: usize, size: u32) -> Result<i64> {
    if size == 0 {
        bail!("signed immediate byte width must be non-zero");
    }
    let value = read_uint(bytes, offset, size)?;
    let bits = size
        .checked_mul(8)
        .ok_or_else(|| anyhow!("signed immediate bit width overflow"))?;
    if bits == 64 {
        Ok(u64_to_i64_bits(value))
    } else {
        let shift = 64 - bits;
        Ok(u64_to_i64_bits(value << shift) >> shift)
    }
}

pub(super) fn constructor_consumes_sequential_operand_bytes(
    compiled: &CompiledFrontend,
    constructor: &CompiledExecutableConstructor,
) -> Result<bool> {
    for handle in &constructor.constructor_template.handles {
        if operand_spec_consumes_sequential_bytes(compiled, &handle.spec, 0)? {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn subtable_consumes_sequential_bytes(
    compiled: &CompiledFrontend,
    table_name: &str,
    depth: usize,
) -> Result<bool> {
    if depth > 8 {
        bail!("SLA sequential-byte recursion limit exceeded");
    }
    let subtable = compiled
        .subtables
        .get(table_name)
        .ok_or_else(|| anyhow!("missing subtable {table_name} for sequential-byte analysis"))?;
    for constructor in &subtable.constructors {
        if constructor_consumes_sequential_operand_bytes_with_depth(
            compiled,
            constructor,
            depth + 1,
        )? {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn constructor_consumes_sequential_operand_bytes_with_depth(
    compiled: &CompiledFrontend,
    constructor: &CompiledExecutableConstructor,
    depth: usize,
) -> Result<bool> {
    for handle in &constructor.constructor_template.handles {
        if operand_spec_consumes_sequential_bytes(compiled, &handle.spec, depth)? {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn operand_spec_consumes_sequential_bytes(
    compiled: &CompiledFrontend,
    spec: &CompiledOperandSpec,
    depth: usize,
) -> Result<bool> {
    match spec {
        CompiledOperandSpec::SlaTokenField { .. }
        | CompiledOperandSpec::SlaVarnodeList { .. }
        | CompiledOperandSpec::SlaValueMap { .. }
        | CompiledOperandSpec::SlaVarnodeListExpression { .. }
        | CompiledOperandSpec::SlaValueMapExpression { .. }
        | CompiledOperandSpec::SlaPatternExpression { .. }
            if operand_spec_primary_sla_token_span(compiled, spec, depth)?.is_some() =>
        {
            Ok(true)
        }
        CompiledOperandSpec::SlaValueMap { .. } => Ok(true),
        CompiledOperandSpec::SubtableEvaluation { table_name, .. } => {
            subtable_consumes_sequential_bytes(compiled, table_name, depth + 1)
        }
        _ => Ok(false),
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
    ensure_u64_byte_width(size, "tokenfield")?;
    let mut res = 0u64;
    for idx in 0..size {
        let off = if big_endian {
            byte_start + idx
        } else {
            byte_end
                .checked_sub(idx)
                .ok_or_else(|| anyhow!("tokenfield byte index underflow"))?
        };
        let off =
            usize::try_from(off).map_err(|_| anyhow!("tokenfield byte offset exceeds usize"))?;
        let absolute_off = base_cursor
            .checked_add(off)
            .ok_or_else(|| anyhow!("tokenfield absolute byte offset overflow"))?;
        let byte = *ctx
            .bytes
            .get(absolute_off)
            .ok_or_else(|| anyhow!("tokenfield byte {} out of range", off))?;
        res = append_tokenfield_byte(res, byte)?;
    }
    let shifted = shifted_sla_token_field(res, shift)?;
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

pub(super) fn opcode_len_from_matcher(matcher: &CompiledPatternMatcher) -> Result<Option<usize>> {
    match matcher {
        CompiledPatternMatcher::ExactBytes(bytes) => Ok(Some(bytes.len())),
        CompiledPatternMatcher::RowCc { prefix, .. } => prefix
            .len()
            .checked_add(1)
            .map(Some)
            .ok_or_else(|| anyhow!("row-cc opcode length overflowed")),
        CompiledPatternMatcher::RowPage { .. } => Ok(Some(1)),
        CompiledPatternMatcher::BitConstraints(constraints) => {
            let mut len = None;
            for constraint in constraints {
                if let crate::compiler::PatternConstraint::Instruction { offset, .. } = constraint {
                    let end = usize::try_from(*offset)
                        .map_err(|_| anyhow!("bit-constraint matcher offset is negative"))?
                        .checked_add(1)
                        .ok_or_else(|| {
                            anyhow!("bit-constraint matcher opcode length overflowed")
                        })?;
                    len = Some(len.map_or(end, |current: usize| current.max(end)));
                }
            }
            Ok(len)
        }
    }
}

fn ensure_u64_byte_width(size: u32, role: &str) -> Result<()> {
    if size > 8 {
        bail!("{role} byte width {size} exceeds u64");
    }
    Ok(())
}

fn byte_shift(index: usize, role: &str) -> Result<u32> {
    let index = u32::try_from(index)
        .map_err(|_| anyhow!("{role} byte index exceeds u32 for shift calculation"))?;
    index
        .checked_mul(8)
        .ok_or_else(|| anyhow!("{role} byte shift overflowed"))
}

fn append_tokenfield_byte(value: u64, byte: u8) -> Result<u64> {
    let shifted = value
        .checked_shl(8)
        .ok_or_else(|| anyhow!("tokenfield byte accumulation exceeds u64 width"))?;
    Ok(shifted | u64::from(byte))
}

fn shifted_sla_token_field(value: u64, shift: i32) -> Result<u64> {
    if shift >= 0 {
        let shift =
            u32::try_from(shift).map_err(|_| anyhow!("tokenfield right shift exceeds u32"))?;
        value
            .checked_shr(shift)
            .ok_or_else(|| anyhow!("tokenfield right shift {shift} exceeds u64 width"))
    } else {
        let amount = shift
            .checked_neg()
            .ok_or_else(|| anyhow!("tokenfield shift amount underflow"))?;
        let amount =
            u32::try_from(amount).map_err(|_| anyhow!("tokenfield left shift exceeds u32"))?;
        value
            .checked_shl(amount)
            .ok_or_else(|| anyhow!("tokenfield left shift {amount} exceeds u64 width"))
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

#[cfg(test)]
mod tests {
    use super::{
        read_sint, read_sla_token_field_at, read_uint, shifted_sla_token_field,
        CompiledInstructionContext,
    };

    #[test]
    fn immediate_reads_fail_closed_above_u64_width() {
        let bytes = [0xff; 9];

        assert_eq!(read_uint(&bytes, 0, 8).unwrap(), u64::MAX);
        assert!(read_uint(&bytes, 0, 9).is_err());
        assert!(read_sint(&bytes, 0, 0).is_err());
        assert!(read_sint(&bytes, 0, 9).is_err());
    }

    #[test]
    fn tokenfield_reads_fail_closed_above_u64_width() {
        let bytes = [0xff; 9];
        let ctx = CompiledInstructionContext::parse(&bytes, 0x1000).expect("context");

        assert!(read_sla_token_field_at(&ctx, 0, true, false, 0, 63, 0, 8, 0).is_err());
    }

    #[test]
    fn tokenfield_shift_fails_closed_on_invalid_amounts() {
        assert_eq!(shifted_sla_token_field(0x80, 7).unwrap(), 1);
        assert_eq!(shifted_sla_token_field(1, -7).unwrap(), 0x80);

        assert!(shifted_sla_token_field(1, 64).is_err());
        assert!(shifted_sla_token_field(1, -64).is_err());
        assert!(shifted_sla_token_field(1, i32::MIN).is_err());
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
