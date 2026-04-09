pub(super) use super::support::*;
use super::*;
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
    type_hints::collect_local_surface_hints(body, pointer_hints, func, &alias_collector, local_hints);
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
            self.virtual_block_map.get(v_idx).copied().unwrap_or(idx % original_count.max(1))
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
                LoweredTerminator::Unsupported => {
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
                    body.push(HirStmt::Expr(HirExpr::Call {
                        target: "__fission_indirect_cf_unsupported".to_string(),
                        args: Vec::new(),
                        ty: NirType::Unknown,
                    }));
                }
                LoweredTerminator::Switch {
                    expr,
                    targets,
                    default_target,
                    min_val,
                } => {
                    let cases = targets
                        .into_iter()
                        .filter(|target| Some(*target) != default_target)
                        .enumerate()
                        .map(|(i, t)| {
                        crate::nir::types::HirSwitchCase {
                            // Use recovered min_val offset; for comparison-chain
                            // switches min_val is 0 (real values come from the chain).
                            values: vec![min_val + i as i64],
                            body: vec![HirStmt::Goto(block_label(t))],
                        }
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
                    origin: Some(NirBindingOrigin::StackOffset(*offset)),
                    initializer: None,
                })
                .chain(self.temps.values().cloned())
                .collect(),
            return_type,
            surface_return_type_name: None,
            body,
            calling_convention: self.options.calling_convention,
            is_64bit: self.options.is_64bit,
            callee_observed_max_arity: HashMap::new(),
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
        self.layout_fallthrough[idx].map(|next_idx| self.block_target_key(next_idx))
    }

    pub(super) fn block_target_key(&self, idx: usize) -> u64 {
        self.block_target_keys[idx]
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
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        };
        self.materialized_vns.insert(key, name.clone());
        self.temps.insert(name, binding.clone());
        binding
    }
}
