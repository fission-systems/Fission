pub(super) fn bind_instruction<'a>(
    compiled: &'a CompiledFrontend,
    native: Option<&'a Arc<NativeBackend>>,
    ctx: &CompiledInstructionContext<'_>,
    selection: RuntimeSelection<'a>,
) -> Result<RuntimeConstructState> {
    constructor_matches(ctx, selection.constructor)?;
    CompiledParserWalker::new(compiled, native, ctx, selection)?.walk()
}

pub(super) struct CompiledParserWalker<'a, 'b> {
    compiled: &'a CompiledFrontend,
    native: Option<&'a Arc<NativeBackend>>,
    ctx: &'a CompiledInstructionContext<'b>,
    selection: RuntimeSelection<'a>,
    minimum_length: usize,
    context_register: u64,
    context_known_mask: u64,
    cursor: usize,
    token_fields: Option<TokenFieldBundle>,
    handles: Vec<Option<RuntimeHandle>>,
    walker: spine::RuntimeParserWalker,
}

pub(super) struct OperandBinding {
    value: BoundOperand,
    subtable_state: Option<RuntimeConstructState>,
    fixed: Option<RuntimeFixedHandle>,
}

impl OperandBinding {
    fn plain(value: BoundOperand) -> Self {
        Self {
            value,
            subtable_state: None,
            fixed: None,
        }
    }

    fn with_fixed(value: BoundOperand, fixed: RuntimeFixedHandle) -> Self {
        Self {
            value,
            subtable_state: None,
            fixed: Some(fixed),
        }
    }
}

impl<'a, 'b> CompiledParserWalker<'a, 'b> {
    fn new(
        compiled: &'a CompiledFrontend,
        native: Option<&'a Arc<NativeBackend>>,
        ctx: &'a CompiledInstructionContext<'b>,
        selection: RuntimeSelection<'a>,
    ) -> Result<Self> {
        let opcode_len = if CompiledTokenCursorPolicy::for_frontend(compiled).uses_legacy_shared_tokens()
            && constructor_replaces_current(selection.constructor)
        {
            0
        } else if selection.constructor.constructor_template.template_source
            == CompiledTemplateSource::SpecDerived
        {
            if selection.trace.root_bucket == "instruction"
                && constructor_consumes_sequential_operand_bytes(compiled, selection.constructor)
            {
                opcode_len_from_context(ctx)?
            } else {
                0
            }
        } else {
            opcode_len_from_matcher(&selection.constructor.matcher)
        };
        let minimum_length = if CompiledTokenCursorPolicy::for_frontend(compiled).uses_legacy_shared_tokens()
            && selection.constructor.constructor_template.template_source
                == CompiledTemplateSource::SpecDerived
            && (matches!(
                selection.constructor.matcher,
                CompiledPatternMatcher::RowCc { .. }
            ) || matches!(
                selection.constructor.construct_tpl_kind,
                CompiledConstructTplKind::Jcc
            ))
        {
            0
        } else {
            selection.constructor.minimum_length as usize
        };
        let handles = vec![None; selection.constructor.constructor_template.handles.len()];
        Ok(Self {
            compiled,
            native,
            ctx,
            selection,
            minimum_length,
            context_register: ctx.context_register,
            context_known_mask: ctx.context_known_mask,
            cursor: ctx.cursor + opcode_len,
            token_fields: None,
            handles,
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
        let legacy_replace_current_wrapper = CompiledTokenCursorPolicy::for_frontend(self.compiled).uses_legacy_shared_tokens()
            && decode_steps.iter().any(|step| {
                matches!(
                    step,
                    CompiledOperandDecodeStep::DescendSubtable {
                        replace_current: true,
                        ..
                    }
                )
            });
        for step in decode_steps {
            match step {
                CompiledOperandDecodeStep::ConsumeTokenFields => {
                    if !legacy_replace_current_wrapper {
                        self.ensure_token_fields()?;
                    }
                }
                CompiledOperandDecodeStep::DecodeOperand { operand_index } => {
                    self.decode_operand(operand_index)?;
                }
                CompiledOperandDecodeStep::DescendSubtable {
                    table_name,
                    replace_current,
                } => {
                    let sub_state = self.decode_subtable(&table_name)?;
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

        let base_length = self.cursor.max(self.ctx.cursor + self.minimum_length);
        let direct_relative_length = CompiledTokenCursorPolicy::for_frontend(self.compiled).uses_legacy_shared_tokens()
            && self.selection.constructor.constructor_template.template_source
                == CompiledTemplateSource::SpecDerived
            && self
                .selection
                .constructor
                .constructor_template
                .handles
                .iter()
                .any(|handle| {
                    matches!(
                        &handle.spec,
                        CompiledOperandSpec::SubtableEvaluation { table_name }
                            if legacy_shared_token_policy_relative_trailing_subtable(table_name)
                    )
                })
            && self.cursor > self.ctx.cursor;
        let length = if direct_relative_length {
            self.cursor
        } else {
            base_length
        };

        Ok(RuntimeConstructState {
            mnemonic: self.selection.constructor.mnemonic.clone(),
            construct_tpl_kind: self.selection.constructor.construct_tpl_kind,
            constructor_template: self.selection.constructor.constructor_template.clone(),
            display_template: self.selection.constructor.display_template.clone(),
            display_operands: self.selection.constructor.display_operands.clone(),
            construct_nodes: self.walker.into_nodes(),
            handles,
            exported_handle,
            operands,
            condition_code,
            length,
            match_trace: self.selection.trace,
        })
    }

    fn materialize_export_handle(&mut self, handles: &[RuntimeHandle]) -> Result<Option<RuntimeHandle>> {
        let Some(export_tpl) = self
            .selection
            .constructor
            .constructor_template
            .export
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
            },
            value,
            fixed,
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
        let fixable = space.is_some()
            && (offset_space.is_none()
                || (offset_size != 0 && temp_space.is_some()));
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
                let value = match selector {
                    CompiledHandleSelector::Space => handle
                        .fixed
                        .space
                        .as_ref()
                        .map(|space| space.index)
                        .ok_or_else(|| anyhow!("export fixed handle missing space"))?,
                    CompiledHandleSelector::Offset => handle.fixed.offset_offset,
                    CompiledHandleSelector::Size => u64::from(handle.fixed.size),
                    CompiledHandleSelector::OffsetPlus => bail!("export OffsetPlus unsupported"),
                };
                Ok(value.wrapping_add(plus.unwrap_or(0)))
            }
            CompiledConstTpl::InstStart => Ok(self.ctx.address),
            CompiledConstTpl::InstNext => Ok(self.ctx.address.saturating_add(self.minimum_length as u64)),
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
        let operand_cursor_start = self.cursor;
        let binding = self.bind_operand(&template)?;
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
            fixed: binding
                .fixed
                .unwrap_or_else(|| fixed_handle_for_bound_operand(&binding.value)),
            value: binding.value,
            subtable_state: binding.subtable_state.map(Box::new),
        });
        Ok(())
    }

    fn ensure_token_fields(&mut self) -> Result<TokenFieldBundle> {
        if self.token_fields.is_none() {
            let token_offset = if self.selection.constructor.constructor_template.template_source
                == CompiledTemplateSource::SpecDerived
                && self.selection.trace.root_bucket == "instruction"
            {
                self.ctx.cursor + opcode_len_from_context(self.ctx)?
            } else {
                self.cursor
            };
            let decoded = parse_token_fields(self.ctx, token_offset)?;
            self.cursor = self.cursor.max(token_offset + decoded.length);
            self.token_fields = Some(decoded);
        }
        self.token_fields
            .ok_or_else(|| anyhow!("failed to decode token fields"))
    }

    fn bind_operand(&mut self, template: &CompiledHandleTemplate) -> Result<OperandBinding> {
        match &template.spec {
            CompiledOperandSpec::TokenFieldExtraction { bit_offset, bit_width, sign_extend } => {
                let token_fields = self.ensure_token_fields()?;
                if token_fields.operand_mode == 3 {
                    Ok(OperandBinding::plain(BoundOperand::Register {
                        index: token_fields.rm,
                        size: *bit_width / 8,
                    }))
                } else {
                    let absolute = token_fields.rip_relative.then(|| {
                        self.ctx
                            .address
                            .wrapping_add(self.cursor as u64)
                            .wrapping_add_signed(token_fields.displacement)
                    });
                    Ok(OperandBinding::plain(BoundOperand::Memory {
                        base: token_fields.base,
                        index: token_fields.index,
                        scale: token_fields.scale,
                        displacement: token_fields.displacement,
                        rip_relative: token_fields.rip_relative,
                        absolute,
                        size: *bit_width / 8,
                    }))
                }
            }
            CompiledOperandSpec::SlaTokenField {
                big_endian,
                sign_bit,
                bit_start,
                bit_end,
                byte_start,
                byte_end,
                shift,
            } => {
                let token_base = self.token_base_for_sla_field();
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
                if CompiledTokenCursorPolicy::for_frontend(self.compiled).uses_legacy_shared_tokens()
                    && legacy_shared_token_policy_sla_field_advances_cursor(self.selection.trace.root_bucket.as_str())
                {
                    self.cursor = self
                        .cursor
                        .max(token_base + ((*byte_end - *byte_start) + 1) as usize);
                }
                Ok(OperandBinding::plain(BoundOperand::Immediate {
                    value,
                    encoded_size: ((*byte_end - *byte_start) + 1).max(1),
                    signed: *sign_bit,
                }))
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
            } => {
                let token_base = self.token_base_for_sla_field();
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
                if CompiledTokenCursorPolicy::for_frontend(self.compiled).uses_legacy_shared_tokens()
                    && legacy_shared_token_policy_sla_field_advances_cursor(self.selection.trace.root_bucket.as_str())
                {
                    self.cursor = self
                        .cursor
                        .max(token_base + ((*byte_end - *byte_start) + 1) as usize);
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
            } => {
                let token_base = self.token_base_for_sla_field();
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
                if CompiledTokenCursorPolicy::for_frontend(self.compiled).uses_legacy_shared_tokens()
                    && legacy_shared_token_policy_sla_field_advances_cursor(self.selection.trace.root_bucket.as_str())
                {
                    self.cursor = self
                        .cursor
                        .max(token_base + ((*byte_end - *byte_start) + 1) as usize);
                }
                let value = values.get(selector as usize).copied().ok_or_else(|| {
                    anyhow!(
                        "value map selector {} out of range for {} entries",
                        selector,
                        values.len()
                    )
                })?;
                Ok(OperandBinding::plain(BoundOperand::Immediate {
                    value: value as u64,
                    encoded_size: ((*byte_end - *byte_start) + 1).max(1),
                    signed: *sign_bit || value < 0,
                }))
            }
            CompiledOperandSpec::SlaFixedVarnode { varnode } => Ok(OperandBinding::with_fixed(
                BoundOperand::NamedVarnode {
                    name: varnode.name.clone(),
                    display_index: None,
                    size: varnode.size,
                },
                fixed_handle_from_resolved_varnode(varnode),
            )),
            CompiledOperandSpec::SlaPatternExpression { expr } => {
                let value = if CompiledTokenCursorPolicy::for_frontend(self.compiled).uses_legacy_shared_tokens() {
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
                        let token_base = self.token_base_for_sla_field();
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
                        self.cursor = self
                            .cursor
                            .max(token_base + ((*byte_end - *byte_start) + 1) as usize);
                        value
                    } else {
                        self.eval_pattern_expression(expr)?
                    }
                } else {
                    self.eval_pattern_expression(expr)?
                };
                Ok(OperandBinding::plain(BoundOperand::Immediate {
                    value: value as u64,
                    encoded_size: 0,
                    signed: value < 0,
                }))
            }
            CompiledOperandSpec::ContextFieldExtraction { bit_offset, bit_width, sign_extend } => {
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
                Ok(OperandBinding::plain(BoundOperand::Immediate {
                    value,
                    encoded_size: (*bit_width / 8).max(1),
                    signed: *sign_extend,
                }))
            }
            CompiledOperandSpec::SubtableEvaluation { table_name } => {
                let cursor_start = self.cursor;
                let sub_state = self.decode_subtable(table_name)?;
                if legacy_shared_token_policy_zero_width_subtable(table_name) {
                    self.cursor = cursor_start;
                } else if CompiledTokenCursorPolicy::for_frontend(self.compiled).uses_legacy_shared_tokens()
                    && self.selection.constructor.constructor_template.template_source
                        == CompiledTemplateSource::SpecDerived
                    && legacy_shared_token_policy_shared_token_subtable(table_name)
                {
                    // x86 SLEIGH subtables such as addr64, Index64, Base64,
                    // and Reg32 often read fields from the same ModRM/SIB
                    // token. Ghidra's ParserWalker keeps the token-relative
                    // cursor stable and computes the final instruction length
                    // from the matched subconstructor. Treating these as a
                    // sequential byte stream makes exact slices like
                    // `8d 04 11` overrun while decoding Base64 after Index64.
                    self.minimum_length = self
                        .minimum_length
                        .max(sub_state.length.saturating_sub(self.ctx.cursor));
                    self.cursor = cursor_start;
                } else if self.selection.constructor.constructor_template.template_source
                    == CompiledTemplateSource::SpecDerived
                    && !subtable_consumes_sequential_bytes(self.compiled, table_name, 0)
                {
                    self.minimum_length = self
                        .minimum_length
                        .max(sub_state.length.saturating_sub(self.ctx.cursor));
                    self.cursor = cursor_start;
                } else {
                    let mut next_cursor = sub_state.length;
                    if next_cursor <= cursor_start && CompiledTokenCursorPolicy::for_frontend(self.compiled).uses_legacy_shared_tokens() {
                        next_cursor = cursor_start.saturating_add(1);
                    }
                    self.cursor = self.cursor.max(next_cursor);
                }
                // Return the exported handle from the sub-constructor. Some
                // x86 subtables are zero-op BUILD checks/prefix hooks and do
                // not export a value; keep those as handle placeholders only,
                // never as synthetic p-code.
                let exported = match sub_state.exported_handle.as_ref() {
                    Some(exported) => exported,
                    None => {
                        let subtable_cursor_start = sub_state
                            .construct_nodes
                            .first()
                            .map(|node| node.absolute_offset)
                            .unwrap_or(cursor_start);
                        if let Some(binding) = self.fallback_binding_for_no_export_subtable(
                            table_name,
                            cursor_start,
                            subtable_cursor_start,
                        )?
                        {
                            return Ok(OperandBinding {
                                value: binding.value,
                                fixed: binding.fixed,
                                subtable_state: Some(sub_state),
                            });
                        }
                        bail!("subtable {table_name} did not export a handle");
                    }
                };
                let value = display_value_for_exported_handle(exported, &sub_state);
                Ok(OperandBinding {
                    value,
                    fixed: Some(exported.fixed.clone()),
                    subtable_state: Some(sub_state),
                })
            }
            CompiledOperandSpec::Immediate { size, signed } => {
                let value = read_uint(self.ctx.bytes, self.cursor, *size)?;
                self.cursor += *size as usize;
                Ok(OperandBinding::plain(BoundOperand::Immediate {
                    value,
                    encoded_size: *size,
                    signed: *signed,
                }))
            }
            CompiledOperandSpec::Relative { size } => {
                let signed = read_sint(self.ctx.bytes, self.cursor, *size)?;
                self.cursor += *size as usize;
                let next_ip = self.ctx.address.wrapping_add(self.cursor as u64);
                Ok(OperandBinding::plain(BoundOperand::Relative {
                    target: next_ip.wrapping_add_signed(signed),
                }))
            }
            CompiledOperandSpec::FixedRegister { reg, size } => {
                let index = match reg {
                    CompiledFixedRegister::Accumulator => 0,
                    CompiledFixedRegister::StackPointer => 4,
                    CompiledFixedRegister::FramePointer => 5,
                };
                Ok(OperandBinding::plain(BoundOperand::Register { index, size: *size }))
            }
    }
}

    fn fallback_binding_for_no_export_subtable(
        &mut self,
        table_name: &str,
        cursor_start: usize,
        subtable_cursor_start: usize,
    ) -> Result<Option<OperandBinding>> {
        if !CompiledTokenCursorPolicy::for_frontend(self.compiled).uses_legacy_shared_tokens() {
            return Ok(None);
        }

        if legacy_shared_token_policy_zero_width_subtable(table_name) {
            self.cursor = cursor_start;
            let value = BoundOperand::Immediate {
                value: 0,
                encoded_size: 0,
                signed: false,
            };
            return Ok(Some(OperandBinding::with_fixed(
                value.clone(),
                fixed_handle_for_bound_operand(&value),
            )));
        }

        let relative_size = table_name
            .strip_prefix("pcRelSimm")
            .or_else(|| table_name.strip_prefix("rel"))
            .and_then(|suffix| suffix.parse::<u32>().ok())
            .map(|bits| (bits / 8).max(1));
        let Some(size) = relative_size else {
            return Ok(None);
        };
        let signed = read_sint(self.ctx.bytes, subtable_cursor_start, size)?;
        let end_cursor = subtable_cursor_start + size as usize;
        self.cursor = self.cursor.max(end_cursor);
        let next_ip = self.ctx.address.wrapping_add(end_cursor as u64);
        let target = next_ip.wrapping_add_signed(signed);
        let value = BoundOperand::Relative { target };
        Ok(Some(OperandBinding::with_fixed(
            value.clone(),
            fixed_handle_for_bound_operand(&value),
        )))
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
            if terminal_reselect_trace_enabled() {
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
            if terminal_reselect_trace_enabled() {
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

    fn token_base_for_sla_field(&self) -> usize {
        if !CompiledTokenCursorPolicy::for_frontend(self.compiled).uses_legacy_shared_tokens() {
            return self.ctx.cursor;
        }
        // x86 SLEIGH models opcode, ModRM, and SIB as separate token
        // streams. Fission's compatibility walker keeps SIB subtables rooted
        // at the ModRM cursor so shared-byte operands can compute instruction
        // length without over-consuming. When the selected subconstructor is a
        // SIB-field table, read the field from the following SIB byte instead
        // of falling back to ModRM. This keeps BUILD execution tied to .sla
        // token fields rather than re-synthesizing an effective address.
        if legacy_shared_token_policy_opcode_token_subtable(self.selection.trace.root_bucket.as_str()) {
            self.ctx.instruction_cursor
        } else if legacy_shared_token_policy_modrm_token_subtable(self.selection.trace.root_bucket.as_str()) {
            self.ctx.instruction_cursor + opcode_len_from_instruction_start(self.ctx).unwrap_or(0)
        } else if legacy_shared_token_policy_sib_token_subtable(self.selection.trace.root_bucket.as_str()) {
            self.ctx.instruction_cursor + opcode_len_from_instruction_start(self.ctx).unwrap_or(0) + 1
        } else {
            self.cursor
        }
    }

    fn eval_pattern_expression(&mut self, expr: &CompiledPatternExpression) -> Result<i64> {
        match expr {
            CompiledPatternExpression::Constant(value) => Ok(*value),
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
                    .ok_or_else(|| anyhow!("operand {} was not decoded for pattern expression", index))?;
                match handle.value.clone() {
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
                    BoundOperand::Memory { absolute, displacement, .. } => {
                        Ok(absolute.unwrap_or(displacement as u64) as i64)
                    }
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
            CompiledPatternExpression::LeftShift(lhs, rhs) => Ok(
                self.eval_pattern_expression(lhs)?
                    << (self.eval_pattern_expression(rhs)? as u32),
            ),
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

    fn decode_subtable(&self, table_name: &str) -> Result<RuntimeConstructState> {
        let mut sub_ctx = (*self.ctx).clone();
        let consumed_instruction_bytes = if CompiledTokenCursorPolicy::for_frontend(self.compiled).uses_legacy_shared_tokens() {
            0
        } else {
            self.selection
                .trace
                .matched_leaf_pattern
                .as_ref()
                .map(disjoint_pattern_instruction_byte_len)
                .unwrap_or(0)
        };
        sub_ctx.cursor = if CompiledTokenCursorPolicy::for_frontend(self.compiled).uses_legacy_shared_tokens()
            && constructor_replaces_current(self.selection.constructor)
            && table_name == "instruction"
        {
            self.ctx.cursor
                + opcode_len_from_matcher(&self.selection.constructor.matcher)
                    .max(self.selection.constructor.minimum_length as usize)
                    .max(1)
        } else if legacy_shared_token_policy_register_subtable(table_name) {
            self.ctx.cursor + opcode_len_from_context(self.ctx).unwrap_or(0)
        } else if CompiledTokenCursorPolicy::for_frontend(self.compiled).uses_legacy_shared_tokens()
            && legacy_shared_token_policy_opcode_token_subtable(table_name)
        {
            self.ctx.cursor
        } else if CompiledTokenCursorPolicy::for_frontend(self.compiled).uses_legacy_shared_tokens()
            && legacy_shared_token_policy_modrm_trailing_subtable(table_name)
            && self.selection.trace.root_bucket == "instruction"
        {
            self.ctx.cursor + opcode_len_from_context(self.ctx).unwrap_or(0)
        } else if CompiledTokenCursorPolicy::for_frontend(self.compiled).uses_legacy_shared_tokens()
            && legacy_shared_token_policy_modrm_trailing_subtable(table_name)
            && legacy_shared_token_policy_shared_token_subtable(self.selection.trace.root_bucket.as_str())
        {
            self.cursor.saturating_add(1)
        } else if self.selection.constructor.context_changes.is_empty()
            || consumed_instruction_bytes == 0
        {
            self.cursor
        } else {
            self.ctx.cursor + consumed_instruction_bytes.max(self.cursor.saturating_sub(self.ctx.cursor))
        };
        sub_ctx.context_register = self.context_register;
        sub_ctx.context_known_mask = self.context_known_mask;
        if terminal_reselect_trace_enabled() {
            eprintln!(
                "[decode-subtable] table={} cursor=0x{:x} ctx=0x{:016x} known=0x{:016x}",
                table_name,
                sub_ctx.cursor,
                sub_ctx.context_register,
                sub_ctx.context_known_mask,
            );
        }

        let selection = if let Some(native) = self
            .native
            .filter(|_| native_backend_allowed(self.compiled, table_name, &sub_ctx))
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
                .ok_or_else(|| anyhow!("invalid constructor index {constructor_index} in subtable {table_name}"))?;
            RuntimeSelection {
                constructor,
                constructor_index,
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
        if terminal_reselect_trace_enabled() {
            eprintln!(
                "[decode-subtable selection] table={} ctor={} mnemonic={} source={}",
                table_name,
                selection.constructor_index,
                selection.constructor.mnemonic,
                selection.constructor.source,
            );
        }

        bind_instruction(self.compiled, self.native, &sub_ctx, selection)
    }
}
