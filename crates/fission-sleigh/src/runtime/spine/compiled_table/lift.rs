pub(super) fn emit_pcode_for_state(
    compiled: &CompiledFrontend,
    address: u64,
    decoded: &RuntimeConstructState,
) -> Result<(Vec<PcodeOp>, RuntimeExecutionDetails)> {
    let mut emitter = CompiledTableEmitter::new(address);
    let details = RuntimeTemplateEvaluator::new(&mut emitter)
        .emit(&compiled.entry_id, decoded)
        .map_err(|err| template_emit_error(compiled, err))?;
    Ok((emitter.finish(), details))
}

pub(super) fn template_emit_error(compiled: &CompiledFrontend, err: anyhow::Error) -> anyhow::Error {
    let msg = err.to_string();
    if msg.contains("HandleTpl")
        || msg.contains("ConstTpl")
        || msg.contains("unsupported")
        || msg.contains("compatibility varnode template")
    {
        RuntimeSleighError::UnsupportedPcodeTemplate {
            language: compiled.entry_id.clone(),
            reason: format!("emission_time_template_resolution_failed: {msg}"),
        }
        .into()
    } else {
        err
    }
}

pub(super) fn compiled_space(name: &str, index: u64) -> CompiledSpaceRef {
    CompiledSpaceRef {
        name: name.to_string(),
        index,
    }
}

pub(super) fn register_offset(index: u8) -> u64 {
    if index < 8 {
        (index as u64) * 8
    } else {
        128 + ((index as u64) - 8) * 8
    }
}

pub(super) fn fixed_handle_for_bound_operand(value: &BoundOperand) -> RuntimeFixedHandle {
    match value {
        BoundOperand::Register { index, size } => RuntimeFixedHandle {
            space: Some(compiled_space("register", 4)),
            size: *size,
            offset_space: None,
            offset_offset: register_offset(*index),
            offset_size: *size,
            temp_space: None,
            temp_offset: 0,
            fixable: true,
        },
        BoundOperand::NamedVarnode { size, .. } => RuntimeFixedHandle {
            space: None,
            size: *size,
            offset_space: None,
            offset_offset: 0,
            offset_size: *size,
            temp_space: None,
            temp_offset: 0,
            fixable: false,
        },
        BoundOperand::Immediate {
            value,
            encoded_size,
            ..
        } => RuntimeFixedHandle {
            space: Some(compiled_space("const", 0)),
            size: *encoded_size,
            offset_space: None,
            offset_offset: *value,
            offset_size: *encoded_size,
            temp_space: None,
            temp_offset: 0,
            fixable: true,
        },
        BoundOperand::Relative { target } => RuntimeFixedHandle {
            space: Some(compiled_space("ram", 3)),
            size: 8,
            offset_space: None,
            offset_offset: *target,
            offset_size: 8,
            temp_space: None,
            temp_offset: 0,
            fixable: true,
        },
        BoundOperand::Memory {
            base,
            index,
            displacement,
            rip_relative,
            size,
            ..
        } if base.is_some() && index.is_none() && *displacement == 0 && !*rip_relative => {
            RuntimeFixedHandle {
                // SpaceId constants in STORE/LOAD use Ghidra's address-space
                // id, while actual pointer varnodes stay in the register space.
                space: Some(compiled_space("ram", 0x1b1)),
                size: *size,
                offset_space: Some(compiled_space("register", 4)),
                offset_offset: register_offset(base.expect("checked above")),
                offset_size: 8,
                temp_space: Some(compiled_space("unique", 2)),
                temp_offset: 0xd400,
                fixable: true,
            }
        }
        BoundOperand::Memory {
            rip_relative: true,
            absolute: Some(absolute),
            size,
            ..
        } => RuntimeFixedHandle {
            space: Some(compiled_space("ram", 3)),
            size: *size,
            offset_space: None,
            offset_offset: *absolute,
            offset_size: *size,
            temp_space: None,
            temp_offset: 0,
            fixable: true,
        },
        BoundOperand::Memory { size, .. } => RuntimeFixedHandle {
            space: Some(compiled_space("ram", 0x1b1)),
            size: *size,
            offset_space: None,
            offset_offset: 0,
            offset_size: 0,
            temp_space: None,
            temp_offset: 0,
            fixable: false,
        },
    }
}

pub(super) fn fixed_handle_from_resolved_varnode(
    varnode: &crate::compiler::CompiledResolvedVarnode,
) -> RuntimeFixedHandle {
    RuntimeFixedHandle {
        space: Some(varnode.space.clone()),
        size: varnode.size,
        offset_space: None,
        offset_offset: varnode.offset,
        offset_size: varnode.size,
        temp_space: None,
        temp_offset: 0,
        fixable: true,
    }
}

pub(super) fn bound_operand_from_fixed_handle(handle: &RuntimeFixedHandle) -> Result<BoundOperand> {
    let space = handle
        .space
        .as_ref()
        .ok_or_else(|| anyhow!("exported fixed handle missing space"))?;
    if space.index == 0 || space.name == "const" {
        return Ok(BoundOperand::Immediate {
            value: handle.offset_offset,
            encoded_size: handle.size.max(1),
            signed: false,
        });
    }
    if space.name == "register" || space.index == 4 {
        return Ok(BoundOperand::NamedVarnode {
            name: format!("register_{:x}", handle.offset_offset),
            display_index: None,
            size: handle.size,
        });
    }
    Ok(BoundOperand::Memory {
        base: None,
        index: None,
        scale: 1,
        displacement: handle.offset_offset as i64,
        rip_relative: false,
        absolute: Some(handle.offset_offset),
        size: handle.size,
    })
}

pub(super) fn varnode_from_fixed_handle(handle: &RuntimeFixedHandle) -> Result<Varnode> {
    if handle.offset_space.is_some() {
        bail!("dynamic fixed handle cannot materialize into a direct varnode");
    }
    let space = handle
        .space
        .as_ref()
        .ok_or_else(|| anyhow!("fixed handle missing space"))?;
    let size = if handle.size > 0 {
        handle.size
    } else {
        handle.offset_size
    };
    if space.name == "const" {
        Ok(Varnode::constant(handle.offset_offset as i64, size))
    } else {
        Ok(Varnode {
            space_id: space.index,
            offset: handle.offset_offset,
            size,
            is_constant: false,
            constant_val: 0,
        })
    }
}

pub(super) fn handle_selector_index_in_space(
    space: &CompiledSpaceTpl,
    selector: CompiledHandleSelector,
) -> Option<usize> {
    let CompiledSpaceTpl::Const(const_tpl) = space else {
        return None;
    };
    handle_selector_index(const_tpl, selector)
}

pub(super) fn negative_handle_selector_index_in_space(
    space: &CompiledSpaceTpl,
    selector: CompiledHandleSelector,
) -> Option<i64> {
    let CompiledSpaceTpl::Const(const_tpl) = space else {
        return None;
    };
    let CompiledConstTpl::Handle {
        handle_index,
        selector: actual_selector,
        plus,
    } = const_tpl.as_ref()
    else {
        return None;
    };
    if *actual_selector == selector && plus.is_none() && *handle_index < 0 {
        Some(*handle_index)
    } else {
        None
    }
}

pub(super) fn matches_handle_selector(
    const_tpl: &CompiledConstTpl,
    handle_index: usize,
    selector: CompiledHandleSelector,
) -> bool {
    handle_selector_index(const_tpl, selector).is_some_and(|idx| idx == handle_index)
}

pub(super) fn handle_selector_index(
    const_tpl: &CompiledConstTpl,
    expected_selector: CompiledHandleSelector,
) -> Option<usize> {
    let CompiledConstTpl::Handle {
        handle_index,
        selector,
        plus,
    } = const_tpl
    else {
        return None;
    };
    if *selector != expected_selector || plus.is_some() || *handle_index < 0 {
        return None;
    }
    Some(*handle_index as usize)
}

pub(super) fn matches_negative_handle_selector(
    const_tpl: &CompiledConstTpl,
    handle_index: i64,
    expected_selector: CompiledHandleSelector,
) -> bool {
    let CompiledConstTpl::Handle {
        handle_index: actual_handle_index,
        selector,
        plus,
    } = const_tpl
    else {
        return false;
    };
    *actual_handle_index == handle_index && *selector == expected_selector && plus.is_none()
}

#[derive(Debug, Clone)]
pub(super) struct CompiledTableEmitter {
    emitter: RuntimePcodeEmitter,
    address: u64,
    built_operands: std::collections::BTreeSet<usize>,
    /// Exported varnodes produced by BUILD subconstructors. Ghidra templates
    /// reference these through negative handle indices in parent templates.
    exported_build_varnodes: std::collections::BTreeMap<i64, Varnode>,
}

#[derive(Debug, Clone)]
pub(super) struct DynamicMemoryTarget {
    space: Varnode,
    ptr: Varnode,
    temp: Varnode,
    size: u32,
}

impl CompiledTableEmitter {
    fn new(address: u64) -> Self {
        // Use Ghidra-compatible unique offset base: 0x9300 is the first
        // canonical unique temp used by the x86 Addr sub-constructors.
        // Ghidra computes: (addr & uniquemask) << 8, but for the SLA
        // template temps these are static offsets baked into the spec.
        // We start at 0x9300 to match Ghidra's allocation pattern.
        Self {
            address,
            emitter: RuntimePcodeEmitter::new(address, 0x9300),
            built_operands: std::collections::BTreeSet::new(),
            exported_build_varnodes: std::collections::BTreeMap::new(),
        }
    }

    fn finish(self) -> Vec<PcodeOp> {
        self.emitter.finish()
    }

    fn emit_op_template(
        &mut self,
        state: &RuntimeConstructState,
        op: &CompiledOpTpl,
    ) -> Result<()> {
        let mnemonic = op.opcode.as_str();
        match op.opcode {
            CompiledOpTplOpcode::Label => Ok(()),
            CompiledOpTplOpcode::Return => {
                if op.output.is_some() || op.inputs.len() > 1 {
                    bail!("RETURN template shape is unsupported");
                }
                if let Some(target_tpl) = op.inputs.first() {
                    let target = self.read_template_varnode(target_tpl, state, 8)?;
                    self.emitter.emit_return_target(target, mnemonic)
                } else {
                    self.emitter.emit_return(mnemonic)
                }
            }
            CompiledOpTplOpcode::Call => {
                let target_tpl = op
                    .inputs
                    .first()
                    .ok_or_else(|| anyhow!("CALL template requires one input"))?;
                let target = self.read_template_varnode(target_tpl, state, 8)?;
                self.emitter.emit_call(target, mnemonic)
            }
            CompiledOpTplOpcode::Branch => {
                let target_tpl = op
                    .inputs
                    .first()
                    .ok_or_else(|| anyhow!("BRANCH template requires one input"))?;
                let target = self.read_template_varnode(target_tpl, state, 8)?;
                let is_indirect = matches!(target_tpl, CompiledVarnodeTpl::Handle { operand_index }
                    if !matches!(state.operands.get(*operand_index), Some(BoundOperand::Relative { .. })));
                if is_indirect {
                    self.emitter.emit_branch_ind(target, mnemonic)
                } else {
                    self.emitter.emit_branch(target, mnemonic)
                }
            }
            CompiledOpTplOpcode::Copy => {
                let out_tpl = op
                    .output
                    .as_ref()
                    .ok_or_else(|| anyhow!("COPY template requires output"))?;
                let out_size = self.template_varnode_size(out_tpl, state)?;
                let input_tpl = op
                    .inputs
                    .first()
                    .ok_or_else(|| anyhow!("COPY template requires one input"))?;

                // Read input at its natural size (pass 0 = accept any size).
                // The SUBPIECE path below handles any size mismatch.
                let value = self.read_template_varnode(input_tpl, state, 0)?;

                if let Some(target) = self.dynamic_memory_target(out_tpl, state)? {
                    let store_value = if value.is_constant {
                        self.emitter
                            .emit_copy(target.temp.clone(), value, mnemonic)?;
                        target.temp
                    } else {
                        value
                    };
                    if store_value.size != target.size {
                        bail!(
                            "dynamic memory STORE value size {} did not match target size {}",
                            store_value.size,
                            target.size
                        );
                    }
                    self.emitter
                        .emit_store(target.space, target.ptr, store_value, mnemonic)
                } else if value.size > out_size && out_size > 0 {
                    // Size mismatch: source is wider than destination.
                    // Emit SUBPIECE to truncate (matches Ghidra's behavior).
                    let out = self.template_write_target(out_tpl, state)?;
                    let trunc_const = Varnode::constant(0, 4);
                    self.emitter.append_checked(
                        fission_pcode::PcodeOpcode::SubPiece,
                        Some(out.clone()),
                        vec![value, trunc_const],
                        "SUBPIECE",
                    )?;
                    // In x86-64, writing to a 32-bit register implicitly
                    // zero-extends to 64 bits. Emit INT_ZEXT if the output
                    // is a 4-byte register (Ghidra does this explicitly).
                    if out.size == 4 && out.space_id == 4 {
                        let extended = Varnode {
                            size: 8,
                            ..out.clone()
                        };
                        self.emitter.append_checked(
                            fission_pcode::PcodeOpcode::IntZExt,
                            Some(extended),
                            vec![out],
                            "INT_ZEXT",
                        )?;
                    }
                    Ok(())
                } else {
                    self.write_template_target(out_tpl, value, state, mnemonic)
                }
            }
            CompiledOpTplOpcode::IntZExt
            | CompiledOpTplOpcode::IntSExt
            | CompiledOpTplOpcode::BoolNegate
            | CompiledOpTplOpcode::PopCount => {
                let out_tpl = op
                    .output
                    .as_ref()
                    .ok_or_else(|| anyhow!("{} template requires output", mnemonic))?;
                let input_tpl = op
                    .inputs
                    .first()
                    .ok_or_else(|| anyhow!("{} template requires one input", mnemonic))?;
                let input = self.read_template_varnode(input_tpl, state, 0)?;
                let out = self.materialize_write_varnode(out_tpl, state, mnemonic)?;
                let opcode = self.unary_pcode_opcode(op.opcode)?;
                self.emitter
                    .emit_int_unop(opcode, out.clone(), input, mnemonic)?;
                self.commit_template_write_target(out_tpl, out, state, mnemonic)
            }
            CompiledOpTplOpcode::IntAdd
            | CompiledOpTplOpcode::IntSub
            | CompiledOpTplOpcode::IntCarry
            | CompiledOpTplOpcode::IntSCarry
            | CompiledOpTplOpcode::IntSBorrow
            | CompiledOpTplOpcode::IntAnd
            | CompiledOpTplOpcode::IntOr
            | CompiledOpTplOpcode::IntXor
            | CompiledOpTplOpcode::IntMult
            | CompiledOpTplOpcode::IntLeft
            | CompiledOpTplOpcode::IntRight
            | CompiledOpTplOpcode::IntSRight
            | CompiledOpTplOpcode::IntEqual
            | CompiledOpTplOpcode::IntNotEqual
            | CompiledOpTplOpcode::IntLess
            | CompiledOpTplOpcode::IntSLess
            | CompiledOpTplOpcode::BoolAnd
            | CompiledOpTplOpcode::BoolOr => {
                let out_tpl = op
                    .output
                    .as_ref()
                    .ok_or_else(|| anyhow!("{} template requires output", mnemonic))?;
                if op.inputs.len() != 2 {
                    bail!("{mnemonic} template requires two inputs");
                }
                let mut lhs = self.read_template_varnode(&op.inputs[0], state, 0)?;
                let mut rhs = self.read_template_varnode(&op.inputs[1], state, 0)?;

                // Enforce P-code invariants: binary op inputs should generally match in size.
                // Ghidra's SLEIGH compiler implicitly folds constants to the required size,
                // but Fission bypasses subconstructors, so we must promote constants here.
                if lhs.is_constant && rhs.size > lhs.size {
                    lhs.size = rhs.size;
                } else if rhs.is_constant && lhs.size > rhs.size {
                    rhs.size = lhs.size;
                }

                let out = self.materialize_write_varnode(out_tpl, state, mnemonic)?;
                let opcode = self.binary_pcode_opcode(op.opcode)?;
                self.emitter
                    .emit_int_binop(opcode, out.clone(), lhs, rhs, mnemonic)?;
                self.commit_template_write_target(out_tpl, out, state, mnemonic)
            }
            CompiledOpTplOpcode::Load => {
                let out_tpl = op
                    .output
                    .as_ref()
                    .ok_or_else(|| anyhow!("LOAD template requires output"))?;
                if op.inputs.len() != 2 {
                    bail!("LOAD template requires two inputs");
                }
                let _out_size = self.template_varnode_size(out_tpl, state)?;
                let space = self.read_template_varnode(&op.inputs[0], state, 8)?;
                let ptr = self.read_template_varnode(&op.inputs[1], state, 8)?;
                let out = self.materialize_write_varnode(out_tpl, state, mnemonic)?;
                self.emitter.emit_load(out.clone(), space, ptr, mnemonic)?;
                self.commit_template_write_target(out_tpl, out, state, mnemonic)
            }
            CompiledOpTplOpcode::Store => {
                if op.output.is_some() || op.inputs.len() != 3 {
                    bail!("STORE template requires three inputs and no output");
                }
                let space = self.read_template_varnode(&op.inputs[0], state, 8)?;
                let ptr = self.read_template_varnode(&op.inputs[1], state, 8)?;
                let value_size = self.template_varnode_size(&op.inputs[2], state)?;
                let value = self.read_template_varnode(&op.inputs[2], state, value_size)?;
                self.emitter.emit_store(space, ptr, value, mnemonic)
            }
            CompiledOpTplOpcode::Piece
            | CompiledOpTplOpcode::Subpiece
            | CompiledOpTplOpcode::CBranch => {
                let out_tpl = op.output.as_ref();
                if matches!(op.opcode, CompiledOpTplOpcode::CBranch) {
                    if out_tpl.is_some() || op.inputs.len() != 2 {
                        bail!("CBRANCH template requires two inputs and no output");
                    }
                    let target = self.read_template_varnode(&op.inputs[0], state, 8)?;
                    let cond = self.read_template_varnode(&op.inputs[1], state, 1)?;
                    self.emitter.emit_cbranch(target, cond, mnemonic)
                } else {
                    let out_tpl =
                        out_tpl.ok_or_else(|| anyhow!("{} template requires output", mnemonic))?;
                    if op.inputs.len() != 2 {
                        bail!("{mnemonic} template requires two inputs");
                    }
                    let out_size = self.template_varnode_size(out_tpl, state)?;
                    let lhs_expected_size = if matches!(op.opcode, CompiledOpTplOpcode::Subpiece)
                    {
                        0
                    } else {
                        out_size
                    };
                    let lhs = self.read_template_varnode(&op.inputs[0], state, lhs_expected_size)?;
                    let rhs_size = self.template_varnode_size(&op.inputs[1], state)?;
                    let rhs = self.read_template_varnode(&op.inputs[1], state, rhs_size)?;
                    let out = self.materialize_write_varnode(out_tpl, state, mnemonic)?;
                    let opcode = self.binary_pcode_opcode(op.opcode)?;
                    self.emitter
                        .emit_int_binop(opcode, out.clone(), lhs, rhs, mnemonic)?;
                    self.commit_template_write_target(out_tpl, out, state, mnemonic)
                }
            }
            // Build: subtable inlining directive. Ghidra executes the matched
            // operand/sub-constructor template; it does not synthesize an
            // architecture-specific effective-address expression from the
            // already decoded display operand.
            CompiledOpTplOpcode::Build => {
                let operand_index = if let Some(input_tpl) = op.inputs.first() {
                    match input_tpl {
                        CompiledVarnodeTpl::Varnode { offset, .. } => match offset.as_ref() {
                            CompiledConstTpl::Real { value } => Some(*value as usize),
                            _ => None,
                        },
                        _ => None,
                    }
                } else {
                    None
                };
                if let Some(idx) = operand_index {
                    if idx == 0
                        && state.condition_code.is_some()
                        && matches!(state.construct_tpl_kind, CompiledConstructTplKind::Jcc)
                    {
                        let predicate = self.emit_condition_predicate(
                            state.condition_code.expect("checked above"),
                            "cc",
                        )?;
                        self.exported_build_varnodes.insert(-1, predicate);
                    }
                    self.emit_build_operand(state, idx)?;
                }
                Ok(())
            }
            // CallOther: user-defined pcodeop. Ghidra emits this as a real
            // CALLOTHER P-code op. Input[0] is the pcodeop index.
            CompiledOpTplOpcode::CallOther => {
                let mut inputs = Vec::new();
                for input in &op.inputs {
                    let size = self.template_varnode_size(input, state).unwrap_or(8);
                    inputs.push(self.read_template_varnode(input, state, size)?);
                }
                let output = if let Some(ref out_ref) = op.output {
                    Some(self.materialize_write_varnode(out_ref, state, mnemonic)?)
                } else {
                    None
                };
                self.emitter.emit_callother(output, inputs, mnemonic)
            }
            CompiledOpTplOpcode::Unsupported => bail!(
                "compiled op template {} is not executable in compiled-table cutover",
                mnemonic
            ),
        }
    }

    fn emit_build_operand(
        &mut self,
        state: &RuntimeConstructState,
        operand_index: usize,
    ) -> Result<()> {
        if !self.built_operands.insert(operand_index) {
            return Ok(());
        }
        let handle = state
            .handles
            .get(operand_index)
            .ok_or_else(|| anyhow!("BUILD operand {operand_index} has no bound handle"))?;
        let Some(child) = handle.subtable_state.as_deref() else {
            return Ok(());
        };
        if child.constructor_template.template_source != CompiledTemplateSource::SpecDerived {
            bail!("BUILD operand {operand_index} is not backed by a SpecDerived subconstructor");
        }
        for child_op in &child.constructor_template.op_templates {
            self.emit_op_template(child, child_op)?;
        }
        if let Some(exported) = child.exported_handle.as_ref() {
            if let Ok(varnode) = varnode_from_fixed_handle(&exported.fixed) {
                let handle_key = -((operand_index as i64) + 1);
                self.exported_build_varnodes.insert(handle_key, varnode);
            }
        }
        Ok(())
    }

    fn template_varnode_size(
        &mut self,
        template: &CompiledVarnodeTpl,
        state: &RuntimeConstructState,
    ) -> Result<u32> {
        match template {
            CompiledVarnodeTpl::Varnode { size, .. } => {
                let value = self.resolve_const_value(size, state)?;
                u32::try_from(value).map_err(|_| anyhow!("VarnodeTpl size {value} exceeds u32"))
            }
            CompiledVarnodeTpl::HandleTpl(handle_tpl) => {
                if let Some(size) = &handle_tpl.size {
                    let value = self.resolve_const_value(size, state)?;
                    u32::try_from(value).map_err(|_| anyhow!("HandleTpl size {value} exceeds u32"))
                } else {
                    Ok(0)
                }
            }
            _ => bail!("compiled-table executor rejects compatibility varnode template"),
        }
    }

    fn read_template_varnode(
        &mut self,
        template: &CompiledVarnodeTpl,
        state: &RuntimeConstructState,
        expected_size: u32,
    ) -> Result<Varnode> {
        match template {
            CompiledVarnodeTpl::Varnode { .. } | CompiledVarnodeTpl::HandleTpl(_) => {
                let varnode = self.resolve_varnode_tpl(template, state)?;
                if expected_size > 0 && varnode.size != expected_size {
                    bail!(
                        "VarnodeTpl size {} did not match expected size {expected_size}",
                        varnode.size
                    );
                }
                Ok(varnode)
            }
            CompiledVarnodeTpl::ConditionPredicate => {
                if let Some(cc) = state.condition_code {
                    let out = self.emit_condition_predicate(cc, "Jcc")?;
                    if expected_size > 0 && out.size != expected_size {
                        bail!("ConditionPredicate size mismatch");
                    }
                    Ok(out)
                } else {
                    bail!("ConditionPredicate used but condition_code is missing")
                }
            }
            CompiledVarnodeTpl::Handle { operand_index } => {
                let handle = state
                    .handles
                    .get(*operand_index)
                    .ok_or_else(|| anyhow!("missing operand index {}", operand_index))?;
                if matches!(
                    handle.value,
                    crate::runtime::spine::construct::BoundOperand::Memory { .. }
                ) {
                    bail!("Handle used for memory operand - expected EffectiveAddress");
                }
                let varnode = varnode_from_fixed_handle(&handle.fixed)?;
                if expected_size > 0
                    && varnode.size != expected_size
                    && !matches!(
                        handle.value,
                        crate::runtime::spine::construct::BoundOperand::Relative { .. }
                    )
                {
                    bail!("Handle size mismatch");
                }
                Ok(varnode)
            }
            _ => bail!(
                "compiled-table executor rejects compatibility varnode template: {:?}",
                template
            ),
        }
    }

    fn emit_condition_predicate(&mut self, cc: u8, mnemonic: &str) -> Result<Varnode> {
        let cf = Varnode {
            space_id: 4,
            offset: 0x0200,
            size: 1,
            is_constant: false,
            constant_val: 0,
        };
        let pf = Varnode {
            space_id: 4,
            offset: 0x0202,
            size: 1,
            is_constant: false,
            constant_val: 0,
        };
        let zf = Varnode {
            space_id: 4,
            offset: 0x0206,
            size: 1,
            is_constant: false,
            constant_val: 0,
        };
        let sf = Varnode {
            space_id: 4,
            offset: 0x0207,
            size: 1,
            is_constant: false,
            constant_val: 0,
        };
        let of = Varnode {
            space_id: 4,
            offset: 0x020B,
            size: 1,
            is_constant: false,
            constant_val: 0,
        };

        match cc {
            0 => Ok(of), // O
            1 => {
                // NO
                let out = self.emitter.tmp(2, 1);
                self.emitter.append_checked(
                    fission_pcode::PcodeOpcode::BoolNegate,
                    Some(out.clone()),
                    vec![of],
                    mnemonic,
                )?;
                Ok(out)
            }
            2 => Ok(cf), // B/C/NAE
            3 => {
                // NB/NC/AE
                let out = self.emitter.tmp(2, 1);
                self.emitter.append_checked(
                    fission_pcode::PcodeOpcode::BoolNegate,
                    Some(out.clone()),
                    vec![cf],
                    mnemonic,
                )?;
                Ok(out)
            }
            4 => Ok(zf), // Z/E
            5 => {
                // NZ/NE
                let out = self.emitter.tmp(2, 1);
                self.emitter.append_checked(
                    fission_pcode::PcodeOpcode::BoolNegate,
                    Some(out.clone()),
                    vec![zf],
                    mnemonic,
                )?;
                Ok(out)
            }
            6 => {
                // BE/NA (CF || ZF)
                let out = self.emitter.tmp(2, 1);
                self.emitter.append_checked(
                    fission_pcode::PcodeOpcode::BoolOr,
                    Some(out.clone()),
                    vec![cf, zf],
                    mnemonic,
                )?;
                Ok(out)
            }
            7 => {
                // NBE/A !(CF || ZF)
                let tmp = self.emitter.tmp(2, 1);
                self.emitter.append_checked(
                    fission_pcode::PcodeOpcode::BoolOr,
                    Some(tmp.clone()),
                    vec![cf, zf],
                    mnemonic,
                )?;
                let out = self.emitter.tmp(2, 1);
                self.emitter.append_checked(
                    fission_pcode::PcodeOpcode::BoolNegate,
                    Some(out.clone()),
                    vec![tmp],
                    mnemonic,
                )?;
                Ok(out)
            }
            8 => Ok(sf), // S
            9 => {
                // NS
                let out = self.emitter.tmp(2, 1);
                self.emitter.append_checked(
                    fission_pcode::PcodeOpcode::BoolNegate,
                    Some(out.clone()),
                    vec![sf],
                    mnemonic,
                )?;
                Ok(out)
            }
            10 => Ok(pf), // P/PE
            11 => {
                // NP/PO
                let out = self.emitter.tmp(2, 1);
                self.emitter.append_checked(
                    fission_pcode::PcodeOpcode::BoolNegate,
                    Some(out.clone()),
                    vec![pf],
                    mnemonic,
                )?;
                Ok(out)
            }
            12 => {
                // L/NGE (SF != OF)
                let out = self.emitter.tmp(2, 1);
                self.emitter.append_checked(
                    fission_pcode::PcodeOpcode::IntNotEqual,
                    Some(out.clone()),
                    vec![of.clone(), sf.clone()],
                    mnemonic,
                )?;
                Ok(out)
            }
            13 => {
                // NL/GE (SF == OF)
                let out = self.emitter.tmp(2, 1);
                self.emitter.append_checked(
                    fission_pcode::PcodeOpcode::IntEqual,
                    Some(out.clone()),
                    vec![of.clone(), sf.clone()],
                    mnemonic,
                )?;
                Ok(out)
            }
            14 => {
                // LE/NG (ZF || (SF != OF))
                let tmp = self.emitter.tmp(2, 1);
                self.emitter.append_checked(
                    fission_pcode::PcodeOpcode::IntNotEqual,
                    Some(tmp.clone()),
                    vec![of.clone(), sf.clone()],
                    mnemonic,
                )?;
                let out = self.emitter.tmp(2, 1);
                self.emitter.append_checked(
                    fission_pcode::PcodeOpcode::BoolOr,
                    Some(out.clone()),
                    vec![zf.clone(), tmp],
                    mnemonic,
                )?;
                Ok(out)
            }
            15 => {
                // NLE/G (!ZF && (SF == OF))
                let tmp = self.emitter.tmp(2, 1);
                self.emitter.append_checked(
                    fission_pcode::PcodeOpcode::IntEqual,
                    Some(tmp.clone()),
                    vec![of, sf],
                    mnemonic,
                )?;
                let zf_not = self.emitter.tmp(2, 1);
                self.emitter.append_checked(
                    fission_pcode::PcodeOpcode::BoolNegate,
                    Some(zf_not.clone()),
                    vec![zf.clone()],
                    mnemonic,
                )?;
                let out = self.emitter.tmp(2, 1);
                self.emitter.append_checked(
                    fission_pcode::PcodeOpcode::BoolAnd,
                    Some(out.clone()),
                    vec![zf_not, tmp],
                    mnemonic,
                )?;
                Ok(out)
            }
            _ => bail!("invalid condition code {}", cc),
        }
    }

    fn materialize_write_varnode(
        &mut self,
        template: &CompiledVarnodeTpl,
        state: &RuntimeConstructState,
        _mnemonic: &str,
    ) -> Result<Varnode> {
        self.template_write_target(template, state)
    }

    fn commit_template_write_target(
        &mut self,
        template: &CompiledVarnodeTpl,
        value: Varnode,
        state: &RuntimeConstructState,
        mnemonic: &str,
    ) -> Result<()> {
        let _ = (value, mnemonic);
        self.template_write_target(template, state).map(|_| ())
    }

    fn write_template_target(
        &mut self,
        template: &CompiledVarnodeTpl,
        value: Varnode,
        state: &RuntimeConstructState,
        mnemonic: &str,
    ) -> Result<()> {
        let out = self.template_write_target(template, state)?;
        self.emitter.emit_copy(out, value, mnemonic)
    }

    fn template_write_target(
        &mut self,
        template: &CompiledVarnodeTpl,
        state: &RuntimeConstructState,
    ) -> Result<Varnode> {
        match template {
            CompiledVarnodeTpl::Varnode { .. } | CompiledVarnodeTpl::HandleTpl(_) => {
                self.resolve_varnode_tpl(template, state)
            }
            _ => bail!("compiled-table executor rejects compatibility varnode template"),
        }
    }

    fn dynamic_memory_target(
        &mut self,
        template: &CompiledVarnodeTpl,
        state: &RuntimeConstructState,
    ) -> Result<Option<DynamicMemoryTarget>> {
        let CompiledVarnodeTpl::Varnode {
            space,
            offset,
            size,
        } = template
        else {
            return Ok(None);
        };
        let Some(handle_index) =
            handle_selector_index_in_space(space, CompiledHandleSelector::Space)
        else {
            return Ok(None);
        };
        if !matches_handle_selector(offset, handle_index, CompiledHandleSelector::Offset) {
            return Ok(None);
        }
        let handle = state
            .handles
            .get(handle_index)
            .ok_or_else(|| anyhow!("handle {} is missing or unresolved", handle_index))?;
        if !matches!(handle.value, BoundOperand::Memory { .. }) {
            return Ok(None);
        }
        if !handle.fixed.fixable {
            bail!("dynamic memory handle {handle_index} is not fixable");
        }
        let space = handle
            .fixed
            .space
            .as_ref()
            .ok_or_else(|| anyhow!("dynamic memory handle {handle_index} missing space"))?;
        let offset_space =
            handle.fixed.offset_space.as_ref().ok_or_else(|| {
                anyhow!("dynamic memory handle {handle_index} missing offset space")
            })?;
        let temp_space =
            handle.fixed.temp_space.as_ref().ok_or_else(|| {
                anyhow!("dynamic memory handle {handle_index} missing temp space")
            })?;
        let target_size = u32::try_from(self.resolve_const_value(size, state)?)
            .map_err(|_| anyhow!("dynamic memory target size exceeds u32"))?;
        let ptr = Varnode {
            space_id: offset_space.index,
            offset: handle.fixed.offset_offset,
            size: handle.fixed.offset_size,
            is_constant: false,
            constant_val: 0,
        };
        Ok(Some(DynamicMemoryTarget {
            space: Varnode::constant(space.index as i64, 4),
            ptr,
            temp: Varnode {
                space_id: temp_space.index,
                offset: handle.fixed.temp_offset,
                size: target_size,
                is_constant: false,
                constant_val: 0,
            },
            size: target_size,
        }))
    }

    fn resolve_varnode_tpl(
        &mut self,
        template: &CompiledVarnodeTpl,
        state: &RuntimeConstructState,
    ) -> Result<Varnode> {
        match template {
            CompiledVarnodeTpl::Varnode {
                space,
                offset,
                size,
            } => {
                if let Some(exported_vn) =
                    self.exported_build_varnode(space, offset, size, state)?
                {
                    return Ok(exported_vn);
                }
                let space = self.resolve_space_tpl(space, state)?;
                if (space.index == 0 || space.name == "const")
                    && handle_selector_index(offset, CompiledHandleSelector::Offset).is_some()
                {
                    let handle_index = handle_selector_index(offset, CompiledHandleSelector::Offset)
                        .expect("checked above");
                    let handle = state
                        .handles
                        .get(handle_index)
                        .ok_or_else(|| anyhow!("handle {} is missing or unresolved", handle_index))?;
                    if let Some(offset_space) = &handle.fixed.offset_space {
                        let size = u32::try_from(self.resolve_const_value(size, state)?)
                            .map_err(|_| anyhow!("VarnodeTpl size exceeds u32"))?;
                        if offset_space.index == 0 || offset_space.name == "const" {
                            return Ok(Varnode::constant(handle.fixed.offset_offset as i64, size));
                        }
                        return Ok(Varnode {
                            space_id: offset_space.index,
                            offset: handle.fixed.offset_offset,
                            size,
                            is_constant: false,
                            constant_val: 0,
                        });
                    }
                }
                let offset = self.resolve_const_value(offset, state)?;
                let size = u32::try_from(self.resolve_const_value(size, state)?)
                    .map_err(|_| anyhow!("VarnodeTpl size exceeds u32"))?;
                if space.index == 0 || space.name == "const" {
                    let value = i64::try_from(offset)
                        .map_err(|_| anyhow!("constant VarnodeTpl offset {offset} exceeds i64"))?;
                    return Ok(Varnode::constant(value, size));
                }
                Ok(Varnode {
                    space_id: space.index,
                    offset,
                    size,
                    is_constant: false,
                    constant_val: 0,
                })
            }
            CompiledVarnodeTpl::HandleTpl(handle_tpl) => {
                if let Some(ptr_space_tpl) = &handle_tpl.ptr_space {
                    let ptr_space = self.resolve_space_tpl(ptr_space_tpl, state)?;
                    let ptr_offset = handle_tpl
                        .ptr_offset
                        .as_ref()
                        .map(|offset| self.resolve_const_value(offset, state))
                        .transpose()?
                        .unwrap_or(0);
                    let ptr_size = handle_tpl
                        .ptr_size
                        .as_ref()
                        .map(|size| self.resolve_const_value(size, state))
                        .transpose()?
                        .and_then(|size| u32::try_from(size).ok())
                        .unwrap_or(0);
                    if ptr_space.index == 0 || ptr_space.name == "const" {
                        let value = i64::try_from(ptr_offset).map_err(|_| {
                            anyhow!("constant HandleTpl ptr_offset {ptr_offset} exceeds i64")
                        })?;
                        return Ok(Varnode::constant(value, ptr_size));
                    }
                    return Ok(Varnode {
                        space_id: ptr_space.index,
                        offset: ptr_offset,
                        size: ptr_size,
                        is_constant: false,
                        constant_val: 0,
                    });
                }
                let space = if let Some(space) = &handle_tpl.space {
                    self.resolve_space_tpl(space, state)?
                } else {
                    bail!("HandleTpl missing space")
                };
                let offset = if let Some(offset) = &handle_tpl.ptr_offset {
                    self.resolve_const_value(offset, state)?
                } else {
                    bail!("HandleTpl missing ptr_offset")
                };
                let size = if let Some(size) = &handle_tpl.size {
                    u32::try_from(self.resolve_const_value(size, state)?)
                        .map_err(|_| anyhow!("HandleTpl size exceeds u32"))?
                } else {
                    0
                };
                if space.index == 0 || space.name == "const" {
                    return Ok(Varnode::constant(offset as i64, size));
                }
                Ok(Varnode {
                    space_id: space.index,
                    offset,
                    size,
                    is_constant: false,
                    constant_val: 0,
                })
            }
            _ => bail!("expected Ghidra VarnodeTpl or HandleTpl"),
        }
    }

    fn exported_build_varnode(
        &mut self,
        space: &CompiledSpaceTpl,
        offset: &CompiledConstTpl,
        size: &CompiledConstTpl,
        state: &RuntimeConstructState,
    ) -> Result<Option<Varnode>> {
        let Some(handle_index) =
            negative_handle_selector_index_in_space(space, CompiledHandleSelector::Space)
        else {
            return Ok(None);
        };
        if !matches_negative_handle_selector(offset, handle_index, CompiledHandleSelector::Offset)
            || !matches_negative_handle_selector(size, handle_index, CompiledHandleSelector::Size)
        {
            return Ok(None);
        }
        let varnode = self
            .exported_build_varnodes
            .get(&handle_index)
            .cloned()
            .ok_or_else(|| anyhow!("exported BUILD handle {handle_index} is missing"))?;
        let _ = (size, state);
        Ok(Some(varnode))
    }

    fn resolve_space_tpl(
        &mut self,
        template: &CompiledSpaceTpl,
        state: &RuntimeConstructState,
    ) -> Result<CompiledSpaceRef> {
        match template {
            CompiledSpaceTpl::SpaceRef(space) => Ok(space.clone()),
            CompiledSpaceTpl::Const(const_tpl) => {
                let space_id = self.resolve_const_value(const_tpl, state)?;
                let name = match space_id {
                    0 => "const",
                    2 => "unique",
                    3 => "ram",
                    4 => "register",
                    _ => "unknown",
                };
                Ok(CompiledSpaceRef {
                    name: name.to_string(),
                    index: space_id,
                })
            }
        }
    }

    fn resolve_const_value(
        &mut self,
        template: &CompiledConstTpl,
        state: &RuntimeConstructState,
    ) -> Result<u64> {
        match template {
            CompiledConstTpl::Real { value } => Ok(*value),
            CompiledConstTpl::Integer { value, .. } if *value >= 0 => Ok(*value as u64),
            CompiledConstTpl::Integer { value, .. } => {
                Ok((*value as i128 as u128 & u64::MAX as u128) as u64)
            }
            CompiledConstTpl::InstStart => Ok(self.address),
            CompiledConstTpl::InstNext => Ok(self.address.saturating_add(state.length as u64)),
            CompiledConstTpl::SpaceId(space) => Ok(space.index),
            CompiledConstTpl::Handle {
                handle_index,
                selector,
                plus,
            } => {
                let handle = state
                    .handles
                    .get(*handle_index as usize)
                    .ok_or_else(|| anyhow!("handle {} is missing or unresolved", handle_index))?;

                let val = self.resolve_fixed_handle_selector(handle, *selector)?;
                if let Some(plus) = plus {
                    Ok(val.wrapping_add(*plus))
                } else {
                    Ok(val)
                }
            }
            CompiledConstTpl::Relative { .. } | CompiledConstTpl::RelativeAddress => {
                bail!("ConstTpl relative label resolution is unsupported")
            }
            CompiledConstTpl::InstNext2 => bail!("ConstTpl inst_next2 resolution is unsupported"),
            CompiledConstTpl::CurSpace => bail!("ConstTpl curspace resolution is unsupported"),
            CompiledConstTpl::CurSpaceSize => {
                bail!("ConstTpl curspace_size resolution is unsupported")
            }
            CompiledConstTpl::FlowRef => bail!("ConstTpl flowref resolution is unsupported"),
            CompiledConstTpl::FlowRefSize => {
                bail!("ConstTpl flowref_size resolution is unsupported")
            }
            CompiledConstTpl::FlowDest => bail!("ConstTpl flowdest resolution is unsupported"),
            CompiledConstTpl::FlowDestSize => {
                bail!("ConstTpl flowdest_size resolution is unsupported")
            }
        }
    }

    fn resolve_fixed_handle_selector(
        &self,
        handle: &RuntimeHandle,
        selector: CompiledHandleSelector,
    ) -> Result<u64> {
        match selector {
            CompiledHandleSelector::Space => handle
                .fixed
                .space
                .as_ref()
                .map(|space| space.index)
                .ok_or_else(|| anyhow!("fixed handle missing space")),
            CompiledHandleSelector::Offset => {
                Ok(handle.fixed.offset_offset)
            }
            CompiledHandleSelector::Size => Ok(handle.fixed.size as u64),
            CompiledHandleSelector::OffsetPlus => bail!("OffsetPlus unsupported"),
        }
    }

    fn unary_pcode_opcode(&self, opcode: CompiledOpTplOpcode) -> Result<PcodeOpcode> {
        Ok(match opcode {
            CompiledOpTplOpcode::IntZExt => PcodeOpcode::IntZExt,
            CompiledOpTplOpcode::IntSExt => PcodeOpcode::IntSExt,
            CompiledOpTplOpcode::BoolNegate => PcodeOpcode::BoolNegate,
            CompiledOpTplOpcode::PopCount => PcodeOpcode::PopCount,
            _ => bail!("unsupported unary compiled opcode {}", opcode.as_str()),
        })
    }

    fn binary_pcode_opcode(&self, opcode: CompiledOpTplOpcode) -> Result<PcodeOpcode> {
        Ok(match opcode {
            CompiledOpTplOpcode::IntAdd => PcodeOpcode::IntAdd,
            CompiledOpTplOpcode::IntSub => PcodeOpcode::IntSub,
            CompiledOpTplOpcode::IntCarry => PcodeOpcode::IntCarry,
            CompiledOpTplOpcode::IntSCarry => PcodeOpcode::IntSCarry,
            CompiledOpTplOpcode::IntSBorrow => PcodeOpcode::IntSBorrow,
            CompiledOpTplOpcode::IntAnd => PcodeOpcode::IntAnd,
            CompiledOpTplOpcode::IntOr => PcodeOpcode::IntOr,
            CompiledOpTplOpcode::IntXor => PcodeOpcode::IntXor,
            CompiledOpTplOpcode::IntMult => PcodeOpcode::IntMult,
            CompiledOpTplOpcode::IntLeft => PcodeOpcode::IntLeft,
            CompiledOpTplOpcode::IntRight => PcodeOpcode::IntRight,
            CompiledOpTplOpcode::IntSRight => PcodeOpcode::IntSRight,
            CompiledOpTplOpcode::IntEqual => PcodeOpcode::IntEqual,
            CompiledOpTplOpcode::IntNotEqual => PcodeOpcode::IntNotEqual,
            CompiledOpTplOpcode::IntLess => PcodeOpcode::IntLess,
            CompiledOpTplOpcode::IntSLess => PcodeOpcode::IntSLess,
            CompiledOpTplOpcode::BoolAnd => PcodeOpcode::BoolAnd,
            CompiledOpTplOpcode::BoolOr => PcodeOpcode::BoolOr,
            CompiledOpTplOpcode::Piece => PcodeOpcode::Piece,
            CompiledOpTplOpcode::Subpiece => PcodeOpcode::SubPiece,
            _ => bail!("unsupported binary compiled opcode {}", opcode.as_str()),
        })
    }
}

impl RuntimeTemplateExecutor for CompiledTableEmitter {
    fn emit_op_template(
        &mut self,
        state: &RuntimeConstructState,
        op: &CompiledOpTpl,
    ) -> Result<()> {
        CompiledTableEmitter::emit_op_template(self, state, op)
    }
}
