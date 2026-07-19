//! Lowers lifted P-code into preview HIR under [`PreviewBuilder`]: control flow, calls,
//! memory surfaces, and unsupported stubs. Counters feed [`super::ir::NirBuildStats`];
//! do not invent parallel telemetry payloads.
//!
//! Guide: `crates/fission-pcode/src/nir/builder/AGENTS.md`.

pub(super) use super::support::*;
use super::*;
use indexmap::IndexMap;
mod state;
pub(super) use state::PreviewBuilder;

mod calls;
mod control;
mod debug;
mod entry_analysis;
mod expr;
mod init;
mod materialize;
mod memory;
mod stats;
pub(super) mod switch_table;
mod telemetry;
mod type_hints;

use self::debug::preview_builder_diag_enabled;
pub(in crate::midend::builder) use memory::aggregate_recovery;
use tracing::trace_span;

pub(super) fn apply_preview_type_hints(
    func: &mut HirFunction,
    context: &PreviewTypeContext,
) -> PreviewHintStats {
    type_hints::apply_preview_type_hints(func, context)
}

fn seed_callee_summaries_from_type_context(
    context: &PreviewTypeContext,
) -> IndexMap<String, CallSummary> {
    let mut summaries = IndexMap::new();
    for (symbol, prototype) in &context.call_prototype_summaries {
        let target = context
            .call_target_refs
            .values()
            .chain(context.iat_target_refs.values())
            .find(|target| target.symbol == *symbol)
            .cloned()
            .unwrap_or_else(|| CallTargetRef {
                address: None,
                symbol: symbol.clone(),
                provenance: CallTargetProvenance::Reference,
                edge_kind: CallEdgeKind::Reference,
                confidence: 128,
            });
        let effect = context.call_effect_summaries.get(symbol);
        summaries.insert(
            symbol.clone(),
            CallSummary {
                target,
                prototype: PrototypeSummary {
                    min_arity: prototype.min_arity,
                    max_arity: prototype.max_arity,
                    locked_exact_arity: prototype.locked_exact_arity,
                    return_lattice: NirType::Unknown,
                    param_lattices: vec![NirType::Unknown; prototype.max_arity],
                    soundness: SummarySoundness::Optimistic,
                },
                effect_summary: CallEffectSummary {
                    reads_memory: effect.and_then(|summary| summary.reads_memory),
                    writes_memory: effect.and_then(|summary| summary.writes_memory),
                    escapes_args: effect.and_then(|summary| summary.escapes_args),
                    regions: Vec::new(),
                    wrapper_class: WrapperClass::None,
                    wrapper_of: None,
                    confidence: 160,
                },
            },
        );
    }
    summaries
}

#[cfg(test)]
pub(super) fn collect_local_surface_hints(
    body: &[HirStmt],
    pointer_hints: &HashMap<String, PreviewCallParamRule>,
    func: &HirFunction,
    local_hints: &mut HashMap<String, String>,
) {
    let alias_collector = type_hints::StackAliasCollector::new(func);
    type_hints::collect_local_surface_hints(
        body,
        pointer_hints,
        func,
        &alias_collector,
        local_hints,
    );
}

impl<'a> PreviewBuilder<'a> {
    fn binding_name_exists(&self, name: &str) -> bool {
        self.temps.contains_key(name)
            || self.params.values().any(|binding| binding.name == name)
            || self.locals.values().any(|slot| slot.name == name)
    }

    fn next_unused_temp_binding_name(&mut self, ty: &NirType) -> String {
        loop {
            let candidate = next_temp_name(ty, &mut self.temp_next_id);
            if !self.binding_name_exists(&candidate) {
                return candidate;
            }
        }
    }

    pub(super) fn bind_materialized_output_to_fresh_temp(
        &mut self,
        op: &PcodeOp,
        output: &Varnode,
        ty: NirType,
        preserve_materialization: bool,
    ) -> String {
        let name = self.next_unused_temp_binding_name(&ty);
        let origin = if preserve_materialization {
            NirBindingOrigin::TempPreserved
        } else {
            NirBindingOrigin::Temp
        };
        self.temps.insert(
            name.clone(),
            NirBinding {
                name: name.clone(),
                ty,
                surface_type_name: None,
                origin: Some(origin),
                initializer: None,
            },
        );
        if preserve_materialization {
            self.telemetry
                .materialization
                .materialization_stabilized_count += 1;
        }
        self.materialized_vns
            .insert(MaterializedVarnodeKey::new(output, op), name.clone());
        self.invalidate_materialization_dependent_caches();
        name
    }

    /// Resolve a block index (which may be a virtual split node) to the
    /// corresponding P-code block index.  Virtual blocks (index ≥ pcode.blocks.len())
    /// are created by node-splitting and share content with the original block.
    #[inline]
    pub(crate) fn pcode_block_idx(&self, idx: usize) -> usize {
        let original_count = self.pcode.blocks.len();
        if idx < original_count {
            idx
        } else {
            let v_idx = idx - original_count;
            self.virtual_block_map
                .get(v_idx)
                .copied()
                .unwrap_or(idx % original_count.max(1))
        }
    }

    #[inline]
    pub(crate) fn pcode_block(&self, idx: usize) -> &crate::pcode::PcodeBasicBlock {
        &self.pcode.blocks[self.pcode_block_idx(idx)]
    }

    #[inline]
    pub(crate) fn block_start_address(&self, idx: usize) -> u64 {
        self.pcode_block(idx).start_address
    }

    #[inline]
    pub(crate) fn block_count(&self) -> usize {
        self.pcode.blocks.len() + self.virtual_block_map.len()
    }

    fn should_suppress_entry_register_params(&self, name: &str, address: u64) -> bool {
        if is_compiler_runtime_param_suppressed_name(name) {
            return true;
        }
        let lower = name.to_ascii_lowercase();
        if lower == "main" || lower == "wmain" || lower == "winmain" || lower == "wwinmain" {
            return true;
        }
        self.binary
            .is_some_and(|binary| binary.entry_point != 0 && binary.entry_point == address)
    }

    pub(super) fn build_hir(
        &mut self,
        name: &str,
        address: u64,
    ) -> Result<HirFunction, MlilPreviewError> {
        self.current_function_name = Some(name.to_string());
        let _build = trace_span!(
            "preview_build_hir",
            fn_name = name,
            address = address,
            blocks = self.pcode.blocks.len()
        )
        .entered();
        if self.should_suppress_entry_register_params(name, address) {
            self.register_param_aliases.clear();
            self.suppress_entry_register_params = true;
        }
        if self.pcode.blocks.is_empty() {
            return Err(MlilPreviewError::UnsupportedPattern("empty pcode"));
        }

        self.run_incremental_heritage();

        let mut body = Vec::new();
        if self.pcode.blocks.len() == 1 {
            if preview_builder_diag_enabled() {
                eprintln!(
                    "[DIAG] build_hir single_block_start: block=0x{:x} ops={}",
                    self.pcode.blocks[0].start_address,
                    self.pcode.blocks[0].ops.len()
                );
            }
            let block = &self.pcode.blocks[0];
            body.extend(self.lower_block_stmts(block)?);
            match self.lower_block_terminator(0)? {
                LoweredTerminator::Return(expr) => body.push(HirStmt::Return(expr)),
                LoweredTerminator::Fallthrough(None) => {}
                LoweredTerminator::Fallthrough(Some(target)) | LoweredTerminator::Goto(target) => {
                    body.push(HirStmt::Goto(block_label(target)))
                }
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
                LoweredTerminator::Unsupported {
                    evidence,
                    target_expr,
                } => {
                    self.record_unsupported_inventory_event(
                        "build_hir_single_block_unsupported_terminator",
                        None,
                        None,
                        None,
                        Some(block.start_address),
                        None,
                        false,
                        "hir_unsupported_emit",
                    );
                    body.push(self.emit_unsupported_control_surface(evidence, target_expr));
                }
                LoweredTerminator::Switch {
                    expr,
                    targets,
                    default_target,
                    min_val,
                    proof,
                } => {
                    let emit_ready =
                        crate::midend::structuring::EmitReadyDecision::from_dispatcher_proof(
                            proof.as_ref(),
                        );
                    if !emit_ready.emit_ready {
                        let evidence = UnsupportedControlEvidence {
                            opcode: "Switch".to_string(),
                            source_block: Some(block.start_address),
                            target_expr: Some(print_expr(&expr)),
                            successor_targets: targets.clone(),
                            failure_family: UnsupportedControlFamily::NonStructuralDispatcher,
                            surface: IndirectControlSurface::DispatcherLike,
                            confidence: 40,
                        };
                        body.push(self.emit_unsupported_control_surface(evidence, Some(expr)));
                    } else {
                        let (case_values, used_proof_payload) = recovered_switch_case_values(
                            &targets,
                            default_target,
                            min_val,
                            proof.as_ref(),
                        );
                        if used_proof_payload {
                            self.telemetry.dispatcher.proof_payload_direct_emit_count += 1;
                        }
                        let cases = case_values
                            .into_iter()
                            .map(|(value, target)| crate::midend::ir::HirSwitchCase {
                                values: vec![value],
                                body: vec![HirStmt::Goto(block_label(target))],
                            })
                            .collect();
                        body.push(HirStmt::Switch {
                            expr,
                            cases,
                            default: default_target
                                .map(block_label)
                                .map(HirStmt::Goto)
                                .into_iter()
                                .collect(),
                        });
                    }
                }
            }
            if preview_builder_diag_enabled() {
                eprintln!("[DIAG] build_hir single_block_done: stmts={}", body.len());
            }
        } else {
            if preview_builder_diag_enabled() {
                eprintln!(
                    "[DIAG] build_hir multiblock_start: blocks={} ops={}",
                    self.pcode.blocks.len(),
                    self.pcode
                        .blocks
                        .iter()
                        .map(|block| block.ops.len())
                        .sum::<usize>()
                );
            }
            let structuring_start = std::time::Instant::now();
            body = self.build_multiblock_body()?;
            self.telemetry.core.structuring_duration_ms +=
                structuring_start.elapsed().as_millis() as usize;
            if preview_builder_diag_enabled() {
                eprintln!("[DIAG] build_hir multiblock_done: stmts={}", body.len());
            }
        }

        let (has_bare_return, has_value_return) = Self::return_surface_shape(&body);
        let return_type = body
            .iter()
            .rev()
            .find_map(|stmt| match stmt {
                HirStmt::Return(Some(expr)) => Some(expr_type(expr)),
                HirStmt::Return(None) => Some(NirType::Unknown),
                _ => None,
            })
            .unwrap_or(NirType::Unknown);

        self.trace_materialize_owner_repartition_summary();

        let callee_summaries = self
            .type_context
            .map(seed_callee_summaries_from_type_context)
            .unwrap_or_default();

        Ok(HirFunction {
            name: name.to_string(),
            params: self.params.values().cloned().collect(),
            locals: self
                .locals
                .iter()
                .map(|(offset, slot)| NirBinding {
                    name: slot.name.clone(),
                    ty: slot.ty.clone(),
                    surface_type_name: None,
                    origin: Some(match slot.origin {
                        NirBindingOrigin::StackOffset(_)
                        | NirBindingOrigin::HomeSlot(_)
                        | NirBindingOrigin::OutgoingArgSlot(_)
                        | NirBindingOrigin::VaRegion
                        | NirBindingOrigin::ReturnScaffold => slot.origin,
                        _ => NirBindingOrigin::StackOffset(*offset),
                    }),
                    initializer: None,
                })
                .chain(self.temps.values().cloned())
                .collect(),
            return_type,
            surface_return_type_name: (has_bare_return && !has_value_return)
                .then(|| "void".to_string()),
            body,
            calling_convention: self.options.calling_convention,
            int_param_offsets: self.options.cspec_param_offsets.clone().unwrap_or_default(),
            is_64bit: self.options.is_64bit,
            suppress_entry_register_params: self.suppress_entry_register_params,
            callee_observed_max_arity: IndexMap::new(),
            callee_summaries,
        })
    }

    fn return_surface_shape(stmts: &[HirStmt]) -> (bool, bool) {
        let mut has_bare_return = false;
        let mut has_value_return = false;
        for stmt in stmts {
            match stmt {
                HirStmt::Return(None) => has_bare_return = true,
                HirStmt::Return(Some(_)) => has_value_return = true,
                HirStmt::Block(body)
                | HirStmt::While { body, .. }
                | HirStmt::DoWhile { body, .. }
                | HirStmt::For { body, .. } => {
                    let (bare, value) = Self::return_surface_shape(body);
                    has_bare_return |= bare;
                    has_value_return |= value;
                }
                HirStmt::If {
                    then_body,
                    else_body,
                    ..
                } => {
                    let (then_bare, then_value) = Self::return_surface_shape(then_body);
                    let (else_bare, else_value) = Self::return_surface_shape(else_body);
                    has_bare_return |= then_bare || else_bare;
                    has_value_return |= then_value || else_value;
                }
                HirStmt::Switch { cases, default, .. } => {
                    for case in cases {
                        let (bare, value) = Self::return_surface_shape(&case.body);
                        has_bare_return |= bare;
                        has_value_return |= value;
                    }
                    let (bare, value) = Self::return_surface_shape(default);
                    has_bare_return |= bare;
                    has_value_return |= value;
                }
                HirStmt::Assign { .. }
                | HirStmt::Expr(_)
                | HirStmt::VaStart { .. }
                | HirStmt::Label(_)
                | HirStmt::Goto(_)
                | HirStmt::Break
                | HirStmt::Continue => {}
            }
        }
        (has_bare_return, has_value_return)
    }

    pub(crate) fn build_unsupported_control_evidence(
        &mut self,
        opcode: PcodeOpcode,
        source_block: Option<u64>,
        target_expr: Option<&HirExpr>,
        successor_targets: Vec<u64>,
        failure_family: UnsupportedControlFamily,
        surface: IndirectControlSurface,
        confidence: u8,
    ) -> UnsupportedControlEvidence {
        match surface {
            IndirectControlSurface::CallInd => {
                self.telemetry
                    .indirect_control
                    .unsupported_indirect_call_count += 1;
            }
            IndirectControlSurface::BranchInd | IndirectControlSurface::SwitchLike => {
                self.telemetry
                    .indirect_control
                    .unsupported_indirect_control_count += 1;
            }
            IndirectControlSurface::DispatcherLike => {}
        }
        if matches!(failure_family, UnsupportedControlFamily::ExternalTarget) {
            self.telemetry
                .indirect_control
                .unsupported_external_target_count += 1;
        }
        UnsupportedControlEvidence {
            opcode: format!("{opcode:?}"),
            source_block,
            target_expr: target_expr.map(print_expr),
            successor_targets,
            failure_family,
            surface,
            confidence,
        }
    }

    pub(crate) fn emit_unsupported_control_surface(
        &mut self,
        evidence: UnsupportedControlEvidence,
        target_expr: Option<HirExpr>,
    ) -> HirStmt {
        if matches!(
            evidence.surface,
            IndirectControlSurface::BranchInd | IndirectControlSurface::SwitchLike
        ) && let Some(HirExpr::Call { .. }) = target_expr.as_ref()
        {
            if evidence.opcode == "BranchInd" {
                return HirStmt::Return(target_expr);
            }
            return HirStmt::Expr(target_expr.expect("call target expression exists after guard"));
        }

        if matches!(
            evidence.surface,
            IndirectControlSurface::BranchInd | IndirectControlSurface::SwitchLike
        ) && let Some(HirExpr::Var(target_name)) = target_expr.as_ref()
            && let Some(resolved_target) = self.resolve_address_like_call_target_name(target_name)
        {
            return HirStmt::Expr(HirExpr::Call {
                target: resolved_target,
                args: Vec::new(),
                ty: NirType::Unknown,
            });
        }

        let pseudo_target = match evidence.surface {
            IndirectControlSurface::BranchInd | IndirectControlSurface::SwitchLike => {
                "__fission_branchind"
            }
            IndirectControlSurface::DispatcherLike => "__fission_dispatcher_indirect",
            IndirectControlSurface::CallInd => "__fission_callind_opaque",
        };
        let can_preserve =
            target_expr.is_some() || matches!(evidence.surface, IndirectControlSurface::CallInd);
        if can_preserve {
            self.telemetry
                .indirect_control
                .indirect_surface_preserved_count += 1;
            return HirStmt::Expr(HirExpr::Call {
                target: pseudo_target.to_string(),
                args: target_expr.into_iter().collect(),
                ty: NirType::Unknown,
            });
        }
        HirStmt::Expr(HirExpr::Call {
            target: "__fission_indirect_cf_unsupported".to_string(),
            args: Vec::new(),
            ty: NirType::Unknown,
        })
    }

    fn with_lowering_site<T>(&mut self, site: LoweringSite, f: impl FnOnce(&mut Self) -> T) -> T {
        let prev = self.current_lowering_site;
        self.lowering_site_depth += 1;
        self.current_lowering_site = Some(site);
        let result = f(self);
        self.current_lowering_site = prev;
        self.lowering_site_depth = self.lowering_site_depth.saturating_sub(1);
        result
    }

    pub(super) fn next_block_address(&self, idx: usize) -> Option<u64> {
        let layout_idx = self.pcode_block_idx(idx);
        self.layout_fallthrough[layout_idx]
            .map(|next_idx| self.block_target_keys[self.pcode_block_idx(next_idx)])
    }

    pub(super) fn block_target_key(&self, idx: usize) -> u64 {
        self.block_target_keys[self.pcode_block_idx(idx)]
    }

    pub(super) fn invalidate_materialization_dependent_caches(&mut self) {
        self.terminator_cache.clear();
        self.selector_representatives.clear();
        self.linear_body_cache.clear();
        self.x86_branch_recovery_attempts = 0;
    }

    pub(super) fn invalidate_scoped_materialization_caches(&mut self) {
        self.terminator_cache.clear();
        self.selector_representatives.clear();
        self.x86_branch_recovery_attempts = 0;
    }

    pub(super) fn ensure_temp_binding_for_output(
        &mut self,
        op: &PcodeOp,
        output: &Varnode,
        preserve_materialization: bool,
    ) -> NirBinding {
        let key = MaterializedVarnodeKey::new(output, op);
        if let Some(name) = self.materialized_vns.get(&key)
            && let Some(binding) = self.temps.get_mut(name)
        {
            let mut stabilized = false;
            if preserve_materialization
                && !binding.preserves_materialization()
                && binding.is_temp_like()
            {
                binding.origin = Some(NirBindingOrigin::TempPreserved);
                stabilized = true;
            }
            let binding = binding.clone();
            if stabilized {
                self.telemetry
                    .materialization
                    .materialization_stabilized_count += 1;
            }
            return binding;
        }

        let ty = pcode_output_type_from_size(op.opcode, output.size);

        let mut name = None;
        if is_register_space_id(output.space_id) {
            let namer = self.register_namer();
            let is_param_reg = namer
                .register_name_with_param_owned(output.offset, output.size)
                .is_some_and(|(_, idx)| idx.is_some());

            let is_ret_reg = namer.is_primary_return_register(output);

            if !is_param_reg && !is_ret_reg {
                let candidate = self
                    .sla_hw_name(output.offset, output.size)
                    .unwrap_or_else(|| "reg".to_string());
                if !self.params.values().any(|b| b.name == candidate)
                    && !self.locals.values().any(|s| s.name == candidate)
                    && (Self::is_x86_status_flag_output(output)
                        || !self.temps.contains_key(&candidate))
                {
                    name = Some(candidate);
                }
            }
        }
        let name = name.unwrap_or_else(|| self.next_unused_temp_binding_name(&ty));

        let origin = if preserve_materialization {
            NirBindingOrigin::TempPreserved
        } else {
            NirBindingOrigin::Temp
        };

        let binding = if let Some(existing) = self.temps.get(&name) {
            let mut updated = existing.clone();
            if preserve_materialization && !existing.preserves_materialization() {
                updated.origin = Some(NirBindingOrigin::TempPreserved);
            }
            updated
        } else {
            NirBinding {
                name: name.clone(),
                ty,
                surface_type_name: None,
                origin: Some(origin),
                initializer: None,
            }
        };

        if preserve_materialization {
            self.telemetry
                .materialization
                .materialization_stabilized_count += 1;
        }
        self.materialized_vns.insert(key, name.clone());
        self.invalidate_materialization_dependent_caches();
        self.temps.insert(name, binding.clone());
        binding
    }

    pub(super) fn ensure_explicit_merge_binding_for_block(
        &mut self,
        block_idx: usize,
        output: &Varnode,
    ) -> NirBinding {
        let key = (block_idx, VarnodeKey::from(output));
        if let Some(name) = self.explicit_merge_bindings.get(&key)
            && let Some(binding) = self.temps.get(name)
        {
            return binding.clone();
        }

        let ty = type_from_size(output.size, false);
        // For x86-64 loop headers: prefer the hardware register name over a fresh
        // temp for GPR-family varnodes. This prevents a RAX=ZExt(EAX) passthrough
        // in the loop body from being given an opaque temp name (e.g. xVar1) that
        // then propagates back into EAX bindings via loop_header_explicit_merge_binding_name.
        // We use SLA-first hardware naming for the narrow canonical name (EAX→"rax").
        let hw_name: Option<String> = if !output.is_constant
            && is_register_space_id(output.space_id)
            && matches!(
                self.options.calling_convention,
                CallingConvention::WindowsX64
                    | CallingConvention::SystemVAmd64
                    | CallingConvention::X86_32
            ) {
            // Only promote when this block is a loop head and the output is a
            // known x86 GPR family member (xor-family check via gpr_family_index).
            let output_key = VarnodeKey::from(output);
            let is_gpr = self.gpr_family_index_for_key(&output_key).is_some();
            let is_loop_head = self.loop_bodies.iter().any(|lb| lb.head == block_idx);
            if is_gpr && is_loop_head {
                self.sla_hw_name(output.offset, output.size)
            } else {
                None
            }
        } else {
            None
        };
        let name = if let Some(hw) = hw_name {
            // Reuse an existing binding with the same hardware name if present.
            if self.temps.contains_key(&hw)
                || self.params.values().any(|b| b.name == hw)
                || self.locals.values().any(|s| s.name == hw)
            {
                hw
            } else {
                hw
            }
        } else {
            self.next_unused_temp_binding_name(&ty)
        };
        let binding = NirBinding {
            name: name.clone(),
            ty,
            surface_type_name: None,
            origin: Some(NirBindingOrigin::TempPreserved),
            initializer: None,
        };
        self.explicit_merge_bindings.insert(key, name.clone());
        self.invalidate_materialization_dependent_caches();
        self.temps.insert(name, binding.clone());
        binding
    }

    pub(super) fn ensure_live_register_binding(&mut self, name: &str, size: u32) -> String {
        if self.params.values().any(|binding| binding.name == name)
            || self.locals.values().any(|slot| slot.name == name)
        {
            return name.to_string();
        }
        self.temps
            .entry(name.to_string())
            .or_insert_with(|| NirBinding {
                name: name.to_string(),
                ty: type_from_size(size, false),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::TempPreserved),
                initializer: None,
            });
        name.to_string()
    }

    /// Ghidra-style hardware register name lookup with SLA-first resolution.
    ///
    /// Queries `self.options.sla_register_map` (populated from SLA register model) first,
    /// then falls back to the checked-in `.slaspec` register model.
    ///
    /// Use this instead of ad-hoc ABI tables anywhere `self.options` is available.
    /// is available — it covers all architectures uniformly via the `.ldefs`/SLA map.
    #[inline]
    pub(crate) fn sla_hw_name(&self, offset: u64, size: u32) -> Option<String> {
        crate::midend::cspec::RegisterNamer::from_options(&self.options).hw_name_at(offset, size)
    }

    /// ABI-independent hardware register name with SLA-first resolution.
    #[inline]
    pub(crate) fn sla_reg_name(&self, offset: u64, size: u32) -> String {
        self.sla_hw_name(offset, size)
            .unwrap_or_else(|| "reg".to_string())
    }

    #[inline]
    pub(crate) fn register_namer(&self) -> crate::midend::cspec::RegisterNamer {
        crate::midend::cspec::RegisterNamer::from_options(&self.options)
    }
}

fn is_compiler_runtime_param_suppressed_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("crtstartup") || lower.contains("__dyn_tls_")
}

pub(super) fn test_refine_partitions(accesses: &[(i64, u32)]) -> Vec<(i64, u32)> {
    self::materialize::test_refine_partitions(accesses)
}
