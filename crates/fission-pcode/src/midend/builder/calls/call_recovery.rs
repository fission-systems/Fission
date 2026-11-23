use super::*;
use fission_midend_core::ir::sanitize_c_identifier;

fn resolve_add_op_stack_address(
    builder: &PreviewBuilder<'_>,
    add: &PcodeOp,
) -> Option<(StackBase, i64)> {
    if add.inputs.len() < 2 {
        return None;
    }
    if let Some((base, offset)) = builder.resolve_stack_address(&add.inputs[0]) {
        return crate::midend::cfg::const_offset(&add.inputs[1])
            .map(|delta| (base, offset + delta));
    }
    if let Some((base, offset)) = builder.resolve_stack_address(&add.inputs[1]) {
        return crate::midend::cfg::const_offset(&add.inputs[0])
            .map(|delta| (base, offset + delta));
    }
    None
}

impl<'a> PreviewBuilder<'a> {
    pub(in crate::midend::builder) fn abi_state(&self) -> AbiState {
        AbiState::new_with_cspec(
            self.options.calling_convention,
            self.options.is_64bit,
            self.options.pointer_size,
            self.stack_frame_size,
            self.options.cspec_param_offsets.clone(),
            self.options.cspec_stack_arg_base,
            self.options.cspec_extrapop,
        )
        .with_frame_pointer_established(self.entry_frame_pointer_established)
    }

    fn surface_call_carrier_name(&mut self, vn: &Varnode) -> Option<String> {
        if let Some(param) = self.register_param(vn) {
            return Some(param);
        }
        if vn.space_id == UNIQUE_SPACE_ID {
            return crate::arch::x86::unique_x86_register_name(vn.offset, vn.size)
                .map(str::to_string);
        }
        if is_register_varnode(vn) {
            return Some(
                self.sla_hw_name(vn.offset, vn.size)
                    .unwrap_or_else(|| "reg".to_string())
                    .to_string(),
            );
        }
        None
    }

    fn param_index_for_varnode(
        &self,
        vn: &Varnode,
        include_rust_sleigh_space: bool,
    ) -> Option<usize> {
        if include_rust_sleigh_space {
            if !is_register_varnode(vn) {
                return None;
            }
        } else if vn.space_id != REGISTER_SPACE_ID {
            return None;
        }
        self.abi_state().param_slot_for_varnode(vn)
    }

    fn debug_call_recovery(&self, message: &str) {
        if preview_debug_enabled() {
            eprintln!("[mlil-preview] stage=call_arg_recovery {message}");
        }
    }

    fn normalize_recovered_call_arg(&self, expr: PreHirExpr) -> PreHirExpr {
        let (value, fallback) = match expr {
            PreHirExpr::Const(value, ty) => (value, PreHirExpr::Const(value, ty)),
            PreHirExpr::Cast { ty, expr } => {
                let PreHirExpr::Const(value, _) = *expr else {
                    return PreHirExpr::Cast { ty, expr };
                };
                (value, PreHirExpr::Const(value, ty))
            }
            expr => return expr,
        };
        if value <= 0 {
            return fallback;
        }
        let Some(ctx) = self.type_context else {
            return fallback;
        };
        ctx.call_target_refs
            .get(&(value as u64))
            .map(|target_ref| PreHirExpr::Var(sanitize_c_identifier(&target_ref.symbol)))
            .unwrap_or(fallback)
    }

    fn is_callee_saved_register_varnode(&self, vn: &Varnode) -> bool {
        if is_register_varnode(vn) && !self.options.cspec_unaffected_offsets.is_empty() {
            if self.options.cspec_unaffected_offsets.contains(&vn.offset) {
                return true;
            }
            let namer = self.register_namer();
            let Some(vn_family) = namer.gpr_family_index_at(vn.offset, vn.size) else {
                return false;
            };
            return self.options.cspec_unaffected_offsets.iter().any(|&offset| {
                namer
                    .gpr_family_index_at(offset, self.options.pointer_size)
                    .is_some_and(|fam| fam == vn_family)
            });
        }
        let reg_name = if is_register_varnode(vn) {
            self.sla_hw_name(vn.offset, vn.size).unwrap_or_default()
        } else if vn.space_id == UNIQUE_SPACE_ID {
            crate::arch::x86::unique_x86_register_name(vn.offset, vn.size)
                .map(str::to_string)
                .unwrap_or_default()
        } else {
            return false;
        };
        // No cspec `<unaffected>` list was loaded: fall back to the
        // convention's own table (ISA data lives with the convention, not in
        // this builder).
        self.options
            .calling_convention
            .callee_saved_register_names()
            .contains(&reg_name.to_ascii_lowercase().as_str())
    }

    fn call_preserves_argument_register(&self, vn: &Varnode, call: &PcodeOp) -> bool {
        let Some(param_slot) = self.param_index_for_varnode(vn, true) else {
            return false;
        };
        let written_slots = match super::super::resolve_lifted_direct_call_target(
            call,
            self.options,
            self.type_context,
        ) {
            Some(symbol) => self
                .type_context
                .and_then(|context| context.call_effect_summaries.get(&symbol))
                .and_then(|summary| summary.modified_argument_register_slots.as_ref()),
            // A local callee with no symbol: its proven writes by address.
            None => (call.opcode == PcodeOpcode::Call)
                .then(|| call.inputs.first())
                .flatten()
                .and_then(|target| {
                    self.type_context.and_then(|context| {
                        context.callee_argument_register_writes.get(&target.offset)
                    })
                }),
        };
        let Some(written_slots) = written_slots else {
            return false;
        };
        !written_slots.contains(&param_slot)
    }

    pub(in crate::midend) fn call_clobbers_register_varnode(
        &self,
        vn: &Varnode,
        op: &PcodeOp,
    ) -> bool {
        op.opcode.is_call()
            && !self.is_callee_saved_register_varnode(vn)
            && !self.call_preserves_argument_register(vn, op)
    }

    fn check_ancestor_realistic(
        &self,
        vn: &Varnode,
        def_site: &crate::midend::builder::DefSite,
        call_block_idx: usize,
        call_op_idx: usize,
    ) -> bool {
        // Every argument register is caller-saved, so a call anywhere on the
        // path from the definition to this call site destroys the value. The
        // intermediate-block sweep below is Ghidra's; it misses the two ends
        // of the path, and a call sits at one of them more often than in the
        // middle -- a block that *ends* in a call, with the next block
        // starting at the call site, is the ordinary shape of two calls in a
        // row. Missing it let a previous call's staged arguments be recovered
        // as this call's: `setutent()`, which takes none, came out as
        // `setutent(14, __act, 0, xVar25)` -- the preceding `sigaction`'s.
        let clobbered_in = |block_idx: usize, range: std::ops::Range<usize>| {
            self.pcode.blocks[block_idx].ops[range]
                .iter()
                .any(|op| self.call_clobbers_register_varnode(vn, op))
        };
        if def_site.block_idx == call_block_idx {
            return def_site.op_idx < call_op_idx
                && !clobbered_in(
                    call_block_idx,
                    def_site.op_idx + 1
                        ..call_op_idx.min(self.pcode.blocks[call_block_idx].ops.len()),
                );
        }

        if !self.dom_tree.dominates(def_site.block_idx, call_block_idx) {
            return false;
        }

        let def_block_len = self.pcode.blocks[def_site.block_idx].ops.len();
        if clobbered_in(
            def_site.block_idx,
            (def_site.op_idx + 1).min(def_block_len)..def_block_len,
        ) || clobbered_in(
            call_block_idx,
            0..call_op_idx.min(self.pcode.blocks[call_block_idx].ops.len()),
        ) {
            return false;
        }

        // According to Ghidra 11.4.2 AncestorRealistic:
        for intermediate_idx in def_site.block_idx + 1..call_block_idx {
            if !self
                .dom_tree
                .dominates(def_site.block_idx, intermediate_idx)
            {
                continue;
            }
            if !self.dom_tree.dominates(intermediate_idx, call_block_idx) {
                continue;
            }
            let inter_block = &self.pcode.blocks[intermediate_idx];
            for op in &inter_block.ops {
                if self.call_clobbers_register_varnode(vn, op) {
                    return false;
                }
            }
        }

        true
    }

    fn collect_unclobbered_reaching_defs(
        &self,
        vn: &Varnode,
        call_block_idx: usize,
        call_idx: usize,
    ) -> Option<Vec<LoweringSite>> {
        const MAX_PREDECESSOR_BLOCKS: usize = 64;
        let call_block = self.pcode.blocks.get(call_block_idx)?;
        for (op_idx, op) in call_block
            .ops
            .iter()
            .enumerate()
            .take(call_idx.min(call_block.ops.len()))
            .rev()
        {
            if Self::is_identity_copy_def(op) {
                continue;
            }
            if op
                .output
                .as_ref()
                .is_some_and(|out| Self::varnode_covers_or_aliases_register(out, vn))
            {
                return Some(vec![LoweringSite {
                    block_idx: call_block_idx,
                    op_idx,
                }]);
            }
            if self.call_clobbers_register_varnode(vn, op) {
                return None;
            }
        }

        let initial_preds = self.predecessors.get(call_block_idx)?;
        if initial_preds.is_empty() {
            return None;
        }
        let mut queue: std::collections::VecDeque<usize> = initial_preds.iter().copied().collect();
        let mut visited = HashSet::default();
        let mut reached_defs: Vec<LoweringSite> = Vec::new();

        while let Some(pred_idx) = queue.pop_front() {
            if !visited.insert(pred_idx) {
                continue;
            }
            if visited.len() > MAX_PREDECESSOR_BLOCKS {
                return None;
            }
            let pred_pcode_idx = self.pcode_block_idx(pred_idx);
            let pred_block = self.pcode.blocks.get(pred_pcode_idx)?;
            let mut defined = false;
            for (op_idx, op) in pred_block.ops.iter().enumerate().rev() {
                if Self::is_identity_copy_def(op) {
                    continue;
                }
                if self.call_clobbers_register_varnode(vn, op) {
                    return None;
                }
                if op
                    .output
                    .as_ref()
                    .is_some_and(|out| Self::varnode_covers_or_aliases_register(out, vn))
                {
                    reached_defs.push(LoweringSite {
                        block_idx: pred_pcode_idx,
                        op_idx,
                    });
                    defined = true;
                    break;
                }
            }
            if !defined {
                let preds = self.predecessors.get(pred_idx)?;
                if preds.is_empty() {
                    return None;
                }
                queue.extend(preds.iter().copied());
            }
        }

        if reached_defs.is_empty() {
            return None;
        }
        reached_defs.sort_by_key(|site| (site.block_idx, site.op_idx));
        reached_defs.dedup_by_key(|site| (site.block_idx, site.op_idx));
        Some(reached_defs)
    }

    pub(in crate::midend::builder) fn recover_unclobbered_predecessor_arg(
        &mut self,
        vn: &Varnode,
        call_block_idx: usize,
        call_idx: usize,
    ) -> Option<PreHirExpr> {
        let reached_defs = self.collect_unclobbered_reaching_defs(vn, call_block_idx, call_idx)?;
        if let Some(expr) = self.ssa_emitted_read(vn) {
            return Some(self.normalize_recovered_call_arg(expr));
        }

        let mut shared_name: Option<String> = None;
        for &site in &reached_defs {
            let def_op = self
                .pcode
                .blocks
                .get(site.block_idx)?
                .ops
                .get(site.op_idx)?;
            let out = def_op.output.as_ref()?;
            let key = MaterializedVarnodeKey::new(out, def_op);
            let Some(name) = self.materialized_vns.get(&key) else {
                shared_name = None;
                break;
            };
            match &shared_name {
                None => shared_name = Some(name.clone()),
                Some(existing) if existing == name => {}
                Some(_) => {
                    shared_name = None;
                    break;
                }
            }
        }
        if let Some(name) = shared_name {
            return Some(PreHirExpr::Var(name));
        }

        let mut path_values: Vec<PreHirExpr> = Vec::with_capacity(reached_defs.len());
        for &site in &reached_defs {
            let def_op = self
                .pcode
                .blocks
                .get(site.block_idx)?
                .ops
                .get(site.op_idx)?
                .clone();
            let expr = self
                .with_lowering_site(site, |this| {
                    this.lower_def_op(&def_op, &mut HashSet::default())
                })
                .ok()?;
            if !path_values.contains(&expr) {
                path_values.push(expr);
            }
        }
        if path_values.len() > 1
            && let Some(expr) = self.triangle_select_arg(&reached_defs, call_block_idx)
        {
            return Some(self.normalize_recovered_call_arg(expr));
        }
        let merged = writes_merge_at_call(reached_defs.len(), &path_values);
        if merged {
            // Every incoming path defines the register and no call clobbers
            // it between that definition and this call (proved above), so the
            // argument is the register's merged value at the call site: one
            // value per path, joined. `lea rsi, msg_k; jmp join` on each edge
            // into `join: call dcgettext` is the ordinary shape; dropping the
            // slot printed `dcgettext(0)` and lost every message.
            //
            // The merged read must come from those definitions. A read equal
            // to one path's value is that path alone, and a read of the
            // register's entry value (its parameter) is a parameter the
            // caller never received: the lowering found no definition at the
            // call even though every path has one. Neither is the join, so
            // the slot stays empty -- and a parameter binding the rejected
            // read created is withdrawn with it. A read of the bare register
            // (`rdx`) is the same entry value before parameter naming.
            let live_in_name = if is_register_varnode(vn) {
                self.sla_hw_name(vn.offset, vn.size)
            } else {
                None
            };
            let had_live_in = live_in_name
                .as_ref()
                .is_some_and(|name| self.temps.contains_key(name));
            let entry_param = {
                let abi = self.abi_state();
                abi.param_slot_for_varnode(vn)
                    .map(|slot| (slot, abi.param_name(slot)))
            };
            let had_binding = entry_param
                .as_ref()
                .is_some_and(|(slot, _)| self.params.contains_key(slot));
            let had_name = entry_param
                .as_ref()
                .is_some_and(|(_, name)| self.used_param_local_names.contains(name));
            let expr = self.lower_varnode(vn, &mut HashSet::default()).ok()?;
            let mut bare = &expr;
            while let PreHirExpr::Cast { expr: inner, .. } = bare {
                bare = inner;
            }
            let reads_entry_value = matches!(bare, PreHirExpr::Var(var)
                if entry_param.as_ref().is_some_and(|(_, name)| var == name)
                    || live_in_name.as_ref().is_some_and(|name| var.eq_ignore_ascii_case(name)));
            self.debug_call_recovery(&format!(
                "join_arg vn=0x{:x}/{} merged={} entry={:?} rejected={}",
                vn.offset,
                vn.size,
                print_prehir_expr(&expr),
                entry_param,
                reads_entry_value || path_values.contains(&expr)
            ));
            // A merged variable that is the emitted name of some of the
            // writes reaching the call but not of the others stands for those
            // writes only: the other paths' values never reach it
            // (`target3(17, ...)` for a three-way merge whose only named
            // write is 17). A name no reaching write owns -- a stack slot or
            // join variable the merge itself is stored in -- is the merge.
            let owners = reached_defs
                .iter()
                .map(|&site| {
                    self.pcode
                        .blocks
                        .get(site.block_idx)
                        .and_then(|block| block.ops.get(site.op_idx))
                        .and_then(|op| self.name_emitted_for_definition(vn, site, op))
                })
                .collect::<Vec<_>>();
            let named_by_some_paths_only = matches!(bare, PreHirExpr::Var(var)
                if owners.iter().any(|owner| matches!(owner, Some(PreHirExpr::Var(name)) if name == var))
                    && owners.iter().any(|owner| !matches!(owner, Some(PreHirExpr::Var(name)) if name == var)));
            let rejected =
                reads_entry_value || path_values.contains(&expr) || named_by_some_paths_only;
            self.debug_call_recovery(&format!(
                "join_arg named_by_some_paths_only={named_by_some_paths_only}"
            ));
            if rejected {
                if let Some(name) = live_in_name
                    && !had_live_in
                {
                    self.temps.remove(&name);
                }
                if let Some((slot, name)) = entry_param {
                    if !had_binding {
                        self.params.remove(&slot);
                    }
                    if !had_name {
                        self.used_param_local_names.remove(&name);
                    }
                }
                return None;
            }
            return Some(self.normalize_recovered_call_arg(expr));
        }
        path_values
            .pop()
            .map(|expr| self.normalize_recovered_call_arg(expr))
    }

    /// Whether more than one write reaches the call with `vn` intact: the
    /// argument is then a merge, and any single write's value is the value on
    /// its own paths only.
    fn argument_register_merges(
        &self,
        vn: &Varnode,
        call_block_idx: usize,
        call_idx: usize,
    ) -> bool {
        self.collect_unclobbered_reaching_defs(vn, call_block_idx, call_idx)
            .is_some_and(|defs| defs.len() > 1)
    }

    /// The merged argument of a conditional overwrite -- `x = a; if (c)
    /// x = b; call(x)`, the shape of a predicated or conditional move -- as
    /// `c ? b : a`.
    ///
    /// Exactly two writes reach the call: one in the branch block before its
    /// conditional branch, one in the arm that branch enters. The arm is
    /// entered only from the branch and leaves only to the call's block, which
    /// the branch's other edge also reaches directly, so the branch condition
    /// decides which write the call sees.
    fn triangle_select_arg(
        &mut self,
        reached_defs: &[LoweringSite],
        call_block_idx: usize,
    ) -> Option<PreHirExpr> {
        let [first, second] = reached_defs else {
            return None;
        };
        let original_count = self.pcode.blocks.len();
        if first.block_idx >= original_count
            || second.block_idx >= original_count
            || call_block_idx >= original_count
            || !self.virtual_block_map.is_empty()
        {
            return None;
        }
        for (branch_def, arm_def) in [(*first, *second), (*second, *first)] {
            let branch_idx = branch_def.block_idx;
            let arm_idx = arm_def.block_idx;
            let branch_succs = self.successors.get(branch_idx)?;
            if branch_succs.len() != 2
                || !branch_succs.contains(&arm_idx)
                || !branch_succs.contains(&call_block_idx)
                || self.predecessors.get(arm_idx).map(Vec::as_slice) != Some(&[branch_idx])
                || self.successors.get(arm_idx).map(Vec::as_slice) != Some(&[call_block_idx])
            {
                continue;
            }
            let branch_block = self.pcode.blocks.get(branch_idx)?.clone();
            let term_idx = self.block_terminator_index(&branch_block)?;
            let term_op = branch_block.ops.get(term_idx)?.clone();
            if term_op.opcode != PcodeOpcode::CBranch
                || term_op.inputs.len() < 2
                || branch_def.op_idx >= term_idx
            {
                continue;
            }
            // The branch's own recovered condition, as the printed `if` uses
            // it: lowering the raw flag operand instead made every flag the
            // condition reads a live variable (`cf = ...; zf = ...;`).
            let LoweredTerminator::Cond {
                cond, true_target, ..
            } = self.lower_block_terminator(branch_idx).ok()?
            else {
                continue;
            };
            let arm_on_true = if true_target == self.block_target_key(arm_idx) {
                true
            } else if true_target == self.block_target_key(call_block_idx) {
                false
            } else {
                continue;
            };
            let lower_def = |this: &mut Self, site: LoweringSite| -> Option<PreHirExpr> {
                let def_op = this
                    .pcode
                    .blocks
                    .get(site.block_idx)?
                    .ops
                    .get(site.op_idx)?
                    .clone();
                this.with_lowering_site(site, |this| {
                    this.lower_def_op(&def_op, &mut HashSet::default())
                })
                .ok()
            };
            let branch_value = lower_def(self, branch_def)?;
            let arm_value = lower_def(self, arm_def)?;
            let (then_expr, else_expr) = if arm_on_true {
                (arm_value, branch_value)
            } else {
                (branch_value, arm_value)
            };
            let def_out = self
                .pcode
                .blocks
                .get(arm_def.block_idx)?
                .ops
                .get(arm_def.op_idx)?
                .output
                .clone()?;
            return Some(PreHirExpr::Select {
                cond: Box::new(cond),
                then_expr: Box::new(then_expr),
                else_expr: Box::new(else_expr),
                ty: type_from_size(def_out.size, false),
            });
        }
        None
    }

    fn resolve_call_store_stack_address(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        store_idx: usize,
    ) -> Option<(StackBase, i64)> {
        let store = &block.ops[store_idx];
        let site = LoweringSite {
            block_idx: block.index as usize,
            op_idx: store_idx,
        };
        self.with_lowering_site(site, |this| {
            if let Some(addr) = this.resolve_stack_address_from_memory_op(store) {
                return Some(addr);
            }
            let ptr = store.inputs.get(1)?;
            let store_addr = store.address;
            for prev_idx in (0..store_idx).rev() {
                let prev = &block.ops[prev_idx];
                if prev.address != store_addr {
                    break;
                }
                if prev.output.as_ref() != Some(ptr) {
                    continue;
                }
                if matches!(prev.opcode, PcodeOpcode::IntAdd | PcodeOpcode::PtrAdd) {
                    return resolve_add_op_stack_address(this, prev);
                }
            }
            this.resolve_stack_address(ptr)
        })
    }

    fn recover_call_stack_args_from_block(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        call_idx: usize,
    ) -> Result<Vec<PreHirExpr>, MlilPreviewError> {
        let abi = self.abi_state();
        if !self.options.is_64bit && self.x86_32_stack_call_args_enabled() {
            return self.recover_x86_32_stack_args_from_block(block, call_idx);
        }
        if !self.options.is_64bit {
            return Ok(Vec::new());
        }

        let scan_end = call_idx.min(block.ops.len());
        let call_address = block.ops.get(call_idx).map(|op| op.address);
        let mut recovered = std::collections::BTreeMap::<usize, PreHirExpr>::new();
        for prev_idx in (0..scan_end).rev() {
            let prev = &block.ops[prev_idx];
            if prev.opcode.is_control_flow() {
                if prev.opcode == PcodeOpcode::CallOther
                    && prev.output.is_none()
                    && Some(prev.address) == call_address
                {
                    continue;
                }
                if self.call_is_terminal_branchind_artifact(block, prev_idx) {
                    continue;
                }
                break;
            }
            // A store the *call instruction itself* lowers to is its return-address
            // push, not an argument: x86's `call` is `RSP = RSP - 8; STORE ram[RSP],
            // <next instruction>; CALL target`, and that store lands at stack offset
            // zero -- exactly where the first stack argument would be. Read as one it
            // appends the return address to every call site's arguments, which the
            // arguments' own stores (always at earlier instructions) never displace.
            // The x86-32 path below already skips these; the 64-bit one did not.
            if Some(prev.address) == call_address {
                continue;
            }
            if prev.opcode != PcodeOpcode::Store || prev.inputs.len() < 3 {
                continue;
            }
            let site = LoweringSite {
                block_idx: block.index as usize,
                op_idx: prev_idx,
            };
            // `push rbp` in the prologue saves the caller's value, which lands
            // where a first stack argument would when nothing else moves the
            // stack pointer before the call: `fcntl(fd, 3, 0)` printed with
            // the saved register appended. A save stores a preserved
            // register's entry value; an argument pushed from one stores a
            // value the function computed into it first.
            if self.with_lowering_site(site, |this| {
                this.is_callee_saved_push_store(prev)
                    && prev
                        .inputs
                        .last()
                        .is_some_and(|saved| this.stores_an_entry_register_value(saved))
            }) {
                continue;
            }
            let stack_address = self.with_lowering_site(site, |this| {
                this.resolve_call_store_stack_address(block, prev_idx)
            });
            let Some((StackBase::Rsp, offset)) = stack_address else {
                continue;
            };
            let Some(stack_index) = abi.stack_argument_index(offset) else {
                continue;
            };
            if recovered.contains_key(&stack_index) {
                continue;
            }
            let value = self.with_lowering_site(site, |this| {
                this.lower_varnode(
                    prev.inputs.last().expect("store rhs"),
                    &mut HashSet::default(),
                )
            })?;
            recovered.insert(stack_index, value);
        }

        let mut out = Vec::new();
        for idx in 0.. {
            let Some(expr) = recovered.remove(&idx) else {
                break;
            };
            out.push(expr);
        }
        self.debug_call_recovery(&format!("stack_args={}", out.len()));
        Ok(out)
    }

    fn x86_32_stack_call_args_enabled(&self) -> bool {
        self.options.pointer_size == 4
            && self.options.calling_convention == CallingConvention::X86_32
    }

    fn recover_x86_32_stack_args_from_block(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        call_idx: usize,
    ) -> Result<Vec<PreHirExpr>, MlilPreviewError> {
        const MAX_STACK_ARGS: usize = 32;

        // Prefer ESP-staged [esp+k] (k≥0) for CallInd/Call: gcc -O0 writes
        // args at [esp]/[esp+4] then pushes retaddr. Push-style reverse scan
        // would see only the [esp] store and stop before [esp+4].
        let mut out = self.recover_x86_32_esp_staged_args_from_block(block, call_idx)?;
        if !out.is_empty() {
            self.debug_call_recovery(&format!("x86_32_stack_args={}", out.len()));
            return Ok(out);
        }

        let scan_end = call_idx.min(block.ops.len());
        let call_address = block.ops.get(call_idx).map(|op| op.address);
        out = Vec::new();
        let mut current_push_address = None;
        for prev_idx in (0..scan_end).rev() {
            if out.len() >= MAX_STACK_ARGS {
                break;
            }
            let prev = &block.ops[prev_idx];
            if Some(prev.address) == call_address {
                continue;
            }
            if prev.opcode.is_control_flow() {
                if prev.opcode == PcodeOpcode::CallOther
                    && prev.output.is_none()
                    && Some(prev.address) == call_address
                {
                    continue;
                }
                if self.call_is_terminal_branchind_artifact(block, prev_idx) {
                    continue;
                }
                break;
            }
            if self.x86_32_stack_push_update(prev) {
                continue;
            }
            if current_push_address.is_some_and(|address| address == prev.address) {
                continue;
            }
            if !self.x86_32_stack_push_store(prev) {
                if !out.is_empty() {
                    break;
                }
                continue;
            }
            let site = LoweringSite {
                block_idx: block.index as usize,
                op_idx: prev_idx,
            };
            let value = self.with_lowering_site(site, |this| {
                this.lower_varnode(
                    prev.inputs.last().expect("store rhs"),
                    &mut HashSet::default(),
                )
            })?;
            out.push(self.normalize_recovered_call_arg(value));
            current_push_address = Some(prev.address);
        }

        if out.is_empty() {
            // cdecl tail-call restage: Stores to [ebp+8+4*i] before BranchInd/CallInd
            // (gcc-m32 -O2 apply_binop) rewrite the frame's param slots in place.
            out = self.recover_x86_32_frame_restaged_args_from_block(block, call_idx)?;
        }

        self.debug_call_recovery(&format!("x86_32_stack_args={}", out.len()));
        Ok(out)
    }

    /// Lower a staged store RHS to a frozen value for cdecl CallInd args.
    ///
    /// gcc-m32 -O0 stages via `mov eax, [ebp+c]; mov [esp], eax` then reloads
    /// `eax` with the function pointer. Naively lowering through live EAX (or a
    /// temp alias of EAX) lets normalize rewrite arg0 to `param_1`. Prefer the
    /// incoming stack-parameter surface reached by the Copy/Load chain at the
    /// defining site.
    fn lower_x86_32_staged_store_value(
        &mut self,
        rhs: &Varnode,
    ) -> Result<PreHirExpr, MlilPreviewError> {
        if let Some((def_site, def_op)) = self.lookup_def_site(rhs)
            && matches!(
                def_op.opcode,
                PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt
            )
            && def_op.inputs.len() == 1
        {
            let src = def_op.inputs[0].clone();
            return self.with_lowering_site(def_site, |this| {
                if let Some(name) = this.try_x86_32_stack_param_snapshot(&src) {
                    return Ok(PreHirExpr::Var(name));
                }
                this.lower_varnode(&src, &mut HashSet::default())
            });
        }
        if let Some(name) = self.try_x86_32_stack_param_snapshot(rhs) {
            return Ok(PreHirExpr::Var(name));
        }
        self.lower_varnode(rhs, &mut HashSet::default())
    }

    /// If `vn` is (or Copy-reaches) a Load from an incoming stack param slot,
    /// return that formal's surface name (`param_N`).
    fn try_x86_32_stack_param_snapshot(&mut self, vn: &Varnode) -> Option<String> {
        if !self.x86_32_stack_call_args_enabled() {
            return None;
        }
        if is_register_varnode(vn) {
            return self.stack_param_name_reaching_register(vn);
        }
        // Unique temp that is a direct Load from [ebp+k], or Copy of such a load.
        if let Some((_, op)) = self.lookup_def_site(vn) {
            if op.opcode == PcodeOpcode::Load {
                return self.stack_param_name_from_load_op(op);
            }
            if matches!(
                op.opcode,
                PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt
            ) && op.inputs.len() == 1
            {
                let input = op.inputs.first()?.clone();
                if is_register_varnode(&input) {
                    return self.stack_param_name_reaching_register(&input);
                }
                if let Some((_, src_op)) = self.lookup_def_site(&input)
                    && src_op.opcode == PcodeOpcode::Load
                {
                    return self.stack_param_name_from_load_op(src_op);
                }
            }
        }
        None
    }

    /// Recover cdecl args staged as `mov [esp+k], val` (k ≥ 0) before CallInd.
    /// Distinct from push-below-ESP (`offset < 0`) and from frame restage (`[ebp+8]`).
    pub(in crate::midend::builder) fn recover_x86_32_esp_staged_args_from_block(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        call_idx: usize,
    ) -> Result<Vec<PreHirExpr>, MlilPreviewError> {
        const MAX_STACK_ARGS: usize = 32;
        let ptr_size = i64::from(self.options.pointer_size);
        if ptr_size <= 0 {
            return Ok(Vec::new());
        }
        let scan_end = call_idx.min(block.ops.len());
        let call_address = block.ops.get(call_idx).map(|op| op.address);
        let mut recovered = std::collections::BTreeMap::<usize, PreHirExpr>::new();

        for prev_idx in (0..scan_end).rev() {
            if recovered.len() >= MAX_STACK_ARGS {
                break;
            }
            let prev = &block.ops[prev_idx];
            // Call instruction may share address with retaddr push — skip those ops.
            if Some(prev.address) == call_address {
                continue;
            }
            if prev.opcode.is_control_flow() {
                if prev.opcode == PcodeOpcode::CallOther
                    && prev.output.is_none()
                    && Some(prev.address) == call_address
                {
                    continue;
                }
                if self.call_is_terminal_branchind_artifact(block, prev_idx) {
                    continue;
                }
                break;
            }
            if prev.opcode != PcodeOpcode::Store || prev.inputs.len() < 3 {
                continue;
            }
            let site = LoweringSite {
                block_idx: block.index as usize,
                op_idx: prev_idx,
            };
            let stack_address = self.with_lowering_site(site, |this| {
                this.resolve_call_store_stack_address(block, prev_idx)
            });
            let Some((StackBase::Rsp, offset)) = stack_address else {
                continue;
            };
            // Staged args sit at [esp+0], [esp+4], … before the retaddr push.
            if offset < 0 || offset % ptr_size != 0 {
                continue;
            }
            let rhs = prev.inputs.last().expect("store rhs");
            let stack_index = (offset / ptr_size) as usize;
            if recovered.contains_key(&stack_index) {
                continue;
            }
            let value =
                self.with_lowering_site(site, |this| this.lower_x86_32_staged_store_value(rhs))?;
            recovered.insert(stack_index, self.normalize_recovered_call_arg(value));
        }

        let mut out = Vec::new();
        for idx in 0.. {
            let Some(expr) = recovered.remove(&idx) else {
                break;
            };
            out.push(expr);
            if out.len() >= MAX_STACK_ARGS {
                break;
            }
        }
        self.debug_call_recovery(&format!("x86_32_esp_staged_args={}", out.len()));
        Ok(out)
    }

    /// Recover stack args from frame-pointer-relative Stores into the incoming
    /// parameter region (`[ebp+8]`, `[ebp+c]`, …) used by cdecl tail-call
    /// restaging before `jmp reg` / `CallInd`.
    fn recover_x86_32_frame_restaged_args_from_block(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        call_idx: usize,
    ) -> Result<Vec<PreHirExpr>, MlilPreviewError> {
        const MAX_STACK_ARGS: usize = 32;
        let abi = self.abi_state();
        let scan_end = call_idx.min(block.ops.len());
        let call_address = block.ops.get(call_idx).map(|op| op.address);
        let mut recovered = std::collections::BTreeMap::<usize, PreHirExpr>::new();

        let mut scan_blocks: Vec<(&crate::pcode::PcodeBasicBlock, usize)> = vec![(block, scan_end)];
        if matches!(
            block.ops.get(call_idx).map(|op| op.opcode),
            Some(PcodeOpcode::BranchInd | PcodeOpcode::CallInd)
        ) {
            if let Some(block_idx) = self.block_position(block)
                && let Some(preds) = self.predecessors.get(block_idx)
                && let [pred_idx] = preds.as_slice()
                && let Some(pred_block) = self.pcode.blocks.get(*pred_idx)
            {
                scan_blocks.push((pred_block, pred_block.ops.len()));
            }
        }

        for (scan_block, scan_limit) in scan_blocks {
            for prev_idx in (0..scan_limit).rev() {
                if recovered.len() >= MAX_STACK_ARGS {
                    break;
                }
                let prev = &scan_block.ops[prev_idx];
                if Some(prev.address) == call_address {
                    continue;
                }
                if prev.opcode.is_control_flow() {
                    if prev.opcode == PcodeOpcode::CallOther
                        && prev.output.is_none()
                        && Some(prev.address) == call_address
                    {
                        continue;
                    }
                    if self.call_is_terminal_branchind_artifact(scan_block, prev_idx) {
                        continue;
                    }
                    if std::ptr::eq(scan_block, block) {
                        break;
                    }
                    continue;
                }
                if prev.opcode != PcodeOpcode::Store || prev.inputs.len() < 3 {
                    continue;
                }
                let site = LoweringSite {
                    block_idx: scan_block.index as usize,
                    op_idx: prev_idx,
                };
                let stack_address = self.with_lowering_site(site, |this| {
                    this.resolve_call_store_stack_address(scan_block, prev_idx)
                });
                let Some((base, offset)) = stack_address else {
                    continue;
                };
                let Some(stack_index) = abi.incoming_stack_argument_index(base, offset) else {
                    continue;
                };
                if recovered.contains_key(&stack_index) {
                    continue;
                }
                let value = self.with_lowering_site(site, |this| {
                    this.lower_varnode(
                        prev.inputs.last().expect("store rhs"),
                        &mut HashSet::default(),
                    )
                })?;
                recovered.insert(stack_index, self.normalize_recovered_call_arg(value));
            }
        }

        let mut out = Vec::new();
        for idx in 0.. {
            let Some(expr) = recovered.remove(&idx) else {
                break;
            };
            out.push(expr);
            if out.len() >= MAX_STACK_ARGS {
                break;
            }
        }
        self.debug_call_recovery(&format!("x86_32_frame_restage_args={}", out.len()));
        Ok(out)
    }

    fn x86_32_stack_push_update(&self, op: &PcodeOp) -> bool {
        op.opcode == PcodeOpcode::IntSub
            && op.inputs.len() >= 2
            && op
                .output
                .as_ref()
                .is_some_and(|output| self.is_x86_32_esp(output))
            && self.is_x86_32_esp(&op.inputs[0])
            && const_offset(&op.inputs[1]) == Some(i64::from(self.options.pointer_size))
    }

    fn x86_32_stack_push_store(&self, op: &PcodeOp) -> bool {
        if op.opcode != PcodeOpcode::Store || op.inputs.len() < 3 {
            return false;
        }
        let is_stack_ptr = self.is_x86_32_esp(&op.inputs[1])
            || self
                .resolve_stack_address_from_memory_op(op)
                .is_some_and(|(base, offset)| base == StackBase::Rsp && offset < 0);
        is_stack_ptr
            && op
                .inputs
                .last()
                .is_some_and(|value| value.size == self.options.pointer_size || value.size == 0)
    }

    pub(in crate::midend::builder) fn x86_32_store_is_recovered_call_arg(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
    ) -> bool {
        let Some(op) = block.ops.get(op_idx) else {
            return false;
        };
        if !self.x86_32_stack_call_args_enabled() {
            return false;
        }
        // cdecl CallInd staging first: Stores to [esp+k] (k ≥ 0). Checked
        // before push-style scan so intermediate Copy/ZExt to the CallInd
        // target does not reject a legitimate staged arg store.
        if op.opcode == PcodeOpcode::Store
            && self.x86_32_store_is_esp_staged_call_arg(block, op_idx)
        {
            return true;
        }
        // ESP push stores before Call/CallInd (offset < 0 or bare ESP push).
        if self.x86_32_stack_push_store(op) {
            for candidate in block.ops.iter().skip(op_idx + 1) {
                if candidate.address == op.address && self.x86_32_stack_push_update(candidate) {
                    continue;
                }
                if self.x86_32_stack_push_update(candidate)
                    || self.x86_32_stack_push_store(candidate)
                {
                    continue;
                }
                if matches!(
                    candidate.opcode,
                    PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther
                ) {
                    return true;
                }
                // Allow pure call-setup ops between push and call (target Copy, etc.).
                if matches!(
                    candidate.opcode,
                    PcodeOpcode::Copy
                        | PcodeOpcode::Cast
                        | PcodeOpcode::IntZExt
                        | PcodeOpcode::IntSExt
                        | PcodeOpcode::IntAdd
                        | PcodeOpcode::IntSub
                        | PcodeOpcode::Load
                ) {
                    continue;
                }
                return false;
            }
            return false;
        }
        // cdecl tail-call restage: Stores into [ebp+8+4*i] before BranchInd/CallInd
        // must not materialize as param reassignments (they only set up callee args).
        if op.opcode == PcodeOpcode::Store
            && self.x86_32_store_is_frame_param_restage(block, op_idx)
        {
            return true;
        }
        false
    }

    pub(in crate::midend::builder) fn x86_32_store_is_esp_staged_call_arg(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
    ) -> bool {
        let Some(op) = block.ops.get(op_idx) else {
            return false;
        };
        if op.opcode != PcodeOpcode::Store || op.inputs.len() < 3 {
            return false;
        }
        let Some((StackBase::Rsp, offset)) =
            self.resolve_stack_address_from_memory_op(op).or_else(|| {
                let ptr = op.inputs.get(1)?;
                let store_addr = op.address;
                for prev_idx in (0..op_idx).rev() {
                    let prev = &block.ops[prev_idx];
                    if prev.address != store_addr {
                        break;
                    }
                    if prev.output.as_ref() != Some(ptr) {
                        continue;
                    }
                    if matches!(prev.opcode, PcodeOpcode::IntAdd | PcodeOpcode::PtrAdd) {
                        return resolve_add_op_stack_address(self, prev);
                    }
                }
                self.resolve_stack_address(ptr)
            })
        else {
            return false;
        };
        let ptr_size = i64::from(self.options.pointer_size);
        if ptr_size <= 0 || offset < 0 || offset % ptr_size != 0 {
            return false;
        }
        for candidate in block.ops.iter().skip(op_idx + 1) {
            if matches!(
                candidate.opcode,
                PcodeOpcode::CallInd | PcodeOpcode::Call | PcodeOpcode::BranchInd
            ) {
                // Skip only the call instruction's own return-address push.
                return candidate.address != op.address;
            }
        }
        false
    }

    fn x86_32_store_is_frame_param_restage(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
    ) -> bool {
        let Some(op) = block.ops.get(op_idx) else {
            return false;
        };
        if op.opcode != PcodeOpcode::Store || op.inputs.len() < 3 {
            return false;
        }
        let Some((base, offset)) = self.resolve_stack_address_from_memory_op(op).or_else(|| {
            // Same-instruction IntAdd ebp, imm → unique then Store via unique.
            let ptr = op.inputs.get(1)?;
            let store_addr = op.address;
            for prev_idx in (0..op_idx).rev() {
                let prev = &block.ops[prev_idx];
                if prev.address != store_addr {
                    break;
                }
                if prev.output.as_ref() != Some(ptr) {
                    continue;
                }
                if matches!(prev.opcode, PcodeOpcode::IntAdd | PcodeOpcode::PtrAdd) {
                    return resolve_add_op_stack_address(self, prev);
                }
            }
            self.resolve_stack_address(ptr)
        }) else {
            return false;
        };
        if self
            .abi_state()
            .incoming_stack_argument_index(base, offset)
            .is_none()
        {
            return false;
        }
        // Require a later CallInd/BranchInd in this block (tail-call restage).
        block.ops.iter().skip(op_idx + 1).any(|candidate| {
            matches!(
                candidate.opcode,
                PcodeOpcode::CallInd | PcodeOpcode::BranchInd | PcodeOpcode::Call
            )
        })
    }

    fn is_x86_32_esp(&self, vn: &Varnode) -> bool {
        self.options.calling_convention == CallingConvention::X86_32
            && is_register_space_id(vn.space_id)
            && vn.offset == 0x10
            && vn.size == 4
    }

    /// Whether `value`, through copies, is a register nothing in this
    /// function has written yet: the value it held on entry.
    fn stores_an_entry_register_value(&self, value: &Varnode) -> bool {
        let mut cursor = value.clone();
        for _ in 0..6 {
            let Some((_, def)) = self.lookup_def_site(&cursor) else {
                return is_register_varnode(&cursor);
            };
            if !matches!(
                def.opcode,
                PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt
            ) {
                return false;
            }
            let Some(input) = def.inputs.first() else {
                return false;
            };
            cursor = input.clone();
        }
        false
    }

    pub(in crate::midend::builder) fn recover_call_args_from_block(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        call_idx: usize,
    ) -> Result<Option<Vec<PreHirExpr>>, MlilPreviewError> {
        self.recover_call_args_from_block_with_mode(block, call_idx, false)
    }

    pub(in crate::midend::builder) fn recover_tail_call_args_from_block(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        call_idx: usize,
    ) -> Result<Option<Vec<PreHirExpr>>, MlilPreviewError> {
        self.recover_call_args_from_block_with_mode(block, call_idx, true)
    }

    /// For CallInd, the register family that holds the callee pointer must not
    /// also appear as a recovered ABI argument (win64 `call r8` writes r8 and
    /// r8 is also param slot 2).
    fn callind_target_param_slot_index(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        call_idx: usize,
    ) -> Option<usize> {
        let op = block.ops.get(call_idx)?;
        if op.opcode != PcodeOpcode::CallInd {
            return None;
        }
        let mut cursor = op.inputs.first()?.clone();
        // Follow a short copy/zext chain to a register-space carrier.
        for _ in 0..6 {
            if is_register_space_id(cursor.space_id) {
                return self.param_index_for_varnode(&cursor, true);
            }
            let (_site, def_op) = self.lookup_def_site(&cursor)?;
            if !matches!(
                def_op.opcode,
                PcodeOpcode::Copy | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt | PcodeOpcode::Cast
            ) {
                return None;
            }
            cursor = def_op.inputs.first()?.clone();
        }
        None
    }

    /// The integer argument registers an exact resolved call's declaration
    /// proves are read, as indexes into the integer parameter slots.
    ///
    /// A call's basic block may stage only a later ABI slot while an earlier
    /// slot remains live from a dominating predecessor. Ordinary recovery
    /// deliberately requires a contiguous same-block prefix because an
    /// unknown call gives us no proof that a live register is an argument.
    /// An exact prototype supplies that missing proof: every declared
    /// integer-register-backed slot is consumed at this call site.
    ///
    /// A floating-point parameter is passed in a vector register and consumes
    /// no integer slot, so the declared arity is not the integer slot count:
    /// `Circle::perimeter(double k) const` is `this` in `rdi` and `k` in
    /// `xmm0`, and counting `k` as `rsi` printed the caller's own stale `rsi`
    /// as an argument. Under Windows x64 parameters are positional, so a
    /// floating-point parameter leaves its integer slot unused; under System V
    /// x86-64 and AArch64 integer and floating-point parameters are numbered
    /// separately.
    fn exact_call_integer_register_slots(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        call_idx: usize,
        param_count: usize,
    ) -> Option<ExactRegisterSlots> {
        let op = block.ops.get(call_idx)?;
        let target = match op.opcode {
            PcodeOpcode::Call => super::super::resolve_lifted_direct_call_target(
                op,
                self.options,
                self.type_context,
            )?,
            // Rust-Sleigh represents an IAT-backed Windows call as
            // `Copy unique <- ram:IAT_slot; CallInd unique`. The existing
            // target resolver already proves that shape and returns the
            // imported symbol; reuse it here rather than guessing the
            // prototype from the indirect opcode alone.
            PcodeOpcode::CallInd => {
                self.resolve_iat_load_call_target_without_telemetry(op.inputs.first()?)?
            }
            // `jmp f` out of the function is a tail call, named by the same
            // lookup `recover_known_external_tail_call_expr` uses.
            PcodeOpcode::Branch => {
                let address = branch_target_address(op.inputs.first()?)?;
                if self.address_to_index.contains_key(&address) {
                    return None;
                }
                sanitize_c_identifier(&super::super::dynamic_guards::resolve_call_target_address(
                    address,
                    self.type_context,
                )?)
            }
            _ => return None,
        };
        // The named parameters of a variadic declaration are as certain as an
        // exact arity: `open(const char *, int, ...)` reads `rdi` and `esi` at
        // every call, whatever follows in the variadic tail.
        let summary = self.type_context?.call_prototype_summaries.get(&target)?;
        let arity = summary
            .locked_exact_arity
            .or(summary.variadic_fixed_arity)?;
        let is_float = |index: usize| {
            summary
                .param_surface_type_names
                .get(index)
                .and_then(|name| name.as_deref())
                .is_some_and(is_floating_point_surface_type)
        };
        let slots: Vec<usize> = match self.options.calling_convention {
            CallingConvention::WindowsX64 => (0..arity).filter(|&index| !is_float(index)).collect(),
            CallingConvention::SystemVAmd64 | CallingConvention::AArch64 => {
                (0..(0..arity).filter(|&index| !is_float(index)).count()).collect()
            }
            // Conventions that may pass floating point in integer registers
            // (soft-float ARM, for one) keep the declared arity.
            _ => (0..arity).collect(),
        };
        // Every declared parameter travels in a register: a locked (not
        // variadic) declaration whose integer parameters all have a slot,
        // under a convention that passes floating point in its own registers.
        // Such a call takes nothing from the stack.
        let register_only = summary.locked_exact_arity.is_some()
            && slots.iter().all(|&slot| slot < param_count)
            && matches!(
                self.options.calling_convention,
                CallingConvention::SystemVAmd64
                    | CallingConvention::AArch64
                    | CallingConvention::WindowsX64
            )
            && (self.options.calling_convention != CallingConvention::WindowsX64
                || arity <= param_count);
        Some(ExactRegisterSlots {
            slots: slots
                .into_iter()
                .filter(|&slot| slot < param_count)
                .collect(),
            register_only,
        })
    }

    /// An exact callee declaration can prove that an ABI register is read by
    /// the call even when the lifted function body contains no p-code use of
    /// that register.  Treat it as an entry-owned parameter only when it still
    /// holds its entry value there.
    fn entry_register_arg_at_exact_call(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        call_idx: usize,
        vn: &Varnode,
        param_index: usize,
    ) -> Option<PreHirExpr> {
        if self
            .declared_variadic_fixed_arity
            .is_some_and(|fixed| param_index >= fixed)
            || !self.register_holds_entry_value_at_call(block, call_idx, vn)
        {
            return None;
        }
        let name = self.ensure_register_param_binding(vn, param_index);
        Some(PreHirExpr::Var(name))
    }

    /// Whether no path from the entry to the call writes `vn` -- through any
    /// view of it -- or makes a call, which clobbers every argument register.
    ///
    /// Having no single reaching definition is not the same thing: at a join
    /// where each arm loads a different format string into `rdx`, there is
    /// none, and reading the entry `rdx` instead printed the caller's third
    /// argument where the machine passes the string.
    fn register_holds_entry_value_at_call(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        call_idx: usize,
        vn: &Varnode,
    ) -> bool {
        let Some(call_block_idx) = self.block_position(block) else {
            return false;
        };
        let clobbers = |op: &PcodeOp| {
            op.opcode.is_call()
                || op.output.as_ref().is_some_and(|output| {
                    is_register_varnode(output)
                        && output.offset < vn.offset + u64::from(vn.size)
                        && vn.offset < output.offset + u64::from(output.size)
                })
        };
        let Some(call_block) = self.pcode.blocks.get(call_block_idx) else {
            return false;
        };
        if call_block.ops[..call_idx.min(call_block.ops.len())]
            .iter()
            .any(clobbers)
        {
            return false;
        }
        let mut seen = HashSet::default();
        let mut work = self
            .predecessors
            .get(call_block_idx)
            .cloned()
            .unwrap_or_default();
        while let Some(idx) = work.pop() {
            if !seen.insert(idx) {
                continue;
            }
            let Some(candidate) = self.pcode.blocks.get(idx) else {
                return false;
            };
            if candidate.ops.iter().any(clobbers) {
                return false;
            }
            if let Some(preds) = self.predecessors.get(idx) {
                work.extend(preds.iter().copied());
            }
        }
        true
    }

    /// Peel same-block Copy/ZExt/SExt/Cast so tail-call arg recovery sees the
    /// staged source register, not the ABI slot being written.
    ///
    /// Example (x64 O2 `jmp rax` after `mov ecx, edx; movsxd rcx, ecx`):
    /// prefer_source on the zext alone surfaces `ecx`/`param_1`; peeling reaches
    /// `edx`/`param_2`.
    ///
    /// Walks `scan_block.ops` locally (not `lookup_def_site`) so a wider zext of
    /// the same register family is not treated as the seed's own definition.
    fn peel_prefer_source_arg_varnode(
        &self,
        scan_block: &crate::pcode::PcodeBasicBlock,
        before_op_idx: usize,
        seed: &Varnode,
    ) -> (Varnode, usize) {
        let mut cursor = seed.clone();
        let mut limit = before_op_idx.min(scan_block.ops.len());
        for _ in 0..6 {
            let mut found: Option<(usize, Varnode)> = None;
            for prev_idx in (0..limit).rev() {
                let prev = &scan_block.ops[prev_idx];
                let Some(output) = prev.output.as_ref() else {
                    continue;
                };
                if !Self::varnode_covers_or_aliases_register(output, &cursor) {
                    continue;
                }
                if !matches!(
                    prev.opcode,
                    PcodeOpcode::Copy
                        | PcodeOpcode::IntZExt
                        | PcodeOpcode::IntSExt
                        | PcodeOpcode::Cast
                ) {
                    break;
                }
                let Some(input) = prev.inputs.first() else {
                    break;
                };
                found = Some((prev_idx, input.clone()));
                break;
            }
            let Some((def_idx, next)) = found else {
                break;
            };
            cursor = next;
            limit = def_idx;
            if cursor.is_constant {
                break;
            }
        }
        (cursor, limit)
    }

    /// True when `def` writes the same register family that `vn` reads
    /// (exact key or overlapping low/high view of the same offset bank).
    fn varnode_covers_or_aliases_register(def: &Varnode, vn: &Varnode) -> bool {
        if !is_register_varnode(def) || !is_register_varnode(vn) {
            return false;
        }
        if def.offset == vn.offset && def.size == vn.size {
            return true;
        }
        // Same base offset family: e.g. rcx (8) covers ecx (4) at offset 0x8.
        if def.offset == vn.offset && def.size >= vn.size {
            return true;
        }
        // Partial write into a wider view: ecx write feeds later rcx read.
        if vn.offset == def.offset && vn.size >= def.size {
            return true;
        }
        false
    }

    fn recover_call_args_from_block_with_mode(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        call_idx: usize,
        prefer_source_values: bool,
    ) -> Result<Option<Vec<PreHirExpr>>, MlilPreviewError> {
        if !self.options.is_64bit
            && !self.x86_32_stack_call_args_enabled()
            && !matches!(
                self.options.calling_convention,
                CallingConvention::Arm32
                    | CallingConvention::PowerPc32
                    | CallingConvention::LoongArch32
                    | CallingConvention::Mips32
            )
        {
            return Ok(None);
        }

        let abi = self.abi_state();
        let param_slots = abi.effective_param_reg_slots();
        let param_count = param_slots.len();
        let mut recovered: Vec<Option<PreHirExpr>> = vec![None; param_count];
        let skip_param = self.callind_target_param_slot_index(block, call_idx);
        // Also skip BranchInd target register if it aliases a param slot (rare).
        let skip_param = skip_param.or_else(|| {
            let op = block.ops.get(call_idx)?;
            if op.opcode != PcodeOpcode::BranchInd {
                return None;
            }
            let target = op.inputs.first()?;
            self.param_index_for_varnode(target, true)
        });
        let scan_end = call_idx.min(block.ops.len());
        let call_address = block.ops.get(call_idx).map(|op| op.address);
        let has_same_instruction_callother_marker = block.ops[..scan_end].iter().any(|prev| {
            prev.opcode == PcodeOpcode::CallOther
                && prev.output.is_none()
                && Some(prev.address) == call_address
        });

        // Scan current block, then (for BranchInd/CallInd tail arms) a single
        // predecessor so args staged before the null-check arm are recovered
        // (x64 O2: rcx/rdx set in parent, jmp in child).
        let mut scan_blocks: Vec<(&crate::pcode::PcodeBasicBlock, usize)> = vec![(block, scan_end)];
        if matches!(
            block.ops.get(call_idx).map(|op| op.opcode),
            Some(PcodeOpcode::BranchInd | PcodeOpcode::CallInd)
        ) {
            if let Some(block_idx) = self.block_position(block)
                && let Some(preds) = self.predecessors.get(block_idx)
                && let [pred_idx] = preds.as_slice()
                && let Some(pred_block) = self.pcode.blocks.get(*pred_idx)
            {
                scan_blocks.push((pred_block, pred_block.ops.len()));
            }
        }

        // A call has no register operands in raw p-code, so an unchanged ABI
        // carrier can sit before an earlier call while the current call
        // stages the rest of its prefix afterwards. Keep the current call's
        // snapshot bounded: cross at most that immediately preceding call,
        // and only after at least two contiguous current-call slots already
        // provide evidence that this is a new argument tuple. Without that
        // evidence, continuing would replay a prior call's staged arguments.
        let mut crossed_intervening_call = false;
        for (scan_block, scan_limit) in scan_blocks {
            for prev_idx in (0..scan_limit).rev() {
                let prev = &scan_block.ops[prev_idx];
                if prev.opcode.is_control_flow() {
                    if prev.opcode == PcodeOpcode::CallOther
                        && prev.output.is_none()
                        && Some(prev.address) == call_address
                    {
                        continue;
                    }
                    if self.call_is_terminal_branchind_artifact(scan_block, prev_idx) {
                        continue;
                    }
                    if prev.opcode.is_call() && std::ptr::eq(scan_block, block) {
                        let current_prefix =
                            recovered.iter().take_while(|expr| expr.is_some()).count();
                        if !crossed_intervening_call && current_prefix >= 2 {
                            crossed_intervening_call = true;
                            continue;
                        }
                    }
                    // Only hard-stop control-flow inside the *call* block.
                    if std::ptr::eq(scan_block, block) {
                        break;
                    }
                    continue;
                }
                let Some(output) = &prev.output else {
                    continue;
                };
                if prefer_source_values
                    && self.op_is_terminal_branchind_target_artifact(scan_block, prev_idx)
                {
                    continue;
                }
                let Some(param_index) = self.param_index_for_varnode(output, true) else {
                    continue;
                };
                if skip_param == Some(param_index) {
                    continue;
                }
                if param_index >= recovered.len() || recovered[param_index].is_some() {
                    continue;
                }

                let direct_rhs = if prefer_source_values
                    || self.options.calling_convention != CallingConvention::Arm32
                    || !has_same_instruction_callother_marker
                {
                    None
                } else {
                    self.try_lower_materialized_output_rhs(scan_block.start_address, prev)?
                };
                // A tail call prefers the register a copy reads (`mov ecx,
                // edx; jmp rax` passes `edx`, not the slot's own entry
                // value), read where the copy reads it: the register may be
                // written again before the jump (`mov rdi, rbx; pop rbx; jmp
                // f`). Only a copy has a source to prefer -- `xor edi, edi`
                // is `edi = edi ^ edi`, whose first input is not its value --
                // so any other definition is read just after itself.
                let (source, source_site) = if prefer_source_values {
                    let copies = matches!(
                        prev.opcode,
                        PcodeOpcode::Copy
                            | PcodeOpcode::Cast
                            | PcodeOpcode::IntZExt
                            | PcodeOpcode::IntSExt
                    );
                    let (source, read_at) = match prev.inputs.first() {
                        Some(seed) if copies => {
                            self.peel_prefer_source_arg_varnode(scan_block, prev_idx, seed)
                        }
                        _ => (output.clone(), prev_idx + 1),
                    };
                    let site = LoweringSite {
                        block_idx: scan_block.index as usize,
                        op_idx: read_at,
                    };
                    (source, Some(site))
                } else {
                    (output.clone(), None)
                };
                let expr = if let Some(expr) = direct_rhs {
                    expr
                } else {
                    let lowered = match source_site {
                        Some(site) => self.with_lowering_site(site, |this| {
                            this.lower_varnode(&source, &mut HashSet::default())
                        }),
                        None => self.lower_varnode(&source, &mut HashSet::default()),
                    };
                    match lowered {
                        Ok(expr) => expr,
                        Err(MlilPreviewError::UnsupportedPattern("opcode"))
                            if self.surface_call_carrier_name(output).is_some() =>
                        {
                            PreHirExpr::Var(
                                self.surface_call_carrier_name(output)
                                    .expect("surface carrier exists after guard"),
                            )
                        }
                        Err(err) => {
                            self.debug_lowering_error(
                                "call_arg_recovery",
                                scan_block.start_address,
                                u64::from(prev.seq_num),
                                prev.opcode,
                                &err,
                            );
                            if matches!(err, MlilPreviewError::UnsupportedPattern("opcode")) {
                                self.record_unsupported_inventory_event(
                                    "call_recovery",
                                    Some(output),
                                    Some(prev),
                                    Some(prev.opcode),
                                    Some(scan_block.start_address),
                                    Some(u64::from(prev.seq_num)),
                                    false,
                                    "call_arg_recovery_lowering_failed",
                                );
                            }
                            continue;
                        }
                    }
                };
                recovered[param_index] = Some(self.normalize_recovered_call_arg(expr));
            }
        }

        // A locked declaration proves that every slot below its arity is a
        // real argument. Fill holes from the existing site-sensitive def-use
        // lookup when the single realistic definition is the only write that
        // reaches the call; otherwise from the CFG-sound unclobbered reaching
        // definitions (a path-restaged register after an earlier call, or a
        // merge of several writes). Only integer-register slots are filled:
        // a floating-point parameter takes no integer slot on SysV/AArch64,
        // and Windows leaves that slot unused.
        let exact_slots = self.exact_call_integer_register_slots(block, call_idx, param_count);
        if let Some(exact) = exact_slots.as_ref() {
            for &param_index in &exact.slots {
                if skip_param == Some(param_index) || recovered[param_index].is_some() {
                    continue;
                }
                let (offset, size) = param_slots[param_index];
                let vn = Varnode {
                    space_id: REGISTER_SPACE_ID,
                    offset,
                    size,
                    is_constant: false,
                    constant_val: 0,
                };
                if let Some((site, _)) = self.lookup_def_site(&vn) {
                    let def_site = crate::midend::support::DefSite {
                        block_idx: site.block_idx,
                        op_idx: site.op_idx,
                        _marker: std::marker::PhantomData,
                    };
                    if self.check_ancestor_realistic(&vn, &def_site, block.index as usize, call_idx)
                        && !self.argument_register_merges(&vn, block.index as usize, call_idx)
                    {
                        let expr = self.lower_varnode(&vn, &mut HashSet::default())?;
                        recovered[param_index] = Some(self.normalize_recovered_call_arg(expr));
                    } else if let Some(expr) = self.recover_unclobbered_predecessor_arg(
                        &vn,
                        block.index as usize,
                        call_idx,
                    ) {
                        recovered[param_index] = Some(expr);
                    }
                } else if let Some(expr) =
                    self.recover_unclobbered_predecessor_arg(&vn, block.index as usize, call_idx)
                {
                    recovered[param_index] = Some(expr);
                } else if let Some(expr) =
                    self.entry_register_arg_at_exact_call(block, call_idx, &vn, param_index)
                {
                    recovered[param_index] = Some(expr);
                }
            }
        }

        let assignments = self.call_arg_carrier_assignments(block, call_idx, &abi);
        for assignment in assignments {
            let param_index = assignment.resource.slot;
            if skip_param == Some(param_index) {
                continue;
            }
            if recovered[param_index].is_some() {
                continue;
            }
            let (offset, size) = param_slots[param_index];

            let vn = Varnode {
                space_id: REGISTER_SPACE_ID,
                offset,
                size,
                is_constant: false,
                constant_val: 0,
            };

            if let Some((site, _)) = self.lookup_def_site(&vn) {
                let def_site = crate::midend::support::DefSite {
                    block_idx: site.block_idx,
                    op_idx: site.op_idx,
                    _marker: std::marker::PhantomData,
                };
                if self.check_ancestor_realistic(&vn, &def_site, block.index as usize, call_idx)
                    && !self.argument_register_merges(&vn, block.index as usize, call_idx)
                {
                    let expr = self.lower_varnode(&vn, &mut HashSet::default())?;
                    recovered[param_index] = Some(self.normalize_recovered_call_arg(expr));
                    continue;
                }
            }
            if let Some(expr) =
                self.recover_unclobbered_predecessor_arg(&vn, block.index as usize, call_idx)
            {
                recovered[param_index] = Some(expr);
            }
        }

        // A declared parameter the recovery could not name still precedes
        // the ones it did: `__fwritex(s, len, f)` with `s` a merge of several
        // writes the builder has no variable for dropped `len` and `f` with
        // it, and printed a spill to `[rsp]` as a stack argument instead.
        // Mark the hole explicitly instead of dropping what follows it (or
        // guessing the register's entry value).
        if let Some(exact) = exact_slots.as_ref()
            && let Some(&last) = exact
                .slots
                .iter()
                .rev()
                .find(|&&slot| recovered.get(slot).is_some_and(Option::is_some))
        {
            for &slot in exact.slots.iter().filter(|&&slot| slot < last) {
                if recovered[slot].is_none() && skip_param != Some(slot) {
                    let size = param_slots[slot].1;
                    recovered[slot] = Some(PreHirExpr::Call {
                        origin: None,
                        target: UNRECOVERED_ARGUMENT.to_string(),
                        args: Vec::new(),
                        ty: type_from_size(size, false),
                    });
                }
            }
        }
        let contiguous_reg_count = recovered.iter().take_while(|expr| expr.is_some()).count();
        let stack_args = if exact_slots
            .as_ref()
            .is_some_and(|exact| exact.register_only)
        {
            Vec::new()
        } else {
            self.recover_call_stack_args_from_block(block, call_idx)?
        };
        if contiguous_reg_count == 0 {
            if !stack_args.is_empty() {
                self.debug_call_recovery(&format!("reg_args=0 total_args={}", stack_args.len()));
                return Ok(Some(stack_args));
            }
            self.debug_call_recovery("no_contiguous_reg_args");
            return Ok(None);
        }

        let mut args = recovered
            .into_iter()
            .take(contiguous_reg_count)
            .map(|expr| expr.expect("contiguous recovered reg arg"))
            .collect::<Vec<_>>();
        args.extend(stack_args);
        self.debug_call_recovery(&format!(
            "reg_args={} total_args={}",
            contiguous_reg_count,
            args.len()
        ));
        Ok(Some(args))
    }

    fn call_arg_carrier_assignments(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        call_idx: usize,
        abi: &AbiState,
    ) -> Vec<CarrierAssignment> {
        // An earlier call in this block clobbered every argument register, so
        // only what is written after it is staged for this call: `xor esi,
        // esi; call dup2; mov edi, ebp; jmp f` printed `f(fd, 0)` from
        // `dup2`'s second argument.
        let last_call = block.ops[..call_idx]
            .iter()
            .enumerate()
            .rposition(|(idx, op)| {
                matches!(op.opcode, PcodeOpcode::Call | PcodeOpcode::CallInd)
                    && !self.call_is_terminal_branchind_artifact(block, idx)
            });
        let same_block_carriers = block.ops[last_call.map_or(0, |idx| idx + 1)..call_idx]
            .iter()
            .filter_map(|op| op.output.as_ref())
            .collect::<Vec<_>>();
        let same_block = abi.assign_carriers(same_block_carriers.iter().copied());
        if !same_block.is_empty() || last_call.is_some() {
            return same_block;
        }

        let block_idx = block.index as usize;
        let Some(pred_indices) = self.predecessors.get(block_idx) else {
            return same_block;
        };
        let mut predecessor_carriers = Vec::new();
        for pred_idx in pred_indices {
            let pred = self.pcode_block(*pred_idx);
            let end = self.block_terminator_index(pred).unwrap_or(pred.ops.len());
            // A predecessor that ends in a call staged those registers for
            // *that* call, and every argument register is caller-saved, so
            // none of them reach this call site. Reading them anyway hands a
            // previous call's arguments to this one: `setutent()`, which takes
            // none, came out carrying the preceding `sigaction`'s.
            if pred.ops.get(end).is_some_and(|op| op.opcode.is_call())
                || pred.ops[..end].iter().any(|op| op.opcode.is_call())
            {
                continue;
            }
            predecessor_carriers.extend(pred.ops[..end].iter().filter_map(|op| op.output.as_ref()));
        }
        abi.assign_carriers(predecessor_carriers)
    }
}

/// Whether a declared parameter type is passed in a floating-point register.
fn is_floating_point_surface_type(surface: &str) -> bool {
    let name = surface
        .split_whitespace()
        .filter(|word| !matches!(*word, "const" | "volatile"))
        .collect::<Vec<_>>()
        .join(" ");
    matches!(name.as_str(), "float" | "double" | "long double")
}

/// The integer-register slots an exact callee declaration fills, and whether
/// the declaration leaves nothing to pass on the stack.
struct ExactRegisterSlots {
    slots: Vec<usize>,
    register_only: bool,
}

/// Printed for a declared argument whose value the recovery could not name --
/// typically a merge of several writes the builder has no variable for:
/// visibly unrecovered, never a guess such as the register's entry value, and
/// never dropped together with the declared arguments after it.
pub(crate) const UNRECOVERED_ARGUMENT: &str = "__fission_unrecovered_arg";

/// Whether the writes reaching a call carry more than one value, given how
/// many writes reach it and their distinct printed expressions.
///
/// Different expressions are different values. Several writes spelled alike
/// are one value only when the spelling reads nothing that can differ between
/// them -- a constant or a global's address: two `lea rsi, msg` agree, but a
/// loop's `rdi += 16` in the preheader and in the latch reads a different
/// `rdi` each time. Passing that expression counted the increment twice
/// (`pop_arg(p + 16)` where the call sees `p`).
fn writes_merge_at_call(reaching_writes: usize, path_values: &[PreHirExpr]) -> bool {
    let path_independent = |expr: &PreHirExpr| {
        let mut bare = expr;
        while let PreHirExpr::Cast { expr: inner, .. } = bare {
            bare = inner;
        }
        matches!(bare, PreHirExpr::Const(..) | PreHirExpr::AddressOfGlobal(_))
    };
    path_values.len() > 1 || (reaching_writes > 1 && !path_values.iter().all(path_independent))
}

#[cfg(test)]
mod writes_merge_tests {
    use super::*;

    fn u64_ty() -> NirType {
        NirType::Int {
            bits: 64,
            signed: false,
        }
    }

    #[test]
    fn alike_writes_that_read_a_register_are_still_a_merge() {
        let update = PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Var("p".into())),
            rhs: Box::new(PreHirExpr::Const(16, u64_ty())),
            ty: u64_ty(),
        };
        assert!(
            writes_merge_at_call(2, std::slice::from_ref(&update)),
            "two `p + 16` writes read different `p`s"
        );
        assert!(!writes_merge_at_call(1, std::slice::from_ref(&update)));
        let constant = PreHirExpr::AddressOfGlobal("\"msg\"".into());
        assert!(!writes_merge_at_call(2, std::slice::from_ref(&constant)));
        assert!(writes_merge_at_call(
            2,
            &[constant, PreHirExpr::Const(7, u64_ty())]
        ));
    }
}
