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
        for (block_idx, block) in pcode.blocks.iter().enumerate() {
            for (op_idx, op) in block.ops.iter().enumerate() {
                if let Some(output) = &op.output {
                    defs.insert(
                        VarnodeKey::from(output),
                        DefSite {
                            block_idx,
                            op_idx,
                            op,
                        },
                    );
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
        let register_param_aliases = collect_entry_register_param_aliases(pcode);
        let stack_frame_size = infer_entry_stack_frame_size(pcode, options);
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
            current_lowering_site: None,
            register_param_aliases,
            stack_frame_size,
            linear_exit_cache: HashMap::new(),
            jump_targets_cache: None,
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
        let block_idx = self
            .address_to_index
            .get(&block.start_address)
            .copied()
            .unwrap_or(0);
        for (op_idx, op) in block.ops.iter().enumerate() {
            if Some(op_idx) == terminator_index {
                continue;
            }
            let site = LoweringSite {
                block_idx,
                op_idx,
            };
            let maybe_stmt = self.with_lowering_site(site, |this| -> Result<Option<HirStmt>, MlilPreviewError> {
                match op.opcode {
                    PcodeOpcode::Store => {
                        if op.inputs.len() < 3 {
                            this.debug_lowering_error(
                                "store_malformed_skip",
                                block.start_address,
                                u64::from(op.seq_num),
                                op.opcode,
                                &MlilPreviewError::UnsupportedExprMemoryBackedVarnode,
                            );
                            return Ok(None);
                        }
                        let lhs = if let Some((slot_name, _slot_ty)) =
                            this.try_stack_slot_lvalue_for_memory_op(
                                op,
                                &op.inputs[1],
                                type_from_size(op.inputs[2].size, false),
                            )
                        {
                            HirLValue::Var(slot_name)
                        } else {
                            HirLValue::Deref {
                                ptr: Box::new(
                                    this.lower_varnode(&op.inputs[1], &mut HashSet::new())
                                        .map_err(|err| {
                                            this.debug_lowering_error(
                                                "store_ptr",
                                                block.start_address,
                                                u64::from(op.seq_num),
                                                op.opcode,
                                                &err,
                                            );
                                            err
                                        })?,
                                ),
                                ty: type_from_size(op.inputs[2].size, false),
                            }
                        };
                        let rhs = if let Some(expr) =
                            this.recover_aggregate_store_rhs_from_block(block, op_idx, &op.inputs[2])?
                        {
                            expr
                        } else {
                            this.lower_varnode(&op.inputs[2], &mut HashSet::new())
                                .map_err(|err| {
                                    this.debug_lowering_error(
                                        "store_rhs",
                                        block.start_address,
                                        u64::from(op.seq_num),
                                        op.opcode,
                                        &err,
                                    );
                                    err
                                })?
                        };
                        Ok(Some(HirStmt::Assign { lhs, rhs }))
                    }
                    PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther => {
                        if op.output.is_none() {
                            let recovered_args = if op.inputs.len() > 1 {
                                None
                            } else {
                                this.recover_call_args_from_block(block, op_idx)?
                            };
                            let expr =
                                this.lower_call(op, recovered_args, &mut HashSet::new()).map_err(
                                    |err| {
                                        this.debug_lowering_error(
                                            "call_expr",
                                            block.start_address,
                                            u64::from(op.seq_num),
                                            op.opcode,
                                            &err,
                                        );
                                        err
                                    },
                                )?;
                            Ok(Some(HirStmt::Expr(expr)))
                        } else {
                            this.maybe_materialize_output_stmt(
                                block.start_address,
                                block,
                                op_idx,
                                terminator_index,
                                op,
                            )
                        }
                    }
                    _ => this.maybe_materialize_output_stmt(
                        block.start_address,
                        block,
                        op_idx,
                        terminator_index,
                        op,
                    ),
                }
            })?;
            if let Some(stmt) = maybe_stmt {
                body.push(stmt);
            }
        }
        Ok(body)
    }

    fn maybe_materialize_output_stmt(
        &mut self,
        block_addr: u64,
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
        if self.output_used_only_by_single_store(block, op_idx, output)
            || self.output_used_only_by_passthrough_chain(block, op_idx, output)
        {
            return Ok(None);
        }
        self.materialize_output_stmt(block_addr, op)
    }

    fn materialize_output_stmt(
        &mut self,
        block_addr: u64,
        op: &PcodeOp,
    ) -> Result<Option<HirStmt>, MlilPreviewError> {
        let Some(output) = &op.output else {
            return Ok(None);
        };
        if !is_materializable_output_opcode(op.opcode) {
            return Ok(None);
        }
        let rhs = match self.lower_def_op(op, &mut HashSet::new()) {
            Ok(rhs) => rhs,
            Err(err)
                if matches!(
                    err,
                    MlilPreviewError::LoweringFailed
                        | MlilPreviewError::UnsupportedExprVarnodeLowering
                        | MlilPreviewError::UnsupportedExprAddressMaterialization
                        | MlilPreviewError::UnsupportedExprIndirectValueSource
                        | MlilPreviewError::UnsupportedExprPieceShape
                        | MlilPreviewError::UnsupportedExprPtrArithmetic
                        | MlilPreviewError::UnsupportedExprMemoryBackedVarnode
                ) =>
            {
                self.debug_lowering_error(
                    "materialize_output_skip",
                    block_addr,
                    u64::from(op.seq_num),
                    op.opcode,
                    &err,
                );
                return Ok(None);
            }
            Err(err) => {
                self.debug_lowering_error(
                    "materialize_output",
                    block_addr,
                    u64::from(op.seq_num),
                    op.opcode,
                    &err,
                );
                return Err(err);
            }
        };
        let lhs = HirLValue::Var(self.ensure_temp_binding_for_output(op, output).name);
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

    fn output_use_sites_in_block<'b>(
        &self,
        block: &'b crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
    ) -> Vec<(usize, &'b PcodeOp)> {
        let key = VarnodeKey::from(output);
        let mut uses = Vec::new();
        for (idx, candidate) in block.ops.iter().enumerate().skip(op_idx + 1) {
            if candidate.output.as_ref().map(VarnodeKey::from) == Some(key.clone()) {
                break;
            }
            if candidate
                .inputs
                .iter()
                .any(|input| VarnodeKey::from(input) == key)
            {
                uses.push((idx, candidate));
            }
        }
        uses
    }

    fn output_used_only_by_single_store(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
    ) -> bool {
        if output.size < 16 {
            return false;
        }
        let uses = self.output_use_sites_in_block(block, op_idx, output);
        uses.len() == 1 && uses[0].1.opcode == PcodeOpcode::Store
    }

    fn output_used_only_by_passthrough_chain(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
    ) -> bool {
        if output.size < 16 {
            return false;
        }
        let uses = self.output_use_sites_in_block(block, op_idx, output);
        !uses.is_empty()
            && uses.iter().all(|(_, op)| {
                matches!(
                    op.opcode,
                    PcodeOpcode::Copy
                        | PcodeOpcode::Cast
                        | PcodeOpcode::IntZExt
                        | PcodeOpcode::IntSExt
                        | PcodeOpcode::SubPiece
                )
            })
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
        self.with_lowering_site(
            LoweringSite {
                block_idx: idx,
                op_idx: term_idx,
            },
            |this| match op.opcode {
                PcodeOpcode::Return => {
                    let expr = op
                        .inputs
                        .last()
                        .map(|input| this.lower_wrapped_varnode(input, &mut HashSet::new()))
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
                        return Err(MlilPreviewError::UnsupportedExprVarnodeLowering);
                    }
                    let Some(true_target) = branch_target_address(&op.inputs[0]) else {
                        return Err(MlilPreviewError::UnsupportedCfgBranchTarget);
                    };
                    let cond = this
                        .lower_wrapped_varnode(&op.inputs[1], &mut HashSet::new())
                        .map_err(|err| {
                            this.debug_lowering_error(
                                "terminator_cond",
                                block.start_address,
                                u64::from(op.seq_num),
                                op.opcode,
                                &err,
                            );
                            err
                        })?;
                    Ok(LoweredTerminator::Cond {
                        cond,
                        true_target,
                        false_target: this.next_block_address(idx),
                    })
                }
                PcodeOpcode::BranchInd => Ok(LoweredTerminator::Unsupported),
                _ => Ok(LoweredTerminator::Fallthrough(this.next_block_address(idx))),
            },
        )
    }

    fn with_lowering_site<T>(
        &mut self,
        site: LoweringSite,
        f: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let prev = self.current_lowering_site;
        self.current_lowering_site = Some(site);
        let result = f(self);
        self.current_lowering_site = prev;
        result
    }

    pub(super) fn next_block_address(&self, idx: usize) -> Option<u64> {
        self.layout_fallthrough[idx].map(|next_idx| self.pcode.blocks[next_idx].start_address)
    }

    pub(super) fn ensure_temp_binding_for_output(
        &mut self,
        op: &PcodeOp,
        output: &Varnode,
    ) -> NirBinding {
        let key = MaterializedVarnodeKey::new(output, op);
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

    fn debug_lowering_error(
        &self,
        stage: &str,
        block_addr: u64,
        seq: u64,
        opcode: PcodeOpcode,
        err: &MlilPreviewError,
    ) {
        if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
            eprintln!(
                "[mlil-preview] stage={} block=0x{:x} seq=0x{:x} opcode={:?} err={}",
                stage, block_addr, seq, opcode, err
            );
        }
    }

    fn lower_wrapped_varnode(
        &mut self,
        vn: &Varnode,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<HirExpr, MlilPreviewError> {
        match self.lower_varnode(vn, visiting) {
            Ok(expr) => Ok(expr),
            Err(err) => {
                let Some((_, op)) = self.lookup_def_site(vn) else {
                    return Err(err);
                };
                match op.opcode {
                    PcodeOpcode::Copy
                    | PcodeOpcode::Cast
                    | PcodeOpcode::IntZExt
                    | PcodeOpcode::IntSExt
                        if op.inputs.len() == 1 =>
                    {
                        self.lower_wrapped_varnode(&op.inputs[0], visiting)
                    }
                    PcodeOpcode::IntAdd | PcodeOpcode::IntSub if op.inputs.len() == 2 => {
                        if const_offset(&op.inputs[0]) == Some(0) {
                            self.lower_wrapped_varnode(&op.inputs[1], visiting)
                        } else if const_offset(&op.inputs[1]) == Some(0) {
                            self.lower_wrapped_varnode(&op.inputs[0], visiting)
                        } else {
                            Err(err)
                        }
                    }
                    _ => Err(err),
                }
            }
        }
    }

    fn recover_call_args_from_block(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        call_idx: usize,
    ) -> Result<Option<Vec<HirExpr>>, MlilPreviewError> {
        if !self.options.is_64bit || call_idx == 0 {
            return Ok(None);
        }

        const WIN64_PARAM_REGS: &[(u64, u32)] = &[(0x08, 8), (0x10, 8), (0x80, 8), (0x88, 8)];
        let mut recovered: Vec<Option<HirExpr>> = vec![None; WIN64_PARAM_REGS.len()];

        for prev_idx in (0..call_idx).rev() {
            let prev = &block.ops[prev_idx];
            if prev.opcode.is_control_flow() {
                break;
            }
            let Some(output) = &prev.output else {
                continue;
            };
            if output.space_id != REGISTER_SPACE_ID {
                continue;
            }
            let Some((_, Some(param_index))) = register_name_with_param(output.offset, output.size)
            else {
                continue;
            };
            if param_index >= recovered.len() || recovered[param_index].is_some() {
                continue;
            }
            let expr = self.lower_varnode(output, &mut HashSet::new()).map_err(|err| {
                self.debug_lowering_error(
                    "call_arg_recovery",
                    block.start_address,
                    u64::from(prev.seq_num),
                    prev.opcode,
                    &err,
                );
                err
            })?;
            recovered[param_index] = Some(expr);
        }

        let Some(highest_recovered) = recovered.iter().rposition(Option::is_some) else {
            return Ok(None);
        };

        for (param_index, (offset, size)) in WIN64_PARAM_REGS.iter().enumerate() {
            if param_index > highest_recovered || recovered[param_index].is_some() {
                continue;
            }
            let expr = self.lower_varnode(
                &Varnode {
                    space_id: REGISTER_SPACE_ID,
                    offset: *offset,
                    size: *size,
                    is_constant: false,
                    constant_val: 0,
                },
                &mut HashSet::new(),
            )?;
            recovered[param_index] = Some(expr);
        }

        if recovered[..=highest_recovered].iter().any(Option::is_none) {
            return Ok(None);
        }

        Ok(Some(
            recovered
                .into_iter()
                .take(highest_recovered + 1)
                .map(|expr| expr.expect("validated recovered call arg"))
                .collect(),
        ))
    }

    fn recover_aggregate_store_rhs_from_block(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        rhs: &Varnode,
    ) -> Result<Option<HirExpr>, MlilPreviewError> {
        let debug = std::env::var_os("FISSION_PREVIEW_DEBUG").is_some();
        if rhs.size < 16 {
            return Ok(None);
        }
        let mut current = rhs.clone();
        let mut scan_end = op_idx;

        for _ in 0..8 {
            if debug {
                eprintln!(
                    "[mlil-preview][agg] block=0x{:x} op_idx={} current=({},0x{:x},sz={}) scan_end={}",
                    block.start_address,
                    op_idx,
                    current.space_id,
                    current.offset,
                    current.size,
                    scan_end
                );
                append_preview_debug_trace(&format!(
                    "[mlil-preview][agg] block=0x{:x} op_idx={} current=({},0x{:x},sz={}) scan_end={}",
                    block.start_address, op_idx, current.space_id, current.offset, current.size, scan_end
                ));
            }
            if current.space_id == REGISTER_SPACE_ID && current.size >= 16 {
                if let Some((source, earliest_idx)) =
                    recover_wide_register_source_from_block(block, scan_end, &current)
                {
                    if debug {
                        eprintln!(
                            "[mlil-preview][agg] wide-reg source -> ({},0x{:x},sz={}) earliest_idx={}",
                            source.space_id, source.offset, source.size, earliest_idx
                        );
                        append_preview_debug_trace(&format!(
                            "[mlil-preview][agg] wide-reg source -> ({},0x{:x},sz={}) earliest_idx={}",
                            source.space_id, source.offset, source.size, earliest_idx
                        ));
                    }
                    current = source;
                    scan_end = earliest_idx;
                    continue;
                }
                if debug {
                    eprintln!("[mlil-preview][agg] wide-reg source lookup failed");
                    append_preview_debug_trace("[mlil-preview][agg] wide-reg source lookup failed");
                }
                return Ok(None);
            }

            let Some((def_idx, def_op)) =
                find_prior_def_in_block(block, scan_end, &current)
            else {
                if debug {
                    eprintln!("[mlil-preview][agg] no prior def found");
                    append_preview_debug_trace("[mlil-preview][agg] no prior def found");
                }
                return Ok(None);
            };
            if debug {
                eprintln!(
                    "[mlil-preview][agg] def idx={} seq=0x{:x} opcode={:?}",
                    def_idx,
                    def_op.seq_num,
                    def_op.opcode
                );
                append_preview_debug_trace(&format!(
                    "[mlil-preview][agg] def idx={} seq=0x{:x} opcode={:?}",
                    def_idx, def_op.seq_num, def_op.opcode
                ));
            }

            match def_op.opcode {
                PcodeOpcode::Load => {
                    if def_op.inputs.len() < 2 {
                        if debug {
                            eprintln!("[mlil-preview][agg] load malformed");
                            append_preview_debug_trace("[mlil-preview][agg] load malformed");
                        }
                        return Ok(None);
                    }
                    if let Some((slot_name, _)) = self.try_stack_slot_lvalue_for_memory_op(
                        def_op,
                        &def_op.inputs[1],
                        type_from_size(current.size, false),
                    ) {
                        if debug {
                            eprintln!("[mlil-preview][agg] resolved slot {}", slot_name);
                            append_preview_debug_trace(&format!(
                                "[mlil-preview][agg] resolved slot {}",
                                slot_name
                            ));
                        }
                        return Ok(Some(HirExpr::Var(slot_name)));
                    }
                    if debug {
                        eprintln!("[mlil-preview][agg] load did not resolve stack slot");
                        append_preview_debug_trace(
                            "[mlil-preview][agg] load did not resolve stack slot",
                        );
                    }
                    return Ok(None);
                }
                PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt => {
                    let Some(next) = def_op.inputs.first() else {
                        if debug {
                            eprintln!("[mlil-preview][agg] copy/cast missing input");
                            append_preview_debug_trace(
                                "[mlil-preview][agg] copy/cast missing input",
                            );
                        }
                        return Ok(None);
                    };
                    if debug {
                        eprintln!(
                            "[mlil-preview][agg] stepping through {:?} -> ({},0x{:x},sz={})",
                            def_op.opcode, next.space_id, next.offset, next.size
                        );
                        append_preview_debug_trace(&format!(
                            "[mlil-preview][agg] stepping through {:?} -> ({},0x{:x},sz={})",
                            def_op.opcode, next.space_id, next.offset, next.size
                        ));
                    }
                    current = next.clone();
                    scan_end = def_idx;
                }
                _ => {
                    if debug {
                        eprintln!("[mlil-preview][agg] unsupported def opcode {:?}", def_op.opcode);
                        append_preview_debug_trace(&format!(
                            "[mlil-preview][agg] unsupported def opcode {:?}",
                            def_op.opcode
                        ));
                    }
                    return Ok(None);
                }
            }
        }

        if debug {
            eprintln!("[mlil-preview][agg] exceeded trace depth");
            append_preview_debug_trace("[mlil-preview][agg] exceeded trace depth");
        }
        Ok(None)
    }
}

fn collect_entry_register_param_aliases(pcode: &PcodeFunction) -> HashMap<u64, usize> {
    let mut aliases = HashMap::new();
    let Some(entry) = pcode.blocks.first() else {
        return aliases;
    };

    for op in &entry.ops {
        match op.opcode {
            PcodeOpcode::Call
            | PcodeOpcode::CallInd
            | PcodeOpcode::CallOther
            | PcodeOpcode::Branch
            | PcodeOpcode::CBranch
            | PcodeOpcode::BranchInd
            | PcodeOpcode::Return => break,
            PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt => {
                let Some(output) = &op.output else {
                    continue;
                };
                if output.space_id != REGISTER_SPACE_ID {
                    continue;
                }
                let Some((_, output_param_index)) =
                    register_name_with_param(output.offset, output.size)
                else {
                    continue;
                };
                if output_param_index.is_some() {
                    continue;
                }
                let Some(input) = op.inputs.first() else {
                    continue;
                };
                if input.space_id != REGISTER_SPACE_ID {
                    continue;
                }
                let alias_param_index = register_name_with_param(input.offset, input.size)
                    .and_then(|(_, input_param_index)| input_param_index)
                    .or_else(|| aliases.get(&input.offset).copied());
                if let Some(param_index) = alias_param_index {
                    aliases.entry(output.offset).or_insert(param_index);
                }
            }
            _ => {}
        }
    }

    aliases
}

fn infer_entry_stack_frame_size(pcode: &PcodeFunction, options: &MlilPreviewOptions) -> i64 {
    let Some(entry) = pcode.blocks.first() else {
        return 0;
    };

    let mut frame_size = 0_i64;
    let mut seen_addrs = HashSet::new();
    let mut started = false;
    for op in &entry.ops {
        if !seen_addrs.insert(op.address) {
            continue;
        }
        let Some(asm) = op.asm_mnemonic.as_deref() else {
            break;
        };
        let asm = asm.trim().to_ascii_uppercase();
        let pointer_size = i64::from(options.pointer_size);
        if asm.starts_with("PUSH ") {
            frame_size += pointer_size;
            started = true;
            continue;
        }
        let sub_rsp = if options.is_64bit {
            asm.strip_prefix("SUB RSP,")
        } else {
            asm.strip_prefix("SUB ESP,")
        };
        if let Some(imm) = sub_rsp.and_then(parse_signed_asm_immediate) {
            frame_size += imm;
            started = true;
            continue;
        }
        if asm.starts_with("MOV RBP,RSP") || asm.starts_with("MOV EBP,ESP") {
            started = true;
            continue;
        }
        if started {
            break;
        }
    }
    frame_size
}

fn parse_signed_asm_immediate(text: &str) -> Option<i64> {
    let text = text.trim();
    if let Some(hex) = text.strip_prefix("0X") {
        i64::from_str_radix(hex, 16).ok()
    } else {
        text.parse().ok()
    }
}

fn append_preview_debug_trace(line: &str) {
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/fission_preview_agg_trace.log")
        .and_then(|mut f| std::io::Write::write_all(&mut f, format!("{line}\n").as_bytes()));
}

fn find_prior_def_in_block<'a>(
    block: &'a crate::pcode::PcodeBasicBlock,
    scan_end: usize,
    target: &Varnode,
) -> Option<(usize, &'a PcodeOp)> {
    let key = VarnodeKey::from(target);
    block
        .ops
        .iter()
        .enumerate()
        .take(scan_end)
        .rev()
        .find(|(_, op)| op.output.as_ref().map(VarnodeKey::from) == Some(key.clone()))
}

fn recover_wide_register_source_from_block(
    block: &crate::pcode::PcodeBasicBlock,
    scan_end: usize,
    reg_vn: &Varnode,
) -> Option<(Varnode, usize)> {
    let debug = std::env::var_os("FISSION_PREVIEW_DEBUG").is_some();
    if reg_vn.space_id != REGISTER_SPACE_ID || reg_vn.size < 16 || reg_vn.size % 4 != 0 {
        return None;
    }

    let lane_size = 4_u32;
    let lane_count = (reg_vn.size / lane_size) as usize;
    let mut source: Option<Varnode> = None;
    let mut earliest_idx = scan_end;

    for lane in 0..lane_count {
        let lane_offset = reg_vn.offset + (lane as u64 * u64::from(lane_size));
        let lane_vn = Varnode {
            space_id: REGISTER_SPACE_ID,
            offset: lane_offset,
            size: lane_size,
            is_constant: false,
            constant_val: 0,
        };
        let (lane_idx, lane_op) = find_prior_def_in_block(block, scan_end, &lane_vn)?;
        if debug {
            eprintln!(
                "[mlil-preview][agg] lane {} def idx={} seq=0x{:x} opcode={:?}",
                lane,
                lane_idx,
                lane_op.seq_num,
                lane_op.opcode
            );
            append_preview_debug_trace(&format!(
                "[mlil-preview][agg] lane {} def idx={} seq=0x{:x} opcode={:?}",
                lane, lane_idx, lane_op.seq_num, lane_op.opcode
            ));
        }
        if !matches!(lane_op.opcode, PcodeOpcode::SubPiece | PcodeOpcode::IntSub)
            || lane_op.inputs.len() < 2
        {
            if debug {
                eprintln!("[mlil-preview][agg] lane {} not subpiece", lane);
                append_preview_debug_trace(&format!(
                    "[mlil-preview][agg] lane {} not subpiece",
                    lane
                ));
            }
            return None;
        }
        let lane_source = lane_op.inputs[0].clone();
        let lane_disp = const_offset(&lane_op.inputs[1])?;
        if lane_disp != (lane as i64 * i64::from(lane_size)) {
            if debug {
                eprintln!(
                    "[mlil-preview][agg] lane {} disp mismatch got {} expected {}",
                    lane,
                    lane_disp,
                    lane as i64 * i64::from(lane_size)
                );
                append_preview_debug_trace(&format!(
                    "[mlil-preview][agg] lane {} disp mismatch got {} expected {}",
                    lane,
                    lane_disp,
                    lane as i64 * i64::from(lane_size)
                ));
            }
            return None;
        }
        match &source {
            Some(existing) if VarnodeKey::from(existing) != VarnodeKey::from(&lane_source) => {
                if debug {
                    eprintln!("[mlil-preview][agg] lane {} source mismatch", lane);
                    append_preview_debug_trace(&format!(
                        "[mlil-preview][agg] lane {} source mismatch",
                        lane
                    ));
                }
                return None;
            }
            None => source = Some(lane_source),
            _ => {}
        }
        earliest_idx = earliest_idx.min(lane_idx);
    }

    source.map(|src| (src, earliest_idx))
}
