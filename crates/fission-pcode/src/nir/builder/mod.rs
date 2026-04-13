pub(super) use super::support::*;
use super::*;
use indexmap::IndexMap;
use std::collections::HashMap;
mod state;
pub(super) use state::PreviewBuilder;

mod aggregate_recovery;
mod call_recovery;
mod debug;
mod entry_analysis;
mod init;
mod lower_expr;
mod materialize;
mod stack_slots;
mod stats;
pub(super) mod switch_table;
mod terminator;
mod type_hints;

use self::debug::preview_builder_diag_enabled;
use tracing::trace_span;

pub(super) fn apply_preview_type_hints(
    func: &mut HirFunction,
    context: &PreviewTypeContext,
) -> PreviewHintStats {
    type_hints::apply_preview_type_hints(func, context)
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

    pub(super) fn build_hir(
        &mut self,
        name: &str,
        _address: u64,
    ) -> Result<HirFunction, MlilPreviewError> {
        let _build = trace_span!(
            "preview_build_hir",
            fn_name = name,
            address = _address,
            blocks = self.pcode.blocks.len()
        )
        .entered();
        if self.pcode.blocks.is_empty() {
            return Err(MlilPreviewError::UnsupportedPattern("empty pcode"));
        }

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
                    let (case_values, used_proof_payload) = recovered_switch_case_values(
                        &targets,
                        default_target,
                        min_val,
                        proof.as_ref(),
                    );
                    if used_proof_payload {
                        self.proof_payload_direct_emit_count += 1;
                    }
                    let cases = case_values
                        .into_iter()
                        .map(|(value, target)| crate::nir::types::HirSwitchCase {
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
            body = self.build_multiblock_body()?;
            if preview_builder_diag_enabled() {
                eprintln!("[DIAG] build_hir multiblock_done: stmts={}", body.len());
            }
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
            surface_return_type_name: None,
            body,
            calling_convention: self.options.calling_convention,
            is_64bit: self.options.is_64bit,
            callee_observed_max_arity: IndexMap::new(),
            callee_summaries: IndexMap::new(),
        })
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
                self.unsupported_indirect_call_count += 1;
            }
            IndirectControlSurface::BranchInd | IndirectControlSurface::SwitchLike => {
                self.unsupported_indirect_control_count += 1;
            }
            IndirectControlSurface::DispatcherLike => {}
        }
        if matches!(failure_family, UnsupportedControlFamily::ExternalTarget) {
            self.unsupported_external_target_count += 1;
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
            self.indirect_surface_preserved_count += 1;
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
            if preserve_materialization
                && !binding.preserves_materialization()
                && binding.is_temp_like()
            {
                binding.origin = Some(NirBindingOrigin::TempPreserved);
                self.materialization_stabilized_count += 1;
            }
            return binding.clone();
        }

        let ty = type_from_size(output.size, false);
        let name = next_temp_name(&ty, &mut self.temp_next_id);
        let binding = NirBinding {
            name: name.clone(),
            ty,
            surface_type_name: None,
            origin: Some(if preserve_materialization {
                NirBindingOrigin::TempPreserved
            } else {
                NirBindingOrigin::Temp
            }),
            initializer: None,
        };
        if preserve_materialization {
            self.materialization_stabilized_count += 1;
        }
        self.materialized_vns.insert(key, name.clone());
        self.temps.insert(name, binding.clone());
        binding
    }
}
