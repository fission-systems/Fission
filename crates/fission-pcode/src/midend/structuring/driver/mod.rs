use super::cleanup::child_body_has_entry_label;
use super::irreducible::{compute_fas_virtual_gotos, compute_node_splits};
use super::*;

mod admission;
pub use admission::*;

pub(crate) mod collapse;
mod orphan_repair;

pub(crate) use fission_midend_structuring::apply_blockgraph_collapse_admission_gate;

impl<'a> PreviewBuilder<'a> {
    #[cfg(test)]
    fn is_switch_scaffold_stmt(stmt: &DirStmt) -> bool {
        fission_midend_structuring::is_switch_scaffold_stmt(stmt)
    }

    #[cfg(test)]
    fn switch_stmt_has_scaffold_only_arms(stmt: &DirStmt) -> bool {
        fission_midend_structuring::switch_stmt_has_scaffold_only_arms(stmt)
    }

    fn region_kind_for_stmt(stmt: &DirStmt) -> Option<RegionKind> {
        fission_midend_structuring::region_kind_for_stmt(stmt)
    }

    fn region_selector_or_condition(stmt: &DirStmt) -> Option<String> {
        fission_midend_structuring::region_selector_or_condition(stmt)
    }

    fn build_region_proof(
        &self,
        start_idx: usize,
        skip_to: usize,
        stmt: &DirStmt,
    ) -> Option<RegionProof> {
        let kind = Self::region_kind_for_stmt(stmt)?;
        Some(RegionProof::structured(
            kind,
            start_idx,
            skip_to,
            Self::region_selector_or_condition(stmt),
        ))
    }

    pub(crate) fn record_region_candidate_impl(&mut self, proof: &RegionProof) {
        self.telemetry.structuring.region_proof_candidate_count += 1;
        if proof.proof_complete {
            self.telemetry.structuring.region_proof_completed_count += 1;
        }
        if matches!(proof.kind, RegionKind::Conditional) {
            self.telemetry
                .structuring
                .conditional_region_candidate_count += 1;
        }
    }

    pub(crate) fn record_selected_region_impl(&mut self, node: &StructureNode) {
        if matches!(
            node.kind,
            StructureNodeKind::Region(RegionKind::Conditional)
        ) {
            self.telemetry.structuring.conditional_region_promoted_count += 1;
        }
    }

    fn consider_structured_candidate(
        &mut self,
        rule: CollapseRule,
        start_idx: usize,
        targeted: &HashSet<u64>,
        last_structuring_failure: &mut Option<MlilPreviewError>,
        candidates: &mut Vec<CollapseCandidate>,
        result: Result<Option<(DirStmt, usize)>, MlilPreviewError>,
    ) -> Result<(), MlilPreviewError> {
        fission_midend_structuring::sese_driver::consider_structured_candidate(
            self,
            rule,
            start_idx,
            targeted,
            last_structuring_failure,
            candidates,
            result,
        )
    }

    fn select_structured_candidate(
        &self,
        candidates: Vec<CollapseCandidate>,
    ) -> Option<CollapseCandidate> {
        fission_midend_structuring::sese_driver::select_structured_candidate(candidates)
    }

    pub(super) fn promote_guarded_tail_regions_until_stable(&mut self, body: &mut Vec<DirStmt>) {
        fission_midend_structuring::promote_guarded_tail_regions_until_stable(self, body)
    }

    pub(crate) fn build_multiblock_body(&mut self) -> Result<Vec<DirStmt>, MlilPreviewError> {
        CollapseDriver::run(self)
    }

    pub(crate) fn sese_region_proof_budget_exceeded(&self) -> bool {
        let calls = self.sese_region_proof_calls.get() + 1;
        self.sese_region_proof_calls.set(calls);
        calls > SESE_REGION_PROOF_BUDGET_CALLS
    }

    pub(crate) fn reset_sese_region_proof_budget(&mut self) {
        self.sese_region_proof_calls.set(0);
    }

    pub(crate) fn build_sese_region_body(
        &mut self,
        entry: usize,
        exit: usize,
        child_map: crate::fast_hash::FastMap<usize, (Vec<DirStmt>, usize, RegionProof)>,
    ) -> Result<Vec<DirStmt>, MlilPreviewError> {
        let child_map_std: HashMap<_, _> = child_map.into_iter().collect();
        fission_midend_structuring::sese_driver::build_sese_region_body(
            self, entry, exit, child_map_std,
        )
    }

    pub(crate) fn structuring_admission_reason(
        &self,
        scc_irreducible_count: usize,
        max_scc_component_size: usize,
    ) -> StructuringAdmissionReason {
        let total_ops: usize = self.pcode.blocks.iter().map(|block| block.ops.len()).sum();
        let block_count = self.pcode.blocks.len();
        let edge_count: usize = self.successors.iter().map(Vec::len).sum();
        let multi_pred_blocks = self
            .predecessors
            .iter()
            .filter(|preds| preds.len() > 1)
            .count();
        let max_predecessors = self.predecessors.iter().map(Vec::len).max().unwrap_or(0);
        decide_structuring_admission(StructuringAdmissionInput {
            block_count,
            total_ops,
            edge_count,
            multi_pred_blocks,
            max_predecessors,
            scc_irreducible_count,
            max_scc_component_size,
            explicit_force_linear: self.options.force_linear_structuring,
        })
    }
}

pub(crate) use fission_midend_structuring::structuring_diag_enabled;

#[cfg(test)]
pub(crate) fn promote_single_entry_guarded_tail_regions_for_test(
    body: &mut Vec<DirStmt>,
) -> PreviewBuildStats {
    let dummy = PcodeFunction { blocks: Vec::new() };
    let options = MlilPreviewOptions {
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
        is_data_ref_origin: false,
        structuring_engine: StructuringEngineKind::GraphCollapseV1,
        global_names: Default::default(),
        global_sizes: Default::default(),
        relocation_names: Default::default(),
        calling_convention: Default::default(),
        userops: Default::default(),
        cspec_param_offsets: None,
        cspec_stack_arg_base: None,
        cspec_extrapop: None,
        sla_register_map: None,
        cspec_return_offset: None,
        cspec_float_return_offset: None,
        cspec_return_target: None,
        pspec_programcounter: None,
        pspec_tracked_context: Vec::new(),
        pspec_hidden_registers: Default::default(),
    };
    let mut builder = PreviewBuilder::new(&dummy, &options, None);
    builder.promote_guarded_tail_regions_until_stable(body);
    builder.preview_build_stats()
}

#[cfg(test)]
pub(crate) fn discover_guarded_tail_candidates_for_test(body: &[DirStmt]) -> PreviewBuildStats {
    discover_guarded_tail_candidates_for_stats(body)
}

pub(crate) fn discover_guarded_tail_candidates_for_stats(body: &[DirStmt]) -> PreviewBuildStats {
    let dummy = PcodeFunction { blocks: Vec::new() };
    let options = MlilPreviewOptions {
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
        is_data_ref_origin: false,
        structuring_engine: StructuringEngineKind::GraphCollapseV1,
        global_names: Default::default(),
        global_sizes: Default::default(),
        relocation_names: Default::default(),
        calling_convention: Default::default(),
        userops: Default::default(),
        cspec_param_offsets: None,
        cspec_stack_arg_base: None,
        cspec_extrapop: None,
        sla_register_map: None,
        cspec_return_offset: None,
        cspec_float_return_offset: None,
        cspec_return_target: None,
        pspec_programcounter: None,
        pspec_tracked_context: Vec::new(),
        pspec_hidden_registers: Default::default(),
    };
    let mut builder = PreviewBuilder::new(&dummy, &options, None);
    builder.discover_guarded_tail_candidates(body);
    builder.preview_build_stats()
}

#[cfg(test)]
mod tests {
    use super::{
        PreviewBuilder, StructuringAdmissionInput, StructuringAdmissionReason,
        apply_blockgraph_collapse_admission_gate, decide_structuring_admission,
    };
    use crate::PcodeFunction;
    use crate::midend::ir::{MlilPreviewOptions, NirType, StructuringEngineKind};
use fission_midend_dir::{DirExpr, DirStmt, DirSwitchCase};
    use crate::midend::{CollapseCandidate, CollapseRule, RegionKind, RegionProof, StructureNode};

    fn const_expr(value: i64) -> DirExpr {
        DirExpr::Const(
            value,
            NirType::Int {
                bits: 32,
                signed: true,
            },
        )
    }

    #[test]
    fn switch_scaffold_detection_accepts_goto_only_arms() {
        let stmt = DirStmt::Switch {
            expr: const_expr(0),
            cases: vec![
                DirSwitchCase {
                    values: vec![0],
                    body: vec![DirStmt::Goto("case_0".to_string())],
                },
                DirSwitchCase {
                    values: vec![1],
                    body: vec![DirStmt::Goto("case_1".to_string())],
                },
            ],
            default: vec![DirStmt::Goto("default".to_string())],
        };
        assert!(PreviewBuilder::switch_stmt_has_scaffold_only_arms(&stmt));
    }

    #[test]
    fn switch_scaffold_detection_rejects_payload_arms() {
        let stmt = DirStmt::Switch {
            expr: const_expr(0),
            cases: vec![DirSwitchCase {
                values: vec![0],
                body: vec![DirStmt::Expr(const_expr(1))],
            }],
            default: vec![],
        };
        assert!(!PreviewBuilder::switch_stmt_has_scaffold_only_arms(&stmt));
    }

    fn test_builder_with_engine(engine: StructuringEngineKind) -> PreviewBuilder<'static> {
        let dummy = Box::leak(Box::new(PcodeFunction { blocks: Vec::new() }));
        let options = Box::leak(Box::new(MlilPreviewOptions {
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
            is_data_ref_origin: false,
            structuring_engine: engine,
            global_names: Default::default(),
            global_sizes: Default::default(),
            relocation_names: Default::default(),
            calling_convention: Default::default(),
            userops: Default::default(),
            cspec_param_offsets: None,
            cspec_stack_arg_base: None,
            cspec_extrapop: None,
            sla_register_map: None,
            cspec_return_offset: None,
            cspec_float_return_offset: None,
            cspec_return_target: None,
            pspec_programcounter: None,
            pspec_tracked_context: Vec::new(),
            pspec_hidden_registers: Default::default(),
        }));
        PreviewBuilder::new(dummy, options, None)
    }

    fn candidate(skip_to: usize, rule: CollapseRule) -> CollapseCandidate {
        CollapseCandidate {
            rule,
            node: StructureNode::region(
                usize::MAX,
                DirStmt::If {
                    cond: const_expr(1),
                    then_body: vec![],
                    else_body: vec![],
                },
                skip_to,
                RegionProof::structured(RegionKind::Conditional, 0, skip_to, Some("cond".into())),
            ),
        }
    }

    #[test]
    fn graph_collapse_v1_preserves_attempt_order() {
        let builder = test_builder_with_engine(StructuringEngineKind::GraphCollapseV1);
        let selected = builder
            .select_structured_candidate(vec![
                candidate(2, CollapseRule::Conditional),
                candidate(8, CollapseRule::Switch),
            ])
            .expect("graph candidate");
        assert_eq!(selected.node.skip_to, 2);
    }

    #[test]
    fn legacy_scored_alias_still_preserves_graph_attempt_order() {
        let builder = test_builder_with_engine(StructuringEngineKind::LegacyScored);
        let selected = builder
            .select_structured_candidate(vec![
                candidate(2, CollapseRule::Conditional),
                candidate(8, CollapseRule::Switch),
            ])
            .expect("legacy candidate");
        assert_eq!(selected.node.skip_to, 2);
    }

    #[test]
    fn structuring_admission_prefers_graph_collapse_for_reducible_medium_cfg() {
        let decision = decide_structuring_admission(StructuringAdmissionInput {
            block_count: 31,
            total_ops: 620,
            edge_count: 58,
            multi_pred_blocks: 10,
            max_predecessors: 3,
            scc_irreducible_count: 0,
            max_scc_component_size: 31,
            explicit_force_linear: false,
        });
        assert_eq!(decision, StructuringAdmissionReason::GraphCollapse);
    }

    #[test]
    fn structuring_admission_forces_linear_for_irreducible_budget() {
        let decision = decide_structuring_admission(StructuringAdmissionInput {
            block_count: 72,
            total_ops: 960,
            edge_count: 220,
            multi_pred_blocks: 18,
            max_predecessors: 6,
            scc_irreducible_count: 2,
            max_scc_component_size: 28,
            explicit_force_linear: false,
        });
        assert_eq!(decision, StructuringAdmissionReason::IrreducibleBudget);
    }

    #[test]
    fn structuring_admission_forces_linear_for_explicit_override() {
        let decision = decide_structuring_admission(StructuringAdmissionInput {
            block_count: 12,
            total_ops: 80,
            edge_count: 14,
            multi_pred_blocks: 1,
            max_predecessors: 2,
            scc_irreducible_count: 0,
            max_scc_component_size: 4,
            explicit_force_linear: true,
        });
        assert_eq!(decision, StructuringAdmissionReason::ExplicitForceLinear);
    }

    #[test]
    fn structuring_admission_forces_linear_for_extreme_budget() {
        let decision = decide_structuring_admission(StructuringAdmissionInput {
            block_count: 220,
            total_ops: 3_400,
            edge_count: 980,
            multi_pred_blocks: 40,
            max_predecessors: 8,
            scc_irreducible_count: 0,
            max_scc_component_size: 80,
            explicit_force_linear: false,
        });
        assert_eq!(decision, StructuringAdmissionReason::ExtremeBudget);
    }

    #[test]
    fn blockgraph_collapse_gate_allows_irreducible_budget_graph_collapse() {
        let decision = apply_blockgraph_collapse_admission_gate(
            StructuringAdmissionReason::IrreducibleBudget,
            true,
        );
        assert_eq!(decision, StructuringAdmissionReason::GraphCollapse);
    }

    #[test]
    fn blockgraph_collapse_gate_stays_fail_closed_for_extreme_budget() {
        let decision = apply_blockgraph_collapse_admission_gate(
            StructuringAdmissionReason::ExtremeBudget,
            true,
        );
        assert_eq!(decision, StructuringAdmissionReason::ExtremeBudget);
    }

    #[test]
    fn blockgraph_collapse_gate_stays_fail_closed_for_explicit_override() {
        let decision = apply_blockgraph_collapse_admission_gate(
            StructuringAdmissionReason::ExplicitForceLinear,
            true,
        );
        assert_eq!(decision, StructuringAdmissionReason::ExplicitForceLinear);
    }

    #[test]
    fn blockgraph_collapse_gate_is_noop_when_disabled() {
        let decision = apply_blockgraph_collapse_admission_gate(
            StructuringAdmissionReason::IrreducibleBudget,
            false,
        );
        assert_eq!(decision, StructuringAdmissionReason::IrreducibleBudget);
    }
}
