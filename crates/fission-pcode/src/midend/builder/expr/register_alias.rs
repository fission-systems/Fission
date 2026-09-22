use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(in crate::midend::builder) fn register_key_covers(
        candidate: &VarnodeKey,
        requested: &VarnodeKey,
    ) -> bool {
        if candidate.is_constant
            || requested.is_constant
            || candidate.space_id != requested.space_id
            || !is_register_space_id(candidate.space_id)
            || candidate.size < requested.size
        {
            return false;
        }
        let candidate_start = candidate.offset;
        let requested_start = requested.offset;
        let Some(candidate_end) = candidate_start.checked_add(u64::from(candidate.size)) else {
            return false;
        };
        let Some(requested_end) = requested_start.checked_add(u64::from(requested.size)) else {
            return false;
        };
        candidate_start <= requested_start && candidate_end >= requested_end
    }

    pub(in crate::midend::builder) fn register_key_zero_extends(
        &self,
        candidate: &VarnodeKey,
        requested: &VarnodeKey,
    ) -> bool {
        self.options.is_64bit
            && !candidate.is_constant
            && !requested.is_constant
            && candidate.space_id == requested.space_id
            && is_register_space_id(candidate.space_id)
            && candidate.offset == requested.offset
            && candidate.size == 4
            && requested.size == 8
            && self
                .register_namer()
                .hw_name_at(candidate.offset, candidate.size)
                .is_some()
    }

    pub(in crate::midend::builder) fn register_key_cross_space_covers(
        &self,
        candidate: &VarnodeKey,
        requested: &VarnodeKey,
    ) -> bool {
        self.options.is_64bit
            && !candidate.is_constant
            && !requested.is_constant
            && candidate.space_id != requested.space_id
            && candidate.size >= requested.size
            && self.gpr_family_index_for_key(candidate) == self.gpr_family_index_for_key(requested)
            && self.gpr_family_index_for_key(candidate).is_some()
    }

    pub(in crate::midend::builder) fn register_key_cross_space_zero_extends(
        &self,
        candidate: &VarnodeKey,
        requested: &VarnodeKey,
    ) -> bool {
        self.options.is_64bit
            && !candidate.is_constant
            && !requested.is_constant
            && candidate.space_id != requested.space_id
            && candidate.size == 4
            && requested.size == 8
            && self.gpr_family_index_for_key(candidate) == self.gpr_family_index_for_key(requested)
            && self.gpr_family_index_for_key(candidate).is_some()
    }

    pub(in crate::midend::builder) fn gpr_family_index_for_key(
        &self,
        key: &VarnodeKey,
    ) -> Option<usize> {
        if key.is_constant {
            return None;
        }
        if let Some(cached) = self.gpr_family_cache.borrow().get(key).copied() {
            return cached;
        }
        let answer = self.gpr_family_index_for_key_uncached(key);
        self.gpr_family_cache
            .borrow_mut()
            .insert(key.clone(), answer);
        answer
    }

    /// The answer itself. `gpr_family_index_at` allocates a `String` for the
    /// register's hardware name before looking it up, and expression lowering
    /// asks this per operand, so the caller above remembers it per key.
    fn gpr_family_index_for_key_uncached(&self, key: &VarnodeKey) -> Option<usize> {
        if is_register_space_id(key.space_id) {
            return self
                .register_namer()
                .gpr_family_index_at(key.offset, key.size);
        }
        if is_unique_space_id(key.space_id) {
            let name = crate::arch::x86::unique_x86_register_name(key.offset, key.size)?;
            return crate::arch::x86::x86_gpr_family_index(name);
        }
        None
    }

    fn varnode_covers(candidate: &Varnode, requested: &Varnode) -> bool {
        let candidate_key = VarnodeKey::from(candidate);
        let requested_key = VarnodeKey::from(requested);
        Self::register_key_covers(&candidate_key, &requested_key)
            || Self::unique_key_covers(&candidate_key, &requested_key)
    }

    pub(super) fn unique_key_covers(candidate: &VarnodeKey, requested: &VarnodeKey) -> bool {
        if candidate.is_constant
            || requested.is_constant
            || !is_unique_space_id(candidate.space_id)
            || !is_unique_space_id(requested.space_id)
            || candidate.space_id != requested.space_id
            || candidate.size < requested.size
        {
            return false;
        }
        let candidate_start = candidate.offset;
        let requested_start = requested.offset;
        let Some(candidate_end) = candidate_start.checked_add(u64::from(candidate.size)) else {
            return false;
        };
        let Some(requested_end) = requested_start.checked_add(u64::from(requested.size)) else {
            return false;
        };
        candidate_start <= requested_start && candidate_end >= requested_end
    }

    pub(in crate::midend::builder) fn varnode_aliases_value(
        &self,
        candidate: &Varnode,
        requested: &Varnode,
    ) -> bool {
        let candidate_key = VarnodeKey::from(candidate);
        let requested_key = VarnodeKey::from(requested);
        Self::varnode_covers(candidate, requested)
            || self.register_key_zero_extends(&candidate_key, &requested_key)
            || self.register_key_cross_space_covers(&candidate_key, &requested_key)
            || self.register_key_cross_space_zero_extends(&candidate_key, &requested_key)
    }

    pub(super) fn project_alias_def_expr(
        &self,
        requested: &Varnode,
        op: &PcodeOp,
        expr: PreHirExpr,
    ) -> PreHirExpr {
        let Some(output) = op.output.as_ref() else {
            return expr;
        };
        if VarnodeKey::from(output) == VarnodeKey::from(requested) {
            return expr;
        }
        if self.register_key_zero_extends(&VarnodeKey::from(output), &VarnodeKey::from(requested)) {
            return PreHirExpr::Cast {
                ty: type_from_size(requested.size, false),
                expr: Box::new(expr),
            };
        }
        if self.register_key_cross_space_zero_extends(
            &VarnodeKey::from(output),
            &VarnodeKey::from(requested),
        ) || self.register_key_cross_space_covers(
            &VarnodeKey::from(output),
            &VarnodeKey::from(requested),
        ) {
            return PreHirExpr::Cast {
                ty: type_from_size(requested.size, false),
                expr: Box::new(expr),
            };
        }
        if self.aarch64_gpr_low_view_alias(output, requested) {
            return PreHirExpr::Cast {
                ty: type_from_size(requested.size, false),
                expr: Box::new(expr),
            };
        }
        if !Self::varnode_covers(output, requested) {
            return expr;
        }
        let byte_offset = self.projection_shift_bytes(output, requested);
        let shifted = if byte_offset == 0 {
            expr
        } else {
            PreHirExpr::Binary {
                op: PreHirBinaryOp::Shr,
                lhs: Box::new(expr),
                rhs: Box::new(PreHirExpr::Const(
                    (byte_offset * 8) as i64,
                    NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                )),
                ty: type_from_size(output.size, false),
            }
        };
        PreHirExpr::Cast {
            ty: type_from_size(requested.size, false),
            expr: Box::new(shifted),
        }
    }

    pub(in crate::midend) fn lower_covering_passthrough_register_lane(
        &mut self,
        requested: &Varnode,
        def_site: LoweringSite,
        def: &PcodeOp,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<Option<PreHirExpr>, MlilPreviewError> {
        if requested.is_constant
            || !is_register_space_id(requested.space_id)
            || !matches!(
                def.opcode,
                PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt
            )
        {
            return Ok(None);
        }
        let Some(output) = def.output.as_ref() else {
            return Ok(None);
        };
        let requested_key = VarnodeKey::from(requested);
        let output_key = VarnodeKey::from(output);
        if output.size < 16
            || output.size <= requested.size
            || !Self::register_key_covers(&output_key, &requested_key)
        {
            return Ok(None);
        }
        let Some(input) = def.inputs.first() else {
            return Ok(None);
        };
        let output_end = output.offset.saturating_add(u64::from(output.size));
        let requested_end = requested.offset.saturating_add(u64::from(requested.size));
        let relative_lsb_offset = if self.options.is_big_endian {
            output_end.saturating_sub(requested_end)
        } else {
            requested.offset.saturating_sub(output.offset)
        };
        let requested_size = u64::from(requested.size);
        let input_size = u64::from(input.size);
        let Some(input_lane_end) = relative_lsb_offset.checked_add(requested_size) else {
            return Ok(None);
        };
        if input_lane_end > input_size {
            return Ok(None);
        }
        if input.is_constant {
            let shift = relative_lsb_offset.saturating_mul(8);
            if shift >= 64 {
                return Ok(None);
            }
            let bits = requested.size.saturating_mul(8);
            let mask = if bits >= 64 {
                u64::MAX
            } else {
                (1u64 << bits) - 1
            };
            let value = ((input.constant_val as u64) >> shift) & mask;
            return Ok(Some(PreHirExpr::Const(
                value as i64,
                type_from_size(requested.size, false),
            )));
        }
        let input_storage_offset = if self.options.is_big_endian {
            input_size - input_lane_end
        } else {
            relative_lsb_offset
        };
        let input_lane = Varnode {
            space_id: input.space_id,
            offset: input.offset.saturating_add(input_storage_offset),
            size: requested.size,
            is_constant: false,
            constant_val: 0,
        };
        self.with_lowering_site(def_site, |this| {
            this.lower_varnode(&input_lane, visiting).map(Some)
        })
    }

    fn projection_shift_bytes(&self, output: &Varnode, requested: &Varnode) -> u64 {
        if self.options.is_big_endian {
            let output_end = output.offset.saturating_add(u64::from(output.size));
            let requested_end = requested.offset.saturating_add(u64::from(requested.size));
            return output_end.saturating_sub(requested_end);
        }
        requested.offset.saturating_sub(output.offset)
    }

    fn aarch64_gpr_low_view_alias(&self, output: &Varnode, requested: &Varnode) -> bool {
        self.options.calling_convention == CallingConvention::AArch64
            && !output.is_constant
            && !requested.is_constant
            && output.space_id == requested.space_id
            && is_register_space_id(output.space_id)
            && output.size == 8
            && requested.size == 4
            && self
                .register_namer()
                .gpr_family_index_at(output.offset, output.size)
                .is_some_and(|output_family| {
                    self.register_namer()
                        .gpr_family_index_at(requested.offset, requested.size)
                        == Some(output_family)
                })
    }

    fn find_loop_carried_variable_for_register(
        &self,
        vn: &Varnode,
        loop_body: &crate::midend::structuring::loop_analysis::LoopBody,
    ) -> Option<String> {
        let output_key = VarnodeKey::from(vn);
        let mut candidates = std::collections::BTreeSet::new();
        for (mkey, name) in &self.materialized_vns {
            if !Self::varnode_key_may_alias_output(&mkey.varnode, &output_key)
                || mkey.varnode.size != output_key.size
                || name.starts_with("param_")
            {
                continue;
            }
            if let Some(sites) = self.def_sites.get(&mkey.varnode) {
                for site in sites {
                    if loop_body.body.contains(&site.block_idx) {
                        let op = &self.pcode.blocks[site.block_idx].ops[site.op_idx];
                        if op.address == mkey.def_addr && op.seq_num == mkey.def_seq {
                            candidates.insert(name.clone());
                        }
                    }
                }
            }
        }
        if candidates.len() == 1 {
            candidates.into_iter().next()
        } else {
            None
        }
    }

    pub(in crate::midend::builder) fn loop_exit_materialized_register_binding(
        &mut self,
        vn: &Varnode,
    ) -> Option<PreHirExpr> {
        if vn.is_constant || !is_register_space_id(vn.space_id) || vn.size < 4 {
            return None;
        }
        let site = self.current_lowering_site?;
        if self.has_prior_local_def_for_varnode(vn, site) {
            return None;
        }
        let predecessor_idxs = self.predecessors.get(site.block_idx)?.clone();

        // Single predecessor path: check if predecessor is inside a loop, and we are exiting it.
        if predecessor_idxs.len() == 1 {
            let pred_idx = predecessor_idxs[0];
            if pred_idx != site.block_idx {
                for loop_body in &self.loop_bodies {
                    if loop_body.body.contains(&pred_idx)
                        && !loop_body.body.contains(&site.block_idx)
                    {
                        if let Some(name) =
                            self.find_loop_carried_variable_for_register(vn, loop_body)
                        {
                            return Some(PreHirExpr::Var(name));
                        }
                    }
                }
            }
        }

        // A loop tail may join an entry bypass at a shared return block.  If
        // the tail copies this register into an entry-owned register alias,
        // that alias is the same logical value on both incoming paths: the
        // bypass still has the entry seed, while the loop path has the
        // carried update.  Prefer the alias carrier instead of selecting the
        // entry formal for every path (which drops the final loop value).
        if let Some(name) = self.loop_exit_entry_alias_binding(vn, &predecessor_idxs) {
            return Some(PreHirExpr::Var(name));
        }

        if predecessor_idxs.len() < 2 || predecessor_idxs.contains(&site.block_idx) {
            return None;
        }

        let mut materialized_name = None;
        let mut materialized_expr = None;
        let mut zero_incoming = false;
        for pred_idx in predecessor_idxs {
            let pred_block = self.pcode.blocks.get(pred_idx)?;
            if self.predecessor_edge_forces_register_zero(pred_idx, site.block_idx, vn) {
                zero_incoming = true;
                continue;
            }
            let term_idx = self
                .block_terminator_index(pred_block)
                .unwrap_or(pred_block.ops.len());
            let Some((_, pred_op)) =
                self.last_register_redefinition_before(pred_block, term_idx, vn)
            else {
                let mut visiting = HashSet::default();
                if self.predecessor_path_has_zero_register_seed(
                    pred_idx,
                    site.block_idx,
                    vn,
                    0,
                    &mut visiting,
                ) {
                    zero_incoming = true;
                    continue;
                }
                return None;
            };
            if self.register_redefinition_is_zero(pred_block, term_idx, pred_op) {
                zero_incoming = true;
                continue;
            }
            if let Some(name) = pred_op.output.as_ref().and_then(|output| {
                self.materialized_vns
                    .get(&MaterializedVarnodeKey::new(output, pred_op))
                    .filter(|_| self.varnode_aliases_value(output, vn))
                    .cloned()
            }) {
                match &materialized_name {
                    Some(existing) if existing != &name => return None,
                    None => {
                        materialized_expr = Some(self.project_alias_def_expr(
                            vn,
                            pred_op,
                            PreHirExpr::Var(name.clone()),
                        ));
                        materialized_name = Some(name);
                    }
                    _ => {}
                }
                continue;
            }
            return None;
        }

        let name = materialized_name?;
        if !zero_incoming {
            return None;
        }
        if let Some(binding) = self.temps.get_mut(&name)
            && binding.initializer.is_none()
        {
            binding.initializer = Some(PreHirExpr::Const(0, type_from_size(vn.size, false)));
        }
        materialized_expr
    }

    fn loop_exit_entry_alias_binding(
        &mut self,
        vn: &Varnode,
        predecessor_idxs: &[usize],
    ) -> Option<String> {
        let loop_tail_blocks: Vec<usize> = self
            .loop_bodies
            .iter()
            .filter(|loop_body| {
                predecessor_idxs
                    .iter()
                    .any(|pred_idx| loop_body.body.contains(pred_idx))
            })
            .flat_map(|loop_body| loop_body.body.iter().copied())
            .filter(|block_idx| predecessor_idxs.contains(block_idx))
            .collect();
        let mut candidate_sites = Vec::new();
        for block_idx in loop_tail_blocks {
            let Some(block) = self.pcode.blocks.get(block_idx) else {
                continue;
            };
            for (op_idx, op) in block.ops.iter().enumerate() {
                if !matches!(
                    op.opcode,
                    PcodeOpcode::Copy
                        | PcodeOpcode::Cast
                        | PcodeOpcode::IntZExt
                        | PcodeOpcode::IntSExt
                ) || op.inputs.len() != 1
                {
                    continue;
                }
                let Some(output) = op.output.as_ref() else {
                    continue;
                };
                let Some(input) = op.inputs.first() else {
                    continue;
                };
                if is_register_space_id(output.space_id)
                    && self.register_param_aliases.contains_key(&output.offset)
                    && self.varnode_aliases_value(input, vn)
                {
                    candidate_sites.push((block_idx, op_idx));
                }
            }
        }

        for (block_idx, op_idx) in candidate_sites {
            let Some(name) = self.with_lowering_site(LoweringSite { block_idx, op_idx }, |this| {
                let block = this.pcode.blocks.get(block_idx)?;
                let op = block.ops.get(op_idx)?;
                let output = op.output.as_ref()?;
                let output_key = VarnodeKey::from(output);
                if block
                    .ops
                    .iter()
                    .skip(op_idx + 1)
                    .any(|candidate| Self::op_kills_varnode_definition(candidate, &output_key))
                {
                    return None;
                }
                let name = this.prior_materialized_same_register_output_name(output)?;
                this.temps.contains_key(&name).then_some(name)
            }) else {
                continue;
            };

            return Some(name);
        }
        None
    }

    fn predecessor_path_has_zero_register_seed(
        &self,
        pred_idx: usize,
        succ_idx: usize,
        vn: &Varnode,
        depth: usize,
        visiting: &mut HashSet<usize>,
    ) -> bool {
        if depth > 8 || pred_idx == succ_idx || !visiting.insert(pred_idx) {
            return false;
        }
        let result = self.pcode.blocks.get(pred_idx).is_some_and(|block| {
            let term_idx = self
                .block_terminator_index(block)
                .unwrap_or(block.ops.len());
            if let Some((def_idx, def_op)) =
                self.last_register_redefinition_before(block, term_idx, vn)
            {
                return self.register_redefinition_is_zero(block, term_idx, def_op)
                    && !Self::block_has_aliasing_side_effect_range(block, def_idx + 1, term_idx);
            }
            if Self::block_has_aliasing_side_effect_range(block, 0, term_idx) {
                return false;
            }
            let incoming = self
                .predecessors
                .get(pred_idx)
                .into_iter()
                .flatten()
                .copied()
                .filter(|incoming_idx| *incoming_idx != succ_idx)
                .collect::<Vec<_>>();
            !incoming.is_empty()
                && incoming.into_iter().all(|incoming_idx| {
                    self.predecessor_path_has_zero_register_seed(
                        incoming_idx,
                        pred_idx,
                        vn,
                        depth + 1,
                        visiting,
                    )
                })
        });
        visiting.remove(&pred_idx);
        result
    }

    pub(in crate::midend::builder) fn block_has_aliasing_side_effect_range(
        block: &crate::pcode::PcodeBasicBlock,
        start: usize,
        end: usize,
    ) -> bool {
        block.ops[start..end.min(block.ops.len())].iter().any(|op| {
            matches!(
                op.opcode,
                PcodeOpcode::Load
                    | PcodeOpcode::Store
                    | PcodeOpcode::Call
                    | PcodeOpcode::CallInd
                    | PcodeOpcode::CallOther
            )
        })
    }

    pub(in crate::midend::builder) fn predecessor_edge_forces_register_zero(
        &self,
        pred_idx: usize,
        succ_idx: usize,
        vn: &Varnode,
    ) -> bool {
        if vn.is_constant || !is_register_space_id(vn.space_id) {
            return false;
        }
        let Some(pred_block) = self.pcode.blocks.get(pred_idx) else {
            return false;
        };
        let Some(term_idx) = self.block_terminator_index(pred_block) else {
            return false;
        };
        let Some(term) = pred_block.ops.get(term_idx) else {
            return false;
        };
        if term.opcode != PcodeOpcode::CBranch || term.inputs.len() < 2 {
            return false;
        }
        let Some(edge_is_taken) = self.cbranch_successor_is_taken_edge(pred_block, succ_idx) else {
            return false;
        };
        let predicate = &term.inputs[1];
        self.predicate_edge_forces_register_zero(pred_block, term_idx, predicate, vn, edge_is_taken)
    }

    fn cbranch_successor_is_taken_edge(
        &self,
        pred_block: &crate::pcode::PcodeBasicBlock,
        succ_idx: usize,
    ) -> Option<bool> {
        let succ_idx = u32::try_from(succ_idx).ok()?;
        let first = pred_block.successors.first().copied()?;
        if first == succ_idx {
            return Some(true);
        }
        if pred_block.successors.get(1).copied() == Some(succ_idx) {
            return Some(false);
        }
        None
    }

    fn predicate_edge_forces_register_zero(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        before_idx: usize,
        predicate: &Varnode,
        vn: &Varnode,
        predicate_value: bool,
    ) -> bool {
        if predicate.is_constant {
            return false;
        }
        let key = VarnodeKey::from(predicate);
        let Some(pred_op) = block.ops.iter().take(before_idx).rev().find(|op| {
            op.output
                .as_ref()
                .is_some_and(|output| VarnodeKey::from(output) == key)
        }) else {
            return false;
        };
        match pred_op.opcode {
            PcodeOpcode::IntEqual | PcodeOpcode::IntNotEqual if pred_op.inputs.len() == 2 => {
                let forces_equal = match pred_op.opcode {
                    PcodeOpcode::IntEqual => predicate_value,
                    PcodeOpcode::IntNotEqual => !predicate_value,
                    _ => false,
                };
                forces_equal && self.compare_predicate_tests_register_against_zero(pred_op, vn)
            }
            _ => false,
        }
    }

    fn compare_predicate_tests_register_against_zero(&self, op: &PcodeOp, vn: &Varnode) -> bool {
        let [left, right] = op.inputs.as_slice() else {
            return false;
        };
        (self.varnode_aliases_value(left, vn) && self.varnode_is_const_zero(right))
            || (self.varnode_aliases_value(right, vn) && self.varnode_is_const_zero(left))
    }

    fn varnode_is_const_zero(&self, vn: &Varnode) -> bool {
        vn.is_constant && vn.constant_val == 0
    }

    pub(in crate::midend::builder) fn last_register_redefinition_before<'b>(
        &self,
        block: &'b crate::pcode::PcodeBasicBlock,
        before_idx: usize,
        vn: &Varnode,
    ) -> Option<(usize, &'b PcodeOp)> {
        let requested = VarnodeKey::from(vn);
        block
            .ops
            .iter()
            .enumerate()
            .take(before_idx)
            .rev()
            .find(|(_, op)| {
                op.output.as_ref().is_some_and(|output| {
                    let candidate = VarnodeKey::from(output);
                    !candidate.is_constant
                        && candidate.space_id == requested.space_id
                        && is_register_space_id(candidate.space_id)
                        && Self::register_key_ranges_overlap_for_lookup(&candidate, &requested)
                })
            })
    }

    fn register_key_ranges_overlap_for_lookup(lhs: &VarnodeKey, rhs: &VarnodeKey) -> bool {
        let Some(lhs_end) = lhs.offset.checked_add(u64::from(lhs.size)) else {
            return false;
        };
        let Some(rhs_end) = rhs.offset.checked_add(u64::from(rhs.size)) else {
            return false;
        };
        lhs.offset < rhs_end && rhs.offset < lhs_end
    }

    fn register_redefinition_is_zero(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        before_idx: usize,
        op: &PcodeOp,
    ) -> bool {
        match op.opcode {
            PcodeOpcode::Copy => op
                .inputs
                .first()
                .is_some_and(|input| input.is_constant && input.constant_val == 0),
            PcodeOpcode::IntZExt | PcodeOpcode::IntSExt | PcodeOpcode::Cast => op
                .inputs
                .first()
                .is_some_and(|input| self.varnode_is_same_block_zero(block, before_idx, input)),
            _ => false,
        }
    }

    fn varnode_is_same_block_zero(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        before_idx: usize,
        vn: &Varnode,
    ) -> bool {
        if vn.is_constant && vn.constant_val == 0 {
            return true;
        }
        let key = VarnodeKey::from(vn);
        block
            .ops
            .iter()
            .take(before_idx)
            .rev()
            .find(|op| {
                op.output
                    .as_ref()
                    .is_some_and(|output| VarnodeKey::from(output) == key)
            })
            .is_some_and(|op| {
                op.opcode == PcodeOpcode::Copy
                    && op
                        .inputs
                        .first()
                        .is_some_and(|input| input.is_constant && input.constant_val == 0)
            })
    }
}
