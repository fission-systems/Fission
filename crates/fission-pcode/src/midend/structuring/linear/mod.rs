//! Linear body: **p-code opcode residual** on PreviewBuilder (ADR 0012).
//!
//! Free owners: `fission-midend-structuring::{linear_body,linear_multiblock}`.
//! Tests and host call free-fns directly; this module keeps only SLEIGH/p-code
//! helpers (`is_trivial_forwarding_op`, jump-target inventory, etc.).

use super::*;

// Re-export free-fn surface for midend callers that still import via structuring::linear.
pub use fission_midend_structuring::{
    can_inline_linear_successor, can_inline_linear_successor_for_region,
    canonicalize_region_target_for_exit_for_test, find_shared_tail_entries_for_region_for_test,
    has_linear_body_cache, linear_exit, linear_exit_with_budget, lower_conditional_tail,
    lower_linear_body, lower_linear_body_for_region_recovery_detailed,
    lower_linear_body_with_budget, shared_exit_for_indices, shared_linear_exit,
};

mod recovery;
mod types;
pub use types::*;

impl<'a> PreviewBuilder<'a> {
    // Linear free-fns are invoked via StructuringHost (host_impl) or tests
    // (`fission_midend_structuring::lower_linear_body`, etc.). No inherent thin wraps.

    pub(super) fn is_trivial_forwarding_block(&self, idx: usize, next_idx: usize) -> bool {
        if idx >= next_idx {
            return false;
        }
        let block = self.pcode_block(idx).clone();
        if block.ops.len() > 8 {
            return false;
        }
        if self.successors[idx].len() != 1 || self.successors[idx][0] != next_idx {
            return false;
        }
        let Some((last, prefix)) = block.ops.split_last() else {
            return false;
        };
        if !prefix
            .iter()
            .all(|op| self.is_trivial_forwarding_op(op.opcode))
        {
            return false;
        }
        self.is_linear_tail_terminator(idx, last.opcode)
            || self.is_trivial_forwarding_op(last.opcode)
    }

    pub(super) fn find_block_index_by_address(&self, address: u64) -> Option<usize> {
        self.target_key_to_index.get(&address).copied().or_else(|| {
            canonical_block_index_for_address(self.pcode, &self.address_to_index, address)
        })
    }

    pub(super) fn fallthrough_index(&self, idx: usize) -> Option<usize> {
        let layout_idx = self.pcode_block_idx(idx);
        self.layout_fallthrough[layout_idx]
            .filter(|succ| self.successors[layout_idx].contains(succ))
    }

    pub(super) fn collect_jump_targets(&mut self) -> Result<HashSet<u64>, MlilPreviewError> {
        if let Some(cached) = &self.jump_targets_cache {
            return Ok(cached.clone());
        }
        let mut targets = HashSet::default();
        for idx in 0..self.pcode.blocks.len() {
            for succ in &self.successors[idx] {
                targets.insert(self.block_target_key(*succ));
            }
            // Do not force-lower uncached terminators here: this helper should
            // stay side-effect free for inventory/stat counters.
            if let Some(term) = self.terminator_cache.get(&idx) {
                match term {
                    LoweredTerminator::Goto(target)
                    | LoweredTerminator::Fallthrough(Some(target)) => {
                        targets.insert(*target);
                    }
                    LoweredTerminator::Cond {
                        true_target,
                        false_target,
                        ..
                    } => {
                        targets.insert(*true_target);
                        if let Some(false_target) = false_target {
                            targets.insert(*false_target);
                        }
                    }
                    LoweredTerminator::Switch {
                        targets: switch_targets,
                        default_target,
                        proof,
                        ..
                    } => {
                        targets.extend(switch_targets.iter().copied());
                        if let Some(default_target) = default_target {
                            targets.insert(*default_target);
                        }
                        if let Some(proof) = proof.as_ref() {
                            targets.extend(proof.candidate_targets.iter().copied());
                            targets.extend(proof.recovered_cases.iter().map(|(_, target)| *target));
                            if let Some(default_target) = proof.default_target {
                                targets.insert(default_target);
                            }
                            if let Some(follow_block) = proof.follow_block {
                                targets.insert(follow_block);
                            }
                            if let Some(legality) = proof.legality_witness.as_ref()
                                && let Some(follow_block) = legality.follow_block
                            {
                                targets.insert(follow_block);
                            }
                        }
                    }
                    LoweredTerminator::Unsupported { evidence, .. } => {
                        targets.extend(evidence.successor_targets.iter().copied());
                    }
                    LoweredTerminator::Return(_) | LoweredTerminator::Fallthrough(None) => {}
                }
            }
        }
        for &(_, dst) in &self.irreducible_edges {
            targets.insert(self.block_target_key(dst));
        }
        self.jump_targets_cache = Some(targets.clone());
        Ok(targets)
    }

    pub(super) fn forwarding_block_defines_return_tail_live_in(
        &self,
        idx: usize,
        join_idx: usize,
    ) -> bool {
        if self.successors.get(idx).map(Vec::as_slice) != Some(&[join_idx][..]) {
            return false;
        }
        let block = self.pcode_block(idx);
        let join_block = self.pcode_block(join_idx);
        let Some(join_term_idx) = join_block.ops.iter().position(|op| {
            matches!(
                op.opcode,
                PcodeOpcode::Branch
                    | PcodeOpcode::CBranch
                    | PcodeOpcode::BranchInd
                    | PcodeOpcode::Return
            )
        }) else {
            return false;
        };
        if join_block.ops[join_term_idx].opcode != PcodeOpcode::Return {
            return false;
        }
        let Some(block_term_idx) = block.ops.iter().position(|op| {
            matches!(
                op.opcode,
                PcodeOpcode::Branch
                    | PcodeOpcode::CBranch
                    | PcodeOpcode::BranchInd
                    | PcodeOpcode::Return
            )
        }) else {
            return false;
        };
        let defs = block
            .ops
            .iter()
            .take(block_term_idx)
            .filter_map(|op| op.output.as_ref())
            .collect::<Vec<_>>();
        if defs.is_empty() {
            return false;
        }
        // Include the join terminator itself: a Return's inputs are the live-in
        // return value. Skipping the Return op made pure arms like
        // `eax = 1; goto join; return eax` look like empty forwards and caused
        // short-circuit OR recovery to drop the positive signum arm.
        if join_block
            .ops
            .iter()
            .take(join_term_idx + 1)
            .flat_map(|op| op.inputs.iter())
            .any(|input| defs.iter().any(|def| Self::varnodes_overlap(def, input)))
        {
            return true;
        }
        // x86 epilogue joins often return via a stack-loaded address while the
        // *value* is the ABI primary return register (EAX/RAX). Treat a def of
        // that register in the forward arm as return-live-in as well.
        let namer = self.register_namer();
        defs.iter().any(|def| namer.is_primary_return_register(def))
    }

    pub(crate) fn is_trivial_linear_tail(&self, idx: usize) -> bool {
        let block = self.pcode_block(idx).clone();
        if block.ops.len() > 24 {
            return false;
        }
        let Some((last, prefix)) = block.ops.split_last() else {
            return false;
        };
        prefix.iter().all(|op| self.is_trivial_tail_op(op.opcode))
            && (self.is_linear_tail_terminator(idx, last.opcode)
                || self.is_trivial_tail_op(last.opcode))
    }

    pub(crate) fn record_conditional_tail_mismatch_subtype(
        &mut self,
        subtype: ConditionalTailMismatchSubtype,
    ) {
        match subtype {
            ConditionalTailMismatchSubtype::NoCommonFollowInWindow => {
                self.telemetry.structuring.region_linearize_rejected_body_lowering_conditional_tail_no_common_follow_in_window_count += 1;
            }
            ConditionalTailMismatchSubtype::FollowBeyondWindow => {
                self.telemetry.structuring.region_linearize_rejected_body_lowering_conditional_tail_follow_beyond_window_count += 1;
            }
            ConditionalTailMismatchSubtype::SideEntryOrExit => {
                self.telemetry.structuring.region_linearize_rejected_body_lowering_conditional_tail_side_entry_or_exit_count += 1;
            }
            ConditionalTailMismatchSubtype::ComplexArmShape => {
                self.telemetry.structuring.region_linearize_rejected_body_lowering_conditional_tail_complex_arm_shape_count += 1;
            }
            ConditionalTailMismatchSubtype::DepthOrBudgetExceeded => {
                self.telemetry.structuring.region_linearize_rejected_body_lowering_conditional_tail_depth_or_budget_exhausted_count += 1;
            }
            ConditionalTailMismatchSubtype::OneArmBodyLoweringFailed => {
                self.telemetry.structuring.region_linearize_rejected_body_lowering_conditional_tail_arm_body_lowering_failed_count += 1;
                self.telemetry.structuring.region_linearize_rejected_body_lowering_conditional_tail_one_arm_body_lowering_failed_count += 1;
            }
            ConditionalTailMismatchSubtype::BothArmsBodyLoweringFailed => {
                self.telemetry.structuring.region_linearize_rejected_body_lowering_conditional_tail_arm_body_lowering_failed_count += 1;
                self.telemetry.structuring.region_linearize_rejected_body_lowering_conditional_tail_both_arms_body_lowering_failed_count += 1;
            }
            ConditionalTailMismatchSubtype::FollowTailLoweringFailed => {
                self.telemetry.structuring.region_linearize_rejected_body_lowering_conditional_tail_arm_body_lowering_failed_count += 1;
                self.telemetry.structuring.region_linearize_rejected_body_lowering_conditional_tail_follow_tail_lowering_failed_count += 1;
            }
        }
    }

    pub(crate) fn record_conditional_tail_mismatch_sample(
        &self,
        origin_idx: usize,
        true_idx: Option<usize>,
        false_idx: Option<usize>,
        exit: LinearExit,
        subtype: ConditionalTailMismatchSubtype,
        stage: &str,
    ) {
        if std::env::var_os("FISSION_RECOVERY_MISMATCH_TRACE").is_none() {
            return;
        }
        let function_addr = self
            .pcode
            .blocks
            .first()
            .map(|block| block.start_address)
            .unwrap_or_default();
        let message = format!(
            "{{\"function\":\"0x{function_addr:x}\",\"origin_idx\":{origin_idx},\"true_idx\":{},\"false_idx\":{},\"exit\":\"{:?}\",\"subtype\":\"{:?}\",\"stage\":\"{}\"}}\n",
            true_idx.map_or("null".to_string(), |idx| idx.to_string()),
            false_idx.map_or("null".to_string(), |idx| idx.to_string()),
            exit,
            subtype,
            stage,
        );
        let path = format!("/tmp/fission_preview_{function_addr:x}_conditional_mismatch.jsonl");
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .and_then(|mut file| std::io::Write::write_all(&mut file, message.as_bytes()));
    }

    fn is_trivial_tail_op(&self, opcode: PcodeOpcode) -> bool {
        matches!(
            opcode,
            PcodeOpcode::Copy
                | PcodeOpcode::Load
                | PcodeOpcode::Cast
                | PcodeOpcode::IntAdd
                | PcodeOpcode::IntSub
                | PcodeOpcode::IntCarry
                | PcodeOpcode::IntSCarry
                | PcodeOpcode::IntSBorrow
                | PcodeOpcode::Int2Comp
                | PcodeOpcode::IntNegate
                | PcodeOpcode::IntAnd
                | PcodeOpcode::IntOr
                | PcodeOpcode::IntXor
                | PcodeOpcode::SubPiece
                | PcodeOpcode::Piece
                | PcodeOpcode::MultiEqual
                | PcodeOpcode::Indirect
                | PcodeOpcode::IntZExt
                | PcodeOpcode::IntSExt
                | PcodeOpcode::IntLeft
                | PcodeOpcode::IntRight
                | PcodeOpcode::IntSRight
                | PcodeOpcode::PtrAdd
                | PcodeOpcode::PtrSub
                | PcodeOpcode::IntEqual
                | PcodeOpcode::IntNotEqual
                | PcodeOpcode::IntLess
                | PcodeOpcode::IntLessEqual
                | PcodeOpcode::IntSLess
                | PcodeOpcode::IntSLessEqual
                | PcodeOpcode::BoolNegate
                | PcodeOpcode::BoolAnd
                | PcodeOpcode::BoolOr
                | PcodeOpcode::Call
        )
    }

    fn is_trivial_forwarding_op(&self, opcode: PcodeOpcode) -> bool {
        matches!(
            opcode,
            PcodeOpcode::Copy
                | PcodeOpcode::Cast
                | PcodeOpcode::MultiEqual
                | PcodeOpcode::Indirect
                | PcodeOpcode::SubPiece
                | PcodeOpcode::Piece
                | PcodeOpcode::IntZExt
                | PcodeOpcode::IntSExt
                | PcodeOpcode::PtrAdd
                | PcodeOpcode::PtrSub
        )
    }

    fn is_linear_tail_terminator(&self, idx: usize, opcode: PcodeOpcode) -> bool {
        match opcode {
            PcodeOpcode::Return => self.successors[idx].is_empty(),
            PcodeOpcode::Branch => self.successors[idx].len() == 1,
            _ => false,
        }
    }

    fn varnodes_overlap(lhs: &Varnode, rhs: &Varnode) -> bool {
        if lhs.is_constant || rhs.is_constant || lhs.space_id != rhs.space_id {
            return false;
        }
        if lhs.offset == rhs.offset && lhs.size == rhs.size {
            return true;
        }
        if !is_register_space_id(lhs.space_id) {
            return false;
        }
        let lhs_end = lhs.offset.saturating_add(u64::from(lhs.size));
        let rhs_end = rhs.offset.saturating_add(u64::from(rhs.size));
        lhs.offset < rhs_end && rhs.offset < lhs_end
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pcode::{PcodeBasicBlock, PcodeFunction, PcodeOp, PcodeOpcode, Varnode};

    fn test_options() -> MlilPreviewOptions {
        MlilPreviewOptions {
            pe_x64_only: true,
            is_64bit: true,
            is_big_endian: false,
            pointer_size: 8,
            format: "PE".to_string(),
            image_base: 0,
            sections: Vec::new(),
            region_linearize_structuring: false,
            force_linear_structuring: false,
            conservative_irreducible_fallback: false,
            structuring_engine: StructuringEngineKind::GraphCollapseV1,
            global_names: Default::default(),
            global_sizes: Default::default(),
            relocation_names: Default::default(),
            calling_convention: Default::default(),
            ..Default::default()
        }
    }

    #[test]
    fn collect_jump_targets_includes_proof_recovered_switch_targets() {
        let func = PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x1000,
                    successors: vec![],
                    ops: vec![],
                },
                PcodeBasicBlock {
                    index: 1,
                    start_address: 0x1100,
                    successors: vec![],
                    ops: vec![],
                },
                PcodeBasicBlock {
                    index: 2,
                    start_address: 0x1200,
                    successors: vec![],
                    ops: vec![],
                },
                PcodeBasicBlock {
                    index: 3,
                    start_address: 0x1300,
                    successors: vec![],
                    ops: vec![],
                },
            ],
        };
        let options = test_options();
        let mut builder = PreviewBuilder::new(&func, &options, None);
        builder.terminator_cache.insert(
            0,
            LoweredTerminator::Switch {
                expr: HirExpr::Var("selector".to_string()),
                targets: vec![0x1100],
                default_target: Some(0x1300),
                min_val: 0,
                proof: Some(DispatcherProofUnit {
                    selector_expr: "selector".to_string(),
                    rendered_selector_expr: Some("selector".to_string()),
                    candidate_targets: vec![0x1100],
                    recovered_cases: vec![(0, 0x1100), (1, 0x1200)],
                    selector_cardinality: 2,
                    target_cardinality: 2,
                    case_map_source: DispatcherCaseMapSource::Merged,
                    default_target: Some(0x1300),
                    guard_set: vec!["ordinal_domain_complete".to_string()],
                    follow_block: Some(0x1300),
                    normalization: None,
                    legality_witness: Some(DispatcherLegality {
                        follow_block: Some(0x1300),
                        postdom_ok: true,
                        side_effect_free_selector: true,
                        ordinal_domain_complete: true,
                        shared_tail_conflict: false,
                        valid: true,
                    }),
                    proof_scope: DispatcherProofScope::OuterDispatch,
                    proof_complete: true,
                    failure_family: None,
                }),
            },
        );

        let targets = builder.collect_jump_targets().expect("targets");
        assert!(targets.contains(&0x1100), "{targets:?}");
        assert!(targets.contains(&0x1200), "{targets:?}");
        assert!(targets.contains(&0x1300), "{targets:?}");
    }

    #[test]
    fn forwarding_block_live_in_guard_detects_return_tail_register_use() {
        let w0 = Varnode {
            space_id: REGISTER_SPACE_ID,
            offset: 0,
            size: 4,
            is_constant: false,
            constant_val: 0,
        };
        let w20 = Varnode {
            space_id: REGISTER_SPACE_ID,
            offset: 0x100,
            size: 4,
            is_constant: false,
            constant_val: 0,
        };
        let x0 = Varnode {
            size: 8,
            ..w0.clone()
        };
        let sum = Varnode {
            space_id: UNIQUE_SPACE_ID,
            offset: 0x2000,
            size: 4,
            is_constant: false,
            constant_val: 0,
        };
        let ret_addr = Varnode::constant(0, 8);
        let func = PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x1000,
                    successors: vec![2],
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Branch,
                        address: 0x1000,
                        output: None,
                        inputs: vec![Varnode::constant(0x1020, 8)],
                        asm_mnemonic: None,
                    }],
                },
                PcodeBasicBlock {
                    index: 1,
                    start_address: 0x1010,
                    successors: vec![2],
                    ops: vec![
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::Copy,
                            address: 0x1010,
                            output: Some(w20.clone()),
                            inputs: vec![Varnode::constant(7, 4)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 2,
                            opcode: PcodeOpcode::Branch,
                            address: 0x1014,
                            output: None,
                            inputs: vec![Varnode::constant(0x1020, 8)],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 2,
                    start_address: 0x1020,
                    successors: vec![],
                    ops: vec![
                        PcodeOp {
                            seq_num: 3,
                            opcode: PcodeOpcode::IntAdd,
                            address: 0x1020,
                            output: Some(sum.clone()),
                            inputs: vec![w0, w20],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 4,
                            opcode: PcodeOpcode::IntZExt,
                            address: 0x1020,
                            output: Some(x0),
                            inputs: vec![sum],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 5,
                            opcode: PcodeOpcode::Return,
                            address: 0x1020,
                            output: None,
                            inputs: vec![ret_addr],
                            asm_mnemonic: None,
                        },
                    ],
                },
            ],
        };
        let options = test_options();
        let builder = PreviewBuilder::new(&func, &options, None);

        assert!(builder.forwarding_block_defines_return_tail_live_in(1, 2));
        assert!(!builder.forwarding_block_defines_return_tail_live_in(0, 2));
    }
}
