use super::cleanup::cleanup_redundant_labels;
use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(crate) fn build_multiblock_body(&mut self) -> Result<Vec<HirStmt>, MlilPreviewError> {
        let diag = structuring_diag_enabled();
        let total_start = Instant::now();
        let force_linear = self.should_force_linear_structuring();
        let scc = self.analyze_cfg_scc();
        self.structuring_scc_component_count += scc.component_count();
        self.structuring_irreducible_scc_count += scc.irreducible_count();
        self.structuring_irreducible_header_count += scc.irreducible_header_total_count();
        if diag {
            eprintln!(
                "[DIAG] structuring start: blocks={} edges={} force_linear={}",
                self.pcode.blocks.len(),
                self.successors.iter().map(Vec::len).sum::<usize>(),
                force_linear
            );
        }
        if force_linear {
            self.forced_linear_structuring_count += 1;
            let result = self.build_linear_multiblock_body();
            if diag {
                eprintln!(
                    "[DIAG] structuring linear done: elapsed={:.3}s success={}",
                    total_start.elapsed().as_secs_f64(),
                    result.is_ok()
                );
            }
            return result;
        }

        if diag {
            let cfg = self.analyze_cfg_edges();
            let dom = self.analyze_cfg_dominators();
            let postdom = self.analyze_cfg_postdominators();
            let sample_ncd = if self.pcode.blocks.len() >= 2 {
                dom.nearest_common_dominator(&[0, self.pcode.blocks.len() - 1])
            } else {
                Some(0)
            };
            eprintln!(
                "[DIAG] structuring cfg-analysis: roots={} tree={} back={} forward={} cross={} dom_roots={} postdom_exits={} scc_components={} irreducible_scc={} sample_ncd={:?}",
                cfg.roots().len(),
                cfg.count_class(EdgeClass::Tree),
                cfg.count_class(EdgeClass::Back),
                cfg.count_class(EdgeClass::Forward),
                cfg.count_class(EdgeClass::Cross),
                dom.roots().len(),
                postdom.exits().len(),
                scc.component_count(),
                scc.irreducible_count(),
                sample_ncd,
            );
        }

        let mut body = Vec::new();
        let targeted = self.collect_jump_targets()?;
        let mut emitted_labels = HashSet::new();
        let mut last_structuring_failure = None;
        let mut idx = 0usize;
        while idx < self.pcode.blocks.len() {
            if diag && idx > 0 && idx % 32 == 0 {
                eprintln!(
                    "[DIAG] structuring progress: idx={} elapsed={:.3}s",
                    idx,
                    total_start.elapsed().as_secs_f64()
                );
            }
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} attempt=switch elapsed={:.3}s",
                    idx,
                    self.pcode.blocks[idx].start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
            if let Some((stmt, skip_to)) = Self::capture_structuring_failure(
                self.try_lower_switch(idx),
                &mut last_structuring_failure,
            )? && self.accept_structured_region(idx, skip_to, &targeted)
            {
                body.push(stmt);
                idx = skip_to;
                continue;
            }
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} attempt=dowhile elapsed={:.3}s",
                    idx,
                    self.pcode.blocks[idx].start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
            if let Some((stmt, skip_to)) = Self::capture_structuring_failure(
                self.try_lower_dowhile(idx),
                &mut last_structuring_failure,
            )? && self.accept_structured_region(idx, skip_to, &targeted)
            {
                body.push(stmt);
                idx = skip_to;
                continue;
            }
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} attempt=while elapsed={:.3}s",
                    idx,
                    self.pcode.blocks[idx].start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
            if let Some((stmt, skip_to)) = Self::capture_structuring_failure(
                self.try_lower_while(idx),
                &mut last_structuring_failure,
            )? && self.accept_structured_region(idx, skip_to, &targeted)
            {
                body.push(stmt);
                idx = skip_to;
                continue;
            }
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} attempt=short_if elapsed={:.3}s",
                    idx,
                    self.pcode.blocks[idx].start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
            if let Some((stmt, skip_to)) = Self::capture_structuring_failure(
                self.try_lower_short_circuit_if(idx),
                &mut last_structuring_failure,
            )? && self.accept_structured_region(idx, skip_to, &targeted)
            {
                body.push(stmt);
                idx = skip_to;
                continue;
            }
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} attempt=if_else elapsed={:.3}s",
                    idx,
                    self.pcode.blocks[idx].start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
            if let Some((stmt, skip_to)) = Self::capture_structuring_failure(
                self.try_lower_if_else(idx),
                &mut last_structuring_failure,
            )? && self.accept_structured_region(idx, skip_to, &targeted)
            {
                body.push(stmt);
                idx = skip_to;
                continue;
            }
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} attempt=if elapsed={:.3}s",
                    idx,
                    self.pcode.blocks[idx].start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
            if let Some((stmt, skip_to)) = Self::capture_structuring_failure(
                self.try_lower_if(idx),
                &mut last_structuring_failure,
            )? && self.accept_structured_region(idx, skip_to, &targeted)
            {
                body.push(stmt);
                idx = skip_to;
                continue;
            }
            if let Some(err) = last_structuring_failure.take() {
                if let Some((recovered_body, skip_to)) = self.try_recover_region_linearized_body(
                    idx,
                    &err,
                    &targeted,
                    &mut emitted_labels,
                )? {
                    body.extend(recovered_body);
                    idx = skip_to;
                    continue;
                }
                return Err(err);
            }

            let block = &self.pcode.blocks[idx];
            let block_key = self.block_target_key(idx);
            if (idx == 0 || targeted.contains(&block_key)) && emitted_labels.insert(block_key) {
                body.push(HirStmt::Label(block_label(block_key)));
            }
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} fallback=lower_block_stmts elapsed={:.3}s",
                    idx,
                    block.start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
            body.extend(self.lower_block_stmts(block)?);
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} fallback=lower_block_terminator elapsed={:.3}s",
                    idx,
                    block.start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
            match self.lower_block_terminator(idx)? {
                LoweredTerminator::Return(expr) => body.push(HirStmt::Return(expr)),
                LoweredTerminator::Goto(target) => {
                    if self.next_block_address(idx) != Some(target) {
                        body.push(HirStmt::Goto(block_label(target)));
                    }
                }
                LoweredTerminator::Cond {
                    cond,
                    true_target,
                    false_target,
                } => {
                    let next_addr = self.next_block_address(idx);
                    let then_body = if next_addr == Some(true_target) {
                        Vec::new()
                    } else {
                        vec![HirStmt::Goto(block_label(true_target))]
                    };
                    let else_body = match false_target {
                        Some(false_target) if Some(false_target) != next_addr => {
                            vec![HirStmt::Goto(block_label(false_target))]
                        }
                        _ => Vec::new(),
                    };
                    body.push(HirStmt::If {
                        cond,
                        then_body,
                        else_body,
                    });
                }
                LoweredTerminator::Fallthrough(_) => {}
                LoweredTerminator::Unsupported => {
                    return Err(MlilPreviewError::UnsupportedCfgIndirectCallRegion);
                }
            }
            idx += 1;
        }
        while self.promote_single_entry_guarded_tail_regions(&mut body) {}
        self.discover_guarded_tail_candidates(&body);
        if diag {
            eprintln!(
                "[DIAG] structuring done: elapsed={:.3}s stmts={}",
                total_start.elapsed().as_secs_f64(),
                body.len()
            );
            eprintln!(
                "[DIAG] structuring promotions: candidates={} promoted={}",
                self.promotion_candidate_count, self.promoted_region_count
            );
        } else if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
            eprintln!(
                "[mlil-preview] stage=structuring promotions candidates={} promoted={}",
                self.promotion_candidate_count, self.promoted_region_count
            );
        }
        Ok(cleanup_redundant_labels(body))
    }

    fn should_force_linear_structuring(&self) -> bool {
        if self.options.force_linear_structuring {
            return true;
        }
        let total_ops: usize = self.pcode.blocks.iter().map(|block| block.ops.len()).sum();
        if self.pcode.blocks.len() > 80 {
            return true;
        }

        if self.options.is_64bit && self.pcode.blocks.len() >= 28 && total_ops >= 350 {
            return true;
        }

        let edge_count: usize = self.successors.iter().map(Vec::len).sum();
        let multi_pred_blocks = self
            .predecessors
            .iter()
            .filter(|preds| preds.len() > 1)
            .count();
        let max_predecessors = self.predecessors.iter().map(Vec::len).max().unwrap_or(0);

        self.pcode.blocks.len() > 32
            && (edge_count > self.pcode.blocks.len().saturating_mul(2)
                || multi_pred_blocks > 8
                || max_predecessors >= 4)
    }
}

pub(crate) fn structuring_diag_enabled() -> bool {
    std::env::var_os("FISSION_PREVIEW_DIAG").is_some()
}

#[cfg(test)]
pub(crate) fn promote_single_entry_guarded_tail_regions_for_test(
    body: &mut Vec<HirStmt>,
) -> PreviewBuildStats {
    let dummy = PcodeFunction { blocks: Vec::new() };
    let options = MlilPreviewOptions {
        pe_x64_only: true,
        is_64bit: true,
        pointer_size: 8,
        format: "PE".to_string(),
        image_base: 0,
        sections: Vec::new(),
        region_linearize_structuring: false,
        force_linear_structuring: false,
        conservative_irreducible_fallback: false,
    };
    let mut builder = PreviewBuilder::new(&dummy, &options, None);
    while builder.promote_single_entry_guarded_tail_regions(body) {}
    builder.preview_build_stats()
}

#[cfg(test)]
pub(crate) fn discover_guarded_tail_candidates_for_test(body: &[HirStmt]) -> PreviewBuildStats {
    discover_guarded_tail_candidates_for_stats(body)
}

pub(crate) fn discover_guarded_tail_candidates_for_stats(body: &[HirStmt]) -> PreviewBuildStats {
    let dummy = PcodeFunction { blocks: Vec::new() };
    let options = MlilPreviewOptions {
        pe_x64_only: true,
        is_64bit: true,
        pointer_size: 8,
        format: "PE".to_string(),
        image_base: 0,
        sections: Vec::new(),
        region_linearize_structuring: false,
        force_linear_structuring: false,
        conservative_irreducible_fallback: false,
    };
    let mut builder = PreviewBuilder::new(&dummy, &options, None);
    builder.discover_guarded_tail_candidates(body);
    builder.preview_build_stats()
}
