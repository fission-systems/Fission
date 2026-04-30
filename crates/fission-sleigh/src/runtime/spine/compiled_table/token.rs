#[derive(Debug, Clone, Copy)]
pub(super) struct TokenFieldBundle {
    pub(super) operand_mode: u8,
    pub(super) reg: u8,
    pub(super) rm: u8,
    pub(super) base: Option<u8>,
    pub(super) index: Option<u8>,
    pub(super) scale: u8,
    pub(super) displacement: i64,
    pub(super) rip_relative: bool,
    pub(super) length: usize,
}

// Transitional shared-token cursor policy.
//
// MIGRATION DEBT: This policy detects whether a frontend uses x86-style shared
// token subtables (e.g. ModRM byte shared between Rmr64, addr64, Reg64) by
// checking for well-known x86 subtable names. This is an architecture-specific
// heuristic that must eventually be replaced with SLA-native token field byte
// range tracking.
//
// The canonical replacement is: each operand's byte range comes from its
// `SlaTokenField` byte_start/byte_end fields. When the walker accumulates
// operand byte ranges from the SLA token field metadata rather than a cursor
// advance, this policy becomes unnecessary.
//
// Do NOT add new subtable name entries to the detection lists below. Fix the
// underlying SLA-native byte-range accumulation instead.
#[derive(Debug, Clone, Copy)]
pub(super) struct CompiledTokenCursorPolicy {
    shared_token_cursor: bool,
}

impl CompiledTokenCursorPolicy {
    pub(super) fn for_frontend(compiled: &CompiledFrontend) -> Self {
        // Detect x86 by presence of well-known ModRM/SIB shared-token subtables.
        // This heuristic works because these subtable names are canonical in the
        // Ghidra x86 SLEIGH spec (x86-64.slaspec). No other architecture in the
        // supported set uses these exact subtable names.
        Self {
            shared_token_cursor: compiled.subtables.contains_key("Rmr64")
                && compiled.subtables.contains_key("addr64")
                && compiled.subtables.contains_key("Reg64")
                && compiled.subtables.contains_key("cc"),
        }
    }

    pub(super) fn uses_shared_token_cursor(self) -> bool {
        self.shared_token_cursor
    }
}

pub(super) fn ensure_token_fields<'a>(
    ctx: &CompiledInstructionContext<'_>,
    cached_token_fields: &'a mut Option<TokenFieldBundle>,
) -> Result<&'a TokenFieldBundle> {
    if cached_token_fields.is_none() {
        *cached_token_fields = Some(decode_shared_token_fields(
            ctx,
            ctx.cursor + opcode_len_from_context(ctx)?,
        )?);
    }
    cached_token_fields
        .as_ref()
        .ok_or_else(|| anyhow!("missing cached token fields"))
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

pub(super) fn decode_shared_token_fields(
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

pub(super) fn constructor_has_shared_token_operand(
    constructor: &CompiledExecutableConstructor,
) -> bool {
    constructor
        .constructor_template
        .handles
        .iter()
        .any(|handle| {
            matches!(
                &handle.spec,
                CompiledOperandSpec::SubtableEvaluation { table_name }
                    if shared_token_cursor_policy_shared_token_subtable(table_name)
            )
        })
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
        CompiledOperandSpec::TokenFieldExtraction { .. }
        | CompiledOperandSpec::SlaValueMap { .. }
        | CompiledOperandSpec::Immediate { .. }
        | CompiledOperandSpec::Relative { .. } => true,
        CompiledOperandSpec::SubtableEvaluation { table_name }
            if CompiledTokenCursorPolicy::for_frontend(compiled).uses_shared_token_cursor() =>
        {
            !shared_token_cursor_policy_zero_width_subtable(table_name)
        }
        CompiledOperandSpec::SubtableEvaluation { table_name } => {
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

pub(super) fn shared_token_cursor_policy_sib_token_subtable(table_name: &str) -> bool {
    matches!(table_name, "Base" | "Base64" | "Index" | "Index64" | "ss")
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

pub(super) fn shared_token_cursor_policy_opcode_row_modrm_subtable(table_name: &str) -> bool {
    matches!(
        table_name,
        "Rmr8" | "Rmr16" | "Rmr32" | "Rmr64" | "CRmr8" | "CRmr16" | "CRmr32"
    )
}

pub(super) fn shared_token_cursor_policy_sla_field_advances_cursor(table_name: &str) -> bool {
    !shared_token_cursor_policy_modrm_token_subtable(table_name)
        && !shared_token_cursor_policy_sib_token_subtable(table_name)
        && !shared_token_cursor_policy_opcode_token_subtable(table_name)
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

pub(super) fn shared_token_cursor_policy_modrm_operand_wrapper_subtable(table_name: &str) -> bool {
    matches!(table_name, "rm8" | "rm16" | "rm32" | "rm64")
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
