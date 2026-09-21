use super::*;

mod accumulator;
mod accumulator_inputs;
mod call_results;
mod contracts;
mod cover_diagnostics;
mod cross_block;
mod gpr_names;
mod incremental;
mod load_roles;
mod loop_carried;
mod merge_scans;
mod no_consumer;
mod partial_gpr;
mod register_join;
mod replacement_plan;
mod same_block;
mod scans;
mod stack_home;
#[cfg(test)]
pub(super) mod test_support;
mod trace;

pub(in crate::midend::builder) use self::contracts::MaterializeOwnerRepartition;
use self::contracts::*;
pub(in crate::midend::builder) use self::loop_carried::LoopCarriedDefinitionProof;
use self::scans::DefinitionReachesReturnProof;

struct SameBlockRegisterJoinProof {
    binding_name: String,
    prior_op_idx: usize,
}

impl<'a> PreviewBuilder<'a> {
    fn is_callee_saved_push_store(&self, op: &PcodeOp) -> bool {
        if op.opcode != PcodeOpcode::Store || op.inputs.len() < 3 {
            return false;
        }
        let saved = &op.inputs[2];
        let register_namer = self.register_namer();
        let mut saved_origin = saved.clone();
        let mut saved_name = None;
        for _ in 0..6 {
            if let Some(name) = register_namer.hw_name(&saved_origin) {
                saved_name = Some(name);
                break;
            }
            let Some((_, def)) = self.lookup_def_site(&saved_origin) else {
                break;
            };
            if !matches!(
                def.opcode,
                PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt
            ) || def.inputs.is_empty()
            {
                break;
            }
            saved_origin = def.inputs[0].clone();
        }
        let preserved = saved_name.is_some_and(|saved_name| {
            self.options.cspec_unaffected_offsets.iter().any(|offset| {
                register_namer
                    .hw_name_at(*offset, saved_origin.size)
                    .is_some_and(|name| name.eq_ignore_ascii_case(&saved_name))
            })
        });

        // `asm_mnemonic` contains p-code opcode labels in the Rust-SLEIGH
        // pipeline, not disassembly.  Prove a push from its semantic shape:
        // the same machine instruction subtracts exactly one saved value
        // from SP and stores that preserved register through the new SP.
        let pcode_proves_push = preserved
            && self.pcode.blocks.first().is_some_and(|entry| {
                entry.ops.iter().any(|candidate| {
                    candidate.address == op.address
                        && candidate.opcode == PcodeOpcode::IntSub
                        && candidate.inputs.len() == 2
                        && candidate.output.as_ref().is_some_and(|output| {
                            self.output_is_stack_pointer_register(output)
                                && self.output_is_stack_pointer_register(&candidate.inputs[0])
                        })
                        && const_offset(&candidate.inputs[1]) == Some(i64::from(saved.size))
                })
            });
        if pcode_proves_push {
            return true;
        }

        // Preserve the legacy path for callers that provide real disassembly
        // text but no resolved `.cspec` model.
        let Some(asm) = op.asm_mnemonic.as_deref() else {
            return false;
        };
        let asm = asm.trim().to_ascii_uppercase();
        asm.starts_with("PUSH RSI")
            || asm.starts_with("PUSH RDI")
            || asm.starts_with("PUSH RBX")
            || asm.starts_with("PUSH RBP")
            || asm.starts_with("PUSH R12")
            || asm.starts_with("PUSH R13")
            || asm.starts_with("PUSH R14")
            || asm.starts_with("PUSH R15")
    }

    /// Element/value type for a store/load varnode: float when the defining
    /// p-code op is float-class, otherwise the integer size default.
    fn memory_value_type_for_varnode(&self, vn: &Varnode) -> NirType {
        let mut current = vn.clone();
        // Peel a short Copy/Cast chain so FLOAT_MULT → unique → Store sees float.
        for _ in 0..6 {
            let Some((_, def)) = self.lookup_def_site(&current) else {
                break;
            };
            match def.opcode {
                PcodeOpcode::Copy | PcodeOpcode::Cast if !def.inputs.is_empty() => {
                    current = def.inputs[0].clone();
                    continue;
                }
                opcode => {
                    return pcode_output_type_from_size(opcode, vn.size);
                }
            }
        }
        type_from_size(vn.size, false)
    }

    pub(in crate::midend) fn lower_block_stmts(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
    ) -> Result<Vec<PreHirStmt>, MlilPreviewError> {
        let block_idx = self.pcode_block_idx(block.index as usize);
        if let Some(cached) = self.lowered_block_stmts_cache.get(&block_idx) {
            return Ok(cached.clone());
        }

        // Tail-of-block cmov (absolute CBranch + guarded body still in this BB)
        // is not a materialize CFG terminator: it must lower via the same-block
        // forward path. Keeping it as terminator bare-skips the guard and either
        // drops or unconditionally applies INT_MIN/INT_MAX-style copies.
        let terminator_index = self.materialize_block_terminator_index(block);
        let diag = preview_builder_diag_enabled();
        let stage_started = diag.then(std::time::Instant::now);
        let mut body = self.synthesize_explicit_merge_bindings_for_block(block)?;
        if let Some(started) = stage_started {
            eprintln!(
                "[DIAG] lower_block_stmts merge: block={} ops={} elapsed_ms={:.3}",
                block_idx,
                block.ops.len(),
                started.elapsed().as_secs_f64() * 1000.0
            );
        }
        let stage_started = diag.then(std::time::Instant::now);
        let lowered = self.lower_block_ops_range(block, 0, block.ops.len(), terminator_index)?;
        if diag {
            // Which *statement* carries a runaway expression, so a blow-up can
            // be attributed to one op rather than to the block as a whole.
            for (i, stmt) in lowered.iter().enumerate() {
                let size = diag_stmt_expr_size(stmt);
                if size > 200 {
                    eprintln!(
                        "[DIAG] huge stmt: block={} stmt={} expr_nodes={} target={}",
                        block_idx,
                        i,
                        size,
                        diag_stmt_target(stmt)
                    );
                }
            }
        }
        body.extend(lowered);
        if let Some(started) = stage_started {
            eprintln!(
                "[DIAG] lower_block_stmts ops: block={} ops={} elapsed_ms={:.3}",
                block_idx,
                block.ops.len(),
                started.elapsed().as_secs_f64() * 1000.0
            );
        }

        self.lowered_block_stmts_cache
            .insert(block_idx, body.clone());
        Ok(body)
    }

    fn lower_block_ops_range(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        start_idx: usize,
        end_idx: usize,
        terminator_index: Option<usize>,
    ) -> Result<Vec<PreHirStmt>, MlilPreviewError> {
        let mut body = Vec::new();
        let block_idx = self.lowering_block_index(block);
        let mut op_idx = start_idx;
        while op_idx < end_idx {
            let op = &block.ops[op_idx];
            // Bare-skip real CFG terminators, but keep same-block-forward CBranch
            // tails (x86 cmov body after absolute CBranch to next BB). Those must
            // lower as guarded skips; skipping them drops INT_MIN/INT_MAX arms or
            // applies the guarded Copy unconditionally.
            if Some(op_idx) == terminator_index {
                let is_same_block_forward_cmov = op.opcode == PcodeOpcode::CBranch
                    && op.inputs.len() >= 2
                    && crate::midend::cfg::same_block_forward_branch_target_op_idx(
                        block,
                        op_idx,
                        end_idx,
                        op,
                        &op.inputs[0],
                    )
                    .is_some_and(|target| target > op_idx + 1);
                if !is_same_block_forward_cmov {
                    op_idx += 1;
                    continue;
                }
            }

            // Handle intra-block control flow (e.g. from cmov).
            // Targets may be relative p-code deltas *or* absolute next-instruction
            // addresses (x86 SLEIGH cmov); both must lower as guarded skips.
            if op.opcode == PcodeOpcode::CBranch && op.inputs.len() >= 2 {
                if let Some(target_op_idx) =
                    crate::midend::cfg::same_block_forward_branch_target_op_idx(
                        block,
                        op_idx,
                        end_idx,
                        op,
                        &op.inputs[0],
                    )
                {
                    // Scope condition lowering to this op so reused unique temps
                    // (x86 cmov BoolNegate slots) resolve to the *prior* def, not a
                    // later cmov in the same block.
                    let site = LoweringSite { block_idx, op_idx };
                    let cond = self.with_lowering_site(site, |this| {
                        this.lower_varnode(&op.inputs[1], &mut HashSet::default())
                    })?;
                    let inverted_cond = PreHirExpr::Unary {
                        op: PreHirUnaryOp::Not,
                        expr: Box::new(cond),
                        ty: NirType::Bool,
                    };
                    // Body range is after this CBranch; clear terminator so nested
                    // lowering does not bare-skip an outer index.
                    let nested_body =
                        self.lower_block_ops_range(block, op_idx + 1, target_op_idx, None)?;
                    if preview_builder_diag_enabled() {
                        eprintln!(
                            "[DIAG] same_block_cmov block=0x{:x} op_idx={} target={} body_stmts={} term_is={:?} end={}",
                            block.start_address,
                            op_idx,
                            target_op_idx,
                            nested_body.len(),
                            terminator_index,
                            end_idx
                        );
                        for (i, stmt) in nested_body.iter().enumerate() {
                            eprintln!("[DIAG]   cmov_body[{i}]={stmt:?}");
                        }
                    }
                    body.push(PreHirStmt::If {
                        cond: inverted_cond,
                        then_body: nested_body.into(),
                        else_body: Vec::new().into(),
                    });
                    op_idx = target_op_idx;
                    continue;
                }
            } else if op.opcode == PcodeOpcode::Branch && !op.inputs.is_empty() {
                if let Some(target_op_idx) =
                    crate::midend::cfg::same_block_forward_branch_target_op_idx(
                        block,
                        op_idx,
                        end_idx,
                        op,
                        &op.inputs[0],
                    )
                {
                    op_idx = target_op_idx;
                    continue;
                }
            }

            let site = LoweringSite { block_idx, op_idx };
            let maybe_stmt = self.with_lowering_site(
                site,
                |this| -> Result<Option<PreHirStmt>, MlilPreviewError> {
                    let mut visiting = HashSet::default();
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
                            if this.is_callee_saved_push_store(op)
                                || this.is_call_return_scaffold_store(block, op_idx, op)
                                || this.x86_32_store_is_recovered_call_arg(block, op_idx)
                            {
                                return Ok(None);
                            }
                            // Prefer float element type when the stored value is
                            // produced by float p-code (FLOAT_MULT/ADD/…); size
                            // alone cannot distinguish float32 from uint32.
                            let store_ty = this.memory_value_type_for_varnode(&op.inputs[2]);
                            let lhs = if let Some((slot_name, _slot_ty)) = this
                                .try_stack_slot_lvalue_for_memory_op(
                                    op,
                                    &op.inputs[1],
                                    store_ty.clone(),
                                ) {
                                PreHirLValue::Var(slot_name)
                            } else if let Some(global_lvalue) =
                                this.try_global_memory_lvalue(op, &op.inputs[1], store_ty.clone())
                            {
                                global_lvalue
                            } else {
                                PreHirLValue::Deref {
                                    ptr: Box::new(
                                        this.lower_memory_pointer(
                                            &op.inputs[1],
                                            &mut HashSet::default(),
                                        )
                                        .map_err(
                                            |err| {
                                                this.debug_lowering_error(
                                                    "store_ptr",
                                                    block.start_address,
                                                    u64::from(op.seq_num),
                                                    op.opcode,
                                                    &err,
                                                );
                                                err
                                            },
                                        )?,
                                    ),
                                    ty: store_ty.clone(),
                                }
                            };
                            let rhs = if let Some(expr) = this
                                .recover_aggregate_store_rhs_from_block(
                                    block,
                                    op_idx,
                                    &op.inputs[2],
                                )? {
                                expr
                            } else if let PreHirLValue::Var(slot_name) = &lhs
                                && let Some(expr) = this.stack_home_accumulator_store_rhs(
                                    block,
                                    op_idx,
                                    op,
                                    slot_name,
                                    &op.inputs[2],
                                )
                            {
                                expr
                            } else {
                                this.lower_varnode(&op.inputs[2], &mut HashSet::default())
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
                            // If the lowered RHS is float-typed but the lvalue
                            // was sized as int (copy/zext chain), align Deref ty.
                            let lhs = match lhs {
                                PreHirLValue::Deref { ptr, ty }
                                    if matches!(
                                        (expr_type(&rhs), &ty),
                                        (NirType::Float { bits: rb }, NirType::Int { bits: ib, .. })
                                            if rb == *ib
                                    ) =>
                                {
                                    PreHirLValue::Deref {
                                        ptr,
                                        ty: expr_type(&rhs),
                                    }
                                }
                                other => other,
                            };
                            let _ = store_ty;
                            Ok(Some(PreHirStmt::Assign { lhs, rhs }))
                        }
                        PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther => {
                            if this.call_is_return_target_artifact(block, op_idx)
                                || this.call_is_terminal_branchind_artifact(block, op_idx)
                                || this.callother_is_same_instruction_call_marker(block, op_idx)
                                || this.callother_is_guarded_trap_marker(block, op_idx)
                            {
                                return Ok(None);
                            }
                            if op.output.is_none() {
                                let recovered_args =
                                    if op.opcode == PcodeOpcode::CallOther || op.inputs.len() > 1 {
                                        None
                                    } else {
                                        this.recover_call_args_from_block(block, op_idx)?
                                    };
                                let expr = this
                                    .lower_call(op, recovered_args, &mut visiting)
                                    .map_err(|err| {
                                        this.debug_lowering_error(
                                            "call_expr",
                                            block.start_address,
                                            u64::from(op.seq_num),
                                            op.opcode,
                                            &err,
                                        );
                                        err
                                    })?;
                                if op.opcode != PcodeOpcode::CallOther
                                    && this.call_result_is_observed(block, op_idx)
                                {
                                    let lhs = PreHirLValue::Var(
                                        this.ensure_call_result_binding(site, op),
                                    );
                                    Ok(Some(PreHirStmt::Assign { lhs, rhs: expr }))
                                } else {
                                    Ok(Some(PreHirStmt::Expr(expr)))
                                }
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
                },
            )?;
            if let Some(stmt) = maybe_stmt {
                // Skip pure `x = x` when multiple p-code ops share one binding.
                if !crate::midend::is_identity_var_assign_stmt(&stmt) {
                    body.push(stmt);
                }
            }
            op_idx += 1;
        }
        Ok(body)
    }

    fn lowering_block_index(&self, block: &crate::pcode::PcodeBasicBlock) -> usize {
        let indexed = block.index as usize;
        if self.pcode.blocks.get(indexed).is_some_and(|candidate| {
            candidate.start_address == block.start_address && candidate.ops.len() == block.ops.len()
        }) {
            return indexed;
        }
        self.address_to_index
            .get(&block.start_address)
            .copied()
            .unwrap_or(0)
    }

    /// Whether this op's output names a location in memory rather than a
    /// register or a temporary.
    ///
    /// SLEIGH lowers an absolute-addressed write (`mov dword ptr [rip+N], 0`)
    /// to an ordinary op whose *output varnode lives in ram space* -- not to a
    /// `Store`. The runtime numbers that space 3, which collides with the
    /// legacy engine's unique space, so such an output reaches the generic
    /// path and is judged by the same "does anything read it?" rules that
    /// govern a temporary. Nothing reads a global inside the function that
    /// writes it, so the write was dropped: 64 of 65 such stores across the
    /// benchmark's binaries never reached the emitted C at all.
    ///
    /// The offset is the discriminator the space id cannot be: a legacy unique
    /// offset is a small slot index, while this is an address the loader
    /// actually mapped, and one no legacy unique register name claims.
    fn output_is_mapped_global_memory(&self, output: &Varnode) -> bool {
        !output.is_constant
            && output.space_id == UNIQUE_SPACE_ID
            && self.pcode_uses_runtime_space_numbering()
            && self.options.is_mapped_global(output.offset)
    }

    /// Whether this function's p-code came from the rust-sleigh runtime.
    ///
    /// Decided from the space ids the op stream actually uses: the runtime
    /// numbers unique 2 and registers 4/5, the legacy engine unique 3 and
    /// registers 1. Either sighting settles it, and space 3 then means ram
    /// rather than unique. Offsets cannot stand in for this -- a PIE image
    /// maps sections from a low base, so `is_mapped_global` is true of the
    /// runtime's own unique offsets too.
    fn pcode_uses_runtime_space_numbering(&self) -> bool {
        if let Some(known) = self.runtime_space_numbering.get() {
            return known;
        }
        let runtime = self.pcode.blocks.iter().any(|block| {
            block.ops.iter().any(|op| {
                op.output.iter().chain(op.inputs.iter()).any(|vn| {
                    !vn.is_constant
                        && (vn.space_id == RUST_SLEIGH_UNIQUE_SPACE_ID
                            || vn.space_id == RUST_SLEIGH_REGISTER_SPACE_ID
                            || vn.space_id == RUST_SLEIGH_ALT_REGISTER_SPACE_ID)
                })
            })
        });
        self.runtime_space_numbering.set(Some(runtime));
        runtime
    }

    fn output_is_read_by_phi(&mut self, block_idx: Option<usize>, op_idx: usize) -> bool {
        let Some(block_idx) = block_idx else {
            return false;
        };
        if self.phi_operand_values.is_none() {
            // Only phis whose own output is actually consumed. Ghidra places
            // `MULTIEQUAL` on a liveness-pruned frontier, so a value that dies
            // at the merge is never a phi operand there; ours places one for
            // every storage crossing the merge, so a pure branch condition --
            // read by its own CBranch and dead after -- looks like a phi
            // operand and would be forced explicit for nothing, which costs
            // short-circuit folding.
            // A phi is *needed* when a real op reads its output, or when a
            // needed phi has it as an operand. Seeded from real reads and run
            // to a fixpoint: counting phi operands as "consumed" up front lets
            // a dead phi keep another dead phi alive, which is how a pure
            // branch condition kept looking live.
            let mut needed: std::collections::HashSet<SsaValueId> =
                std::collections::HashSet::new();
            for pieces in self.scalar_ssa.operation_inputs.values() {
                for piece in pieces {
                    needed.insert(piece.value);
                }
            }
            loop {
                let mut grew = false;
                for phis in self.scalar_ssa.phis.values() {
                    for phi in phis {
                        if !needed.contains(&phi.output) {
                            continue;
                        }
                        for operand in &phi.operands {
                            grew |= needed.insert(operand.value);
                        }
                    }
                }
                if !grew {
                    break;
                }
            }
            let mut set = std::collections::HashSet::new();

            for phis in self.scalar_ssa.phis.values() {
                for phi in phis {
                    if !needed.contains(&phi.output) {
                        continue;
                    }
                    for operand in &phi.operands {
                        set.insert(operand.value);
                    }
                }
            }
            self.phi_operand_values = Some(set);
        }
        let Some(values) = self.phi_operand_values.as_ref() else {
            return false;
        };
        if values.is_empty() {
            return false;
        }
        let key = fission_midend_core::ir::SsaOpSite {
            block: block_idx as u32,
            op: op_idx as u32,
        };
        self.scalar_ssa
            .operation_outputs
            .get(&key)
            .is_some_and(|pieces| pieces.iter().any(|piece| values.contains(&piece.value)))
    }

    /// Ghidra's `max_implied_ref` (`architecture.cc`: "2 is best"). A value
    /// read more times than this gets its own statement and is referred to by
    /// name; below it, it may be inlined into its uses.
    const MAX_IMPLIED_REF: usize = 2;

    /// Whether the definition at this site must keep a statement purely
    /// because too many places read the value it produces.
    ///
    /// This is the half of Ghidra's explicit/implied rule Fission never had.
    /// The existing completeness proofs count uses *inside one block*
    /// (`output_use_sites_in_block`), so a value used once where it is defined
    /// and read by every successor looks inlineable -- and `lower_varnode`,
    /// whose `visiting` set is scoped to one path, then rebuilds it once per
    /// path. That is the duplication the work budgets exist to survive.
    ///
    /// The set is computed once, before any of it is needed, from the SSA
    /// def-use graph -- which is what makes it equivalent to Ghidra's rule.
    /// An earlier attempt counted uses by *storage* (`use_counts`), and a
    /// register holding two hundred values across a function reads as "used
    /// two hundred times" there, which would force every definition explicit.
    fn output_exceeds_implied_ref_limit(&mut self, block_idx: usize, op_idx: usize) -> bool {
        if !Self::use_count_explicit_rule_enabled() {
            return false;
        }
        self.ensure_explicit_def_sites();
        let site = fission_midend_core::ir::SsaOpSite {
            block: block_idx as u32,
            op: op_idx as u32,
        };
        self.explicit_def_sites
            .as_ref()
            .is_some_and(|sites| sites.contains(&site))
    }

    /// The name an explicit definition ships under, deriving and recording it
    /// if this is the first side to ask.
    ///
    /// Lowering reaches a use before the defining block is materialized
    /// whenever control flows backwards, so the name cannot be left for the
    /// materializer to choose.
    pub(in crate::midend) fn explicit_binding_name(
        &mut self,
        block_idx: usize,
        op_idx: usize,
        output: &Varnode,
    ) -> String {
        let key = (block_idx, op_idx, VarnodeKey::from(output));
        if let Some(name) = self.materialized_output_names.get(&key) {
            return name.clone();
        }
        let base = self
            .sla_hw_name(output.offset, output.size)
            .unwrap_or_else(|| format!("tmp_{:x}", output.offset));
        let name = self.ensure_live_register_binding(&base, output.size);
        self.materialized_output_names.insert(key, name.clone());
        name
    }

    /// Whether the definition at this site was settled as explicit.
    pub(in crate::midend) fn def_site_is_explicit(
        &mut self,
        block_idx: usize,
        op_idx: usize,
    ) -> bool {
        if !Self::use_count_explicit_rule_enabled() {
            return false;
        }
        self.ensure_explicit_def_sites();
        let site = fission_midend_core::ir::SsaOpSite {
            block: block_idx as u32,
            op: op_idx as u32,
        };
        self.explicit_def_sites
            .as_ref()
            .is_some_and(|sites| sites.contains(&site))
    }

    /// Ghidra's `ActionMarkExplicit`, as a pass rather than a question asked
    /// mid-lowering: every SSA value's readers are counted once, and the
    /// definitions that exceed `MAX_IMPLIED_REF` are settled before the first
    /// expression is built.
    fn ensure_explicit_def_sites(&mut self) {
        if self.explicit_def_sites.is_some() {
            return;
        }
        let mut reads: HashMap<fission_midend_core::ir::SsaValueId, usize> = HashMap::default();
        for pieces in self.scalar_ssa.operation_inputs.values() {
            for piece in pieces {
                *reads.entry(piece.value).or_insert(0) += 1;
            }
        }
        let mut sites = std::collections::HashSet::new();
        for value in &self.scalar_ssa.values {
            let fission_midend_core::ir::SsaValueDefinition::Operation(site) = value.definition
            else {
                continue;
            };
            if reads.get(&value.id).copied().unwrap_or(0) > Self::MAX_IMPLIED_REF {
                sites.insert(site);
            }
        }
        self.explicit_def_sites = Some(sites);
    }

    fn use_count_explicit_rule_enabled() -> bool {
        static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *ENABLED.get_or_init(|| {
            matches!(
                std::env::var("FISSION_USE_COUNT_EXPLICIT"),
                Ok(value) if matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES")
            )
        })
    }

    fn maybe_materialize_output_stmt(
        &mut self,
        block_addr: u64,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        terminator_index: Option<usize>,
        op: &PcodeOp,
    ) -> Result<Option<PreHirStmt>, MlilPreviewError> {
        let Some(output) = &op.output else {
            return Ok(None);
        };
        // A write to memory is observable whether or not this function reads
        // it back, so it is decided before every suppression rule below --
        // all of which reason about a value's consumers.
        if self.output_is_mapped_global_memory(output) {
            // Keep the memory identity in the IR. A bare `Var(name)` reaches
            // `rescue_undeclared_bindings`, which correctly makes ordinary
            // undeclared names Temp locals but thereby turns this global write
            // into an automatic local. `AddressOfGlobal` is the existing
            // provenance-bearing representation used by the Store path.
            let name = self
                .options
                .global_names
                .get(&output.offset)
                .cloned()
                .unwrap_or_else(|| format!("tmp_{:x}", output.offset));
            if let Some(rhs) = self.try_lower_materialized_output_rhs(block_addr, op)? {
                return Ok(Some(PreHirStmt::Assign {
                    lhs: PreHirLValue::Deref {
                        ptr: Box::new(PreHirExpr::AddressOfGlobal(name)),
                        ty: type_from_size(output.size, false),
                    },
                    rhs,
                }));
            }
        }
        if self.output_used_only_as_stack_return_target(block, op_idx, terminator_index, op, output)
        {
            return Ok(None);
        }
        // Ordinary `sub/add rsp, N` prologue/epilogue adjustments are always
        // invisible bookkeeping (their effect is fully captured by the
        // stack-slot addressing model elsewhere), so this stays suppressed
        // by default. The one exception -- see `block_writes_rsp_register`
        // in stack_slots.rs for the full rationale and false-positive
        // history (calls' own implicit rsp bookkeeping) -- is a call-free,
        // self-looping block's own direct arithmetic write to rsp: GCC/
        // Linux's inlined stack-probe loop for a >1-page frame (`sub
        // rsp,0x1000; or [rsp],0; cmp rsp,r11; jnz`) re-adjusts rsp on
        // every iteration, so a later bare read of rsp (the loop's own exit
        // condition) needs a real, visible value to compare against --
        // suppressing it here left `while (rsp != r11)` with nothing in its
        // body ever able to make that true. Confirmed on a real
        // GCC-compiled >16KB-local fixture.
        if self.output_is_stack_pointer_register(output)
            && !self.block_writes_rsp_register(self.lowering_block_index(block))
        {
            return Ok(None);
        }
        // Stack-adjust flag soup (`sub rsp, N` sets ZF/CF/…): never program
        // predicates. Materializing them yields undeclared `rsp` compares.
        if self.flag_def_is_stack_pointer_only(op, output) {
            return Ok(None);
        }
        if self.output_used_only_by_single_store(block, op_idx, output) {
            return Ok(None);
        }
        if Self::is_predicate_passthrough_to_terminator(op)
            && self.output_used_only_by_block_terminator(block, op_idx, terminator_index, output)
        {
            return Ok(None);
        }
        let loop_carried_lhs_name = self
            .loop_carried_output_binding_name(block, op_idx, op, output)
            .or_else(|| {
                self.loop_carried_passthrough_output_binding_name(block, op_idx, op, output)
            })
            .or_else(|| self.loop_head_phi_latch_binding_name(block, op_idx, output));
        if loop_carried_lhs_name.is_none()
            && self.output_used_only_by_passthrough_chain(block, op_idx, output)
        {
            return Ok(None);
        }
        let block_idx_for_rhs = self.address_to_index.get(&block.start_address).copied();
        let scoped_loop_keys = loop_carried_lhs_name.as_ref().and_then(|name| {
            let block_idx = block_idx_for_rhs?;
            let output_key = VarnodeKey::from(output);
            let mut keys = vec![output_key.clone()];
            for input in &op.inputs {
                if input.is_constant {
                    continue;
                }
                let input_key = VarnodeKey::from(input);
                if input_key != output_key
                    && Self::varnode_key_may_alias_output(&input_key, &output_key)
                {
                    keys.push(input_key);
                }
            }
            keys.sort_by_key(|key| (key.space_id, key.offset, key.size));
            keys.dedup();
            let previous = keys
                .into_iter()
                .map(|key| {
                    let scoped_key = (block_idx, key);
                    let previous = self
                        .explicit_merge_bindings
                        .insert(scoped_key.clone(), name.clone());
                    (scoped_key, previous)
                })
                .collect::<Vec<_>>();
            self.invalidate_materialization_dependent_caches();
            Some(previous)
        });
        let rhs = if scoped_loop_keys.is_some()
            && let Some(block_idx) = block_idx_for_rhs
        {
            self.with_lowering_site(LoweringSite { block_idx, op_idx }, |this| {
                this.try_lower_materialized_output_rhs(block_addr, op)
            })
        } else {
            self.try_lower_materialized_output_rhs(block_addr, op)
        };
        if let Some(previous_bindings) = scoped_loop_keys {
            for (key, previous) in previous_bindings {
                if let Some(previous) = previous {
                    self.explicit_merge_bindings.insert(key, previous);
                } else {
                    self.explicit_merge_bindings.remove(&key);
                }
            }
            self.invalidate_materialization_dependent_caches();
        }
        let Some(rhs) = rhs? else {
            return Ok(None);
        };
        let legacy_inline_candidate =
            self.output_replacement_is_complete(block, op_idx, output, &rhs);
        let replacement_plan =
            self.build_replacement_value_plan(block, op_idx, terminator_index, output, &rhs);
        let direct_successor_merge_lhs_name =
            self.merge_binding_name_for_direct_successor_accumulator(block, op_idx, output, &rhs);
        let merge_lhs_name = if loop_carried_lhs_name.is_none() {
            self.merge_binding_name_for_materialized_output(block, op_idx, output, &rhs)
        } else {
            None
        };
        let primary_return_live_out_name = block_idx_for_rhs
            .and_then(|block_idx| {
                self.prove_definition_reaches_return(block_idx, op_idx, output)
                    .map(|proof| (block_idx, proof))
            })
            .and_then(|(block_idx, proof)| {
                self.primary_return_name_from_live_out_proof(output, block_idx, op_idx, proof)
            });
        if replacement_plan.is_complete()
            && !block_idx_for_rhs
                .is_some_and(|block_idx| self.output_exceeds_implied_ref_limit(block_idx, op_idx))
            && loop_carried_lhs_name.is_none()
            && merge_lhs_name.is_none()
            && !self.output_is_read_by_phi(block_idx_for_rhs, op_idx)
            // SLEIGH Return inputs are control/stack targets, so same-block
            // consumer analysis under-counts ABI live-out. Preserve the
            // binding only when this exact definition has a kill-free path to
            // a machine return; unrelated register roles may still be inlined.
            && primary_return_live_out_name.is_none()
        {
            self.trace_materialization_plan(
                block_addr,
                op,
                output,
                &rhs,
                replacement_plan,
                "representative_downgrade",
            );
            self.telemetry
                .materialization
                .representative_downgrade_count += 1;
            return Ok(None);
        }
        let no_consumer_profile =
            self.analyze_no_consumer_materialization_profile(block, op_idx, output, &rhs);
        let no_consumer_hazard = if replacement_plan.rejection_reason()
            == Some(MaterializationRejectionReason::AliasUnsafe)
        {
            Some(Self::classify_alias_unsafe_hazard(
                block,
                op_idx,
                terminator_index,
                output,
                &rhs,
            ))
        } else {
            None
        };
        let no_consumer_decision = Self::classify_no_consumer_materialization_decision(
            output,
            &rhs,
            legacy_inline_candidate,
            replacement_plan,
            no_consumer_hazard,
            no_consumer_profile,
            self.is_hidden_pspec_register_output(output),
        );
        match no_consumer_decision {
            NoConsumerMaterializationDecision::Suppress
            | NoConsumerMaterializationDecision::SuppressAlways => {
                let suppression_enabled = matches!(
                    no_consumer_decision,
                    NoConsumerMaterializationDecision::SuppressAlways
                ) || Self::no_consumer_suppression_enabled();
                self.trace_no_consumer_materialization(
                    block_addr,
                    op.seq_num,
                    if suppression_enabled {
                        "suppressed"
                    } else {
                        "suppression_candidate"
                    },
                    output,
                    &rhs,
                    Self::should_preserve_materialized_expr(&rhs),
                    legacy_inline_candidate,
                    no_consumer_profile,
                );
                self.trace_no_consumer_suppression_detail(
                    block,
                    op_idx,
                    output,
                    &rhs,
                    suppression_enabled,
                );
                if suppression_enabled
                    && merge_lhs_name.is_none()
                    && direct_successor_merge_lhs_name.is_none()
                {
                    self.trace_no_consumer_suppressed(block_addr, op.seq_num, output, &rhs);
                    return Ok(None);
                }
                self.trace_no_consumer_kept(
                    block_addr,
                    op.seq_num,
                    output,
                    &rhs,
                    NoConsumerMaterializationKeepReason::SuppressionDisabled,
                );
            }
            NoConsumerMaterializationDecision::Keep(reason) => {
                if reason != NoConsumerMaterializationKeepReason::NotUnknownNoConsumerFound {
                    self.trace_no_consumer_materialization(
                        block_addr,
                        op.seq_num,
                        "kept",
                        output,
                        &rhs,
                        Self::should_preserve_materialized_expr(&rhs),
                        legacy_inline_candidate,
                        no_consumer_profile,
                    );
                    self.trace_no_consumer_kept(block_addr, op.seq_num, output, &rhs, reason);
                }
            }
        }
        if legacy_inline_candidate {
            self.telemetry
                .materialization
                .materialization_inline_suppressed_count += 1;
            self.trace_materialization_plan(
                block_addr,
                op,
                output,
                &rhs,
                replacement_plan,
                "inline_suppressed",
            );
        } else {
            self.trace_materialization_plan(
                block_addr,
                op,
                output,
                &rhs,
                replacement_plan,
                "materialized_binding",
            );
        }
        let preserve_materialization = Self::should_preserve_materialized_expr(&rhs);
        let name_claim_is_safe = |this: &Self, name: &str| {
            block_idx_for_rhs.is_none_or(|bi| {
                !this.cover_proves_existing_name_claim_interferes(bi, op_idx, output, name)
            })
        };
        let mut lhs_name_is_proven_loop_carried = false;
        let mut lhs_name = if let Some(name) =
            loop_carried_lhs_name.filter(|n| name_claim_is_safe(self, n))
        {
            self.seed_loop_carried_binding_initializer_from_edge_zero(block, output, &name);
            self.bind_materialized_output_to_existing_name(
                op,
                output,
                &name,
                preserve_materialization,
            );
            lhs_name_is_proven_loop_carried = true;
            name
        } else if let Some(name) =
            direct_successor_merge_lhs_name.filter(|n| name_claim_is_safe(self, n))
        {
            self.bind_materialized_output_to_existing_name(
                op,
                output,
                &name,
                preserve_materialization,
            );
            name
        } else if let Some(name) = merge_lhs_name.filter(|n| name_claim_is_safe(self, n)) {
            self.bind_materialized_output_to_existing_name(
                op,
                output,
                &name,
                preserve_materialization,
            );
            name
        } else if let Some(name) = self
            .full_width_primary_return_surface_name(block, op_idx, op, output)
            .filter(|n| name_claim_is_safe(self, n))
        {
            // A full-width primary-return extension must establish the ABI
            // surface when it overwrites an observed call carrier. Reusing the
            // narrower lane's binding leaves later cross-block RAX reads able
            // to select the pre-extension call result instead of this value.
            self.ensure_live_register_binding(&name, self.options.pointer_size);
            self.bind_materialized_output_to_existing_name(
                op,
                output,
                &name,
                preserve_materialization,
            );
            name
        } else if let Some((name, binding_size)) = self
            .live_register_lhs_name_for_partial_gpr_join_family(output)
            .filter(|(n, _)| name_claim_is_safe(self, n))
        {
            self.ensure_live_register_binding(&name, binding_size);
            self.bind_materialized_output_to_existing_name(op, output, &name, true);
            name
        } else if let Some((name, binding_size)) = self
            .live_register_lhs_name_for_passthrough_join_store_producer(block, op_idx, output, &rhs)
            .filter(|(n, _)| name_claim_is_safe(self, n))
        {
            self.ensure_live_register_binding(&name, binding_size);
            self.bind_materialized_output_to_existing_name(op, output, &name, true);
            name
        } else if let Some((name, binding_size)) = self
            .live_register_lhs_name_for_safe_missing_merge(
                block,
                op_idx,
                op,
                output,
                &rhs,
                replacement_plan,
            )
            .filter(|(n, _)| name_claim_is_safe(self, n))
        {
            self.ensure_live_register_binding(&name, binding_size);
            self.bind_materialized_output_to_existing_name(op, output, &name, true);
            name
        } else if let Some(name) =
            self.same_block_prior_register_binding_name(block, op_idx, output)
        {
            // Reuse the prior same-block binding for register redefs (cmov default
            // + overrides on EAX/RAX). Without this, each write to the primary
            // return register gets a fresh temp and the cmov chain cannot compose.
            // When the prior is a full-width primary-return surface (rax) and this
            // write is EAX, alias join already returns that name.
            self.bind_materialized_output_to_existing_name(
                op,
                output,
                &name,
                preserve_materialization,
            );
            name
        } else if primary_return_live_out_name.is_some()
            && !Self::output_has_consumed_interval_before_redefinition(block, op_idx, output)
            // ARM/AArch64 r0/x0 are both param and return; forcing the HW name
            // there collapses call-arg vs result identity. x86 eax/rax is
            // return-only under the active CCs.
            && !self
                .register_namer()
                .register_name_with_param_owned(output.offset, output.size)
                .is_some_and(|(_, idx)| idx.is_some())
            // Always prefer the ABI return register surface for non-param
            // primary-return writes so sum/cmov arms share one name with
            // `return eax`/`return rax`. Normalize folds adjacent
            // `eax = C; return eax` via collapse_trivial_assign_returns.
            && let Some(name) =
                primary_return_live_out_name.filter(|n| name_claim_is_safe(self, n))
        {
            // Cross-block cmov tails (e.g. saturating_add underflow): the guarded
            // Copy writes the ABI return register with no prior def in *this*
            // block. A fresh uVar is dead after epilogue `return eax` recovery
            // and is stripped by eliminate_dead_temp_assigns — keep the HW name.
            self.ensure_live_register_binding(&name, output.size);
            self.bind_materialized_output_to_existing_name(
                op,
                output,
                &name,
                preserve_materialization,
            );
            name
        } else {
            let fallback_name = self
                .ensure_temp_binding_for_output(op, output, preserve_materialization)
                .name;
            fallback_name
        };
        // A name proven loop-carried (`prove_loop_carried_register_update`)
        // already established that THIS definition is the update feeding the
        // next iteration's read of the same register -- e.g. `cur = cur->next`
        // reusing `cur`'s name for the reloaded pointer. The load-address-role
        // check below exists to catch a *different*, unproven case (a name
        // used as a load address elsewhere getting silently repurposed for an
        // unrelated raw-integer value), and doesn't know about that proof; it
        // would otherwise strip the correct name into a fresh temp because the
        // reloaded pointer's p-code type is still a plain integer (`Node *`
        // isn't recognized as `Ptr` until later type inference), leaving the
        // loop's induction variable never reassigned -- an infinite loop at
        // runtime, not merely a readability regression.
        if !lhs_name_is_proven_loop_carried
            && self.materialized_lhs_conflicts_with_load_address_role(&lhs_name, &rhs)
        {
            lhs_name = self.bind_materialized_output_to_fresh_temp(
                op,
                output,
                expr_type(&rhs),
                preserve_materialization,
            );
        }
        self.record_load_value_roles(&lhs_name, &rhs);
        if self.emit_ready_trace_enabled_for_current_fn() {
            self.emit_ready_trace(format!(
                "materialized-output-binding block=0x{:x} op_seq={} output=space:{} off:0x{:x} size:{} lhs={} rhs={:?}",
                block_addr,
                op.seq_num,
                output.space_id,
                output.offset,
                output.size,
                lhs_name,
                rhs,
            ));
        }
        // One name per def site, first writer wins. A use lowered before this
        // block reaches materialization derives the name itself
        // (`explicit_binding_name`); whichever side is first, both must spell
        // the value the same way or the definition ships under one name and
        // its readers reference another.
        let mut lhs_name = lhs_name;
        if Self::use_count_explicit_rule_enabled() {
            let key = (
                self.lowering_block_index(block),
                op_idx,
                VarnodeKey::from(output),
            );
            match self.materialized_output_names.get(&key) {
                Some(existing) => lhs_name = existing.clone(),
                None => {
                    self.materialized_output_names.insert(key, lhs_name.clone());
                }
            }
        }
        let lhs = PreHirLValue::Var(lhs_name);
        Ok(Some(PreHirStmt::Assign { lhs, rhs }))
    }

    fn merge_binding_name_for_materialized_output(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &PreHirExpr,
    ) -> Option<String> {
        let block_idx = self.lowering_block_index(block);
        let key = VarnodeKey::from(output);
        for succ_idx in self.successors.get(block_idx)? {
            let Some(reach_proof) =
                self.prove_definition_reaches_block_entry(block_idx, op_idx, output, *succ_idx)
            else {
                continue;
            };
            debug_assert_eq!(reach_proof.definition_site(), (block_idx, op_idx));
            debug_assert_eq!(reach_proof.target_block(), *succ_idx);
            if let Some(name) = self.explicit_merge_bindings.get(&(*succ_idx, key.clone())) {
                return Some(name.clone());
            }
        }
        if let Some(name) =
            self.merge_binding_name_for_direct_successor_accumulator(block, op_idx, output, rhs)
        {
            return Some(name);
        }

        let proof = self.describe_merge_binding_candidate_proof(block, op_idx, output, rhs)?;
        let duplicate_start = self.duplicate_start_merge_block(proof.merge_block);
        if !self.merge_binding_proof_allows_predecessor_assignment(&proof, duplicate_start) {
            return None;
        }
        let (merge_idx, merge_addr, _, _) =
            self.first_output_use_site_outside_block_by_index(block_idx, op_idx, output)?;
        let reach_proof =
            self.prove_definition_reaches_block_entry(block_idx, op_idx, output, merge_idx)?;
        debug_assert_eq!(reach_proof.definition_site(), (block_idx, op_idx));
        debug_assert_eq!(reach_proof.target_block(), merge_idx);
        if merge_addr == proof.merge_block
            && let Some(name) = self.explicit_merge_bindings.get(&(merge_idx, key.clone()))
        {
            return Some(name.clone());
        }
        if merge_addr != proof.merge_block
            || !self
                .successors
                .get(block_idx)
                .is_some_and(|succs| succs.contains(&merge_idx))
        {
            return None;
        }
        let binding = self.ensure_explicit_merge_binding_for_block(merge_idx, output);
        self.trace_explicit_merge_binding_trial(
            proof.merge_block,
            output,
            &[],
            &[],
            &proof.incoming_value_kinds,
            proof.rhs_kind,
            &binding.name,
            true,
            ExplicitMergeBindingTrialReason::PhiLikeBindingMaterialized,
        );
        Some(binding.name)
    }

    fn loop_body_has_side_entry_or_irreducible_edge(
        &self,
        loop_body: &crate::midend::structuring::loop_analysis::LoopBody,
    ) -> bool {
        let body = loop_body.body.iter().copied().collect::<HashSet<_>>();
        for block_idx in &loop_body.body {
            if self
                .predecessors
                .get(*block_idx)
                .into_iter()
                .flatten()
                .any(|pred| !body.contains(pred) && *block_idx != loop_body.head)
            {
                return true;
            }
        }
        self.irreducible_edges
            .iter()
            .any(|(from, to)| body.contains(from) || body.contains(to))
    }

    fn pred_path_has_live_accumulator_def(
        &self,
        pred_idx: usize,
        target_idx: usize,
        loop_body: &crate::midend::structuring::loop_analysis::LoopBody,
        family_idx: usize,
        conservative_mem_check: bool,
    ) -> bool {
        let body = loop_body.body.iter().copied().collect::<HashSet<_>>();
        let mut visiting = HashSet::default();
        self.pred_path_has_live_accumulator_def_inner(
            pred_idx,
            target_idx,
            &body,
            family_idx,
            0,
            &mut visiting,
            conservative_mem_check,
        )
    }

    fn pred_path_has_live_accumulator_def_inner(
        &self,
        idx: usize,
        target_idx: usize,
        loop_body: &HashSet<usize>,
        family_idx: usize,
        depth: usize,
        visiting: &mut HashSet<usize>,
        conservative_mem_check: bool,
    ) -> bool {
        if depth > 8 || idx == target_idx || !loop_body.contains(&idx) || !visiting.insert(idx) {
            return false;
        }
        let result = self.pcode.blocks.get(idx).is_some_and(|block| {
            let has_side_effect =
                |block: &crate::pcode::PcodeBasicBlock, start: usize, end: usize| {
                    if conservative_mem_check {
                        self.has_aliasing_side_effect_between_ops(block, start, end)
                    } else {
                        self.has_call_between_ops(block, start, end)
                    }
                };
            if let Some(def_idx) = self.last_x86_gpr_family_definition(block, family_idx) {
                return !has_side_effect(block, def_idx + 1, block.ops.len());
            }
            if has_side_effect(block, 0, block.ops.len()) {
                return false;
            }
            let incoming = self
                .predecessors
                .get(idx)
                .into_iter()
                .flatten()
                .copied()
                .filter(|pred| *pred != target_idx && loop_body.contains(pred))
                .collect::<Vec<_>>();
            !incoming.is_empty()
                && incoming.into_iter().all(|pred| {
                    self.pred_path_has_live_accumulator_def_inner(
                        pred,
                        target_idx,
                        loop_body,
                        family_idx,
                        depth + 1,
                        visiting,
                        conservative_mem_check,
                    )
                })
        });
        visiting.remove(&idx);
        result
    }

    fn loop_body_has_live_accumulator_def(
        &self,
        loop_body: &crate::midend::structuring::loop_analysis::LoopBody,
        family_idx: usize,
    ) -> bool {
        loop_body.body.iter().any(|idx| {
            self.pcode
                .blocks
                .get(*idx)
                .and_then(|block| self.last_x86_gpr_family_definition(block, family_idx))
                .is_some()
        })
    }

    fn last_x86_gpr_family_definition(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        family_idx: usize,
    ) -> Option<usize> {
        block.ops.iter().enumerate().rev().find_map(|(idx, op)| {
            let output = op.output.as_ref()?;
            let (_, output_family) = self.canonical_x86_gpr64_name_for_value(output)?;
            (output_family == family_idx && Self::output_def_is_safe_direct_successor_merge(op))
                .then_some(idx)
        })
    }

    fn x86_gpr_definition_is_zero_in_block(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        budget: usize,
    ) -> bool {
        if budget == 0 {
            return false;
        }
        let Some(op) = block.ops.get(op_idx) else {
            return false;
        };
        match op.opcode {
            PcodeOpcode::Copy => op
                .inputs
                .first()
                .is_some_and(|input| input.is_constant && input.constant_val == 0),
            PcodeOpcode::Cast
            | PcodeOpcode::IntZExt
            | PcodeOpcode::IntSExt
            | PcodeOpcode::SubPiece => op.inputs.first().is_some_and(|input| {
                input.is_constant && input.constant_val == 0
                    || self.value_has_prior_zero_def_in_block(block, op_idx, input, budget - 1)
            }),
            PcodeOpcode::IntXor if op.inputs.len() >= 2 => {
                self.varnode_aliases_value(&op.inputs[0], &op.inputs[1])
            }
            _ => false,
        }
    }

    fn value_has_prior_zero_def_in_block(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        before_idx: usize,
        value: &Varnode,
        budget: usize,
    ) -> bool {
        if budget == 0 {
            return false;
        }
        block.ops[..before_idx.min(block.ops.len())]
            .iter()
            .enumerate()
            .rev()
            .find_map(|(idx, candidate)| {
                candidate
                    .output
                    .as_ref()
                    .is_some_and(|output| self.varnode_aliases_value(output, value))
                    .then_some(idx)
            })
            .is_some_and(|idx| self.x86_gpr_definition_is_zero_in_block(block, idx, budget - 1))
    }

    fn has_aliasing_side_effect_between_ops(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        start: usize,
        end: usize,
    ) -> bool {
        block.ops[start..end.min(block.ops.len())].iter().any(|op| {
            if matches!(op.opcode, PcodeOpcode::Load | PcodeOpcode::Store) {
                if let Some(ptr) = op.inputs.get(1) {
                    if let Some(sh_ptr) = &self.current_stack_home_ptr {
                        if self.memory_ops_may_alias(ptr, sh_ptr) {
                            return true;
                        }
                    } else {
                        return false;
                    }
                }
                false
            } else {
                matches!(
                    op.opcode,
                    PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther
                )
            }
        })
    }

    fn memory_ops_may_alias(&self, ptr1: &Varnode, ptr2: &Varnode) -> bool {
        if VarnodeKey::from(ptr1) == VarnodeKey::from(ptr2) {
            return true;
        }
        let addr1 = self.resolve_stack_address(ptr1);
        let addr2 = self.resolve_stack_address(ptr2);
        match (addr1, addr2) {
            (Some((base1, offset1)), Some((base2, offset2))) => {
                base1 == base2 && offset1 == offset2
            }
            _ => false,
        }
    }

    fn has_call_between_ops(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        start: usize,
        end: usize,
    ) -> bool {
        let res = block.ops[start..end.min(block.ops.len())].iter().any(|op| {
            if matches!(op.opcode, PcodeOpcode::Call | PcodeOpcode::CallInd) {
                let mut target_name = None;
                if op.opcode == PcodeOpcode::Call {
                    if let Some(name) = self.options.relocation_names.get(&op.address) {
                        target_name = Some(name.as_str());
                    }
                }
                if target_name.is_none() {
                    if let Some(target_vn) = op.inputs.first() {
                        if target_vn.is_constant {
                            let addr = if target_vn.offset != 0 {
                                target_vn.offset
                            } else {
                                target_vn.constant_val as u64
                            };
                            if let Some(ctx) = self.type_context {
                                if let Some(target_ref) = ctx.call_target_refs.get(&addr) {
                                    target_name = Some(target_ref.symbol.as_str());
                                }
                            }
                        }
                    }
                }
                if let Some(name) = target_name {
                    if Self::materialize_call_target_is_known_pure_intrinsic(name) {
                        return false;
                    }
                }
                true
            } else {
                false
            }
        });
        res
    }

    fn varnode_known_const_zero(&self, value: &Varnode, budget: usize) -> bool {
        if value.is_constant {
            return value.constant_val == 0;
        }
        if budget == 0 {
            return false;
        }
        let Some((_, op)) = self.lookup_def_site(value) else {
            return false;
        };
        match op.opcode {
            PcodeOpcode::Copy
            | PcodeOpcode::Cast
            | PcodeOpcode::IntZExt
            | PcodeOpcode::IntSExt
            | PcodeOpcode::SubPiece => op
                .inputs
                .first()
                .is_some_and(|input| self.varnode_known_const_zero(input, budget - 1)),
            PcodeOpcode::IntXor if op.inputs.len() >= 2 => {
                self.varnode_aliases_value(&op.inputs[0], &op.inputs[1])
            }
            _ => false,
        }
    }

    fn merge_binding_proof_allows_predecessor_assignment(
        &self,
        proof: &MergeBindingCandidateProof,
        duplicate_start: bool,
    ) -> bool {
        proof.can_synthesize_phi_like_binding
            && (proof.predecessor_count > 2 || (duplicate_start && proof.predecessor_count == 2))
            && proof.missing_incoming_count == 0
            && proof.conflicting_incoming_count >= 1
            && matches!(
                proof.consumer_kind,
                DisallowedSingleConsumerConsumerKind::OtherData
                    | DisallowedSingleConsumerConsumerKind::Predicate
            )
            && proof.incoming_value_kinds.iter().all(|kind| {
                matches!(
                    kind,
                    MergeBindingCandidateIncomingKind::VarOrConst
                        | MergeBindingCandidateIncomingKind::Arithmetic
                )
            })
    }

    fn duplicate_start_merge_block(&self, merge_block: u64) -> bool {
        self.pcode
            .blocks
            .iter()
            .filter(|block| block.start_address == merge_block)
            .take(2)
            .count()
            >= 2
    }

    fn output_used_only_as_stack_return_target(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        terminator_index: Option<usize>,
        op: &PcodeOp,
        output: &Varnode,
    ) -> bool {
        if op.opcode != PcodeOpcode::Load || op.inputs.len() < 2 {
            return false;
        }
        if !self
            .stack_pointer_register_name(&op.inputs[1])
            .is_some_and(|name| matches!(name.as_str(), "rsp" | "esp" | "sp"))
        {
            return false;
        }
        let Some(term_idx) = terminator_index else {
            return false;
        };
        let Some(term) = block.ops.get(term_idx) else {
            return false;
        };
        term.opcode == PcodeOpcode::Return
            && term.inputs.last().is_some_and(|input| input == output)
            && self
                .output_use_sites_in_block(block, op_idx, output)
                .into_iter()
                .all(|(use_idx, _)| use_idx == term_idx)
    }

    fn output_is_stack_pointer_register(&self, output: &Varnode) -> bool {
        self.stack_pointer_register_name(output)
            .is_some_and(|name| matches!(name.as_str(), "rsp" | "esp" | "sp"))
    }

    /// True when `op` defines a condition flag from stack-pointer arithmetic only
    /// (prologue/epilogue `sub/add rsp` flag noise).
    fn flag_def_is_stack_pointer_only(&self, op: &PcodeOp, output: &Varnode) -> bool {
        if !is_register_space_id(output.space_id) {
            return false;
        }
        // x86 flag-bank offsets used by SLEIGH (CF/PF/AF/ZF/SF/OF-class).
        let is_flag = matches!(output.offset, 0x200 | 0x201 | 0x202 | 0x206 | 0x207 | 0x20b)
            && output.size <= 1;
        if !is_flag {
            return false;
        }
        let mut saw_stack = false;
        for input in &op.inputs {
            if input.is_constant {
                continue;
            }
            if self.output_is_stack_pointer_register(input) {
                saw_stack = true;
                continue;
            }
            // Any non-const, non-stack input means a real data predicate.
            return false;
        }
        saw_stack
    }

    fn is_predicate_passthrough_to_terminator(op: &PcodeOp) -> bool {
        matches!(
            op.opcode,
            PcodeOpcode::BoolNegate
                | PcodeOpcode::BoolAnd
                | PcodeOpcode::BoolOr
                | PcodeOpcode::BoolXor
                | PcodeOpcode::IntEqual
                | PcodeOpcode::IntNotEqual
                | PcodeOpcode::IntLess
                | PcodeOpcode::IntLessEqual
                | PcodeOpcode::IntSLess
                | PcodeOpcode::IntSLessEqual
        )
    }

    pub(in crate::midend::builder) fn block_terminator_index(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
    ) -> Option<usize> {
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

    /// Terminator index for statement materialization.
    ///
    /// Same-block-forward CBranch tails (cmov body after a branch to the next
    /// machine instruction / next BB start) stay in the op stream so
    /// `lower_block_ops_range` can emit `if (!cond) { body }`. Other consumers
    /// of [`Self::block_terminator_index`] keep the raw control-flow op.
    fn materialize_block_terminator_index(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
    ) -> Option<usize> {
        let mut idx = self.block_terminator_index(block)?;
        loop {
            let op = block.ops.get(idx)?;
            let is_tail_cmov = op.opcode == PcodeOpcode::CBranch
                && op.inputs.len() >= 2
                && crate::midend::cfg::same_block_forward_branch_target_op_idx(
                    block,
                    idx,
                    block.ops.len(),
                    op,
                    &op.inputs[0],
                )
                .is_some_and(|target| target > idx + 1);
            if !is_tail_cmov {
                return Some(idx);
            }
            // Walk earlier for a real CFG terminator (branch/return).
            idx = block.ops[..idx].iter().rposition(|candidate| {
                matches!(
                    candidate.opcode,
                    PcodeOpcode::Branch
                        | PcodeOpcode::CBranch
                        | PcodeOpcode::BranchInd
                        | PcodeOpcode::Return
                )
            })?;
        }
    }
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod materialize_tests;

pub(super) fn test_refine_partitions(accesses: &[(i64, u32)]) -> Vec<(i64, u32)> {
    self::incremental::refine_partitions(accesses)
}

/// Expression nodes carried by one statement -- diagnostics only, so a runaway
/// expression can be attributed to the statement that carries it.
#[allow(dead_code)]
fn diag_stmt_expr_size(stmt: &PreHirStmt) -> usize {
    use fission_midend_structuring::structuring_quality::expr_size;
    match stmt {
        PreHirStmt::Assign { rhs, .. } => expr_size(rhs),
        PreHirStmt::Expr(e) | PreHirStmt::Return(Some(e)) => expr_size(e),
        PreHirStmt::If { cond, .. } => expr_size(cond),
        PreHirStmt::While { cond, .. } | PreHirStmt::DoWhile { cond, .. } => expr_size(cond),
        PreHirStmt::Switch { expr, .. } => expr_size(expr),
        _ => 0,
    }
}

/// The name a statement writes to, for the runaway-expression diagnostic.
#[allow(dead_code)]
fn diag_stmt_target(stmt: &PreHirStmt) -> String {
    match stmt {
        PreHirStmt::Assign { lhs, .. } => format!("{lhs:?}"),
        other => format!("{:?}", std::mem::discriminant(other)),
    }
}
