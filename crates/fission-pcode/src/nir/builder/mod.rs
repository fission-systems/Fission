use super::*;

mod lower_expr;
mod stack_slots;
mod type_hints;

pub(super) fn apply_preview_type_hints(func: &mut HirFunction, context: &PreviewTypeContext) {
    type_hints::apply_preview_type_hints(func, context);
}

#[cfg(test)]
pub(super) fn collect_local_surface_hints(
    body: &[HirStmt],
    pointer_hints: &HashMap<String, PreviewCallParamRule>,
    func: &HirFunction,
    local_hints: &mut HashMap<String, String>,
) {
    type_hints::collect_local_surface_hints(body, pointer_hints, func, local_hints);
}

impl<'a> PreviewBuilder<'a> {
    pub(super) fn new(
        pcode: &'a PcodeFunction,
        options: &'a MlilPreviewOptions,
        type_context: Option<&'a PreviewTypeContext>,
    ) -> Self {
        let mut defs = HashMap::new();
        for block in &pcode.blocks {
            for op in &block.ops {
                if let Some(output) = &op.output {
                    defs.insert(VarnodeKey::from(output), op);
                }
            }
        }
        let address_to_index = pcode
            .blocks
            .iter()
            .enumerate()
            .map(|(idx, block)| (block.start_address, idx))
            .collect::<HashMap<_, _>>();
        let layout_fallthrough = build_layout_fallthrough_map(pcode);
        let successors = build_successor_index_map(pcode, &address_to_index, &layout_fallthrough);
        let predecessors = build_predecessor_index_map(&successors);
        Self {
            pcode,
            options,
            type_context,
            defs,
            address_to_index,
            layout_fallthrough,
            successors,
            predecessors,
            params: BTreeMap::new(),
            locals: BTreeMap::new(),
            locals_next_id: 0,
            temps: BTreeMap::new(),
            temp_next_id: 0,
            materialized_vns: HashMap::new(),
        }
    }

    pub(super) fn build_hir(
        &mut self,
        name: &str,
        _address: u64,
    ) -> Result<HirFunction, MlilPreviewError> {
        if self.pcode.blocks.is_empty() {
            return Err(MlilPreviewError::UnsupportedPattern("empty pcode"));
        }

        let mut body = Vec::new();
        if self.pcode.blocks.len() == 1 {
            let block = &self.pcode.blocks[0];
            body.extend(self.lower_block_stmts(block)?);
            match self.lower_block_terminator(0)? {
                LoweredTerminator::Return(expr) => body.push(HirStmt::Return(expr)),
                LoweredTerminator::Fallthrough(None) => {}
                LoweredTerminator::Fallthrough(Some(target))
                | LoweredTerminator::Goto(target) => body.push(HirStmt::Goto(block_label(target))),
                LoweredTerminator::Cond {
                    cond,
                    true_target,
                    false_target,
                } => body.push(HirStmt::If {
                    cond,
                    then_body: vec![HirStmt::Goto(block_label(true_target))],
                    else_body: false_target
                        .map(block_label)
                        .map(HirStmt::Goto)
                        .into_iter()
                        .collect(),
                }),
                LoweredTerminator::Unsupported => {
                    return Err(MlilPreviewError::UnsupportedCfgIndirectCallRegion);
                }
            }
        } else {
            body = self.build_multiblock_body()?;
        }

        let return_type = body
            .iter()
            .rev()
            .find_map(|stmt| match stmt {
                HirStmt::Return(Some(expr)) => Some(expr_type(expr)),
                HirStmt::Return(None) => Some(NirType::Unknown),
                _ => None,
            })
            .unwrap_or(NirType::Unknown);

        Ok(HirFunction {
            name: name.to_string(),
            params: self.params.values().cloned().collect(),
            locals: self
                .locals
                .values()
                .map(|slot| NirBinding {
                    name: slot.name.clone(),
                    ty: slot.ty.clone(),
                    surface_type_name: None,
                    initializer: None,
                })
                .chain(self.temps.values().cloned())
                .collect(),
            return_type,
            body,
        })
    }

    pub(super) fn lower_block_stmts(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
    ) -> Result<Vec<HirStmt>, MlilPreviewError> {
        let mut body = Vec::new();
        let terminator_index = self.block_terminator_index(block);
        for (op_idx, op) in block.ops.iter().enumerate() {
            if Some(op_idx) == terminator_index {
                continue;
            }
            match op.opcode {
                PcodeOpcode::Store => {
                    if op.inputs.len() < 3 {
                        return Err(MlilPreviewError::LoweringFailed);
                    }
                    let lhs = if let Some((slot_name, _slot_ty)) = self.try_stack_slot_lvalue(
                        &op.inputs[1],
                        type_from_size(op.inputs[2].size, false),
                    ) {
                        HirLValue::Var(slot_name)
                    } else {
                        HirLValue::Deref {
                            ptr: Box::new(self.lower_varnode(&op.inputs[1], &mut HashSet::new())?),
                            ty: type_from_size(op.inputs[2].size, false),
                        }
                    };
                    let rhs = self.lower_varnode(&op.inputs[2], &mut HashSet::new())?;
                    body.push(HirStmt::Assign { lhs, rhs });
                }
                PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther => {
                    if op.output.is_none() {
                        let expr = self.lower_call(op, &mut HashSet::new())?;
                        body.push(HirStmt::Expr(expr));
                    } else if let Some(stmt) =
                        self.maybe_materialize_output_stmt(block, op_idx, terminator_index, op)?
                    {
                        body.push(stmt);
                    }
                }
                _ => {
                    if let Some(stmt) =
                        self.maybe_materialize_output_stmt(block, op_idx, terminator_index, op)?
                    {
                        body.push(stmt);
                    }
                }
            }
        }
        Ok(body)
    }

    fn maybe_materialize_output_stmt(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        terminator_index: Option<usize>,
        op: &PcodeOp,
    ) -> Result<Option<HirStmt>, MlilPreviewError> {
        let Some(output) = &op.output else {
            return Ok(None);
        };
        if self.output_used_only_by_block_terminator(block, op_idx, terminator_index, output) {
            return Ok(None);
        }
        self.materialize_output_stmt(op)
    }

    fn materialize_output_stmt(
        &mut self,
        op: &PcodeOp,
    ) -> Result<Option<HirStmt>, MlilPreviewError> {
        let Some(output) = &op.output else {
            return Ok(None);
        };
        if !is_materializable_output_opcode(op.opcode) {
            return Ok(None);
        }
        let rhs = self.lower_def_op(op, &mut HashSet::new())?;
        let lhs = HirLValue::Var(self.ensure_temp_binding_for_output(output).name);
        Ok(Some(HirStmt::Assign { lhs, rhs }))
    }

    fn output_used_only_by_block_terminator(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        terminator_index: Option<usize>,
        output: &Varnode,
    ) -> bool {
        let key = VarnodeKey::from(output);
        let mut use_sites = block
            .ops
            .iter()
            .enumerate()
            .skip(op_idx + 1)
            .filter(|(_, candidate)| {
                candidate
                    .inputs
                    .iter()
                    .any(|input| VarnodeKey::from(input) == key)
            })
            .map(|(idx, _)| idx);

        let Some(first_use) = use_sites.next() else {
            return false;
        };
        if use_sites.next().is_some() {
            return false;
        }
        Some(first_use) == terminator_index
    }

    fn block_terminator_index(&self, block: &crate::pcode::PcodeBasicBlock) -> Option<usize> {
        block.ops.iter().rposition(|op| {
            matches!(
                op.opcode,
                PcodeOpcode::Branch
                    | PcodeOpcode::CBranch
                    | PcodeOpcode::BranchInd
                    | PcodeOpcode::Return
            )
        })
    }

    pub(super) fn lower_block_terminator(
        &mut self,
        idx: usize,
    ) -> Result<LoweredTerminator, MlilPreviewError> {
        let block = &self.pcode.blocks[idx];
        let Some(term_idx) = self.block_terminator_index(block) else {
            return Ok(LoweredTerminator::Fallthrough(self.next_block_address(idx)));
        };
        let op = &block.ops[term_idx];
        match op.opcode {
            PcodeOpcode::Return => {
                let expr = op
                    .inputs
                    .last()
                    .map(|input| self.lower_varnode(input, &mut HashSet::new()))
                    .transpose()?;
                Ok(LoweredTerminator::Return(expr))
            }
            PcodeOpcode::Branch if op.inputs.len() == 1 => {
                let Some(target) = op.inputs.first().and_then(branch_target_address) else {
                    return Err(MlilPreviewError::UnsupportedCfgBranchTarget);
                };
                Ok(LoweredTerminator::Goto(target))
            }
            PcodeOpcode::CBranch | PcodeOpcode::Branch if op.inputs.len() >= 2 => {
                if op.inputs.len() < 2 {
                    return Err(MlilPreviewError::LoweringFailed);
                }
                let Some(true_target) = branch_target_address(&op.inputs[0]) else {
                    return Err(MlilPreviewError::UnsupportedCfgBranchTarget);
                };
                let cond = self.lower_varnode(&op.inputs[1], &mut HashSet::new())?;
                Ok(LoweredTerminator::Cond {
                    cond,
                    true_target,
                    false_target: self.next_block_address(idx),
                })
            }
            PcodeOpcode::BranchInd => Ok(LoweredTerminator::Unsupported),
            _ => Ok(LoweredTerminator::Fallthrough(self.next_block_address(idx))),
        }
    }

    pub(super) fn next_block_address(&self, idx: usize) -> Option<u64> {
        self.layout_fallthrough[idx].map(|next_idx| self.pcode.blocks[next_idx].start_address)
    }

    pub(super) fn ensure_temp_binding_for_output(&mut self, output: &Varnode) -> NirBinding {
        let key = VarnodeKey::from(output);
        if let Some(name) = self.materialized_vns.get(&key)
            && let Some(binding) = self.temps.get(name)
        {
            return binding.clone();
        }

        let ty = type_from_size(output.size, false);
        let name = next_temp_name(&ty, &mut self.temp_next_id);
        let binding = NirBinding {
            name: name.clone(),
            ty,
            surface_type_name: None,
            initializer: None,
        };
        self.materialized_vns.insert(key, name.clone());
        self.temps.insert(name, binding.clone());
        binding
    }
}
