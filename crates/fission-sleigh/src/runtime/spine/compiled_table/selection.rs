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
        select_constructor(compiled, "instruction", ctx).ok_or_else(|| {
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
) -> Option<RuntimeSelection<'a>> {
    let subtable = compiled.subtables.get(table_name)?;
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
            } => possible_context_probe_values(
                self.ctx.context_register,
                self.ctx.context_known_mask,
                u32::from(offset),
                8,
            )?
            .into_iter()
            .map(|value| ((value & u64::from(mask)) >> shift) as u8)
            .collect(),
            CompiledDecisionProbe::SlaInstructionBits {
                start_bit,
                bit_size,
            } => {
                let byte_offset = start_bit / 8;
                let bit_offset = start_bit % 8;
                let byte_cnt = (bit_offset + bit_size + 7) / 8;
                let mut word = 0u64;
                for i in 0..byte_cnt {
                    let absolute = self.ctx.cursor + byte_offset as usize + i as usize;
                    let byte = self.ctx.bytes.get(absolute).copied().ok_or_else(|| {
                        anyhow!("instruction bit read out of range at bit {start_bit}")
                    })?;
                    word <<= 8;
                    word |= u64::from(byte);
                }
                let shift = (8 * byte_cnt) - bit_offset - bit_size;
                vec![((word >> shift) & ((1u64 << bit_size) - 1)) as u8]
            }
            CompiledDecisionProbe::SlaContextBits {
                start_bit,
                bit_size,
            } => {
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
        let mut word = 0u32;
        for i in 0..size as usize {
            let byte =
                self.ctx.bytes.get(start + i).copied().ok_or_else(|| {
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
        packed_context_bytes(self.ctx.context_register, offset as u32, size)
    }
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
                    crate::compiler::PatternConstraint::Instruction {
                        offset,
                        mask,
                        value,
                    } => {
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
                    crate::compiler::PatternConstraint::Context {
                        offset,
                        mask,
                        value,
                    } => {
                        let val = (ctx.context_register >> offset) & mask;
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
