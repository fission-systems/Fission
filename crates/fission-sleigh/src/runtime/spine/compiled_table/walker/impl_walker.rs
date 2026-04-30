impl<'a, 'b> CompiledParserWalker<'a, 'b> {
    fn new(
        compiled: &'a CompiledFrontend,
        strategy: RuntimeDecodeStrategy<'a>,
        ctx: &'a CompiledInstructionContext<'b>,
        selection: RuntimeSelection<'a>,
    ) -> Result<Self> {
        let opcode_len = 0;
        let minimum_length = selection.constructor.minimum_length as usize;
        let handles = vec![None; selection.constructor.constructor_template.handles.len()];
        if std::env::var("FISSION_REL_FALLBACK_DEBUG").is_ok() {
            eprintln!(
                "[bind-instr] bucket={} opcode_len={opcode_len} ctx.cursor={} sel_src={:?} \
                 matcher_len={} matcher={:?}",
                selection.trace.root_bucket, ctx.cursor, selection.constructor.constructor_template.template_source,
                matcher_instruction_length(&selection.constructor.matcher),
                selection.constructor.matcher
            );
        }
        Ok(Self {
            compiled,
            strategy,
            ctx,
            selection,
            minimum_length,
            context_register: ctx.context_register,
            context_known_mask: ctx.context_known_mask,
            cursor: ctx.cursor + opcode_len,
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
        for step in decode_steps {
            match step {
                CompiledOperandDecodeStep::ConsumeTokenFields => {
                    bail!("legacy token-bundle decode step is not canonical .sla metadata");
                }
                CompiledOperandDecodeStep::DecodeOperand { operand_index } => {
                    self.decode_operand(operand_index)?;
                }
                CompiledOperandDecodeStep::DescendSubtable {
                    table_name,
                    replace_current,
                } => {
                    // For non-shared-cursor architectures, look up reloffset/offsetbase
                    // from the handle template whose spec is SubtableEvaluation for this table.
                    let (reloffset, offsetbase) = self
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
                                    return Some((Some(reloffset), Some(offsetbase)));
                                }
                            }
                            None
                        })
                        .unwrap_or((None, None));
                    let sub_state = self.decode_subtable(&table_name, reloffset, offsetbase)?;
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

        let condition_code = None;

        let base_length = self.cursor.max(self.ctx.cursor + self.minimum_length);
        let length = base_length;

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
            (None, offset_offset.wrapping_mul(addr_unit), 0u32, None, 0u64)
        } else {
            (offset_space, offset_offset, offset_size, temp_space, temp_offset)
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
                if let Some(found) = self.compiled.sla_spaces.get(&index) {
                    return Ok(found.clone());
                }
                let name = if index == 0 { "const" } else { "unknown" };
                Ok(CompiledSpaceRef {
                    name: name.to_string(),
                    index,
                    word_size: if index == 0 { 0 } else { 1 },
                    addr_size: 0,
                    sleigh_delay_slots: -1,
                    sleigh_is_ram_class: false,
                    sleigh_is_unique_space: false,
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
            fixed: binding.fixed.ok_or_else(|| {
                anyhow!("operand {operand_index} did not materialize an exported fixed handle")
            })?,
            debug_value: binding.debug_value,
            subtable_state: binding.subtable_state.map(Box::new),
        });
        Ok(())
    }

    fn bind_operand(&mut self, template: &CompiledHandleTemplate) -> Result<OperandBinding> {
        match &template.spec {
            CompiledOperandSpec::TokenFieldExtraction { .. } => {
                bail!("legacy token-field extraction operand is not canonical .sla metadata")
            }
            CompiledOperandSpec::SlaTokenField {
                big_endian,
                sign_bit,
                bit_start,
                bit_end,
                byte_start,
                byte_end,
                shift,
                reloffset,
            } => {
                let token_base = self.token_base_for_sla_field(*reloffset);
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
                let size = ((*byte_end - *byte_start) + 1).max(1);
                self.cursor = self.cursor.max(token_base + size as usize);
                Ok(OperandBinding::with_fixed(
                    Some(BoundOperand::Immediate {
                        value,
                        encoded_size: size,
                        signed: *sign_bit,
                    }),
                    fixed_const_handle(value, size),
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
                reloffset,
            } => {
                let token_base = self.token_base_for_sla_field(*reloffset);
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
                self.cursor = self
                    .cursor
                    .max(token_base + ((*byte_end - *byte_start) + 1) as usize);
                let entry = entries.get(selector as usize).ok_or_else(|| {
                    anyhow!(
                        "varnode list selector {} out of range for {} entries",
                        selector,
                        entries.len()
                    )
                })?;
                Ok(OperandBinding::with_fixed(
                    Some(BoundOperand::NamedVarnode {
                        name: entry.name.clone(),
                        display_index: Some(selector as u32),
                        size: entry.size,
                    }),
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
                reloffset,
            } => {
                let token_base = self.token_base_for_sla_field(*reloffset);
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
                let size = ((*byte_end - *byte_start) + 1).max(1);
                self.cursor = self.cursor.max(token_base + size as usize);
                let value = values.get(selector as usize).copied().ok_or_else(|| {
                    anyhow!(
                        "value map selector {} out of range for {} entries",
                        selector,
                        values.len()
                    )
                })?;
                Ok(OperandBinding::with_fixed(
                    Some(BoundOperand::Immediate {
                        value: value as u64,
                        encoded_size: size,
                        signed: *sign_bit || value < 0,
                    }),
                    fixed_const_handle(value as u64, size),
                ))
            }
            CompiledOperandSpec::SlaFixedVarnode { varnode } => Ok(OperandBinding::with_fixed(
                Some(BoundOperand::NamedVarnode {
                    name: varnode.name.clone(),
                    display_index: None,
                    size: varnode.size,
                }),
                fixed_handle_from_resolved_varnode(varnode),
            )),
            CompiledOperandSpec::SlaPatternExpression { expr, reloffset } => {
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
                    let token_base = self.token_base_for_sla_field(*reloffset);
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
                };
                Ok(OperandBinding::with_fixed(
                    Some(BoundOperand::Immediate {
                        value: value as u64,
                        encoded_size: 0,
                        signed: value < 0,
                    }),
                    fixed_const_handle(value as u64, 0),
                ))
            }
            CompiledOperandSpec::ContextFieldExtraction {
                ..
            } => {
                bail!("legacy context-field extraction operand is not canonical .sla metadata")
            }
            CompiledOperandSpec::SubtableEvaluation {
                table_name,
                reloffset,
                offsetbase,
            } => {
                let sub_state = self.decode_subtable(table_name, Some(*reloffset), Some(*offsetbase))?;
                self.minimum_length = self
                    .minimum_length
                    .max(sub_state.length.saturating_sub(self.ctx.cursor));
                self.cursor = self.cursor.max(sub_state.length);
                let exported = match sub_state.exported_handle.as_ref() {
                    Some(exported) => exported,
                    None => bail!("subtable {table_name} did not export a handle"),
                };
                let value = display_value_for_exported_handle(exported, &sub_state);
                Ok(OperandBinding {
                    debug_value: Some(value),
                    fixed: Some(exported.fixed.clone()),
                    subtable_state: Some(sub_state),
                })
            }
            CompiledOperandSpec::Immediate { .. } => {
                bail!("legacy immediate operand is not canonical .sla metadata")
            }
            CompiledOperandSpec::Relative { .. } => {
                bail!("legacy relative operand is not canonical .sla metadata")
            }
            CompiledOperandSpec::FixedRegister { .. } => {
                bail!("legacy fixed-register operand is not canonical .sla metadata")
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

    fn token_base_for_sla_field(&self, reloffset: i32) -> usize {
        (self.ctx.cursor as i64 + reloffset as i64).max(0) as usize
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
                    .ok_or_else(|| {
                        anyhow!("operand {} was not decoded for pattern expression", index)
                    })?;
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
                    _ => bail!(
                        "operand {index} debug value is not a canonical scalar pattern source"
                    ),
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
        &self,
        table_name: &str,
        reloffset: Option<i32>,
        offsetbase: Option<i32>,
    ) -> Result<RuntimeConstructState> {
        let mut sub_ctx = (*self.ctx).clone();
        sub_ctx.cursor = if reloffset.is_some_and(|rel| rel >= 0)
            && offsetbase.unwrap_or(-1) < 0
        {
            self.ctx.cursor + reloffset.unwrap() as usize
        } else if constructor_replaces_current(self.selection.constructor) && table_name == "instruction" {
            self.ctx.cursor
        } else if self.selection.constructor.context_changes.is_empty()
        {
            self.cursor
        } else {
            self.cursor
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
