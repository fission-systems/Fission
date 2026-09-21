//! Recovery of indirect-branch and jump-table terminator surfaces.
//!
//! This module owns target inference, selector recovery, jump-table decoding,
//! and the conservative bounds used to turn an indirect branch into a switch.
//! The parent terminator module remains responsible for dispatching the
//! lowering pipeline and for direct conditional/return terminators.

use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(super) fn infer_switch_default_target(&self, idx: usize, targets: &[u64]) -> Option<u64> {
        let fallthrough = self.next_block_address(idx)?;
        targets.contains(&fallthrough).then_some(fallthrough)
    }

    pub(super) fn infer_unconditional_branch_successor_target(&self, idx: usize) -> Option<u64> {
        let use_current_cfg = idx >= self.pcode.blocks.len()
            || self.successors.get(idx).is_some_and(|successors| {
                successors
                    .iter()
                    .any(|succ| *succ >= self.pcode.blocks.len())
            });
        if use_current_cfg {
            let successors = self.successors.get(idx)?;
            if successors.len() != 1 {
                return None;
            }
            return Some(self.block_target_key(successors[0]));
        }
        let block = self.pcode.blocks.get(idx)?;
        if block.successors.len() != 1 {
            return None;
        }
        let succ_idx = block.successors[0] as usize;
        (succ_idx < self.pcode.blocks.len()).then(|| self.block_target_key(succ_idx))
    }

    pub(super) fn infer_cbranch_true_target_from_successors(&self, idx: usize) -> Option<u64> {
        let use_current_cfg = idx >= self.pcode.blocks.len()
            || self.successors.get(idx).is_some_and(|successors| {
                successors
                    .iter()
                    .any(|succ| *succ >= self.pcode.blocks.len())
            });
        let block = self.pcode.blocks.get(idx);
        let fallthrough = self.next_block_address(idx);
        let mut candidates = Vec::new();
        if use_current_cfg {
            for succ_idx in self.successors.get(idx)? {
                let target = self.block_target_key(*succ_idx);
                if Some(target) == fallthrough {
                    continue;
                }
                if !candidates.contains(&target) {
                    candidates.push(target);
                }
            }
        } else {
            let block = block?;
            for succ_idx in &block.successors {
                let succ_idx = *succ_idx as usize;
                if succ_idx >= self.pcode.blocks.len() {
                    continue;
                }
                let target = self.block_target_key(succ_idx);
                if Some(target) == fallthrough {
                    continue;
                }
                if !candidates.contains(&target) {
                    candidates.push(target);
                }
            }
        }
        if candidates.len() == 1 {
            Some(candidates[0])
        } else {
            None
        }
    }

    pub(super) fn infer_branchind_target_from_input(
        &self,
        idx: usize,
        op: &PcodeOp,
        switch_var: &Varnode,
    ) -> Option<u64> {
        self.resolve_branch_target_index_with_recovery(idx, op, switch_var)
            .or_else(|| self.infer_branchind_target_from_load_address(switch_var))
            .map(|target_idx| self.block_target_key(target_idx))
    }

    pub(super) fn resolve_branch_target_index_with_recovery(
        &self,
        idx: usize,
        op: &PcodeOp,
        vn: &Varnode,
    ) -> Option<usize> {
        let target_idx = resolve_branch_target_index(
            self.pcode,
            &self.address_to_index,
            idx,
            op,
            vn,
        )
        .or_else(|| {
            let peeled = self.peel_passthrough_varnode(vn);
            if peeled != *vn {
                if let Some(target_idx) = resolve_branch_target_index(
                    self.pcode,
                    &self.address_to_index,
                    idx,
                    op,
                    &peeled,
                ) {
                    return Some(target_idx);
                }
            }

            let target_addr = self.infer_branch_target_address_one_step(vn)?;
            canonical_block_index_for_address(self.pcode, &self.address_to_index, target_addr)
        })?;
        if self.virtual_block_map.is_empty() {
            return Some(target_idx);
        }

        // Address resolution identifies the p-code body, not the current CFG
        // node. Node splitting deliberately gives a redirected edge a virtual
        // successor whose body is that same p-code block. Prefer that
        // successor when the source block's current graph contains one; this
        // keeps branch lowering aligned with the graph that structuring sees.
        let Some(successors) = self.successors.get(idx) else {
            return Some(target_idx);
        };
        if successors.contains(&target_idx) {
            return Some(target_idx);
        }
        successors
            .iter()
            .copied()
            .find(|successor| self.pcode_block_idx(*successor) == target_idx)
            .or(Some(target_idx))
    }

    pub(super) fn infer_branch_target_address_one_step(&self, vn: &Varnode) -> Option<u64> {
        if let Some(addr) = branch_target_address(vn) {
            return Some(addr);
        }

        let peeled = self.peel_passthrough_varnode(vn);
        if let Some(addr) = branch_target_address(&peeled) {
            return Some(addr);
        }

        let (_, def) = self.lookup_def_site(&peeled)?;
        match def.opcode {
            PcodeOpcode::IntAdd | PcodeOpcode::IntSub if def.inputs.len() == 2 => {
                self.eval_one_step_address_expr(def.opcode, &def.inputs[0], &def.inputs[1])
            }
            _ => None,
        }
    }

    pub(super) fn eval_one_step_address_expr(
        &self,
        opcode: PcodeOpcode,
        lhs: &Varnode,
        rhs: &Varnode,
    ) -> Option<u64> {
        let lhs_const = const_offset(lhs);
        let rhs_const = const_offset(rhs);
        let (base_vn, delta) = match (lhs_const, rhs_const) {
            (Some(delta), None) => (rhs, delta),
            (None, Some(delta)) => (lhs, delta),
            _ => return None,
        };

        let base_addr = branch_target_address(&self.peel_passthrough_varnode(base_vn))?;
        let base = i128::from(base_addr);
        let delta = i128::from(delta);
        let value = match opcode {
            PcodeOpcode::IntAdd => base + delta,
            PcodeOpcode::IntSub => base - delta,
            _ => return None,
        };
        (0..=i128::from(u64::MAX))
            .contains(&value)
            .then_some(value as u64)
    }

    pub(super) fn infer_branchind_target_from_load_address(
        &self,
        switch_var: &Varnode,
    ) -> Option<usize> {
        let peeled = self.peel_passthrough_varnode(switch_var);
        let (_, def) = self.lookup_def_site(&peeled)?;
        if def.opcode != PcodeOpcode::Load || def.inputs.len() < 2 {
            return None;
        }

        // For simple jump-table like forms, treat the computed LOAD address itself as
        // candidate target when it already lands inside the current CFG slice.
        let load_addr_vn = def.inputs.last()?;
        let load_addr = self.infer_branch_target_address_one_step(load_addr_vn)?;
        canonical_block_index_for_address(self.pcode, &self.address_to_index, load_addr)
    }

    pub(super) fn lower_branchind_switch_expr(
        &mut self,
        idx: usize,
        switch_var: &Varnode,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<PreHirExpr, MlilPreviewError> {
        let exact_expr = self.lower_wrapped_varnode(switch_var, visiting).ok();
        let alias_expr = self.lower_branchind_same_block_alias_expr(idx, switch_var, visiting);
        let predecessor_expr =
            self.recover_branchind_switch_expr_from_predecessors(idx, switch_var, visiting);

        let best_jump_table_expr = exact_expr
            .iter()
            .chain(alias_expr.iter())
            .chain(predecessor_expr.iter())
            .find(|expr| super::super::switch_table::has_jump_table_surface(expr, &self.options))
            .cloned();

        match (
            best_jump_table_expr,
            exact_expr,
            alias_expr,
            predecessor_expr,
        ) {
            (Some(expr), _, _, _) => Ok(expr),
            (None, Some(expr), _, _) => Ok(expr),
            (None, None, Some(alias), _) => Ok(alias),
            (None, None, None, Some(expr)) => Ok(expr),
            (None, None, None, None) => self.lower_wrapped_varnode(switch_var, visiting),
        }
    }

    pub(super) fn lower_branchind_same_block_alias_expr(
        &mut self,
        idx: usize,
        switch_var: &Varnode,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Option<PreHirExpr> {
        let pcode_idx = self.pcode_block_idx(idx);
        let block = &self.pcode.blocks[pcode_idx];
        let term_idx = self.block_terminator_index(block)?;
        let key = VarnodeKey::from(switch_var);
        let exact_local = self
            .block_defs
            .get(pcode_idx)
            .and_then(|defs| defs.get(&key))
            .and_then(|indices| {
                indices
                    .iter()
                    .copied()
                    .rev()
                    .find(|def_idx| *def_idx < term_idx)
            });

        for def_idx in (0..term_idx).rev() {
            let op = &block.ops[def_idx];
            let Some(output) = op.output.as_ref() else {
                continue;
            };
            if output.is_constant
                || output.space_id != switch_var.space_id
                || output.offset != switch_var.offset
                || output.size < switch_var.size
            {
                continue;
            }
            if !is_safe_selector_provenance_opcode(op.opcode) {
                if exact_local == Some(def_idx) {
                    continue;
                }
                continue;
            }
            let site = LoweringSite {
                block_idx: pcode_idx,
                op_idx: def_idx,
            };
            if let Ok(expr) = self.with_lowering_site(site, |this| {
                this.lower_selector_source_expr(output, visiting)
            }) {
                return Some(expr);
            }
        }
        None
    }

    pub(super) fn recover_branchind_render_selector_expr(
        &mut self,
        idx: usize,
        selector_alias: &Varnode,
        fallback: PreHirExpr,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> PreHirExpr {
        self.lower_branchind_same_block_alias_expr(idx, selector_alias, visiting)
            .or_else(|| self.recover_selector_expr_from_predecessors(idx, selector_alias, visiting))
            .unwrap_or(fallback)
    }

    pub(super) fn recover_branchind_switch_expr_from_predecessors(
        &mut self,
        idx: usize,
        switch_var: &Varnode,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Option<PreHirExpr> {
        let predecessors = self.predecessors.get(idx)?.clone();
        if preview_builder_diag_enabled() {
            let pred_blocks = predecessors
                .iter()
                .map(|pred_idx| format!("0x{:x}", self.block_start_address(*pred_idx)))
                .collect::<Vec<_>>()
                .join(",");
            eprintln!(
                "[DIAG] branchind_pred_scan block=0x{:x} preds=[{}] switch_var=space={} off=0x{:x} size={}",
                self.block_start_address(idx),
                pred_blocks,
                switch_var.space_id,
                switch_var.offset,
                switch_var.size
            );
        }
        for pred_idx in predecessors {
            let pcode_idx = self.pcode_block_idx(pred_idx);
            let block = self.pcode.blocks.get(pcode_idx)?;
            let term_idx = self
                .block_terminator_index(block)
                .unwrap_or(block.ops.len());
            for op_idx in (0..term_idx).rev() {
                let op = &block.ops[op_idx];
                let Some(output) = op.output.as_ref() else {
                    continue;
                };
                if output.is_constant
                    || output.space_id != switch_var.space_id
                    || output.offset != switch_var.offset
                {
                    continue;
                }
                let site = LoweringSite {
                    block_idx: pcode_idx,
                    op_idx,
                };
                if let Ok(expr) = self
                    .with_lowering_site(site, |this| this.lower_wrapped_varnode(output, visiting))
                {
                    if preview_builder_diag_enabled() {
                        eprintln!(
                            "[DIAG] branchind_pred_expr block=0x{:x} pred=0x{:x} op_seq=0x{:x} expr={}",
                            self.block_start_address(idx),
                            block.start_address,
                            op.seq_num,
                            print_prehir_expr(&expr)
                        );
                    }
                    return Some(expr);
                }
            }
            if term_idx < block.ops.len() {
                let term_op = &block.ops[term_idx];
                if term_op.opcode == PcodeOpcode::BranchInd
                    && let Some(term_input) = term_op.inputs.first()
                {
                    let site = LoweringSite {
                        block_idx: pcode_idx,
                        op_idx: term_idx,
                    };
                    if let Ok(expr) = self.with_lowering_site(site, |this| {
                        this.lower_wrapped_varnode(term_input, visiting)
                    }) && super::super::switch_table::has_jump_table_surface(
                        &expr,
                        &self.options,
                    ) {
                        if preview_builder_diag_enabled() {
                            eprintln!(
                                "[DIAG] branchind_pred_term_expr block=0x{:x} pred=0x{:x} term_seq=0x{:x} expr={}",
                                self.block_start_address(idx),
                                block.start_address,
                                term_op.seq_num,
                                print_prehir_expr(&expr)
                            );
                        }
                        return Some(expr);
                    }
                }
            }
        }

        None
    }

    pub(super) fn normalize_rendered_selector_expr(
        &self,
        expr: PreHirExpr,
        min_val: i64,
    ) -> (PreHirExpr, i64) {
        let Some((base_expr, offset)) =
            super::super::switch_table::split_selector_base_offset(&expr)
        else {
            return (expr, min_val);
        };
        let Some(next_min) = min_val.checked_add(offset) else {
            return (expr, min_val);
        };
        (base_expr, next_min)
    }

    pub(super) fn selector_normalization_for_branchind(
        &self,
        expr: &PreHirExpr,
        min_val: i64,
        entry_size: u64,
        recovered_cases: Option<&[(i64, u64)]>,
    ) -> SelectorNormalization {
        let guard_bounds = recovered_cases
            .filter(|cases| !cases.is_empty())
            .map(|cases| {
                let min_case = cases.iter().map(|(value, _)| *value).min();
                let max_case = cases.iter().map(|(value, _)| *value).max();
                vec![(min_case, max_case)]
            })
            .unwrap_or_default();
        SelectorNormalization {
            base_subtract: (min_val != 0).then_some(min_val),
            mask: None,
            stride: (entry_size > 1).then_some(entry_size),
            width: Self::selector_expr_width(expr),
            address_space: None,
            guard_bounds,
        }
    }

    pub(super) fn selector_expr_width(expr: &PreHirExpr) -> Option<u32> {
        match expr {
            PreHirExpr::Const(_, ty)
            | PreHirExpr::Load { ty, .. }
            | PreHirExpr::Cast { ty, .. }
            | PreHirExpr::Unary { ty, .. }
            | PreHirExpr::Binary { ty, .. }
            | PreHirExpr::FieldAccess { ty, .. } => Self::nir_type_width(ty),
            PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) | PreHirExpr::AddressOfLocal(_) => {
                None
            }
            PreHirExpr::Call { ty, .. } => Self::nir_type_width(ty),
            PreHirExpr::PtrOffset { .. } => None,
            PreHirExpr::AggregateCopy { size, .. } => Some(*size * 8),
            PreHirExpr::Index { elem_ty, .. } => Self::nir_type_width(elem_ty),
            PreHirExpr::Select { ty, .. } => Self::nir_type_width(ty),
        }
    }

    pub(super) fn nir_type_width(ty: &NirType) -> Option<u32> {
        match ty {
            NirType::Bool => Some(1),
            NirType::Int { bits, .. } => Some(*bits),
            NirType::Ptr(_) => None,
            NirType::Aggregate { size, .. } => Some(*size * 8),
            NirType::Float { bits } => Some(*bits),
            NirType::Unknown => None,
        }
    }

    pub(super) fn selector_expr_is_side_effect_free(expr: &PreHirExpr) -> bool {
        match expr {
            PreHirExpr::Const(_, _)
            | PreHirExpr::Var(_)
            | PreHirExpr::AddressOfGlobal(_)
            | PreHirExpr::AddressOfLocal(_) => true,
            PreHirExpr::Cast { expr, .. }
            | PreHirExpr::Unary { expr, .. }
            | PreHirExpr::Load { ptr: expr, .. }
            | PreHirExpr::PtrOffset { base: expr, .. }
            | PreHirExpr::AggregateCopy { src: expr, .. }
            | PreHirExpr::FieldAccess { base: expr, .. } => {
                Self::selector_expr_is_side_effect_free(expr)
            }
            PreHirExpr::Binary { lhs, rhs, .. } => {
                Self::selector_expr_is_side_effect_free(lhs)
                    && Self::selector_expr_is_side_effect_free(rhs)
            }
            PreHirExpr::Index { base, index, .. } => {
                Self::selector_expr_is_side_effect_free(base)
                    && Self::selector_expr_is_side_effect_free(index)
            }
            PreHirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                Self::selector_expr_is_side_effect_free(cond)
                    && Self::selector_expr_is_side_effect_free(then_expr)
                    && Self::selector_expr_is_side_effect_free(else_expr)
            }
            PreHirExpr::Call { .. } => false,
        }
    }

    pub(super) fn infer_branchind_targets_from_jump_table_expr(
        &mut self,
        idx: usize,
        switch_expr: &PreHirExpr,
        selector_alias: Option<&Varnode>,
    ) -> Option<InferredJumpTableTargets> {
        const MAX_JUMP_TABLE_CASES: u64 = 256;

        let binary = self.binary?;
        let selector =
            super::super::switch_table::recover_switch_discriminant(switch_expr, &self.options)?;
        if preview_builder_diag_enabled() {
            eprintln!(
                "[DIAG] branchind_switch_selector block=0x{:x} expr={} discrim={} min={} table=0x{:x} target_base={:?} relative={} entry_size={}",
                self.block_start_address(idx),
                print_prehir_expr(switch_expr),
                print_prehir_expr(&selector.discriminant),
                selector.min_val,
                selector.table_base,
                selector.target_base.map(|addr| format!("0x{addr:x}")),
                selector.relative_entries,
                selector.entry_size
            );
        }
        let normalized_selector = selector_alias
            .and_then(|alias| {
                let mut selector_visiting = HashSet::default();
                self.recover_selector_expr_from_predecessors(idx, alias, &mut selector_visiting)
            })
            .unwrap_or_else(|| selector.discriminant.clone());
        let direct_bound = self.infer_branchind_selector_upper_bound(
            idx,
            &normalized_selector,
            selector_alias,
            selector.min_val,
        );
        let alias_family_bound = selector_alias.and_then(|alias| {
            self.infer_branchind_selector_upper_bound_from_alias_family(
                idx,
                alias,
                selector.min_val,
            )
        });
        let max_selector = match (direct_bound, alias_family_bound) {
            (Some(direct), Some(alias_family)) => direct.max(alias_family),
            (Some(bound), None) | (None, Some(bound)) => bound,
            (None, None) => return None,
        };
        if preview_builder_diag_enabled() {
            eprintln!(
                "[DIAG] branchind_switch_bound block=0x{:x} normalized_selector={} max_selector={} min={}",
                self.block_start_address(idx),
                print_prehir_expr(&normalized_selector),
                max_selector,
                selector.min_val
            );
        }
        let proven_case_count = max_selector.saturating_add(1).min(MAX_JUMP_TABLE_CASES);
        if proven_case_count < 2 || selector.entry_size == 0 {
            return None;
        }

        let pointer_size = u64::from(self.options.pointer_size.max(1));
        let entry_width = selector.entry_size.min(pointer_size).max(4) as usize;
        let little_endian = !binary.arch_spec.contains(":BE:");
        let decode_modes = branchind_decode_modes(
            selector.relative_entries,
            selector.table_base,
            selector.target_base,
            self.options.image_base,
            &self.options.sections,
        );

        let mut best: Option<InferredJumpTableTargets> = None;
        for (decode_mode, relative_entries, relative_base) in decode_modes {
            let mut recovered_cases = Vec::new();
            let mut unique_targets = Vec::new();
            for ordinal in 0..MAX_JUMP_TABLE_CASES {
                let Some(entry_addr) = selector
                    .table_base
                    .checked_add(ordinal.saturating_mul(selector.entry_size))
                else {
                    break;
                };
                if ordinal >= proven_case_count && self.address_to_index.contains_key(&entry_addr) {
                    break;
                }
                let Some(raw) = binary.get_bytes(entry_addr, entry_width) else {
                    break;
                };
                let Some(target_addr) =
                    decode_jump_table_target(&raw, little_endian, relative_entries, relative_base)
                else {
                    if ordinal >= proven_case_count {
                        break;
                    }
                    continue;
                };
                let Some(target_idx) = canonical_block_index_for_address(
                    self.pcode,
                    &self.address_to_index,
                    target_addr,
                ) else {
                    if ordinal >= proven_case_count {
                        break;
                    }
                    continue;
                };
                let target = self.block_target_key(target_idx);
                recovered_cases.push((selector.min_val + ordinal as i64, target));
                if !unique_targets.contains(&target) {
                    unique_targets.push(target);
                }
            }
            if unique_targets.len() < 2 || recovered_cases.len() < 2 {
                continue;
            }
            let selector_cardinality = (proven_case_count as usize).max(recovered_cases.len());
            let candidate = InferredJumpTableTargets {
                unique_targets,
                recovered_cases,
                selector_cardinality,
                decode_mode,
            };
            let replace = best.as_ref().is_none_or(|current| {
                candidate.recovered_cases.len() > current.recovered_cases.len()
                    || (candidate.recovered_cases.len() == current.recovered_cases.len()
                        && candidate.unique_targets.len() > current.unique_targets.len())
            });
            if replace {
                best = Some(candidate);
            }
        }

        if preview_builder_diag_enabled() {
            if let Some(best) = best.as_ref() {
                eprintln!(
                    "[DIAG] branchind_switch_targets block=0x{:x} mode={} targets={:?} cases={:?}",
                    self.block_start_address(idx),
                    best.decode_mode,
                    best.unique_targets,
                    best.recovered_cases
                );
            } else {
                eprintln!(
                    "[DIAG] branchind_switch_targets block=0x{:x} mode=none targets=[]",
                    self.block_start_address(idx)
                );
            }
        }

        best
    }

    pub(super) fn emulate_branchind_targets_with_emulator(
        &mut self,
        idx: usize,
        op: &PcodeOp,
        switch_var: &Varnode,
    ) -> Option<InferredJumpTableTargets> {
        let (ordered_ops, leaves) =
            collect_switch_dependencies(switch_var, &self.defs, self.pcode)?;

        let little_endian = if let Some(bin) = self.binary {
            !bin.arch_spec.contains(":BE:")
        } else {
            !self.options.is_big_endian
        };

        let mut selector_leaf: Option<VarnodeKey> = None;
        let mut other_leaf_values = HashMap::default();

        for leaf in &leaves {
            let is_reg = is_register_space_id(leaf.space_id);
            let name = if is_reg {
                self.sla_hw_name(leaf.offset, leaf.size)
            } else {
                None
            };

            let is_pc = name.as_deref() == Some("pc")
                || (self.options.calling_convention == CallingConvention::SystemVAmd64
                    && leaf.offset == 0x80)
                || (self.options.calling_convention == CallingConvention::WindowsX64
                    && leaf.offset == 0x80);

            if is_pc {
                other_leaf_values.insert(leaf.clone(), op.address);
            } else if name.as_deref() == Some("sp")
                || name.as_deref() == Some("rsp")
                || name.as_deref() == Some("rbp")
            {
                other_leaf_values.insert(leaf.clone(), 0);
            } else if selector_leaf.is_none() {
                selector_leaf = Some(leaf.clone());
            } else {
                other_leaf_values.insert(leaf.clone(), 0);
            }
        }

        let mut unique_targets = Vec::new();
        let mut recovered_cases = Vec::new();
        let mut consecutive_failures = 0;

        for s in 0..64 {
            let mut leaf_values = other_leaf_values.clone();
            if let Some(ref sel) = selector_leaf {
                leaf_values.insert(sel.clone(), s);
            }
            if let Some(target_addr) =
                emulate_path(&ordered_ops, &leaf_values, self.binary, !little_endian)
            {
                if let Some(&target_idx) = self.address_to_index.get(&target_addr) {
                    let target = self.block_target_key(target_idx);
                    recovered_cases.push((s as i64, target));
                    if !unique_targets.contains(&target) {
                        unique_targets.push(target);
                    }
                    consecutive_failures = 0;
                    continue;
                }
            }
            consecutive_failures += 1;
            if consecutive_failures >= 3 {
                break;
            }
        }

        if unique_targets.len() >= 2 && recovered_cases.len() >= 2 {
            if preview_builder_diag_enabled() {
                eprintln!(
                    "[DIAG] branchind_emulator_targets block=0x{:x} targets={:?} cases={:?}",
                    self.block_start_address(idx),
                    unique_targets,
                    recovered_cases
                );
            }
            Some(InferredJumpTableTargets {
                unique_targets,
                recovered_cases,
                selector_cardinality: 64,
                decode_mode: "emulator",
            })
        } else {
            None
        }
    }

    pub(super) fn recover_branchind_jump_table_selector_varnode(
        &self,
        idx: usize,
    ) -> Option<Varnode> {
        let pcode_idx = self.pcode_block_idx(idx);
        let block = self.pcode.blocks.get(pcode_idx)?;
        let term_idx = self.block_terminator_index(block)?;
        for op_idx in (0..term_idx).rev() {
            let op = &block.ops[op_idx];
            if op.opcode != PcodeOpcode::Load || op.inputs.len() < 2 {
                continue;
            }
            if let Some(selector) = self.extract_jump_table_selector_varnode(&op.inputs[1]) {
                return Some(selector);
            }
        }
        None
    }

    pub(super) fn extract_jump_table_selector_varnode(&self, ptr: &Varnode) -> Option<Varnode> {
        let (_, op) = self.lookup_def_site(ptr)?;
        if op.opcode == PcodeOpcode::IntAdd && op.inputs.len() == 2 {
            return self
                .extract_scaled_selector_varnode(&op.inputs[0])
                .or_else(|| self.extract_scaled_selector_varnode(&op.inputs[1]));
        }
        self.extract_scaled_selector_varnode(ptr)
    }

    pub(super) fn extract_scaled_selector_varnode(&self, vn: &Varnode) -> Option<Varnode> {
        let peeled = self.peel_passthrough_varnode(vn);
        if peeled.is_constant || self.materializes_const_address(&peeled) {
            return None;
        }
        let (_, op) = self.lookup_def_site(&peeled)?;
        match op.opcode {
            PcodeOpcode::IntLeft | PcodeOpcode::IntMult if op.inputs.len() == 2 => {
                if op.inputs[0].is_constant {
                    Some(op.inputs[1].clone())
                } else if op.inputs[1].is_constant {
                    Some(op.inputs[0].clone())
                } else {
                    None
                }
            }
            _ => Some(peeled),
        }
    }

    pub(super) fn materializes_const_address(&self, vn: &Varnode) -> bool {
        self.materializes_const_address_within(vn, PASSTHROUGH_PEEL_MAX_STEPS)
    }

    /// Walk a chain of pass-through definitions looking for a constant, with a
    /// budget.
    ///
    /// Bounded because the definition graph this walks can contain a cycle:
    /// `lookup_def_site` answers block-locally, and SLEIGH reuses one unique
    /// offset many times inside a block, so a temporary's "definition" can be
    /// an op that reads the same temporary. `peel_passthrough_varnode` has
    /// always guarded against exactly this; this walk did not, and once jump
    /// tables started resolving it was the first caller to reach such a chain
    /// -- `bash`'s `unwind_frame_discard_internal` stopped terminating, with
    /// 91% of its time inside `lookup_def_site`.
    pub(super) fn materializes_const_address_within(&self, vn: &Varnode, budget: usize) -> bool {
        let Some(budget) = budget.checked_sub(1) else {
            return false;
        };
        let Some((_, op)) = self.lookup_def_site(vn) else {
            return false;
        };
        match op.opcode {
            PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt
                if op.inputs.len() == 1 =>
            {
                op.inputs[0].is_constant
                    || self.materializes_const_address_within(&op.inputs[0], budget)
            }
            PcodeOpcode::IntAdd | PcodeOpcode::IntSub if op.inputs.len() == 2 => {
                op.inputs[0].is_constant && op.inputs[1].is_constant
            }
            _ => false,
        }
    }

    pub(super) fn recover_selector_expr_from_predecessors(
        &mut self,
        idx: usize,
        selector_alias: &Varnode,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Option<PreHirExpr> {
        let cache_key = (idx, selector_alias.space_id, selector_alias.offset);
        if let Some(cached) = self.selector_representatives.get(&cache_key) {
            return Some(cached.clone());
        }

        let predecessors = self.predecessors.get(idx)?.clone();
        let selector_family = (selector_alias.space_id, selector_alias.offset);
        for pred_idx in predecessors {
            let pcode_idx = self.pcode_block_idx(pred_idx);
            let block = self.pcode.blocks.get(pcode_idx)?;
            let term_idx = self
                .block_terminator_index(block)
                .unwrap_or(block.ops.len());
            for op_idx in (0..term_idx).rev() {
                let op = &block.ops[op_idx];
                let Some(output) = op.output.as_ref() else {
                    continue;
                };
                if (output.space_id, output.offset) != selector_family {
                    continue;
                }
                let site = LoweringSite {
                    block_idx: pcode_idx,
                    op_idx,
                };
                if let Ok(expr) = self.with_lowering_site(site, |this| {
                    this.lower_selector_source_expr(output, visiting)
                }) {
                    self.selector_representatives
                        .insert(cache_key, expr.clone());
                    return Some(expr);
                }
            }
        }

        None
    }

    pub(super) fn lower_selector_source_expr(
        &mut self,
        vn: &Varnode,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<PreHirExpr, MlilPreviewError> {
        let peeled = self.peel_passthrough_varnode(vn);
        // Unlike `lower_wrapped_varnode`'s own Copy/Cast peel fallback, this
        // recursion never routes through `lower_varnode`'s `visiting`-set
        // cycle guard -- it walks `op.inputs[0]` directly. A loop-carried
        // selector (e.g. a switch discriminant fed by the loop induction
        // variable) can have its Copy chain fold back on a def already on
        // this call stack, which would otherwise recurse forever. Guard it
        // with the same key locally.
        let key = VarnodeKey::from(&peeled);
        if !visiting.insert(key.clone()) {
            return self.lower_wrapped_varnode(&peeled, visiting);
        }
        let result = if let Some((_, op)) = self.lookup_def_site(&peeled) {
            match op.opcode {
                PcodeOpcode::Copy
                | PcodeOpcode::Cast
                | PcodeOpcode::IntZExt
                | PcodeOpcode::IntSExt
                | PcodeOpcode::SubPiece
                    if !op.inputs.is_empty() =>
                {
                    let input = op.inputs[0].clone();
                    self.lower_selector_source_expr(&input, visiting)
                }
                _ => self.lower_wrapped_varnode(&peeled, visiting),
            }
        } else {
            self.lower_wrapped_varnode(&peeled, visiting)
        };
        visiting.remove(&key);
        result
    }

    pub(super) fn extract_modulo_bound(expr: &PreHirExpr) -> Option<u64> {
        let mut current = expr;
        loop {
            match current {
                PreHirExpr::Cast { expr, .. } => {
                    current = expr;
                }
                PreHirExpr::Binary {
                    op: PreHirBinaryOp::Mod,
                    rhs,
                    ..
                } => {
                    let stripped_rhs = strip_casts(rhs);
                    if let PreHirExpr::Const(divisor, _) = stripped_rhs {
                        if divisor > 0 {
                            return Some((divisor - 1) as u64);
                        }
                    }
                    return None;
                }
                _ => return None,
            }
        }
    }

    pub(super) fn infer_branchind_selector_upper_bound(
        &mut self,
        idx: usize,
        selector: &PreHirExpr,
        selector_alias: Option<&Varnode>,
        min_val: i64,
    ) -> Option<u64> {
        let normalized = strip_casts(selector);
        let mut best: Option<u64> = None;
        let predecessors = self.predecessors.get(idx)?.clone();

        let mut selector_names = HashSet::default();
        if let PreHirExpr::Var(name) = &normalized {
            selector_names.insert(name.clone());
        }
        if let PreHirExpr::Var(name) = strip_casts(selector) {
            selector_names.insert(name.clone());
        }

        let mut queue = Vec::new();
        if let Some(alias) = selector_alias {
            queue.push(alias.clone());
        }

        let mut visited_vns = HashSet::default();
        while let Some(current_vn) = queue.pop() {
            let key = VarnodeKey::from(&current_vn);
            if !visited_vns.insert(key) {
                continue;
            }

            for (key, name) in &self.materialized_vns {
                if key.varnode.space_id == current_vn.space_id
                    && key.varnode.offset == current_vn.offset
                {
                    selector_names.insert(name.clone());
                }
            }

            if is_register_space_id(current_vn.space_id) {
                if let Some(&param_idx) = self.register_param_aliases.get(&current_vn.offset) {
                    if param_idx < self.entry_arity {
                        let param_name = self.abi_state().param_name(param_idx);
                        selector_names.insert(param_name);
                    }
                }
                for size in [1, 2, 4, 8] {
                    let name = self
                        .sla_hw_name(current_vn.offset, size)
                        .unwrap_or_else(|| "reg".to_string());
                    selector_names.insert(name.to_string());
                }
            }

            if let Some((_, op)) = self.lookup_def_site(&current_vn) {
                if matches!(
                    op.opcode,
                    PcodeOpcode::Copy
                        | PcodeOpcode::Cast
                        | PcodeOpcode::IntZExt
                        | PcodeOpcode::IntSExt
                        | PcodeOpcode::SubPiece
                ) && !op.inputs.is_empty()
                {
                    queue.push(op.inputs[0].clone());
                }
            }
        }

        let is_match = |expr: &PreHirExpr| {
            let stripped = strip_casts(expr);
            if let PreHirExpr::Var(name) = &stripped {
                if selector_names.contains(name) {
                    return true;
                }
            }
            stripped == normalized
        };

        for pred_idx in predecessors {
            let terminator = self.lower_block_terminator(pred_idx);
            let LoweredTerminator::Cond {
                cond,
                true_target,
                false_target,
            } = terminator.ok()?
            else {
                continue;
            };
            let current_target = self.block_target_key(idx);
            let Some(bound) = (if true_target == current_target {
                extract_selector_upper_bound_from_cond(&cond, &is_match, true)
            } else if false_target == Some(current_target) {
                extract_selector_upper_bound_from_cond(&cond, &is_match, false)
            } else {
                None
            }) else {
                continue;
            };
            let normalized_bound = if min_val <= 0 {
                bound.checked_add((-min_val) as u64)?
            } else {
                bound.checked_sub(min_val as u64)?
            };
            best = Some(best.map_or(normalized_bound, |existing| existing.min(normalized_bound)));
        }

        if best.is_none() {
            if let Some(bound) = Self::extract_modulo_bound(selector) {
                let normalized_bound = if min_val <= 0 {
                    bound.checked_add((-min_val) as u64)
                } else {
                    bound.checked_sub(min_val as u64)
                };
                best = normalized_bound;
            }
        }

        best
    }

    pub(super) fn infer_branchind_selector_upper_bound_from_alias_family(
        &mut self,
        idx: usize,
        selector_alias: &Varnode,
        min_val: i64,
    ) -> Option<u64> {
        let mut best: Option<u64> = None;
        let predecessors = self.predecessors.get(idx)?.clone();
        let selector_family = (selector_alias.space_id, selector_alias.offset);

        for pred_idx in predecessors {
            let current_target = self.block_target_key(idx);
            let LoweredTerminator::Cond {
                true_target,
                false_target,
                ..
            } = self.lower_block_terminator(pred_idx).ok()?
            else {
                continue;
            };
            let current_on_true = if true_target == current_target {
                true
            } else if false_target == Some(current_target) {
                false
            } else {
                continue;
            };
            let bound = self.extract_selector_upper_bound_from_predicate_family(
                pred_idx,
                selector_family,
                current_on_true,
            )?;
            let normalized_bound = if min_val <= 0 {
                bound.checked_add((-min_val) as u64)?
            } else {
                bound.checked_sub(min_val as u64)?
            };
            best = Some(best.map_or(normalized_bound, |existing| existing.min(normalized_bound)));
        }

        best
    }

    pub(super) fn extract_selector_upper_bound_from_predicate_family(
        &self,
        pred_idx: usize,
        selector_family: (u64, u64),
        current_on_true: bool,
    ) -> Option<u64> {
        let pcode_idx = self.pcode_block_idx(pred_idx);
        let block = self.pcode.blocks.get(pcode_idx)?;
        let term_idx = self
            .block_terminator_index(block)
            .unwrap_or(block.ops.len());
        let mut less_than_bound: Option<u64> = None;
        let mut equality_bound: Option<u64> = None;

        for op in block.ops.iter().take(term_idx) {
            match op.opcode {
                PcodeOpcode::IntLess | PcodeOpcode::IntSLess if op.inputs.len() == 2 => {
                    let in0 = self.peel_passthrough_varnode(&op.inputs[0]);
                    let in1 = self.peel_passthrough_varnode(&op.inputs[1]);
                    if same_family_varnode(&in0, selector_family) && in1.is_constant {
                        less_than_bound = u64::try_from(in1.constant_val).ok();
                    } else if in0.is_constant && same_family_varnode(&in1, selector_family) {
                        less_than_bound = u64::try_from(in0.constant_val).ok();
                    }
                }
                PcodeOpcode::IntLessEqual | PcodeOpcode::IntSLessEqual if op.inputs.len() == 2 => {
                    let in0 = self.peel_passthrough_varnode(&op.inputs[0]);
                    let in1 = self.peel_passthrough_varnode(&op.inputs[1]);
                    if same_family_varnode(&in0, selector_family) && in1.is_constant {
                        less_than_bound = u64::try_from(in1.constant_val)
                            .ok()
                            .and_then(|value| value.checked_add(1));
                    } else if in0.is_constant && same_family_varnode(&in1, selector_family) {
                        less_than_bound = u64::try_from(in0.constant_val).ok();
                    }
                }
                PcodeOpcode::IntEqual | PcodeOpcode::IntNotEqual if op.inputs.len() == 2 => {
                    let in0 = self.peel_passthrough_varnode(&op.inputs[0]);
                    let in1 = self.peel_passthrough_varnode(&op.inputs[1]);
                    if in1.is_zero()
                        && let Some(operands) = self.match_cmp_diff_from_peeled(&in0)
                        && same_family_varnode(&operands.lhs, selector_family)
                        && operands.rhs.is_constant
                    {
                        equality_bound = u64::try_from(operands.rhs.constant_val).ok();
                    } else if in0.is_zero()
                        && let Some(operands) = self.match_cmp_diff_from_peeled(&in1)
                        && same_family_varnode(&operands.lhs, selector_family)
                        && operands.rhs.is_constant
                    {
                        equality_bound = u64::try_from(operands.rhs.constant_val).ok();
                    }
                }
                _ => {}
            }
        }

        match (current_on_true, less_than_bound, equality_bound) {
            (false, Some(less), Some(eq)) if less == eq || less.checked_add(1) == Some(eq) => {
                Some(eq)
            }
            (true, Some(less), _) => less.checked_sub(1),
            _ => None,
        }
    }
}
