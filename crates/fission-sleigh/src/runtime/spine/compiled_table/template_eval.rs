pub(super) fn emit_pcode_for_state(
    compiled: &CompiledFrontend,
    address: u64,
    decoded: &RuntimeConstructState,
) -> Result<(Vec<PcodeOp>, RuntimeExecutionDetails)> {
    let mut emitter = CompiledTableEmitter::new(compiled, address);
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


/// Sentinel used to tag branch targets that reference pcode-internal relative labels.
/// Convention follows Ghidra: `-(label_num + 1)` as i64, stored as u64.
/// Any branch target constant with value > RELATIVE_LABEL_SENTINEL_THRESHOLD is a sentinel.
const RELATIVE_LABEL_SENTINEL_THRESHOLD: u64 = u64::MAX - 0x10000;

fn encode_relative_sentinel(label_num: u64) -> u64 {
    (-(label_num as i64 + 1)) as u64
}

fn decode_relative_sentinel(sentinel: u64) -> Option<u64> {
    if sentinel > RELATIVE_LABEL_SENTINEL_THRESHOLD {
        let label_num = (-(sentinel as i64) - 1) as u64;
        Some(label_num)
    } else {
        None
    }
}

#[derive(Debug, Clone)]
pub(super) struct CompiledTableEmitter {
    emitter: RuntimePcodeEmitter,
    address: u64,
    built_operands: std::collections::BTreeSet<usize>,
    /// Exported varnodes produced by BUILD subconstructors. Ghidra templates
    /// reference these through negative handle indices in parent templates.
    exported_build_varnodes: std::collections::BTreeMap<i64, Varnode>,
    /// Index of the unique (temporary) address space, derived from `.sla` metadata.
    unique_space_index: u64,
    /// Mapping from space index to space reference, derived from `.sla` metadata.
    sla_spaces: std::collections::BTreeMap<u64, CompiledSpaceRef>,
    /// Label positions: `label_num` → emitter op count at the time the Label was seen.
    /// Used for `resolveRelatives()` post-processing.
    label_positions: std::collections::BTreeMap<u64, u32>,
}

#[derive(Debug, Clone)]
pub(super) struct DynamicMemoryTarget {
    space: Varnode,
    ptr: Varnode,
    temp: Varnode,
    size: u32,
}

impl CompiledTableEmitter {
    fn new(compiled: &CompiledFrontend, address: u64) -> Self {
        // Unique base offset comes from the `.sla` `uniqbase` attribute
        // (Ghidra: (addr & uniqmask) | uniqbase pattern). Using the SLA-derived
        // value ensures correct unique varnode allocation for any architecture.
        let uniqbase = compiled.sla_uniqbase;
        Self {
            address,
            emitter: RuntimePcodeEmitter::new(address, uniqbase),
            built_operands: std::collections::BTreeSet::new(),
            exported_build_varnodes: std::collections::BTreeMap::new(),
            unique_space_index: compiled.sla_unique_space_index,
            sla_spaces: compiled.sla_spaces.clone(),
            label_positions: std::collections::BTreeMap::new(),
        }
    }

    fn finish(self) -> Vec<PcodeOp> {
        let label_positions = self.label_positions;
        let mut ops = self.emitter.finish();
        // resolveRelatives: replace sentinel branch targets with actual relative offsets.
        // Follows Ghidra's PcodeCacher::resolveRelatives() convention.
        for i in 0..ops.len() {
            let opcode = ops[i].opcode;
            if !matches!(opcode, PcodeOpcode::Branch | PcodeOpcode::CBranch) {
                continue;
            }
            // Branch: input[0] is the target. CBranch: input[0] is target, input[1] is cond.
            if let Some(target_vn) = ops[i].inputs.first() {
                if !target_vn.is_constant {
                    continue;
                }
                let raw = target_vn.constant_val as u64;
                if let Some(label_num) = decode_relative_sentinel(raw) {
                    if let Some(&label_op_count) = label_positions.get(&label_num) {
                        // Relative offset = label_op_count - (branch_op_index + 1)
                        // Ghidra convention: positive = forward, negative = backward.
                        let branch_op = i as i64;
                        let label_pos = label_op_count as i64;
                        let relative = label_pos - branch_op;
                        ops[i].inputs[0] = Varnode::constant(relative, 8);
                    }
                }
            }
        }
        ops
    }

    fn emit_op_template(
        &mut self,
        state: &RuntimeConstructState,
        op: &CompiledOpTpl,
    ) -> Result<()> {
        let mnemonic = op.opcode.as_str();
        match op.opcode {
            CompiledOpTplOpcode::Label => {
                // Record the current emitter op count as this label's position.
                // Labels themselves don't emit pcode ops; they are position markers.
                // The label number is encoded in the output varnode's offset field.
                let label_num = op.output.as_ref().and_then(|out| {
                    if let CompiledVarnodeTpl::Varnode { offset, .. } = out {
                        if let CompiledConstTpl::Real { value } = offset.as_ref() {
                            return Some(*value);
                        }
                    }
                    None
                }).unwrap_or(0);
                // Use the emitter's actual op count so even recursively emitted ops
                // (via BUILD) are accounted for correctly.
                self.label_positions.insert(label_num, self.emitter.op_count());
                Ok(())
            }
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
            CompiledOpTplOpcode::CallInd => {
                let target_tpl = op
                    .inputs
                    .first()
                    .ok_or_else(|| anyhow!("CALLIND template requires one input"))?;
                let target = self.read_template_varnode(target_tpl, state, 8)?;
                self.emitter.emit_call_ind(target, mnemonic)
            }
            CompiledOpTplOpcode::Branch => {
                let target_tpl = op
                    .inputs
                    .first()
                    .ok_or_else(|| anyhow!("BRANCH template requires one input"))?;
                // Ghidra principle: BRANCH vs BRANCHIND is determined solely by the
                // SLA template opcode, NOT by the target's address space. A direct
                // jmp with a RAM-space absolute target is still a BRANCH.
                let target = self.read_template_varnode(target_tpl, state, 8)?;
                self.emitter.emit_branch(target, mnemonic)
            }
            CompiledOpTplOpcode::BranchInd => {
                let target_tpl = op
                    .inputs
                    .first()
                    .ok_or_else(|| anyhow!("BRANCHIND template requires one input"))?;
                let target = self.read_template_varnode(target_tpl, state, 8)?;
                self.emitter.emit_branch_ind(target, mnemonic)
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
        for child_op in &child.constructor_template.ops {
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
                bail!("ConditionPredicate is display/compatibility-only and cannot emit raw P-code")
            }
            _ => bail!(
                "compiled-table executor rejects compatibility varnode template: {:?}",
                template
            ),
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
        if !handle
            .fixed
            .space
            .as_ref()
            .is_some_and(|space| space.index == 0x1b1 || space.name == "ram")
        {
            return Ok(None);
        }
        if handle.fixed.offset_space.is_none() {
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
                // Look up the space name from the SLA-derived space table instead
                // of using a hardcoded architecture-specific mapping.
                if let Some(space_ref) = self.sla_spaces.get(&space_id) {
                    return Ok(space_ref.clone());
                }
                // Const space (ID 0) is always the constant space regardless of
                // architecture. Fall back to index-only for unknown spaces.
                let name = if space_id == 0 { "const" } else { "unknown" };
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
            CompiledConstTpl::Relative { value: label_num } => {
                // Emit a sentinel value; resolveRelatives() in finish() will replace it
                // with the actual relative op offset after all ops are emitted.
                // Uses Ghidra's convention: -(label_num + 1) so the value is in the
                // RELATIVE_LABEL_SENTINEL_THRESHOLD range (large unsigned values).
                Ok(encode_relative_sentinel(*label_num))
            }
            CompiledConstTpl::RelativeAddress => {
                // RelativeAddress is a backward-compat form of Relative without an
                // explicit label num; treat as label 0.
                Ok(encode_relative_sentinel(0))
            }
            CompiledConstTpl::InstNext2 => {
                // Delay-slot architecture: address of the instruction after the delay
                // slot. Approximation: inst_next + inst_length (will be refined when
                // delay slot decoding is fully implemented).
                Ok(self.address.saturating_add(state.length as u64 * 2))
            }
            CompiledConstTpl::CurSpace => {
                // The default (ram/code) address space ID. Use the first non-const,
                // non-unique, non-register space from the SLA space table.
                for (index, space) in &self.sla_spaces {
                    if space.name != "const" && space.name != "unique" && space.name != "register" {
                        return Ok(*index);
                    }
                }
                // Fallback: index 1 is typically the ram/default space.
                Ok(1)
            }
            CompiledConstTpl::CurSpaceSize => {
                // Size of the default address space in bytes (typically 4 or 8).
                // Use 8 as a safe default for 64-bit architectures.
                Ok(8)
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
