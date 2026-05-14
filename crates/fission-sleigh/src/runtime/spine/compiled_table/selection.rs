use super::*;

pub(super) fn candidate_selections<'a>(
    compiled: &'a CompiledFrontend,
    strategy: &RuntimeDecodeStrategy<'a>,
    ctx: &CompiledInstructionContext<'_>,
    address: u64,
) -> Result<Vec<RuntimeSelection<'a>>> {
    let instruction_table = compiled.subtables.get("instruction").ok_or_else(|| {
        RuntimeSleighError::UnsupportedPcodeTemplate {
            language: compiled.entry_id.clone(),
            reason: "selection_no_instruction_root".to_string(),
        }
    })?;
    let primary = if let Some(native) = strategy.native_for_table(compiled, "instruction", ctx) {
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
        let (subtable_id, constructor_slot) = constructor_sla_selection_identity(
            instruction_table,
            constructor,
            constructor_index,
            "instruction",
        )?;
        RuntimeSelection {
            constructor,
            constructor_index,
            subtable_id,
            constructor_id: constructor.constructor_id,
            constructor_slot,
            trace: spine::RuntimeMatchTrace {
                root_bucket: "native".to_string(),
                probes: Vec::new(),
                leaf_constructor_indexes: vec![constructor_index],
                matched_leaf_pattern: None,
            },
        }
    } else {
        select_constructor(compiled, "instruction", ctx)?.ok_or_else(|| {
            RuntimeSleighError::DecodeNoMatch {
                language: compiled.entry_id.clone(),
                address,
            }
        })?
    };

    Ok(vec![primary])
}

pub(super) fn constructor_sla_selection_identity(
    subtable: &CompiledSubtableDefinition,
    constructor: &CompiledExecutableConstructor,
    constructor_index: usize,
    table_name: &str,
) -> Result<(u32, usize)> {
    if let Some(identity) = &constructor.sla_identity {
        return Ok((identity.subtable_id, identity.constructor_slot));
    }
    if subtable.sla_subtable_id == 0 {
        return Ok((0, constructor_index));
    }
    bail!("selected SLA constructor in {table_name} is missing SLA identity")
}

pub(super) fn select_constructor<'a>(
    compiled: &'a CompiledFrontend,
    table_name: &str,
    ctx: &CompiledInstructionContext<'_>,
) -> Result<Option<RuntimeSelection<'a>>> {
    let subtable = compiled
        .subtables
        .get(table_name)
        .ok_or_else(|| anyhow!("missing subtable {table_name} for constructor selection"))?;
    spine::select_constructor(
        compiled,
        [(
            table_name.to_string(),
            subtable.decision_tree.root_node_index,
        )],
        || CompiledDecisionProbeEvaluator::new(ctx),
        |constructor| constructor_matches(ctx, constructor),
    )
}

pub(super) struct CompiledDecisionProbeEvaluator<'a, 'b> {
    ctx: &'a CompiledInstructionContext<'b>,
}

impl<'a, 'b> CompiledDecisionProbeEvaluator<'a, 'b> {
    fn new(ctx: &'a CompiledInstructionContext<'b>) -> Self {
        Self { ctx }
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
                let absolute = self
                    .ctx
                    .cursor
                    .checked_add(usize::from(offset))
                    .ok_or_else(|| anyhow!("instruction byte probe cursor overflow"))?;
                let byte =
                    *self.ctx.bytes.get(absolute).ok_or_else(|| {
                        anyhow!("missing instruction byte probe at offset {offset}")
                    })?;
                vec![(byte & mask) >> shift]
            }
            CompiledDecisionProbe::ContextBitSlice {
                offset,
                mask,
                shift,
            } => possible_context_probe_values(
                self.ctx.context_register,
                self.ctx.context_known_mask,
                u32::from(offset),
                8,
            )?
            .into_iter()
            .map(|value| {
                decision_probe_value_u8((value & u64::from(mask)) >> shift, "context bit slice")
            })
            .collect::<Result<Vec<_>>>()?,
            CompiledDecisionProbe::SlaInstructionBits {
                start_bit,
                bit_size,
            } => {
                ensure_u8_decision_probe_width(bit_size, "SLA instruction bits")?;
                let byte_offset = start_bit / 8;
                let bit_offset = start_bit % 8;
                let byte_cnt = (bit_offset + bit_size + 7) / 8;
                ensure_decision_probe_byte_width(byte_cnt, "SLA instruction bits")?;
                let mut word = 0u64;
                let byte_offset = usize::try_from(byte_offset)
                    .map_err(|_| anyhow!("instruction bit probe byte offset exceeds usize"))?;
                let start = self
                    .ctx
                    .cursor
                    .checked_add(byte_offset)
                    .ok_or_else(|| anyhow!("instruction bit probe cursor overflow"))?;
                let byte_cnt_u32 = byte_cnt;
                let byte_count = usize::try_from(byte_cnt_u32)
                    .map_err(|_| anyhow!("instruction bit probe byte count exceeds usize"))?;
                for i in 0..byte_count {
                    let absolute = start
                        .checked_add(i)
                        .ok_or_else(|| anyhow!("instruction bit probe range overflow"))?;
                    let byte = self.ctx.bytes.get(absolute).copied().ok_or_else(|| {
                        anyhow!("instruction bit read out of range at bit {start_bit}")
                    })?;
                    word = append_decision_probe_byte(word, byte)?;
                }
                let shift = (8 * byte_cnt_u32)
                    .checked_sub(bit_offset)
                    .and_then(|value| value.checked_sub(bit_size))
                    .ok_or_else(|| anyhow!("instruction bit probe shift underflow"))?;
                let value = (word >> shift) & ((1u64 << bit_size) - 1);
                vec![decision_probe_value_u8(value, "SLA instruction bits")?]
            }
            CompiledDecisionProbe::SlaContextBits {
                start_bit,
                bit_size,
            } => {
                ensure_u8_decision_probe_width(bit_size, "SLA context bits")?;
                vec![decision_probe_value_u8(
                    u64::from(packed_context_bits(
                        self.ctx.context_register,
                        start_bit,
                        bit_size,
                    )?),
                    "SLA context bits",
                )?]
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
            .checked_add_signed(
                isize::try_from(offset)
                    .map_err(|_| anyhow!("instruction byte read offset exceeds isize"))?,
            )
            .ok_or_else(|| anyhow!("instruction byte read underflow at offset {offset}"))?;
        let mut word = 0u32;
        let size = usize::try_from(size)
            .map_err(|_| anyhow!("instruction byte read size exceeds usize"))?;
        for i in 0..size {
            let absolute = start
                .checked_add(i)
                .ok_or_else(|| anyhow!("instruction byte read range overflow"))?;
            let byte =
                self.ctx.bytes.get(absolute).copied().ok_or_else(|| {
                    anyhow!("instruction byte read out of range at offset {offset}")
                })?;
            word <<= 8;
            word |= u32::from(byte);
        }
        Ok(word)
    }

    fn context_bytes(&self, offset: i32, size: u32) -> Result<u32> {
        if offset < 0 {
            bail!("context byte read underflow at offset {offset}");
        }
        let offset =
            u32::try_from(offset).map_err(|_| anyhow!("context byte offset exceeds u32"))?;
        packed_context_bytes(self.ctx.context_register, offset, size)
    }
}

fn ensure_u8_decision_probe_width(bit_size: u32, role: &str) -> Result<()> {
    if bit_size > 8 {
        bail!("{role} decision probe width {bit_size} exceeds u8 branch key width");
    }
    Ok(())
}

fn ensure_decision_probe_byte_width(byte_cnt: u32, role: &str) -> Result<()> {
    if byte_cnt > 8 {
        bail!("{role} decision probe byte width {byte_cnt} exceeds u64");
    }
    Ok(())
}

fn append_decision_probe_byte(value: u64, byte: u8) -> Result<u64> {
    let shifted = value
        .checked_shl(8)
        .ok_or_else(|| anyhow!("decision probe byte accumulation exceeds u64 width"))?;
    Ok(shifted | u64::from(byte))
}

fn decision_probe_value_u8(value: u64, role: &str) -> Result<u8> {
    u8::try_from(value).map_err(|_| anyhow!("{role} decision probe value {value} exceeds u8"))
}

fn shifted_instruction_constraint_byte(byte: u8, index: usize) -> Result<u64> {
    let index = u32::try_from(index)
        .map_err(|_| anyhow!("instruction constraint byte index exceeds u32"))?;
    let shift = index
        .checked_mul(8)
        .ok_or_else(|| anyhow!("instruction constraint byte shift overflowed"))?;
    u64::from(byte)
        .checked_shl(shift)
        .ok_or_else(|| anyhow!("instruction constraint byte shift {shift} exceeds u64 width"))
}

fn shifted_context_constraint_value(context_register: u64, offset: u32) -> Result<u64> {
    context_register
        .checked_shr(offset)
        .ok_or_else(|| anyhow!("context constraint shift {offset} exceeds u64 width"))
}

pub(super) fn possible_context_probe_values(
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
    let known = u64::from(packed_context_bits(
        context_known_mask,
        start_bit,
        bit_size,
    )?);
    let known_value =
        u64::from(packed_context_bits(context_register, start_bit, bit_size)?) & known;
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

pub(super) fn unsupported_constructor_error(
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

pub(super) fn constructor_matches(
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

    match &constructor.matcher {
        CompiledPatternMatcher::ExactBytes(bytes) => {
            let end = ctx
                .cursor
                .checked_add(bytes.len())
                .ok_or_else(|| anyhow!("exact opcode range overflow"))?;
            if ctx.bytes.get(ctx.cursor..end) != Some(bytes.as_slice()) {
                bail!("exact opcode mismatch");
            }
        }
        CompiledPatternMatcher::RowCc { prefix, row } => {
            let prefix_end = ctx
                .cursor
                .checked_add(prefix.len())
                .ok_or_else(|| anyhow!("row prefix range overflow"))?;
            if ctx.bytes.get(ctx.cursor..prefix_end) != Some(prefix.as_slice()) {
                bail!("prefix mismatch");
            }
            let opcode = *ctx
                .bytes
                .get(prefix_end)
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
                    crate::compiler::PatternConstraint::Instruction {
                        offset,
                        mask,
                        value,
                    } => {
                        let mut inst_val = 0u64;
                        let required_bytes = if *mask == 0 {
                            0usize
                        } else {
                            let leading = usize::try_from(mask.leading_zeros())
                                .map_err(|_| anyhow!("mask leading-zero count exceeds usize"))?;
                            (64usize - leading).div_ceil(8)
                        };
                        let offset = usize::try_from(*offset)
                            .map_err(|_| anyhow!("instruction constraint offset overflow"))?;
                        let start = ctx
                            .cursor
                            .checked_add(offset)
                            .ok_or_else(|| anyhow!("instruction constraint cursor overflow"))?;
                        for i in 0..required_bytes {
                            let absolute = start
                                .checked_add(i)
                                .ok_or_else(|| anyhow!("instruction constraint range overflow"))?;
                            let byte = ctx.bytes.get(absolute).copied().ok_or_else(|| {
                                anyhow!("instruction bit constraint byte out of range")
                            })?;
                            inst_val |= shifted_instruction_constraint_byte(byte, i)?;
                        }
                        if (inst_val & mask) != *value {
                            bail!("instruction bit constraint mismatch");
                        }
                    }
                    crate::compiler::PatternConstraint::Context {
                        offset,
                        mask,
                        value,
                    } => {
                        let val =
                            shifted_context_constraint_value(ctx.context_register, *offset)? & mask;
                        if val != *value {
                            bail!("context bit constraint mismatch");
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

pub(super) fn disjoint_pattern_instruction_byte_len(
    pattern: &CompiledDisjointPattern,
) -> Result<usize> {
    match pattern {
        CompiledDisjointPattern::Instruction(block) => pattern_block_byte_len(block),
        CompiledDisjointPattern::Context(_) => Ok(0),
        CompiledDisjointPattern::Combine { instruction, .. } => pattern_block_byte_len(instruction),
        CompiledDisjointPattern::Or(patterns) => Ok(patterns
            .iter()
            .map(disjoint_pattern_instruction_byte_len)
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .max()
            .ok_or_else(|| anyhow!("empty SLA OR pattern has no instruction length"))?),
    }
}

pub(super) fn pattern_block_byte_len(block: &CompiledPatternBlock) -> Result<usize> {
    if block.nonzero_size <= 0 {
        return Ok(0);
    }
    let len = block
        .offset
        .max(0)
        .checked_add(block.nonzero_size)
        .ok_or_else(|| anyhow!("SLA pattern byte length overflow"))?;
    usize::try_from(len).map_err(|_| anyhow!("SLA pattern byte length {len} does not fit usize"))
}

#[cfg(test)]
mod tests {
    use super::{
        append_decision_probe_byte, decision_probe_value_u8, ensure_decision_probe_byte_width,
        ensure_u8_decision_probe_width, shifted_context_constraint_value,
        shifted_instruction_constraint_byte,
    };

    #[test]
    fn decision_probe_values_fail_closed_above_u8_width() {
        assert!(ensure_u8_decision_probe_width(8, "test").is_ok());
        assert!(ensure_u8_decision_probe_width(9, "test").is_err());
        assert_eq!(decision_probe_value_u8(255, "test").unwrap(), u8::MAX);
        assert!(decision_probe_value_u8(256, "test").is_err());
    }

    #[test]
    fn decision_probe_byte_accumulation_fails_closed_above_u64_width() {
        assert!(ensure_decision_probe_byte_width(8, "test").is_ok());
        assert!(ensure_decision_probe_byte_width(9, "test").is_err());
        assert_eq!(append_decision_probe_byte(0x12, 0x34).unwrap(), 0x1234);
    }

    #[test]
    fn bit_constraint_shifts_fail_closed_above_u64_width() {
        assert_eq!(
            shifted_instruction_constraint_byte(0x12, 7).unwrap(),
            0x1200_0000_0000_0000
        );
        assert!(shifted_instruction_constraint_byte(0x12, 8).is_err());

        assert_eq!(
            shifted_context_constraint_value(0x8000_0000_0000_0000, 63).unwrap(),
            1
        );
        assert!(shifted_context_constraint_value(1, 64).is_err());
    }
}
