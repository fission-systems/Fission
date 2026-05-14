use super::*;

pub(super) fn bind_instruction<'a>(
    compiled: &'a CompiledFrontend,
    strategy: RuntimeDecodeStrategy<'a>,
    ctx: &CompiledInstructionContext<'_>,
    selection: RuntimeSelection<'a>,
) -> Result<RuntimeConstructState> {
    constructor_matches(ctx, selection.constructor)?;
    CompiledParserWalker::new(compiled, strategy, ctx, selection)?.walk()
}

pub(super) struct CompiledParserWalker<'a, 'b> {
    compiled: &'a CompiledFrontend,
    strategy: RuntimeDecodeStrategy<'a>,
    ctx: &'a CompiledInstructionContext<'b>,
    selection: RuntimeSelection<'a>,
    minimum_length: usize,
    context_register: u64,
    context_known_mask: u64,
    cursor: usize,
    handles: Vec<Option<RuntimeHandle>>,
    operand_absolute_offsets: Vec<Option<usize>>,
    operand_relative_lengths: Vec<Option<usize>>,
    handle_reference_bitmap: Vec<bool>,
    walker: spine::RuntimeParserWalker,
}

pub(super) struct OperandBinding {
    debug_value: Option<BoundOperand>,
    subtable_state: Option<RuntimeConstructState>,
    fixed: Option<RuntimeFixedHandle>,
    requires_fixed: bool,
}

impl OperandBinding {
    fn plain(value: BoundOperand) -> Self {
        Self {
            debug_value: Some(value),
            subtable_state: None,
            fixed: None,
            requires_fixed: true,
        }
    }

    fn with_fixed(value: BoundOperand, fixed: RuntimeFixedHandle) -> Self {
        Self {
            debug_value: Some(value),
            subtable_state: None,
            fixed: Some(fixed),
            requires_fixed: true,
        }
    }

    fn guard_only(subtable_state: RuntimeConstructState) -> Self {
        Self {
            debug_value: None,
            subtable_state: Some(subtable_state),
            fixed: None,
            requires_fixed: false,
        }
    }
}

fn instruction_terminal_pattern_len(selection: &RuntimeSelection<'_>) -> Result<usize> {
    let pattern = selection
        .trace
        .matched_leaf_pattern
        .as_ref()
        .ok_or_else(|| anyhow!("instruction selection missing terminal SLA pattern"))?;
    let len = disjoint_pattern_instruction_byte_len(pattern)?;
    if len == 0 {
        bail!("instruction terminal SLA pattern has zero instruction byte length");
    }
    Ok(len)
}

fn spec_derived_instruction_opcode_len(selection: &RuntimeSelection<'_>) -> Result<usize> {
    if let Some(len) = opcode_len_from_matcher(&selection.constructor.matcher)? {
        return Ok(len);
    }
    let len = usize::try_from(selection.constructor.minimum_length)
        .map_err(|_| anyhow!("constructor minimum length exceeds usize"))?;
    if len == 0 {
        bail!("instruction constructor has neither matcher opcode span nor minimum length");
    }
    Ok(len)
}

fn operand_spec_offsets(spec: &CompiledOperandSpec) -> Option<(i32, i32)> {
    match spec {
        CompiledOperandSpec::SlaTokenField {
            reloffset,
            offsetbase,
            ..
        }
        | CompiledOperandSpec::SlaVarnodeList {
            reloffset,
            offsetbase,
            ..
        }
        | CompiledOperandSpec::SlaVarnodeListExpression {
            reloffset,
            offsetbase,
            ..
        }
        | CompiledOperandSpec::SlaValueMap {
            reloffset,
            offsetbase,
            ..
        }
        | CompiledOperandSpec::SlaValueMapExpression {
            reloffset,
            offsetbase,
            ..
        }
        | CompiledOperandSpec::SlaPatternExpression {
            reloffset,
            offsetbase,
            ..
        }
        | CompiledOperandSpec::SubtableEvaluation {
            reloffset,
            offsetbase,
            ..
        } => Some((*reloffset, *offsetbase)),
        _ => None,
    }
}

fn required_const_tpl_u32(value: Option<u64>, role: &str) -> Result<u32> {
    let value = value.ok_or_else(|| anyhow!("{role} is missing"))?;
    u32::try_from(value).map_err(|_| anyhow!("{role} value {value} exceeds u32"))
}

#[cfg(test)]
mod construct_state_offset_tests {
    use super::{
        checked_pattern_add, checked_pattern_div, checked_pattern_left_shift, checked_pattern_mul,
        checked_pattern_negate, checked_pattern_right_shift, checked_pattern_sub,
        checked_relative_offset, context_change_expr_word, context_change_mask_word,
        pattern_context_bits_i64, shifted_context_change_word,
    };
    use crate::compiler::{compile_x86_64_frontend, discovery};

    #[test]
    fn opcode_register_subtable_reads_from_sla_operand_offset() {
        if !discovery::ghidra_packaged_sla_available() {
            eprintln!("skip: packaged Ghidra .sla not available for x86-64 push decode");
            return;
        }

        let compiled = compile_x86_64_frontend().expect("compile x86-64 frontend");
        let decoded = crate::runtime::spine::compiled_table::decode_instruction(
            &compiled,
            None,
            &[0x57],
            0x1000,
        )
        .expect("decode push rdi");

        assert_eq!(decoded.length, 1);
        assert_eq!(decoded.mnemonic, "push");
        assert_eq!(decoded.operands_text, "RDI");
    }

    #[test]
    fn shared_token_operands_do_not_require_legacy_cursor_policy() {
        if !discovery::ghidra_packaged_sla_available() {
            eprintln!("skip: packaged Ghidra .sla not available for x86-64 shared token decode");
            return;
        }

        let compiled = compile_x86_64_frontend().expect("compile x86-64 frontend");
        for bytes in [&[0x57][..], &[0x48, 0x89, 0x5c, 0x24, 0x08][..]] {
            let (_ops, length, details) =
                crate::runtime::spine::compiled_table::decode_and_lift_with_details(
                    &compiled, None, bytes, 0x1000,
                )
                .expect("decode/lift shared-token sample");

            assert_eq!(length as usize, bytes.len());
            assert_eq!(
                details.template_source,
                Some(crate::compiler::CompiledTemplateSource::SpecDerived)
            );
        }
    }

    #[test]
    fn context_change_expr_word_fails_closed_on_out_of_range_values() {
        assert_eq!(context_change_expr_word(0xffff_ffff).unwrap(), u32::MAX);
        assert_eq!(context_change_expr_word(-1).unwrap(), u32::MAX);
        assert_eq!(context_change_expr_word(-2).unwrap(), 0xffff_fffe);
        assert!(context_change_expr_word(i64::from(i32::MIN) - 1).is_err());
        assert!(context_change_expr_word(i64::from(u32::MAX) + 1).is_err());
    }

    #[test]
    fn context_change_mask_word_fails_closed_above_u32() {
        assert_eq!(context_change_mask_word(0xffff_ffff).unwrap(), u32::MAX);
        assert!(context_change_mask_word(u64::from(u32::MAX) + 1).is_err());
    }

    #[test]
    fn context_change_shift_fails_closed_above_word_width() {
        assert_eq!(shifted_context_change_word(1, 31).unwrap(), 1u32 << 31);
        assert_eq!(shifted_context_change_word(0x8000_0000, -31).unwrap(), 1);
        assert!(shifted_context_change_word(1, 32).is_err());
        assert!(shifted_context_change_word(1, -32).is_err());
    }

    #[test]
    fn pattern_expression_arithmetic_fails_closed_on_overflow() {
        assert_eq!(checked_pattern_add(40, 2).unwrap(), 42);
        assert_eq!(checked_pattern_sub(40, 2).unwrap(), 38);
        assert_eq!(checked_pattern_mul(6, 7).unwrap(), 42);
        assert_eq!(checked_pattern_div(84, 2).unwrap(), 42);
        assert_eq!(checked_pattern_negate(42).unwrap(), -42);

        assert!(checked_pattern_add(i64::MAX, 1).is_err());
        assert!(checked_pattern_sub(i64::MIN, 1).is_err());
        assert!(checked_pattern_mul(i64::MAX, 2).is_err());
        assert!(checked_pattern_div(1, 0).is_err());
        assert!(checked_pattern_div(i64::MIN, -1).is_err());
        assert!(checked_pattern_negate(i64::MIN).is_err());
    }

    #[test]
    fn pattern_expression_shifts_fail_closed_on_invalid_amounts() {
        assert_eq!(checked_pattern_left_shift(1, 63).unwrap(), i64::MIN);
        assert_eq!(checked_pattern_right_shift(-1, 63).unwrap(), 1);

        assert!(checked_pattern_left_shift(1, -1).is_err());
        assert!(checked_pattern_left_shift(1, 64).is_err());
        assert!(checked_pattern_right_shift(1, -1).is_err());
        assert!(checked_pattern_right_shift(1, 64).is_err());
    }

    #[test]
    fn relative_offsets_fail_closed_outside_usize_window() {
        assert_eq!(checked_relative_offset(10, -3, "test").unwrap(), 7);
        assert_eq!(checked_relative_offset(10, 3, "test").unwrap(), 13);
        assert!(checked_relative_offset(0, -1, "test").is_err());
        assert!(checked_relative_offset(usize::MAX, 1, "test").is_err());
    }

    #[test]
    fn pattern_context_bits_preserve_or_sign_extend_bit_patterns() {
        assert_eq!(pattern_context_bits_i64(0xff, 8, false).unwrap(), 0xff);
        assert_eq!(pattern_context_bits_i64(0xff, 8, true).unwrap(), -1);
        assert_eq!(pattern_context_bits_i64(0x80, 8, true).unwrap(), -128);
        assert_eq!(
            pattern_context_bits_i64(0x8000_0000_0000_0000, 64, false).unwrap(),
            i64::MIN
        );
    }
}

impl<'a, 'b> CompiledParserWalker<'a, 'b> {
    fn new(
        compiled: &'a CompiledFrontend,
        strategy: RuntimeDecodeStrategy<'a>,
        ctx: &'a CompiledInstructionContext<'b>,
        selection: RuntimeSelection<'a>,
    ) -> Result<Self> {
        let replace_current_wrapper = constructor_replaces_current(selection.constructor);
        let opcode_len = if replace_current_wrapper {
            0
        } else if selection.constructor.constructor_template.template_source
            == CompiledTemplateSource::SpecDerived
        {
            if selection.trace.root_bucket == "instruction"
                && constructor_consumes_sequential_operand_bytes(compiled, selection.constructor)?
            {
                instruction_terminal_pattern_len(&selection)?
            } else if selection.trace.root_bucket == "instruction" {
                // Some instruction-level constructors encode address bytes directly in the
                // terminal matcher instead of through a descendant operand subtable. If the
                // matcher is context-only, the SLA-derived constructor minimum length is the
                // typed cursor advance before binding displacement/address operands.
                spec_derived_instruction_opcode_len(&selection)?
            } else {
                0
            }
        } else {
            opcode_len_from_matcher(&selection.constructor.matcher)?.ok_or_else(|| {
                anyhow!("native matcher has no instruction byte span for opcode length")
            })?
        };
        let minimum_length = selection.constructor.minimum_length as usize;
        let handles = vec![None; selection.constructor.constructor_template.handles.len()];
        let operand_absolute_offsets =
            vec![None; selection.constructor.constructor_template.handles.len()];
        let operand_relative_lengths =
            vec![None; selection.constructor.constructor_template.handles.len()];
        let handle_reference_bitmap = constructor_template_handle_reference_bitmap(
            &selection.constructor.constructor_template,
        )?;
        Ok(Self {
            compiled,
            strategy,
            ctx,
            selection,
            minimum_length,
            context_register: ctx.context_register,
            context_known_mask: ctx.context_known_mask,
            cursor: ctx
                .cursor
                .checked_add(opcode_len)
                .ok_or_else(|| anyhow!("constructor cursor overflowed"))?,
            handles,
            operand_absolute_offsets,
            operand_relative_lengths,
            handle_reference_bitmap,
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
        for step in decode_steps {
            match step {
                CompiledOperandDecodeStep::DecodeOperand { operand_index } => {
                    self.decode_operand(operand_index)?;
                }
                CompiledOperandDecodeStep::DescendSubtable {
                    table_name,
                    replace_current,
                } => {
                    // Mirror Ghidra's operand positioning from the handle template:
                    // ParserWalker uses getOffset(offsetbase) + reloffset before
                    // descending into a subtable.
                    let mut subtable_offset = (None, None, None);
                    for h in &self.selection.constructor.constructor_template.handles {
                        if let CompiledOperandSpec::SubtableEvaluation {
                            table_name: ref tn,
                            reloffset,
                            offsetbase,
                        } = h.spec
                        {
                            if tn.as_str() == table_name.as_str() {
                                subtable_offset = (
                                    Some(reloffset),
                                    Some(offsetbase),
                                    Some(self.operand_absolute_offset(&h.spec)?),
                                );
                                break;
                            }
                        }
                    }
                    let (reloffset, offsetbase, operand_absolute_offset) = subtable_offset;
                    let sub_state = self.decode_subtable(
                        &table_name,
                        reloffset,
                        offsetbase,
                        operand_absolute_offset,
                    )?;
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
            .filter_map(|handle| handle.debug_value.clone())
            .collect::<Vec<_>>();

        let length = self
            .cursor
            .max(self.constructor_minimum_end()?)
            .max(self.max_operand_end()?);
        let absolute_offset = self.ctx.cursor;
        let relative_length = length
            .checked_sub(absolute_offset)
            .ok_or_else(|| anyhow!("constructor length resolved before instruction start"))?;

        Ok(RuntimeConstructState {
            subtable_id: self.selection.subtable_id,
            constructor_id: self.selection.constructor_id,
            constructor_slot: self.selection.constructor_slot,
            mnemonic: self.selection.constructor.mnemonic.clone(),
            construct_tpl_kind: self.selection.constructor.construct_tpl_kind,
            constructor_template: self.selection.constructor.constructor_template.clone(),
            named_templates: self.selection.constructor.named_templates.clone(),
            context_commits: self.selection.constructor.context_commits.clone(),
            display_template: self.selection.constructor.display_template.clone(),
            display_operands: self.selection.constructor.display_operands.clone(),
            construct_nodes: self.walker.into_nodes(),
            handles,
            exported_handle,
            operands,
            context_register: self.context_register,
            context_known_mask: self.context_known_mask,
            absolute_offset,
            relative_length,
            length,
            match_trace: self.selection.trace,
        })
    }

    fn materialize_export_handle(
        &mut self,
        handles: &[RuntimeHandle],
    ) -> Result<Option<RuntimeHandle>> {
        let Some(export_tpl) = self
            .selection
            .constructor
            .constructor_template
            .result
            .clone()
        else {
            return Ok(None);
        };
        let fixed = self.fixed_handle_from_handle_tpl(&export_tpl, handles)?;
        let value = self
            .display_operand_from_exported_display_template(handles)
            .transpose()?;
        Ok(Some(RuntimeHandle {
            operand_index: usize::MAX,
            spec: CompiledOperandSpec::SubtableEvaluation {
                table_name: self.selection.constructor.source.clone(),
                reloffset: 0,
                offsetbase: -1,
            },
            fixed,
            debug_value: value,
            subtable_state: None,
        }))
    }

    fn display_operand_from_exported_display_template(
        &self,
        handles: &[RuntimeHandle],
    ) -> Option<Result<BoundOperand>> {
        let template = &self.selection.constructor.display_template;
        let mut operand_indices = if let Some(flowthru_index) = template.flowthru_operand_index {
            vec![flowthru_index]
        } else {
            template
                .pieces
                .iter()
                .filter_map(|piece| match piece {
                    crate::compiler::CompiledDisplayPiece::OperandRef(index) => Some(*index),
                    crate::compiler::CompiledDisplayPiece::Literal(_) => None,
                })
                .collect::<Vec<_>>()
        };
        operand_indices.sort_unstable();
        operand_indices.dedup();
        let mut referenced_values = operand_indices
            .into_iter()
            .filter_map(|index| handles.get(index))
            .filter_map(|handle| handle.debug_value.clone())
            .collect::<Vec<_>>();
        if referenced_values.len() == 1 {
            Some(Ok(referenced_values.remove(0)))
        } else {
            None
        }
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
            .transpose()
            .and_then(|value| required_const_tpl_u32(value, "export HandleTpl size"))?;
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
            .ok_or_else(|| anyhow!("export HandleTpl ptr_offset is missing"))?;
        let offset_size = handle_tpl
            .ptr_size
            .as_ref()
            .map(|value| self.resolve_export_const_tpl(value, handles))
            .transpose()
            .and_then(|value| required_const_tpl_u32(value, "export HandleTpl ptr_size"))?;
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
            .ok_or_else(|| anyhow!("export HandleTpl temp_offset is missing"))?;
        // Ghidra: HandleTpl.fix() sets offset_space = null when the pointer space is
        // the constant address space (TYPE_CONSTANT). This distinguishes static handles
        // (register/RAM at a constant offset) from truly dynamic handles (pointer in unique).
        // Without this, register operands with const ptr_space incorrectly appear dynamic.
        let is_const_space = offset_space
            .as_ref()
            .is_some_and(|s| s.index == 0 || s.name == "const");
        let (offset_space, offset_offset, offset_size, temp_space, temp_offset) = if is_const_space
        {
            // Convert constant offset_space to static handle: multiply offset by addressable unit
            // size. For RAM (word_size=1) this is a no-op; for other spaces it scales correctly.
            let space_ref = space
                .as_ref()
                .ok_or_else(|| anyhow!("static HandleTpl missing target space"))?;
            let target_space = self
                .compiled
                .sla_spaces
                .get(&space_ref.index)
                .ok_or_else(|| anyhow!("static HandleTpl target space missing SLA metadata"))?;
            let addr_unit = if target_space.index == 0
                || target_space.name == "const"
                || target_space.name == "unique"
            {
                1
            } else if target_space.word_size == 0 {
                bail!(
                    "static HandleTpl target space {} has word_size=0",
                    target_space.name
                );
            } else {
                u64::from(target_space.word_size)
            };
            (
                None,
                offset_offset
                    .checked_mul(addr_unit)
                    .ok_or_else(|| anyhow!("static HandleTpl address-unit scaling overflowed"))?,
                0u32,
                None,
                0u64,
            )
        } else {
            (
                offset_space,
                offset_offset,
                offset_size,
                temp_space,
                temp_offset,
            )
        };
        let fixable = space.is_some()
            && (offset_space.is_none() || (offset_size != 0 && temp_space.is_some()));
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
                if let Some(space_ref) = self.compiled.sla_spaces.get(&index) {
                    return Ok(space_ref.clone());
                }
                if index == 0 {
                    return Ok(CompiledSpaceRef {
                        name: "const".to_string(),
                        index: 0,
                        word_size: 0,
                        addr_size: 0,
                    });
                }
                bail!("export SpaceTpl references unknown SLA space id {index}")
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
            CompiledConstTpl::Integer { value, .. } if *value >= 0 => u64::try_from(*value)
                .map_err(|_| anyhow!("positive export integer ConstTpl exceeds u64")),
            CompiledConstTpl::Integer { value, .. } => Ok(i64_to_u64_bits(*value)),
            CompiledConstTpl::SpaceId(space) => Ok(space.index),
            CompiledConstTpl::Handle {
                handle_index,
                selector,
                plus,
            } => {
                let handle_index = checked_handle_index(*handle_index, "export")?;
                let handle = handles
                    .get(handle_index)
                    .ok_or_else(|| anyhow!("export handle {} is missing", handle_index))?;
                if matches!(selector, CompiledHandleSelector::OffsetPlus) {
                    let plus =
                        plus.ok_or_else(|| anyhow!("export offset_plus handle is missing plus"))?;
                    return resolve_offset_plus_pub(handle, plus);
                }
                let value = match selector {
                    CompiledHandleSelector::Space => handle
                        .fixed
                        .space
                        .as_ref()
                        .map(|space| space.index)
                        .ok_or_else(|| anyhow!("export fixed handle missing space"))?,
                    CompiledHandleSelector::Offset => handle.fixed.offset_offset,
                    CompiledHandleSelector::Size => u64::from(handle.fixed.size),
                    CompiledHandleSelector::OffsetPlus => unreachable!(),
                };
                reject_non_offset_handle_plus(*plus, "export")?;
                Ok(value)
            }
            CompiledConstTpl::InstStart => Ok(self.ctx.address),
            CompiledConstTpl::InstNext => {
                let minimum_length =
                    checked_usize_to_u64(self.minimum_length, "export InstNext minimum length")?;
                self.ctx
                    .address
                    .checked_add(minimum_length)
                    .ok_or_else(|| anyhow!("export InstNext address overflowed"))
            }
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
        let operand_absolute_offset = self.operand_absolute_offset(&template.spec)?;
        let binding = self.bind_operand(&template, operand_absolute_offset)?;
        let handle_index = operand_index;
        let operand_relative_length = match binding.subtable_state.as_ref() {
            Some(state) => state.relative_length,
            None => usize::try_from(template.minimum_length)
                .map_err(|_| anyhow!("operand minimum length exceeds usize"))?,
        };
        self.walker.record_operand_node(
            operand_index,
            0,
            operand_absolute_offset,
            operand_relative_length,
            handle_index,
        );
        self.operand_absolute_offsets[operand_index] = Some(operand_absolute_offset);
        self.operand_relative_lengths[operand_index] = Some(operand_relative_length);
        let fixed = match binding.fixed {
            Some(fixed) => fixed,
            None if !binding.requires_fixed => RuntimeFixedHandle::default(),
            None => bail!(
                "missing_sla_exported_fixed_handle: operand {operand_index} did not produce a fixed handle"
            ),
        };
        self.handles[operand_index] = Some(RuntimeHandle {
            operand_index,
            spec: template.spec,
            fixed,
            debug_value: binding.debug_value,
            subtable_state: binding.subtable_state.map(Box::new),
        });
        Ok(())
    }

    fn operand_absolute_offset(&self, spec: &CompiledOperandSpec) -> Result<usize> {
        let Some((reloffset, offsetbase)) = operand_spec_offsets(spec) else {
            return self.offset_irrelevant_operand_start(spec);
        };
        let base = self.offset_for_operand_base(offsetbase, "operand offset")?;
        checked_relative_offset(base, reloffset, "operand offset")
    }

    fn offset_irrelevant_operand_start(&self, spec: &CompiledOperandSpec) -> Result<usize> {
        match spec {
            CompiledOperandSpec::SlaFixedVarnode { .. }
            | CompiledOperandSpec::ContextFieldExtraction { .. } => Ok(self.ctx.cursor),
            other => bail!("SLA operand spec is missing offset metadata: {other:?}"),
        }
    }

    fn offset_for_operand_base(&self, offsetbase: i32, role: &str) -> Result<usize> {
        if offsetbase < 0 {
            return Ok(self.ctx.cursor);
        }
        let index = usize::try_from(offsetbase)
            .map_err(|_| anyhow!("{role} base {offsetbase} does not fit usize"))?;
        let offset = (*self
            .operand_absolute_offsets
            .get(index)
            .ok_or_else(|| anyhow!("{role} base {offsetbase} is out of range"))?)
        .ok_or_else(|| anyhow!("{role} base {offsetbase} has unresolved offset"))?;
        let length = (*self
            .operand_relative_lengths
            .get(index)
            .ok_or_else(|| anyhow!("{role} base {offsetbase} is out of range"))?)
        .ok_or_else(|| anyhow!("{role} base {offsetbase} has unresolved length"))?;
        offset
            .checked_add(length)
            .ok_or_else(|| anyhow!("{role} base {offsetbase} end offset overflowed"))
    }

    fn max_operand_end(&self) -> Result<usize> {
        let mut max_end = self.ctx.cursor;
        for (offset, length) in self
            .operand_absolute_offsets
            .iter()
            .zip(self.operand_relative_lengths.iter())
        {
            let (Some(offset), Some(length)) = (*offset, *length) else {
                continue;
            };
            let end = offset
                .checked_add(length)
                .ok_or_else(|| anyhow!("operand end offset overflowed"))?;
            max_end = max_end.max(end);
        }
        Ok(max_end)
    }

    fn constructor_minimum_end(&self) -> Result<usize> {
        self.ctx
            .cursor
            .checked_add(self.minimum_length)
            .ok_or_else(|| anyhow!("constructor minimum length overflowed"))
    }

    fn subtable_offset_from_sla_operands(
        &self,
        reloffset: Option<i32>,
        offsetbase: Option<i32>,
    ) -> Result<Option<usize>> {
        let Some(rel) = reloffset else {
            return Ok(None);
        };
        let base_index = offsetbase
            .ok_or_else(|| anyhow!("subtable offset missing base for reloffset {rel}"))?;
        let base = self.offset_for_operand_base(base_index, "subtable offset")?;
        Ok(Some(checked_relative_offset(base, rel, "subtable offset")?))
    }

    fn bind_operand(
        &mut self,
        template: &CompiledHandleTemplate,
        operand_absolute_offset: usize,
    ) -> Result<OperandBinding> {
        match &template.spec {
            CompiledOperandSpec::SlaTokenField {
                big_endian,
                sign_bit,
                bit_start,
                bit_end,
                byte_start,
                byte_end,
                shift,
                reloffset: _,
                offsetbase: _,
            } => {
                let token_base = self.token_base_for_sla_field(operand_absolute_offset);
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
                let encoded_size = ((*byte_end - *byte_start) + 1).max(1);
                self.advance_cursor_past_sla_field(token_base, encoded_size)?;
                Ok(OperandBinding::with_fixed(
                    BoundOperand::Immediate {
                        value,
                        encoded_size,
                        signed: *sign_bit,
                    },
                    fixed_handle_for_const_value(value, encoded_size),
                ))
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
                reloffset: _,
                offsetbase: _,
            } => {
                let token_base = self.token_base_for_sla_field(operand_absolute_offset);
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
                let encoded_size = ((*byte_end - *byte_start) + 1).max(1);
                self.advance_cursor_past_sla_field(token_base, encoded_size)?;
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
            CompiledOperandSpec::SlaVarnodeListExpression {
                expr,
                entries,
                reloffset: _,
                offsetbase: _,
            } => {
                let selector = self.eval_pattern_expression(expr)?;
                let selector_index = usize::try_from(selector)
                    .map_err(|_| anyhow!("varnode list selector {selector} is negative"))?;
                let entry = entries.get(selector_index).ok_or_else(|| {
                    anyhow!(
                        "varnode list selector {} out of range for {} entries",
                        selector,
                        entries.len()
                    )
                })?;
                Ok(OperandBinding::with_fixed(
                    BoundOperand::NamedVarnode {
                        name: entry.name.clone(),
                        display_index: Some(selector_index as u32),
                        size: entry.size,
                    },
                    fixed_handle_from_resolved_varnode(entry),
                ))
            }
            CompiledOperandSpec::SlaValueMap {
                big_endian,
                sign_bit,
                bit_start,
                bit_end,
                byte_start,
                byte_end,
                shift,
                values,
                reloffset: _,
                offsetbase: _,
            } => {
                let token_base = self.token_base_for_sla_field(operand_absolute_offset);
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
                let value = values.get(selector as usize).copied().ok_or_else(|| {
                    anyhow!(
                        "value map selector {} out of range for {} entries",
                        selector,
                        values.len()
                    )
                })?;
                let encoded_size = ((*byte_end - *byte_start) + 1).max(1);
                self.advance_cursor_past_sla_field(token_base, encoded_size)?;
                Ok(OperandBinding::with_fixed(
                    BoundOperand::Immediate {
                        value: value as u64,
                        encoded_size,
                        signed: *sign_bit || value < 0,
                    },
                    fixed_handle_for_const_value(value as u64, encoded_size),
                ))
            }
            CompiledOperandSpec::SlaValueMapExpression {
                expr,
                values,
                reloffset: _,
                offsetbase: _,
            } => {
                let selector = self.eval_pattern_expression(expr)?;
                let selector_index = usize::try_from(selector)
                    .map_err(|_| anyhow!("value map selector {selector} is negative"))?;
                let value = values.get(selector_index).copied().ok_or_else(|| {
                    anyhow!(
                        "value map selector {} out of range for {} entries",
                        selector,
                        values.len()
                    )
                })?;
                Ok(OperandBinding::with_fixed(
                    BoundOperand::Immediate {
                        value: value as u64,
                        encoded_size: 0,
                        signed: value < 0,
                    },
                    fixed_handle_for_const_value(value as u64, 0),
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
            CompiledOperandSpec::SlaPatternExpression {
                expr,
                reloffset: _,
                offsetbase: _,
            } => {
                let mut encoded_size = 0;
                let value = if let CompiledPatternExpression::TokenField {
                    big_endian,
                    sign_bit,
                    bit_start,
                    bit_end,
                    byte_start,
                    byte_end,
                    shift,
                } = expr
                {
                    let token_base = self.token_base_for_sla_field(operand_absolute_offset);
                    let value = u64_to_i64_bits(read_sla_token_field_at(
                        self.ctx,
                        token_base,
                        *big_endian,
                        *sign_bit,
                        *bit_start,
                        *bit_end,
                        *byte_start,
                        *byte_end,
                        *shift,
                    )?);
                    encoded_size = ((*byte_end - *byte_start) + 1).max(1);
                    self.advance_cursor_past_sla_field(token_base, encoded_size)?;
                    value
                } else {
                    self.eval_pattern_expression(expr)?
                };
                Ok(OperandBinding::with_fixed(
                    BoundOperand::Immediate {
                        value: value as u64,
                        encoded_size,
                        signed: value < 0,
                    },
                    fixed_handle_for_const_value(value as u64, encoded_size),
                ))
            }
            CompiledOperandSpec::ContextFieldExtraction {
                bit_offset,
                bit_width,
                sign_extend,
            } => {
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
                let encoded_size = (*bit_width / 8).max(1);
                Ok(OperandBinding::with_fixed(
                    BoundOperand::Immediate {
                        value,
                        encoded_size,
                        signed: *sign_extend,
                    },
                    fixed_handle_for_const_value(value, encoded_size),
                ))
            }
            CompiledOperandSpec::SubtableEvaluation {
                table_name,
                reloffset,
                offsetbase,
            } => {
                let cursor_start = self.cursor;
                let sub_state = self.decode_subtable(
                    table_name,
                    Some(*reloffset),
                    Some(*offsetbase),
                    Some(operand_absolute_offset),
                )?;
                let spec_derived_sla_operand = self
                    .selection
                    .constructor
                    .constructor_template
                    .template_source
                    == CompiledTemplateSource::SpecDerived
                    && operand_spec_offsets(&template.spec).is_some();
                let sub_relative_length = sub_state
                    .length
                    .checked_sub(self.ctx.cursor)
                    .ok_or_else(|| anyhow!("subtable length resolved before instruction start"))?;
                if spec_derived_sla_operand {
                    self.minimum_length = self.minimum_length.max(sub_relative_length);
                    self.cursor = cursor_start;
                } else if !subtable_consumes_sequential_bytes(self.compiled, table_name, 0)? {
                    self.minimum_length = self.minimum_length.max(sub_relative_length);
                    self.cursor = cursor_start;
                } else {
                    self.cursor = self.cursor.max(sub_state.length);
                }
                // Return the exported handle from the sub-constructor. If no
                // handle is exported, only pure guard subtables may continue:
                // the parent ConstructTpl must not reference this operand
                // handle. This keeps no-export subtables out of raw P-code
                // handle resolution instead of inventing dummy handles.
                let exported = match sub_state.exported_handle.as_ref() {
                    Some(exported) => exported,
                    None => {
                        if constructor_template_references_handle(
                            &self.handle_reference_bitmap,
                            template.operand_index,
                        )? {
                            bail!(
                                "missing_sla_exported_fixed_handle: subtable {table_name} did not export handle for referenced operand {}",
                                template.operand_index
                            );
                        }
                        return Ok(OperandBinding::guard_only(sub_state));
                    }
                };
                let value = if exported.debug_value.is_some() {
                    Some(display_value_for_exported_handle(exported, &sub_state)?)
                } else {
                    None
                };
                Ok(OperandBinding {
                    debug_value: value,
                    fixed: Some(exported.fixed.clone()),
                    subtable_state: Some(sub_state),
                    requires_fixed: true,
                })
            }
        }
    }

    fn apply_context_change(&mut self, change: &crate::compiler::CompiledContextOp) -> Result<()> {
        if let Some(expr) = &change.expr {
            let saved_cursor = self.cursor;
            let raw = context_change_expr_word(self.eval_pattern_expression(expr)?)?;
            self.cursor = saved_cursor;
            let value = shifted_context_change_word(raw, change.shift)?;
            set_packed_context_word(
                &mut self.context_register,
                change.word_index,
                value,
                context_change_mask_word(change.mask)?,
            )?;
            set_packed_context_word(
                &mut self.context_known_mask,
                change.word_index,
                context_change_mask_word(change.mask)?,
                context_change_mask_word(change.mask)?,
            )?;
            if crate::runtime::diagnostics::terminal_reselect_trace_enabled() {
                eprintln!(
                    "[context-change expr] word={} mask=0x{:08x} value=0x{:08x} ctx=0x{:016x} known=0x{:016x}",
                    change.word_index,
                    context_change_mask_word(change.mask)?,
                    value,
                    self.context_register,
                    self.context_known_mask,
                );
            }
            Ok(())
        } else {
            if change.bit_offset >= 64 {
                let value = u32::try_from(change.value)
                    .map_err(|_| anyhow!("context word value exceeds u32"))?;
                let mask = context_change_mask_word(change.mask)?;
                set_packed_context_word(
                    &mut self.context_register,
                    change.word_index,
                    value,
                    mask,
                )?;
                set_packed_context_word(
                    &mut self.context_known_mask,
                    change.word_index,
                    mask,
                    mask,
                )?;
                return Ok(());
            }
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
            if crate::runtime::diagnostics::terminal_reselect_trace_enabled() {
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

    fn token_base_for_sla_field(&mut self, operand_absolute_offset: usize) -> usize {
        operand_absolute_offset
    }

    fn advance_cursor_past_sla_field(
        &mut self,
        token_base: usize,
        encoded_size: u32,
    ) -> Result<()> {
        if self.sla_field_is_within_constructor_minimum(token_base, encoded_size)? {
            return Ok(());
        }
        let field_end = token_base
            .checked_add(encoded_size as usize)
            .ok_or_else(|| anyhow!("SLA token field end offset overflowed"))?;
        self.cursor = self.cursor.max(field_end);
        Ok(())
    }

    fn sla_field_is_within_constructor_minimum(
        &self,
        token_base: usize,
        encoded_size: u32,
    ) -> Result<bool> {
        let constructor_end = self
            .ctx
            .cursor
            .checked_add(self.selection.constructor.minimum_length as usize)
            .ok_or_else(|| anyhow!("constructor minimum token range overflowed"))?;
        let token_end = token_base
            .checked_add(encoded_size as usize)
            .ok_or_else(|| anyhow!("SLA token field end offset overflowed"))?;
        Ok(token_base == self.ctx.cursor && token_end <= constructor_end)
    }

    fn eval_pattern_expression(&mut self, expr: &CompiledPatternExpression) -> Result<i64> {
        match expr {
            CompiledPatternExpression::Constant(value) => Ok(*value),
            CompiledPatternExpression::InstStart => Ok(self.ctx.address as i64),
            CompiledPatternExpression::InstNext => {
                let construct_end = self
                    .ctx
                    .cursor
                    .checked_add(self.selection.constructor.minimum_length as usize)
                    .ok_or_else(|| anyhow!("pattern InstNext construct end overflowed"))?;
                let next_offset = self.cursor.max(construct_end);
                let next_offset = checked_usize_to_u64(next_offset, "pattern InstNext offset")?;
                Ok(self
                    .ctx
                    .address
                    .checked_add(next_offset)
                    .ok_or_else(|| anyhow!("pattern InstNext address overflowed"))?
                    as i64)
            }
            CompiledPatternExpression::InstNext2 => {
                bail!("pattern expression inst_next2 requires delayed instruction context")
            }
            CompiledPatternExpression::TokenField {
                big_endian,
                sign_bit,
                bit_start,
                bit_end,
                byte_start,
                byte_end,
                shift,
            } => Ok(u64_to_i64_bits(read_sla_token_field(
                self.ctx,
                *big_endian,
                *sign_bit,
                *bit_start,
                *bit_end,
                *byte_start,
                *byte_end,
                *shift,
            )?)),
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
                pattern_context_bits_i64(raw, bit_width, *sign_bit)
            }
            CompiledPatternExpression::OperandValue { index } => {
                self.eval_operand_value_expression(*index)
            }
            CompiledPatternExpression::Add(lhs, rhs) => checked_pattern_add(
                self.eval_pattern_expression(lhs)?,
                self.eval_pattern_expression(rhs)?,
            ),
            CompiledPatternExpression::Sub(lhs, rhs) => checked_pattern_sub(
                self.eval_pattern_expression(lhs)?,
                self.eval_pattern_expression(rhs)?,
            ),
            CompiledPatternExpression::Mul(lhs, rhs) => checked_pattern_mul(
                self.eval_pattern_expression(lhs)?,
                self.eval_pattern_expression(rhs)?,
            ),
            CompiledPatternExpression::Div(lhs, rhs) => {
                let rhs = self.eval_pattern_expression(rhs)?;
                checked_pattern_div(self.eval_pattern_expression(lhs)?, rhs)
            }
            CompiledPatternExpression::LeftShift(lhs, rhs) => checked_pattern_left_shift(
                self.eval_pattern_expression(lhs)?,
                self.eval_pattern_expression(rhs)?,
            ),
            CompiledPatternExpression::RightShift(lhs, rhs) => checked_pattern_right_shift(
                self.eval_pattern_expression(lhs)?,
                self.eval_pattern_expression(rhs)?,
            ),
            CompiledPatternExpression::And(lhs, rhs) => {
                Ok(self.eval_pattern_expression(lhs)? & self.eval_pattern_expression(rhs)?)
            }
            CompiledPatternExpression::Or(lhs, rhs) => {
                Ok(self.eval_pattern_expression(lhs)? | self.eval_pattern_expression(rhs)?)
            }
            CompiledPatternExpression::Xor(lhs, rhs) => {
                Ok(self.eval_pattern_expression(lhs)? ^ self.eval_pattern_expression(rhs)?)
            }
            CompiledPatternExpression::Negate(inner) => {
                checked_pattern_negate(self.eval_pattern_expression(inner)?)
            }
            CompiledPatternExpression::Not(inner) => Ok(!self.eval_pattern_expression(inner)?),
        }
    }

    fn eval_operand_value_expression(&mut self, operand_index: usize) -> Result<i64> {
        let spec = self
            .selection
            .constructor
            .constructor_template
            .handles
            .get(operand_index)
            .ok_or_else(|| anyhow!("missing operand {operand_index} for pattern expression"))?
            .spec
            .clone();
        let operand_absolute_offset = self.operand_absolute_offset(&spec)?;
        match &spec {
            CompiledOperandSpec::SlaTokenField {
                big_endian,
                sign_bit,
                bit_start,
                bit_end,
                byte_start,
                byte_end,
                shift,
                ..
            }
            | CompiledOperandSpec::SlaVarnodeList {
                big_endian,
                sign_bit,
                bit_start,
                bit_end,
                byte_start,
                byte_end,
                shift,
                ..
            }
            | CompiledOperandSpec::SlaValueMap {
                big_endian,
                sign_bit,
                bit_start,
                bit_end,
                byte_start,
                byte_end,
                shift,
                ..
            } => Ok(u64_to_i64_bits(read_sla_token_field_at(
                self.ctx,
                operand_absolute_offset,
                *big_endian,
                *sign_bit,
                *bit_start,
                *bit_end,
                *byte_start,
                *byte_end,
                *shift,
            )?)),
            CompiledOperandSpec::SlaVarnodeListExpression { expr, .. }
            | CompiledOperandSpec::SlaValueMapExpression { expr, .. }
            | CompiledOperandSpec::SlaPatternExpression { expr, .. } => {
                self.eval_pattern_expression(expr)
            }
            CompiledOperandSpec::ContextFieldExtraction {
                bit_offset,
                bit_width,
                sign_extend,
            } => {
                let raw = u64::from(packed_context_bits(
                    self.context_register,
                    *bit_offset,
                    *bit_width,
                )?);
                pattern_context_bits_i64(raw, *bit_width, *sign_extend)
            }
            CompiledOperandSpec::SlaFixedVarnode { .. }
            | CompiledOperandSpec::SubtableEvaluation { .. } => {
                self.decode_operand(operand_index)?;
                let handle = self
                    .handles
                    .get(operand_index)
                    .and_then(|value| value.as_ref())
                    .ok_or_else(|| {
                        anyhow!("operand {operand_index} was not decoded for pattern expression")
                    })?;
                let fixed = &handle.fixed;
                if fixed.offset_space.is_none()
                    && fixed
                        .space
                        .as_ref()
                        .is_some_and(|space| space.name == "const" || space.index == 0)
                {
                    return Ok(fixed.offset_offset as i64);
                }
                bail!(
                    "operand {operand_index} has no evaluable defining expression for pattern expression"
                )
            }
        }
    }

    fn decode_subtable(
        &mut self,
        table_name: &str,
        reloffset: Option<i32>,
        offsetbase: Option<i32>,
        operand_absolute_offset: Option<usize>,
    ) -> Result<RuntimeConstructState> {
        let mut sub_ctx = (*self.ctx).clone();
        sub_ctx.cursor = if let Some(offset) = operand_absolute_offset {
            offset
        } else if let Some(offset) =
            self.subtable_offset_from_sla_operands(reloffset, offsetbase)?
        {
            offset
        } else if self.selection.constructor.context_changes.is_empty() {
            self.cursor
        } else {
            let pattern = self
                .selection
                .trace
                .matched_leaf_pattern
                .as_ref()
                .ok_or_else(|| {
                    anyhow!("context-dependent subtable {table_name} missing terminal SLA pattern")
                })?;
            let consumed_instruction_bytes = disjoint_pattern_instruction_byte_len(pattern)?;
            if consumed_instruction_bytes == 0 {
                self.cursor
            } else {
                let cursor_delta = self
                    .cursor
                    .checked_sub(self.ctx.cursor)
                    .ok_or_else(|| anyhow!("subtable cursor resolved before instruction start"))?;
                let advance = consumed_instruction_bytes.max(cursor_delta);
                self.ctx
                    .cursor
                    .checked_add(advance)
                    .ok_or_else(|| anyhow!("subtable cursor overflowed"))?
            }
        };
        sub_ctx.context_register = self.context_register;
        sub_ctx.context_known_mask = self.context_known_mask;
        if crate::runtime::diagnostics::terminal_reselect_trace_enabled() {
            eprintln!(
                "[decode-subtable] table={} cursor=0x{:x} ctx=0x{:016x} known=0x{:016x}",
                table_name, sub_ctx.cursor, sub_ctx.context_register, sub_ctx.context_known_mask,
            );
        }

        let selection = if let Some(native) =
            self.strategy
                .native_for_table(self.compiled, table_name, &sub_ctx)
        {
            let decode_no_match_address = subtable_decode_address(&sub_ctx)?;
            let constructor_index = native
                .decode_match(table_name, self.ctx.bytes, sub_ctx.context_register)?
                .ok_or_else(|| {
                    anyhow!(
                        "DecodeNoMatch in subtable {table_name} at 0x{decode_no_match_address:x}"
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
                .ok_or_else(|| {
                    anyhow!(
                        "invalid constructor index {constructor_index} in subtable {table_name}"
                    )
                })?;
            let (subtable_id, constructor_slot) = constructor_sla_selection_identity(
                subtable,
                constructor,
                constructor_index,
                table_name,
            )?;
            RuntimeSelection {
                constructor,
                constructor_index,
                subtable_id,
                constructor_id: constructor.constructor_id,
                constructor_slot,
                trace: spine::RuntimeMatchTrace {
                    root_bucket: format!("native:{}", table_name),
                    probes: Vec::new(),
                    leaf_constructor_indexes: vec![constructor_index],
                    matched_leaf_pattern: None,
                },
            }
        } else {
            let decode_no_match_address = subtable_decode_address(&sub_ctx)?;
            select_constructor(self.compiled, table_name, &sub_ctx)?.ok_or_else(|| {
                anyhow!("DecodeNoMatch in subtable {table_name} at 0x{decode_no_match_address:x}")
            })?
        };
        if crate::runtime::diagnostics::terminal_reselect_trace_enabled() {
            eprintln!(
                "[decode-subtable selection] table={} ctor={} mnemonic={} source={}",
                table_name,
                selection.constructor_index,
                selection.constructor.mnemonic,
                selection.constructor.source,
            );
        }

        bind_instruction(self.compiled, self.strategy, &sub_ctx, selection)
    }
}

fn subtable_decode_address(ctx: &CompiledInstructionContext<'_>) -> Result<u64> {
    let cursor = checked_usize_to_u64(ctx.cursor, "subtable cursor")?;
    ctx.address
        .checked_add(cursor)
        .ok_or_else(|| anyhow!("subtable decode address overflowed"))
}

fn checked_usize_to_u64(value: usize, role: &str) -> Result<u64> {
    u64::try_from(value).map_err(|_| anyhow!("{role} {value} exceeds u64"))
}

fn checked_relative_offset(base: usize, rel: i32, role: &str) -> Result<usize> {
    let rel = isize::try_from(rel)
        .map_err(|_| anyhow!("{role} relative offset {rel} does not fit isize"))?;
    base.checked_add_signed(rel)
        .ok_or_else(|| anyhow!("{role} resolved outside addressable decode window"))
}

fn context_change_expr_word(value: i64) -> Result<u32> {
    if value < i64::from(i32::MIN) || value > i64::from(u32::MAX) {
        return Err(anyhow!("context expression value {value} exceeds u32 word"));
    }
    Ok(value as u32)
}

fn context_change_mask_word(mask: u64) -> Result<u32> {
    u32::try_from(mask).map_err(|_| anyhow!("context change mask 0x{mask:x} exceeds u32"))
}

fn shifted_context_change_word(value: u32, shift: i32) -> Result<u32> {
    if shift >= 0 {
        value
            .checked_shl(shift as u32)
            .ok_or_else(|| anyhow!("context expression left shift {shift} exceeds u32 width"))
    } else {
        let amount = shift
            .checked_neg()
            .ok_or_else(|| anyhow!("context expression shift underflow"))?
            as u32;
        value
            .checked_shr(amount)
            .ok_or_else(|| anyhow!("context expression right shift {amount} exceeds u32 width"))
    }
}

fn checked_pattern_add(lhs: i64, rhs: i64) -> Result<i64> {
    lhs.checked_add(rhs)
        .ok_or_else(|| anyhow!("pattern expression add overflowed: {lhs} + {rhs}"))
}

fn checked_pattern_sub(lhs: i64, rhs: i64) -> Result<i64> {
    lhs.checked_sub(rhs)
        .ok_or_else(|| anyhow!("pattern expression subtract overflowed: {lhs} - {rhs}"))
}

fn checked_pattern_mul(lhs: i64, rhs: i64) -> Result<i64> {
    lhs.checked_mul(rhs)
        .ok_or_else(|| anyhow!("pattern expression multiply overflowed: {lhs} * {rhs}"))
}

fn checked_pattern_div(lhs: i64, rhs: i64) -> Result<i64> {
    lhs.checked_div(rhs)
        .ok_or_else(|| anyhow!("pattern expression divide overflowed: {lhs} / {rhs}"))
}

fn checked_pattern_negate(value: i64) -> Result<i64> {
    value
        .checked_neg()
        .ok_or_else(|| anyhow!("pattern expression negate overflowed: -{value}"))
}

fn checked_pattern_left_shift(lhs: i64, rhs: i64) -> Result<i64> {
    let amount = pattern_shift_amount(rhs)?;
    lhs.checked_shl(amount)
        .ok_or_else(|| anyhow!("pattern expression left shift {amount} exceeds i64 width"))
}

fn checked_pattern_right_shift(lhs: i64, rhs: i64) -> Result<i64> {
    let amount = pattern_shift_amount(rhs)?;
    let shifted = (lhs as u64)
        .checked_shr(amount)
        .ok_or_else(|| anyhow!("pattern expression right shift {amount} exceeds i64 width"))?;
    Ok(shifted as i64)
}

fn pattern_context_bits_i64(raw: u64, bit_width: u32, sign_extend: bool) -> Result<i64> {
    if bit_width > 64 {
        bail!("pattern context bit width {bit_width} exceeds i64 width");
    }
    if sign_extend {
        let shift = 64u32
            .checked_sub(bit_width)
            .ok_or_else(|| anyhow!("pattern context sign-extension shift underflow"))?;
        let shifted = raw
            .checked_shl(shift)
            .ok_or_else(|| anyhow!("pattern context left shift {shift} exceeds i64 width"))?;
        Ok(u64_to_i64_bits(shifted) >> shift)
    } else {
        Ok(u64_to_i64_bits(raw))
    }
}

fn pattern_shift_amount(value: i64) -> Result<u32> {
    let amount = u32::try_from(value)
        .map_err(|_| anyhow!("pattern expression shift amount {value} is negative or too large"))?;
    if amount >= 64 {
        bail!("pattern expression shift amount {amount} exceeds i64 width");
    }
    Ok(amount)
}
