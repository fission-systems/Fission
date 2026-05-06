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
    shared_token_operand_end: usize,
    handles: Vec<Option<RuntimeHandle>>,
    operand_absolute_offsets: Vec<Option<usize>>,
    operand_relative_lengths: Vec<Option<usize>>,
    handle_reference_bitmap: Vec<bool>,
    walker: spine::RuntimeParserWalker,
    legacy_path_audit: crate::runtime::RuntimeLegacyPathAudit,
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
        | CompiledOperandSpec::SlaValueMap {
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

#[cfg(test)]
mod construct_state_offset_tests {
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
                details.legacy_path_audit,
                crate::runtime::RuntimeLegacyPathAudit::default(),
                "SLA operand offsets and ConstructState lengths should not need legacy audit paths"
            );
        }
    }
}

impl<'a, 'b> CompiledParserWalker<'a, 'b> {
    fn new(
        compiled: &'a CompiledFrontend,
        strategy: RuntimeDecodeStrategy<'a>,
        ctx: &'a CompiledInstructionContext<'b>,
        selection: RuntimeSelection<'a>,
    ) -> Result<Self> {
        let token_policy = CompiledTokenCursorPolicy::for_frontend(compiled);
        let replace_current_wrapper = token_policy.uses_shared_token_cursor()
            && constructor_replaces_current(selection.constructor);
        let opcode_len = if replace_current_wrapper {
            0
        } else if selection.constructor.constructor_template.template_source
            == CompiledTemplateSource::SpecDerived
        {
            if selection.trace.root_bucket == "instruction"
                && constructor_consumes_sequential_operand_bytes(compiled, selection.constructor)
            {
                opcode_len_from_context(ctx)?
            } else if selection.trace.root_bucket == "instruction" {
                // For instruction-level constructors whose subtables aren't yet tracked in
                // compiled.subtables (e.g. 32-bit architectures where rel32/rel8 are native-only),
                // fall back to the matcher length so the cursor is positioned after the opcode
                // before binding displacement/address operands.
                opcode_len_from_matcher(&selection.constructor.matcher)
            } else {
                0
            }
        } else {
            opcode_len_from_matcher(&selection.constructor.matcher)
        };
        let minimum_length = selection.constructor.minimum_length as usize;
        let handles = vec![None; selection.constructor.constructor_template.handles.len()];
        let operand_absolute_offsets =
            vec![None; selection.constructor.constructor_template.handles.len()];
        let operand_relative_lengths =
            vec![None; selection.constructor.constructor_template.handles.len()];
        if std::env::var("FISSION_REL_FALLBACK_DEBUG").is_ok() {
            let matcher_len = opcode_len_from_matcher(&selection.constructor.matcher);
            let seq_bytes =
                constructor_consumes_sequential_operand_bytes(compiled, selection.constructor);
            eprintln!(
                "[bind-instr] bucket={} opcode_len={opcode_len} ctx.cursor={} sel_src={:?} \
                 matcher_len={matcher_len} seq_bytes={seq_bytes} matcher={:?}",
                selection.trace.root_bucket,
                ctx.cursor,
                selection.constructor.constructor_template.template_source,
                selection.constructor.matcher
            );
        }
        let compatibility_template_source =
            selection.constructor.constructor_template.template_source
                != CompiledTemplateSource::SpecDerived;
        let handle_reference_bitmap = constructor_template_handle_reference_bitmap(
            &selection.constructor.constructor_template,
        );
        Ok(Self {
            compiled,
            strategy,
            ctx,
            selection,
            minimum_length,
            context_register: ctx.context_register,
            context_known_mask: ctx.context_known_mask,
            cursor: ctx.cursor + opcode_len,
            shared_token_operand_end: 0,
            handles,
            operand_absolute_offsets,
            operand_relative_lengths,
            handle_reference_bitmap,
            walker: spine::RuntimeParserWalker::new(ctx.cursor, opcode_len),
            legacy_path_audit: crate::runtime::RuntimeLegacyPathAudit {
                compatibility_template_source,
                ..Default::default()
            },
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
                CompiledOperandDecodeStep::ConsumeTokenFields => {
                    bail!(
                        "compatibility token-field decode step is not a canonical compiled-table runtime path"
                    );
                }
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
                    let (reloffset, offsetbase, operand_absolute_offset) = self
                        .selection
                        .constructor
                        .constructor_template
                        .handles
                        .iter()
                        .find_map(|h| {
                            if let CompiledOperandSpec::SubtableEvaluation {
                                table_name: ref tn,
                                reloffset,
                                offsetbase,
                            } = h.spec
                            {
                                if tn.as_str() == table_name.as_str() {
                                    return Some((
                                        Some(reloffset),
                                        Some(offsetbase),
                                        self.operand_absolute_offset(&h.spec),
                                    ));
                                }
                            }
                            None
                        })
                        .unwrap_or((None, None, None));
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

        let length = self
            .cursor
            .max(self.ctx.cursor + self.minimum_length)
            .max(self.max_operand_end());
        let absolute_offset = self.ctx.cursor;
        let relative_length = length.saturating_sub(absolute_offset);

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
            condition_code,
            absolute_offset,
            relative_length,
            length,
            match_trace: self.selection.trace,
            legacy_path_audit: self.legacy_path_audit,
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
        let value = bound_operand_from_fixed_handle(&fixed)?;
        Ok(Some(RuntimeHandle {
            operand_index: usize::MAX,
            spec: CompiledOperandSpec::SubtableEvaluation {
                table_name: self.selection.constructor.source.clone(),
                reloffset: 0,
                offsetbase: -1,
            },
            fixed,
            debug_value: Some(value),
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
            let addr_unit = space
                .as_ref()
                .and_then(|s| {
                    self.compiled
                        .sla_spaces
                        .get(&s.index)
                        .map(|sr| sr.word_size.max(1) as u64)
                })
                .unwrap_or(1);
            (
                None,
                offset_offset.wrapping_mul(addr_unit),
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
                    word_size: 0,
                    addr_size: 0,
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
                if matches!(selector, CompiledHandleSelector::OffsetPlus) {
                    return Ok(resolve_offset_plus_pub(handle, plus.unwrap_or(0)));
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
                Ok(value.wrapping_add(plus.unwrap_or(0)))
            }
            CompiledConstTpl::InstStart => Ok(self.ctx.address),
            CompiledConstTpl::InstNext => {
                Ok(self.ctx.address.saturating_add(self.minimum_length as u64))
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
        let operand_absolute_offset = self
            .operand_absolute_offset(&template.spec)
            .unwrap_or(self.cursor);
        let binding = self.bind_operand(&template, operand_absolute_offset)?;
        let handle_index = operand_index;
        let operand_relative_length = binding
            .subtable_state
            .as_ref()
            .map(|state| state.relative_length)
            .unwrap_or_else(|| {
                self.cursor
                    .saturating_sub(operand_absolute_offset)
                    .max(template.minimum_length as usize)
            });
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

    fn operand_absolute_offset(&self, spec: &CompiledOperandSpec) -> Option<usize> {
        let (reloffset, offsetbase) = operand_spec_offsets(spec)?;
        let base = self.offset_for_operand_base(offsetbase)?;
        let offset = base as i64 + i64::from(reloffset);
        usize::try_from(offset.max(0)).ok()
    }

    fn offset_for_operand_base(&self, offsetbase: i32) -> Option<usize> {
        if offsetbase < 0 {
            return Some(self.ctx.cursor);
        }
        let index = usize::try_from(offsetbase).ok()?;
        let offset = (*self.operand_absolute_offsets.get(index)?)?;
        let length = (*self.operand_relative_lengths.get(index)?)?;
        Some(offset.saturating_add(length))
    }

    fn max_operand_end(&self) -> usize {
        self.operand_absolute_offsets
            .iter()
            .zip(self.operand_relative_lengths.iter())
            .filter_map(|(offset, length)| Some((*offset)? + (*length)?))
            .max()
            .unwrap_or(self.ctx.cursor)
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
                if !self.sla_field_is_within_constructor_minimum(token_base, encoded_size) {
                    self.cursor = self
                        .cursor
                        .max(token_base.saturating_add(encoded_size as usize));
                }
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
                if !self.sla_field_is_within_constructor_minimum(token_base, encoded_size) {
                    self.cursor = self
                        .cursor
                        .max(token_base.saturating_add(encoded_size as usize));
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
                if !self.sla_field_is_within_constructor_minimum(token_base, encoded_size) {
                    self.cursor = self
                        .cursor
                        .max(token_base.saturating_add(encoded_size as usize));
                }
                Ok(OperandBinding::with_fixed(
                    BoundOperand::Immediate {
                        value: value as u64,
                        encoded_size,
                        signed: *sign_bit || value < 0,
                    },
                    fixed_handle_for_const_value(value as u64, encoded_size),
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
                let value = if CompiledTokenCursorPolicy::for_frontend(self.compiled)
                    .uses_shared_token_cursor()
                {
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
                        )? as i64;
                        encoded_size = ((*byte_end - *byte_start) + 1).max(1);
                        if !self.sla_field_is_within_constructor_minimum(token_base, encoded_size) {
                            self.cursor = self
                                .cursor
                                .max(token_base.saturating_add(encoded_size as usize));
                        }
                        value
                    } else {
                        self.eval_pattern_expression(expr)?
                    }
                } else if let CompiledPatternExpression::TokenField {
                    big_endian,
                    sign_bit,
                    bit_start,
                    bit_end,
                    byte_start,
                    byte_end,
                    shift,
                } = expr
                {
                    // Non-shared-cursor (e.g. x86-32): apply reloffset so the token field is
                    // read from `ctx.cursor + reloffset + byte_start`, matching Ghidra's
                    // `point.getOffset() + bytestart` computation.
                    let token_base = self.token_base_for_sla_field(operand_absolute_offset);
                    let raw = read_sla_token_field_at(
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
                    encoded_size = ((*byte_end - *byte_start) + 1).max(1);
                    if !self.sla_field_is_within_constructor_minimum(token_base, encoded_size) {
                        self.cursor = self
                            .cursor
                            .max(token_base.saturating_add(encoded_size as usize));
                    }
                    raw
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
                self.legacy_path_audit = self.legacy_path_audit.merge(sub_state.legacy_path_audit);
                let spec_derived_sla_operand = self
                    .selection
                    .constructor
                    .constructor_template
                    .template_source
                    == CompiledTemplateSource::SpecDerived
                    && operand_spec_offsets(&template.spec).is_some();
                if spec_derived_sla_operand {
                    self.minimum_length = self
                        .minimum_length
                        .max(sub_state.length.saturating_sub(self.ctx.cursor));
                    self.cursor = cursor_start;
                    self.shared_token_operand_end =
                        self.shared_token_operand_end.max(sub_state.length);
                } else if shared_token_cursor_policy_zero_width_subtable(table_name) {
                    self.cursor = cursor_start;
                } else if self
                    .selection
                    .constructor
                    .constructor_template
                    .template_source
                    == CompiledTemplateSource::SpecDerived
                    && !subtable_consumes_sequential_bytes(self.compiled, table_name, 0)
                {
                    self.minimum_length = self
                        .minimum_length
                        .max(sub_state.length.saturating_sub(self.ctx.cursor));
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
                        ) {
                            bail!(
                                "missing_sla_exported_fixed_handle: subtable {table_name} did not export handle for referenced operand {}",
                                template.operand_index
                            );
                        }
                        return Ok(OperandBinding::guard_only(sub_state));
                    }
                };
                let value = display_value_for_exported_handle(exported, &sub_state);
                Ok(OperandBinding {
                    debug_value: Some(value),
                    fixed: Some(exported.fixed.clone()),
                    subtable_state: Some(sub_state),
                    requires_fixed: true,
                })
            }
            CompiledOperandSpec::Immediate { size, signed } => {
                let value = read_uint(self.ctx.bytes, self.cursor, *size)?;
                self.cursor += *size as usize;
                Ok(OperandBinding::with_fixed(
                    BoundOperand::Immediate {
                        value,
                        encoded_size: *size,
                        signed: *signed,
                    },
                    fixed_handle_for_const_value(value, *size),
                ))
            }
            CompiledOperandSpec::Relative { size } => {
                let signed = read_sint(self.ctx.bytes, self.cursor, *size)?;
                self.cursor += *size as usize;
                let next_ip = self.ctx.address.wrapping_add(self.cursor as u64);
                let target = next_ip.wrapping_add_signed(signed);
                let addr_size = self.compiled.sla_ram_address_size().max(*size);
                Ok(OperandBinding::with_fixed(
                    BoundOperand::Relative { target },
                    fixed_handle_for_ram_target(target, addr_size),
                ))
            }
            CompiledOperandSpec::FixedRegister { reg, size } => {
                let index = match reg {
                    CompiledFixedRegister::Accumulator => 0,
                    CompiledFixedRegister::StackPointer => 4,
                    CompiledFixedRegister::FramePointer => 5,
                };
                Ok(OperandBinding::with_fixed(
                    BoundOperand::Register { index, size: *size },
                    fixed_handle_for_register_index(index, *size),
                ))
            }
        }
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
            if crate::runtime::diagnostics::terminal_reselect_trace_enabled() {
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
        if !CompiledTokenCursorPolicy::for_frontend(self.compiled).uses_shared_token_cursor() {
            return self.cursor.max(operand_absolute_offset);
        }
        operand_absolute_offset
    }

    fn sla_field_is_within_constructor_minimum(
        &self,
        token_base: usize,
        encoded_size: u32,
    ) -> bool {
        let constructor_end = self.ctx.cursor + self.selection.constructor.minimum_length as usize;
        token_base == self.ctx.cursor && token_base + encoded_size as usize <= constructor_end
    }

    fn eval_pattern_expression(&mut self, expr: &CompiledPatternExpression) -> Result<i64> {
        match expr {
            CompiledPatternExpression::Constant(value) => Ok(*value),
            CompiledPatternExpression::InstStart => Ok(self.ctx.address as i64),
            CompiledPatternExpression::InstNext => {
                let construct_end = self
                    .ctx
                    .cursor
                    .saturating_add(self.selection.constructor.minimum_length as usize);
                let next_offset = self.cursor.max(construct_end);
                Ok(self.ctx.address.saturating_add(next_offset as u64) as i64)
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
                    .ok_or_else(|| {
                        anyhow!("operand {} was not decoded for pattern expression", index)
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
                let Some(debug_value) = handle.debug_value.clone() else {
                    bail!("operand {index} has no debug numeric value for pattern expression");
                };
                match debug_value {
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
                    BoundOperand::Memory {
                        absolute,
                        displacement,
                        ..
                    } => Ok(absolute.unwrap_or(displacement as u64) as i64),
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
            CompiledPatternExpression::LeftShift(lhs, rhs) => Ok(self
                .eval_pattern_expression(lhs)?
                << (self.eval_pattern_expression(rhs)? as u32)),
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

    fn decode_subtable(
        &mut self,
        table_name: &str,
        reloffset: Option<i32>,
        offsetbase: Option<i32>,
        operand_absolute_offset: Option<usize>,
    ) -> Result<RuntimeConstructState> {
        let mut sub_ctx = (*self.ctx).clone();
        let consumed_instruction_bytes =
            if CompiledTokenCursorPolicy::for_frontend(self.compiled).uses_shared_token_cursor() {
                0
            } else {
                self.selection
                    .trace
                    .matched_leaf_pattern
                    .as_ref()
                    .map(disjoint_pattern_instruction_byte_len)
                    .unwrap_or(0)
            };
        sub_ctx.cursor = if let Some(offset) = operand_absolute_offset {
            offset
        } else if CompiledTokenCursorPolicy::for_frontend(self.compiled).uses_shared_token_cursor()
            && constructor_replaces_current(self.selection.constructor)
            && table_name == "instruction"
        {
            self.ctx.cursor
                + opcode_len_from_matcher(&self.selection.constructor.matcher)
                    .max(self.selection.constructor.minimum_length as usize)
                    .max(1)
        } else if CompiledTokenCursorPolicy::for_frontend(self.compiled).uses_shared_token_cursor()
            && shared_token_cursor_policy_modrm_trailing_subtable(table_name)
            && self.selection.trace.root_bucket == "instruction"
        {
            self.shared_token_operand_end
                .max(self.cursor)
                .max(self.ctx.cursor + opcode_len_from_context(self.ctx).unwrap_or(0))
        } else if CompiledTokenCursorPolicy::for_frontend(self.compiled).uses_shared_token_cursor()
            && shared_token_cursor_policy_modrm_trailing_subtable(table_name)
            && shared_token_cursor_policy_shared_token_subtable(
                self.selection.trace.root_bucket.as_str(),
            )
        {
            let matched_pattern_len = self
                .selection
                .trace
                .matched_leaf_pattern
                .as_ref()
                .map(disjoint_pattern_instruction_byte_len)
                .unwrap_or(0);
            if matched_pattern_len > 0 {
                self.ctx.cursor.saturating_add(matched_pattern_len)
            } else {
                self.cursor
            }
        } else if !CompiledTokenCursorPolicy::for_frontend(self.compiled).uses_shared_token_cursor()
            && reloffset.is_some_and(|rel| rel >= 0)
            && offsetbase.unwrap_or(-1) < 0
        {
            // Non-shared-cursor architecture (e.g. 32-bit x86, ARM): position the
            // sub-walker using the operand's relative offset within the parent constructor.
            // Mirrors Ghidra's ParserWalker.pushOperand() / OperandSymbol.reloffset logic.
            if std::env::var("FISSION_REL_FALLBACK_DEBUG").is_ok() {
                eprintln!(
                    "[non-shared-cursor] table={table_name} ctx.cursor={} reloffset={:?} offsetbase={:?} → sub_ctx.cursor={}",
                    self.ctx.cursor,
                    reloffset,
                    offsetbase,
                    self.ctx.cursor + reloffset.unwrap() as usize,
                );
            }
            self.ctx.cursor + reloffset.unwrap() as usize
        } else if self.selection.constructor.context_changes.is_empty()
            || consumed_instruction_bytes == 0
        {
            self.cursor
        } else {
            self.ctx.cursor
                + consumed_instruction_bytes.max(self.cursor.saturating_sub(self.ctx.cursor))
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
                .ok_or_else(|| {
                    anyhow!(
                        "invalid constructor index {constructor_index} in subtable {table_name}"
                    )
                })?;
            RuntimeSelection {
                constructor,
                constructor_index,
                subtable_id: constructor
                    .sla_identity
                    .as_ref()
                    .map(|identity| identity.subtable_id)
                    .unwrap_or(0),
                constructor_id: constructor.constructor_id,
                constructor_slot: constructor
                    .sla_identity
                    .as_ref()
                    .map(|identity| identity.constructor_slot)
                    .unwrap_or(constructor_index),
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
