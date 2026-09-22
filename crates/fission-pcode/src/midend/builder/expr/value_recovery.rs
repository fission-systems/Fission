use super::*;

enum LowLaneUse<'a> {
    Transform { value: &'a Varnode, known_size: u32 },
    Terminal,
}

impl<'a> PreviewBuilder<'a> {
    pub(super) fn try_lower_scalar_ssa_piece_reassembly(
        &mut self,
        vn: &Varnode,
        key: &VarnodeKey,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<Option<PreHirExpr>, MlilPreviewError> {
        if !is_register_space_id(vn.space_id) || vn.size < 2 || vn.size > 8 {
            return Ok(None);
        }
        let Some(use_site) = self.current_lowering_site else {
            return Ok(None);
        };
        let Some(op) = self
            .pcode
            .blocks
            .get(use_site.block_idx)
            .and_then(|block| block.ops.get(use_site.op_idx))
        else {
            return Ok(None);
        };
        let Some(input_idx) = op
            .inputs
            .iter()
            .position(|input| VarnodeKey::from(input) == *key)
        else {
            return Ok(None);
        };
        let Ok(block) = u32::try_from(use_site.block_idx) else {
            return Ok(None);
        };
        let Ok(op_idx) = u32::try_from(use_site.op_idx) else {
            return Ok(None);
        };
        let Ok(input) = u32::try_from(input_idx) else {
            return Ok(None);
        };
        let Some(pieces) = self
            .scalar_ssa
            .operation_inputs
            .get(&SsaUseSite {
                block,
                op: op_idx,
                input,
            })
            .filter(|pieces| pieces.len() >= 2)
            .cloned()
        else {
            return Ok(None);
        };

        let mut expected_byte_offset = 0u32;
        let mut plan = Vec::with_capacity(pieces.len());
        for piece in pieces {
            let Some(value) = self.scalar_ssa.value(piece.value) else {
                return Ok(None);
            };
            if piece.byte_offset != expected_byte_offset
                || value.storage.space_id != vn.space_id
                || value.storage.offset != vn.offset.saturating_add(u64::from(piece.byte_offset))
                || value.storage.size == 0
            {
                return Ok(None);
            }
            let SsaValueDefinition::Operation(def_site) = value.definition else {
                return Ok(None);
            };
            let Some(def_op) = self
                .pcode
                .blocks
                .get(def_site.block as usize)
                .and_then(|block| block.ops.get(def_site.op as usize))
                .cloned()
            else {
                return Ok(None);
            };
            if !def_op.output.as_ref().is_some_and(|output| {
                !output.is_constant
                    && output.space_id == value.storage.space_id
                    && output.offset == value.storage.offset
                    && output.size == value.storage.size
            }) {
                return Ok(None);
            }
            expected_byte_offset = expected_byte_offset.saturating_add(value.storage.size);
            plan.push((piece.byte_offset, value.storage.size, def_site, def_op));
        }
        if expected_byte_offset != vn.size || !visiting.insert(key.clone()) {
            return Ok(None);
        }

        let result_ty = type_from_size(vn.size, false);
        let mut combined = None;
        for (byte_offset, piece_size, def_site, def_op) in plan {
            let piece_expr = match self.with_lowering_site(
                LoweringSite {
                    block_idx: def_site.block as usize,
                    op_idx: def_site.op as usize,
                },
                |this| this.lower_def_op(&def_op, visiting),
            ) {
                Ok(expr) => expr,
                Err(_) => {
                    visiting.remove(key);
                    return Ok(None);
                }
            };
            let narrowed = PreHirExpr::Cast {
                ty: type_from_size(piece_size, false),
                expr: Box::new(piece_expr),
            };
            let widened = PreHirExpr::Cast {
                ty: result_ty.clone(),
                expr: Box::new(narrowed),
            };
            let shift_bytes = if self.options.is_big_endian {
                vn.size
                    .saturating_sub(byte_offset.saturating_add(piece_size))
            } else {
                byte_offset
            };
            let positioned = if shift_bytes == 0 {
                widened
            } else {
                PreHirExpr::Binary {
                    op: PreHirBinaryOp::Shl,
                    lhs: Box::new(widened),
                    rhs: Box::new(PreHirExpr::Const(
                        i64::from(shift_bytes) * 8,
                        NirType::Int {
                            bits: 64,
                            signed: false,
                        },
                    )),
                    ty: result_ty.clone(),
                }
            };
            combined = Some(match combined {
                None => positioned,
                Some(lhs) => PreHirExpr::Binary {
                    op: PreHirBinaryOp::Or,
                    lhs: Box::new(lhs),
                    rhs: Box::new(positioned),
                    ty: result_ty.clone(),
                },
            });
        }
        visiting.remove(key);
        Ok(combined)
    }

    pub(super) fn current_store_value_read_at_join(&self, vn: &Varnode) -> bool {
        let Some(site) = self.current_lowering_site else {
            return false;
        };
        if self
            .predecessors
            .get(site.block_idx)
            .is_none_or(|preds| preds.len() < 2)
        {
            return false;
        }
        let Some(op) = self
            .pcode
            .blocks
            .get(site.block_idx)
            .and_then(|block| block.ops.get(site.op_idx))
        else {
            return false;
        };
        op.opcode == PcodeOpcode::Store
            && op
                .inputs
                .get(2)
                .is_some_and(|input| self.varnode_aliases_value(input, vn))
    }

    pub(super) fn current_join_register_update_reads_live_register(&self, vn: &Varnode) -> bool {
        let Some(site) = self.current_lowering_site else {
            return false;
        };
        if self
            .predecessors
            .get(site.block_idx)
            .is_none_or(|preds| preds.len() < 2)
        {
            return false;
        }
        let Some(block) = self.pcode.blocks.get(site.block_idx) else {
            return false;
        };
        let Some(op) = block.ops.get(site.op_idx) else {
            return false;
        };
        if !op
            .inputs
            .iter()
            .any(|input| self.varnode_aliases_value(input, vn))
        {
            return false;
        }
        block.ops.iter().skip(site.op_idx + 1).any(|candidate| {
            candidate
                .output
                .as_ref()
                .is_some_and(|output| self.varnode_aliases_value(output, vn))
        })
    }

    pub(super) fn live_register_name_for_join_register_read(&self, vn: &Varnode) -> Option<String> {
        if !is_register_space_id(vn.space_id) {
            return None;
        }
        if self.options.calling_convention == CallingConvention::AArch64 && vn.size == 8 {
            return self.sla_hw_name(vn.offset, 4);
        }
        self.sla_hw_name(vn.offset, vn.size)
    }

    pub(super) fn try_lower_diamond_select_for_varnode(
        &mut self,
        vn: &Varnode,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<Option<PreHirExpr>, MlilPreviewError> {
        if !is_register_space_id(vn.space_id) {
            return Ok(None);
        }
        let key = VarnodeKey::from(vn);
        if visiting.contains(&key) {
            return Ok(None);
        }
        let Some(site) = self.current_lowering_site else {
            return Ok(None);
        };
        let Some(preds) = self.predecessors.get(site.block_idx).cloned() else {
            return Ok(None);
        };
        let [pred_a, pred_b] = preds.as_slice() else {
            return Ok(None);
        };
        let Some((branch_idx, branch_term_idx)) =
            self.find_diamond_branch_for_predecessors(*pred_a, *pred_b)
        else {
            return Ok(None);
        };
        let branch_block = self.pcode.blocks[branch_idx].clone();
        let branch_op = branch_block.ops[branch_term_idx].clone();
        if branch_op.opcode != PcodeOpcode::CBranch {
            return Ok(None);
        }
        let Some(cond_vn) = branch_op.inputs.last().cloned() else {
            return Ok(None);
        };
        let Some(target_vn) = branch_op.inputs.first() else {
            return Ok(None);
        };
        let Some(true_succ_idx) = resolve_branch_target_index(
            self.pcode,
            &self.address_to_index,
            branch_idx,
            &branch_op,
            target_vn,
        ) else {
            return Ok(None);
        };
        let false_succ_idx = if true_succ_idx == *pred_a {
            *pred_b
        } else if true_succ_idx == *pred_b {
            *pred_a
        } else {
            return Ok(None);
        };

        const DIAMOND_SELECT_DEPTH_CAP: u32 = 64;
        if self.diamond_select_depth >= DIAMOND_SELECT_DEPTH_CAP {
            return Ok(None);
        }

        visiting.insert(key.clone());
        self.diamond_select_depth += 1;
        let true_res = self.lower_predecessor_incoming_value(true_succ_idx, vn, visiting);
        let false_res = if true_res.is_ok() {
            self.lower_predecessor_incoming_value(false_succ_idx, vn, visiting)
        } else {
            Err(MlilPreviewError::LoweringFailed)
        };
        self.diamond_select_depth -= 1;
        visiting.remove(&key);

        let Some(true_expr) = true_res? else {
            return Ok(None);
        };
        let Some(false_expr) = false_res? else {
            return Ok(None);
        };
        if strip_casts(&true_expr) == strip_casts(&false_expr) {
            return Ok(Some(true_expr));
        }
        let cond = self.with_lowering_site(
            LoweringSite {
                block_idx: branch_idx,
                op_idx: branch_term_idx,
            },
            |this| this.lower_varnode(&cond_vn, visiting),
        )?;
        Ok(Some(PreHirExpr::Select {
            cond: Box::new(cond),
            then_expr: Box::new(true_expr),
            else_expr: Box::new(false_expr),
            ty: type_from_size(vn.size, false),
        }))
    }

    pub(in crate::midend::builder) fn find_diamond_branch_for_predecessors(
        &self,
        pred_a: usize,
        pred_b: usize,
    ) -> Option<(usize, usize)> {
        for (block_idx, succs) in self.successors.iter().enumerate() {
            if succs.len() != 2 || !succs.contains(&pred_a) || !succs.contains(&pred_b) {
                continue;
            }
            let block = self.pcode.blocks.get(block_idx)?;
            let term_idx = self.block_terminator_index(block)?;
            if block.ops.get(term_idx)?.opcode == PcodeOpcode::CBranch {
                return Some((block_idx, term_idx));
            }
        }
        None
    }

    fn lower_predecessor_incoming_value(
        &mut self,
        pred_idx: usize,
        vn: &Varnode,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<Option<PreHirExpr>, MlilPreviewError> {
        let Some(pred_block) = self.pcode.blocks.get(pred_idx).cloned() else {
            return Ok(None);
        };
        let term_idx = self
            .block_terminator_index(&pred_block)
            .unwrap_or(pred_block.ops.len());
        // CallInd/Call leave the ABI return value in the primary return register
        // without a p-code write. Prefer the materialize-time call-result binding
        // over a pre-call EAX/RAX write that only staged arguments
        // (apply_binop: `rax = (*(fp))(a,b)` then join must not re-read `a`).
        if self.register_namer().is_primary_return_register(vn)
            && let Some(name) =
                self.live_call_result_binding_in_block_for_return_register(vn, pred_idx, term_idx)
        {
            return Ok(Some(PreHirExpr::Var(name)));
        }
        let Some(def_idx) = self.last_alias_def_in_block(&pred_block, term_idx, vn) else {
            return Ok(None);
        };
        // If a Call/CallInd occurs after the last register write, that call is the
        // true producer of the return value (p-code does not model ABI out regs).
        if self.register_namer().is_primary_return_register(vn)
            && pred_block.ops[def_idx + 1..term_idx.min(pred_block.ops.len())]
                .iter()
                .any(|op| {
                    op.output.is_none()
                        && matches!(
                            op.opcode,
                            PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther
                        )
                })
            && let Some(name) =
                self.live_call_result_binding_in_block_for_return_register(vn, pred_idx, term_idx)
        {
            return Ok(Some(PreHirExpr::Var(name)));
        }
        let op = pred_block.ops[def_idx].clone();
        let expr = self.with_lowering_site(
            LoweringSite {
                block_idx: pred_idx,
                op_idx: def_idx,
            },
            |this| this.lower_def_op(&op, visiting),
        )?;
        Ok(Some(self.project_alias_def_expr(vn, &op, expr)))
    }

    pub(super) fn try_lower_zero_extended_partial_register(
        &mut self,
        vn: &Varnode,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<Option<PreHirExpr>, MlilPreviewError> {
        if vn.is_constant || !is_register_space_id(vn.space_id) || vn.size <= 1 {
            return Ok(None);
        }
        let Some(site) = self.current_lowering_site else {
            return Ok(None);
        };
        let Some(block) = self.pcode.blocks.get(self.pcode_block_idx(site.block_idx)) else {
            return Ok(None);
        };
        let scan_end = site.op_idx.min(block.ops.len());
        let requested_start = vn.offset;
        let requested_end = requested_start.saturating_add(u64::from(vn.size));
        let mut zeroed_ranges = Vec::new();
        // x86 `xor reg,reg; setcc low` zeros *before* the partial write. Reverse
        // scan therefore sees the setcc first; keep it pending until an older
        // clear covers the upper bytes (AArch64-style zero-*after*-partial still
        // resolves immediately when the clear is already in `zeroed_ranges`).
        let mut pending_partial_idx: Option<usize> = None;

        for idx in (0..scan_end).rev() {
            let op = &block.ops[idx];
            let Some(output) = op.output.as_ref() else {
                continue;
            };
            if output.is_constant
                || output.space_id != vn.space_id
                || !is_register_space_id(output.space_id)
                || !Self::varnode_ranges_overlap(output.offset, output.size, vn.offset, vn.size)
            {
                continue;
            }

            if Self::is_register_clear_def(op, output) {
                zeroed_ranges.push((
                    output.offset.max(requested_start),
                    output
                        .offset
                        .saturating_add(u64::from(output.size))
                        .min(requested_end),
                ));
                if let Some(partial_idx) = pending_partial_idx {
                    if let Some(expr) = self.try_finish_zero_extended_partial(
                        block,
                        site.block_idx,
                        partial_idx,
                        vn,
                        requested_start,
                        requested_end,
                        &zeroed_ranges,
                        visiting,
                    )? {
                        return Ok(Some(expr));
                    }
                }
                continue;
            }

            // Same-register low→wide IntZExt. Two roles:
            // 1) After `xor eax,eax`, SLEIGH emits `IntZExt rax ← eax`. That
            //    only zeros above EAX and must not block composing a later
            //    `setnz al` with the xor clear when reading EAX.
            // 2) `movzx eax, al` is value-defining for the requested width
            //    (input narrower than the read). Treat as a real def — do not
            //    skip (checksum byte accumulator: add al; movzx eax,al).
            if Self::is_same_register_low_zext(op, output) {
                let input = &op.inputs[0];
                let low_end = output.offset.saturating_add(u64::from(input.size));
                let out_end = output.offset.saturating_add(u64::from(output.size));
                // Upper bytes introduced by the zext are zero.
                let z_start = low_end.max(requested_start);
                let z_end = out_end.min(requested_end);
                if z_start < z_end {
                    zeroed_ranges.push((z_start, z_end));
                }
                // Transparent only when the zext input already covers the full
                // requested width (extension is strictly above what we read).
                let input_covers_request =
                    input.offset <= requested_start && low_end >= requested_end;
                if input_covers_request {
                    if let Some(partial_idx) = pending_partial_idx {
                        if let Some(expr) = self.try_finish_zero_extended_partial(
                            block,
                            site.block_idx,
                            partial_idx,
                            vn,
                            requested_start,
                            requested_end,
                            &zeroed_ranges,
                            visiting,
                        )? {
                            return Ok(Some(expr));
                        }
                    }
                    continue;
                }
                // Value-defining movzx into/within the requested range: stop.
                return Ok(None);
            }

            if output.offset == requested_start && output.size < vn.size {
                if let Some(expr) = self.try_finish_zero_extended_partial(
                    block,
                    site.block_idx,
                    idx,
                    vn,
                    requested_start,
                    requested_end,
                    &zeroed_ranges,
                    visiting,
                )? {
                    return Ok(Some(expr));
                }
                // Upper not covered yet — wait for an older clear (xor-before-setcc).
                if pending_partial_idx.is_none() {
                    pending_partial_idx = Some(idx);
                }
                continue;
            }

            // Other overlapping def: cannot compose through it.
            return Ok(None);
        }

        Ok(None)
    }

    fn try_finish_zero_extended_partial(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        block_idx: usize,
        partial_idx: usize,
        vn: &Varnode,
        requested_start: u64,
        requested_end: u64,
        zeroed_ranges: &[(u64, u64)],
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<Option<PreHirExpr>, MlilPreviewError> {
        let op = &block.ops[partial_idx];
        let Some(output) = op.output.as_ref() else {
            return Ok(None);
        };
        if output.offset != requested_start || output.size >= vn.size {
            return Ok(None);
        }
        let upper_start = requested_start.saturating_add(u64::from(output.size));
        if !Self::ranges_cover(upper_start, requested_end, zeroed_ranges) {
            return Ok(None);
        }
        let expr = self.with_lowering_site(
            LoweringSite {
                block_idx,
                op_idx: partial_idx,
            },
            |this| this.lower_def_op(op, visiting),
        )?;
        Ok(Some(PreHirExpr::Cast {
            ty: type_from_size(vn.size, false),
            expr: Box::new(expr),
        }))
    }

    /// Recover a wider read from a narrow register definition when the
    /// dependency cone proves that no consumer can observe the unknown upper
    /// bytes.
    ///
    /// A partial write such as `setcc r8b` must not be treated as a complete
    /// definition of `r8`: a later comparison or address calculation really
    /// can observe the stale upper bytes.  It is nevertheless sound to use
    /// the low lane when every same-block consumer is a low-lane-preserving
    /// operation and the value is ultimately observed through that lane.  The
    /// proof is deliberately bounded to the current block; a live-out use is
    /// rejected unless the existing reaching-use scan proves that no such use
    /// exists.
    pub(super) fn try_lower_observed_low_lane_partial_register(
        &mut self,
        vn: &Varnode,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<Option<PreHirExpr>, MlilPreviewError> {
        if vn.is_constant || !is_register_space_id(vn.space_id) || vn.size <= 1 {
            return Ok(None);
        }
        let Some(site) = self.current_lowering_site else {
            return Ok(None);
        };
        let block_idx = self.pcode_block_idx(site.block_idx);
        let Some(block) = self.pcode.blocks.get(block_idx) else {
            return Ok(None);
        };
        let scan_end = site.op_idx.min(block.ops.len());

        // Find the latest low-lane definition that can reach this wide read.
        // Any intervening overlapping definition invalidates the proof unless
        // it is itself the candidate we are trying to recover.
        let mut candidate = None;
        for idx in (0..scan_end).rev() {
            let Some(output) = block.ops[idx].output.as_ref() else {
                continue;
            };
            if output.is_constant
                || output.space_id != vn.space_id
                || !Self::varnode_ranges_overlap(output.offset, output.size, vn.offset, vn.size)
            {
                continue;
            }
            if output.offset == vn.offset && output.size < vn.size {
                candidate = Some((idx, output.clone()));
            }
            // An overlapping full-width or higher-lane write is the nearest
            // reaching definition, so the older partial write is not the
            // value read here.
            break;
        }
        let Some((candidate_idx, candidate)) = candidate else {
            return Ok(None);
        };
        if candidate.size == 0 {
            return Ok(None);
        }

        let mut active = HashSet::default();
        if !self.prove_low_lane_observation_paths(
            block_idx,
            candidate_idx,
            &candidate,
            candidate.size,
            &mut active,
        ) {
            return Ok(None);
        }

        let op = &block.ops[candidate_idx];
        let expr = self.with_lowering_site(
            LoweringSite {
                block_idx: site.block_idx,
                op_idx: candidate_idx,
            },
            |this| this.lower_def_op(op, visiting),
        )?;
        Ok(Some(PreHirExpr::Cast {
            ty: type_from_size(vn.size, false),
            expr: Box::new(expr),
        }))
    }

    fn prove_low_lane_observation_paths(
        &self,
        block_idx: usize,
        definition_idx: usize,
        value: &Varnode,
        known_size: u32,
        active: &mut HashSet<(usize, usize, VarnodeKey, u32)>,
    ) -> bool {
        if known_size == 0 {
            return false;
        }
        let state = (
            block_idx,
            definition_idx,
            VarnodeKey::from(value),
            known_size,
        );
        if !active.insert(state.clone()) {
            return false;
        }

        let result = self.pcode.blocks.get(block_idx).is_some_and(|block| {
            let key = &state.2;
            let mut valid = true;
            for (op_idx, op) in block.ops.iter().enumerate().skip(definition_idx + 1) {
                let input_idx = op
                    .inputs
                    .iter()
                    .position(|input| Self::varnode_matches_key(input, key));
                let output_overlaps = op
                    .output
                    .as_ref()
                    .is_some_and(|output| Self::varnode_matches_key(output, key));

                if let Some(input_idx) = input_idx {
                    let Some(use_kind) = Self::classify_low_lane_use(op, input_idx, known_size)
                    else {
                        let Some(output) = op.output.as_ref() else {
                            valid = false;
                            break;
                        };
                        let mut dead_active = HashSet::default();
                        if !Self::is_dead_propagating_opcode(op.opcode)
                            || !self.prove_value_is_dead(
                                block_idx,
                                op_idx,
                                output,
                                &mut dead_active,
                            )
                        {
                            valid = false;
                            break;
                        }
                        if output_overlaps {
                            break;
                        }
                        continue;
                    };
                    if let LowLaneUse::Transform {
                        value: next,
                        known_size: next_known_size,
                    } = use_kind
                    {
                        if !self.prove_low_lane_observation_paths(
                            block_idx,
                            op_idx,
                            next,
                            next_known_size,
                            active,
                        ) {
                            valid = false;
                            break;
                        }
                    }
                    // A register-space output overlapping the current
                    // state replaces it. The transformed output proof (if
                    // any) now covers all later observations.
                    if output_overlaps {
                        break;
                    }
                } else if output_overlaps {
                    // A write that does not read the tracked value kills
                    // this dependency path without observing it.
                    break;
                }
            }

            valid
                && self
                    .first_reaching_output_use_after_block_exit(block_idx, definition_idx, value)
                    .is_none()
        });
        active.remove(&state);
        result
    }

    fn prove_value_is_dead(
        &self,
        block_idx: usize,
        definition_idx: usize,
        value: &Varnode,
        active: &mut HashSet<(usize, usize, VarnodeKey)>,
    ) -> bool {
        let state = (block_idx, definition_idx, VarnodeKey::from(value));
        if !active.insert(state.clone()) {
            return false;
        }
        let result = self.pcode.blocks.get(block_idx).is_some_and(|block| {
            let key = &state.2;
            let mut valid = true;
            for (op_idx, op) in block.ops.iter().enumerate().skip(definition_idx + 1) {
                let input_matches = op
                    .inputs
                    .iter()
                    .any(|input| Self::varnode_matches_key(input, key));
                let output_overlaps = op
                    .output
                    .as_ref()
                    .is_some_and(|output| Self::varnode_matches_key(output, key));
                if input_matches {
                    let Some(output) = op.output.as_ref() else {
                        valid = false;
                        break;
                    };
                    if !Self::is_dead_propagating_opcode(op.opcode)
                        || !self.prove_value_is_dead(block_idx, op_idx, output, active)
                    {
                        valid = false;
                        break;
                    }
                    if output_overlaps {
                        break;
                    }
                } else if output_overlaps {
                    break;
                }
            }
            valid
                && self
                    .first_reaching_output_use_after_block_exit(block_idx, definition_idx, value)
                    .is_none()
        });
        active.remove(&state);
        result
    }

    fn is_dead_propagating_opcode(opcode: PcodeOpcode) -> bool {
        !matches!(
            opcode,
            PcodeOpcode::Store
                | PcodeOpcode::Branch
                | PcodeOpcode::CBranch
                | PcodeOpcode::BranchInd
                | PcodeOpcode::Call
                | PcodeOpcode::CallInd
                | PcodeOpcode::CallOther
                | PcodeOpcode::Return
                | PcodeOpcode::Unknown
        )
    }

    fn classify_low_lane_use(
        op: &PcodeOp,
        input_idx: usize,
        known_size: u32,
    ) -> Option<LowLaneUse<'_>> {
        let input = op.inputs.get(input_idx)?;
        let output = op.output.as_ref();
        match op.opcode {
            PcodeOpcode::Copy
            | PcodeOpcode::Cast
            | PcodeOpcode::IntZExt
            | PcodeOpcode::IntSExt
            | PcodeOpcode::IntAdd
            | PcodeOpcode::IntSub
            | PcodeOpcode::IntMult
            | PcodeOpcode::IntAnd
            | PcodeOpcode::IntOr
            | PcodeOpcode::IntXor
            | PcodeOpcode::IntLeft
            | PcodeOpcode::Int2Comp
            | PcodeOpcode::IntNegate => {
                output.map(|output| Self::low_lane_transform(op, input_idx, output, known_size))
            }
            PcodeOpcode::SubPiece
                if input_idx == 0
                    && op
                        .inputs
                        .get(1)
                        .and_then(const_offset)
                        .is_some_and(|offset| offset == 0) =>
            {
                output.map(|output| Self::low_lane_transform(op, input_idx, output, known_size))
            }
            PcodeOpcode::BoolNegate
            | PcodeOpcode::BoolXor
            | PcodeOpcode::BoolAnd
            | PcodeOpcode::BoolOr
            | PcodeOpcode::IntEqual
            | PcodeOpcode::IntNotEqual
            | PcodeOpcode::IntSLess
            | PcodeOpcode::IntSLessEqual
            | PcodeOpcode::IntLess
            | PcodeOpcode::IntLessEqual
                if input.size <= known_size =>
            {
                output.map(|output| Self::low_lane_transform(op, input_idx, output, known_size))
            }
            PcodeOpcode::Store if input_idx == 2 && input.size <= known_size => {
                Some(LowLaneUse::Terminal)
            }
            PcodeOpcode::CBranch
                if input_idx + 1 == op.inputs.len() && input.size <= known_size =>
            {
                Some(LowLaneUse::Terminal)
            }
            _ => None,
        }
    }

    fn known_low_lane_width(
        op: &PcodeOp,
        input_idx: usize,
        output: &Varnode,
        known_size: u32,
    ) -> u32 {
        let input = &op.inputs[input_idx];
        match op.opcode {
            PcodeOpcode::Copy | PcodeOpcode::Cast if input.size <= known_size => {
                known_size.min(output.size)
            }
            PcodeOpcode::IntZExt | PcodeOpcode::IntSExt if input.size <= known_size => output.size,
            PcodeOpcode::IntAnd
                if op.inputs.iter().enumerate().any(|(idx, candidate)| {
                    idx != input_idx
                        && candidate.is_constant
                        && Self::constant_has_no_bits_above(candidate, known_size)
                }) =>
            {
                output.size
            }
            _ => known_size.min(output.size),
        }
    }

    fn low_lane_transform<'value>(
        op: &PcodeOp,
        input_idx: usize,
        output: &'value Varnode,
        known_size: u32,
    ) -> LowLaneUse<'value> {
        LowLaneUse::Transform {
            value: output,
            known_size: Self::known_low_lane_width(op, input_idx, output, known_size),
        }
    }

    fn constant_has_no_bits_above(value: &Varnode, known_size: u32) -> bool {
        if !value.is_constant {
            return false;
        }
        let bits = known_size.saturating_mul(8);
        bits >= 64 || ((value.constant_val as u64) >> bits) == 0
    }

    fn is_zero_copy(op: &PcodeOp) -> bool {
        op.opcode == PcodeOpcode::Copy
            && op
                .inputs
                .first()
                .is_some_and(|input| input.is_constant && input.constant_val == 0)
    }

    /// True when `op` definitively clears `output`'s storage (upper-byte zeroing
    /// for partial-register composition).
    ///
    /// Covers:
    /// - `mov reg, 0`
    /// - `xor reg, reg` / `sub reg, reg` (x86 zeroing idioms used before setcc)
    /// - `and reg, 0`
    fn is_register_clear_def(op: &PcodeOp, output: &Varnode) -> bool {
        if Self::is_zero_copy(op) {
            return true;
        }
        if !is_register_space_id(output.space_id) {
            return false;
        }
        match op.opcode {
            PcodeOpcode::IntXor | PcodeOpcode::IntSub if op.inputs.len() >= 2 => {
                let a = &op.inputs[0];
                let b = &op.inputs[1];
                !a.is_constant
                    && !b.is_constant
                    && a.space_id == output.space_id
                    && b.space_id == output.space_id
                    && a.offset == output.offset
                    && b.offset == output.offset
                    && a.size == output.size
                    && b.size == output.size
            }
            PcodeOpcode::IntAnd if op.inputs.len() >= 2 => op
                .inputs
                .iter()
                .any(|input| input.is_constant && input.constant_val == 0),
            _ => false,
        }
    }

    /// `IntZExt wide ← low` on the same register family (same space + offset,
    /// low.size < wide.size). Low lane is copied; upper bytes become zero.
    fn is_same_register_low_zext(op: &PcodeOp, output: &Varnode) -> bool {
        if op.opcode != PcodeOpcode::IntZExt || op.inputs.len() != 1 {
            return false;
        }
        if !is_register_space_id(output.space_id) {
            return false;
        }
        let input = &op.inputs[0];
        !input.is_constant
            && input.space_id == output.space_id
            && input.offset == output.offset
            && input.size < output.size
    }

    fn varnode_ranges_overlap(
        lhs_offset: u64,
        lhs_size: u32,
        rhs_offset: u64,
        rhs_size: u32,
    ) -> bool {
        let Some(lhs_end) = lhs_offset.checked_add(u64::from(lhs_size)) else {
            return false;
        };
        let Some(rhs_end) = rhs_offset.checked_add(u64::from(rhs_size)) else {
            return false;
        };
        lhs_offset < rhs_end && rhs_offset < lhs_end
    }

    fn ranges_cover(start: u64, end: u64, ranges: &[(u64, u64)]) -> bool {
        if start >= end {
            return true;
        }
        let mut covered_until = start;
        let mut sorted = ranges.to_vec();
        sorted.sort_unstable();
        for (range_start, range_end) in sorted {
            if range_end <= covered_until {
                continue;
            }
            if range_start > covered_until {
                return false;
            }
            covered_until = range_end;
            if covered_until >= end {
                return true;
            }
        }
        false
    }

    fn last_alias_def_in_block(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        term_idx: usize,
        vn: &Varnode,
    ) -> Option<usize> {
        block
            .ops
            .iter()
            .enumerate()
            .take(term_idx)
            .rev()
            .find_map(|(op_idx, op)| {
                op.output
                    .as_ref()
                    .is_some_and(|output| self.varnode_aliases_value(output, vn))
                    .then_some(op_idx)
            })
    }
}
