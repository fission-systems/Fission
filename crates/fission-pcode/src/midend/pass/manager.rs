use crate::midend::pass::{AnalysisStore, NirFunc};
use fission_midend_core::ir::{
    IrInvariant, IrInvariantSet, IrStage, PassContract, StructuralValidationReport,
    StructuralViolation, ValidationMode, ValidationSite, validate_cfg_edges,
};
use fission_midend_prehir::ir::validate_prehir_stmts;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PassResult {
    NoChange,
    Changed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RepeatMode {
    Once,
    UntilStable,
}

/// Hard cap on the number of [`NirPass`] instances registered in a single [`PassManager`].
///
/// # Anti-overfitting policy
///
/// To raise this limit, you must file an ADR (`docs/adr/`) explaining the structural gap
/// that the new pass fills. Structural gap = a CFG/dominance/SCC/loop invariant that is
/// *not* already covered by the existing 4 passes.
///
/// **Never raise the cap to accommodate a pass that targets a specific function, binary,
/// address range, or compiler output pattern.** That is overfitting.
pub(crate) const MAX_STRUCTURING_PASSES: usize = 6;

/// The structural CFG invariant that justifies a [`NirPass`].
///
/// Every pass **must** declare which invariant it operates on. This declaration is:
/// - Checked at review time to prevent address/function-specific workarounds.
/// - Documented in [`NirPass::invariant_basis`].
///
/// # Rules
///
/// - The basis must be provable from CFG facts alone (edge sets, dom/postdom trees,
///   SCC membership, loop bodies).
/// - It must **not** depend on: function name, binary address, compiler flags,
///   specific instruction counts, or any other binary-specific knowledge.
/// - If no existing variant applies, the pass needs an ADR before proceeding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InvariantBasis {
    /// CFG edge classification: forward, back, cross, tree edges.
    /// Use for passes that operate on a single well-classified edge type.
    EdgeClassification,

    /// Dominator tree structure (idom, dominance frontier).
    /// Use for passes that require `a dom b` or natural interval membership.
    DominatorTree,

    /// Post-dominator tree structure (ipostdom, control dependence).
    /// Use for passes that identify common post-dominators (follows, join nodes).
    PostDominatorTree,

    /// SCC membership and inter-SCC edge structure.
    /// Use for reducibility analysis, irreducible-graph transforms (node split / FAS).
    StronglyConnectedComponents,

    /// Loop body analysis: back-edges, natural exits, header dominance.
    /// Use for while/do-while/for/infloop structuring passes.
    LoopBody,

    /// Post-structuring cleanup: operates on already-structured HIR statements.
    /// The pass may read structured HIR but must not re-analyze raw CFG edges
    /// by function identity or address.
    PostStructuringCleanup,
}

pub(crate) trait NirPass {
    fn name(&self) -> &str;

    /// The structural CFG invariant that justifies this pass.
    ///
    /// # Contract
    ///
    /// The implementation **must not** inspect function names, binary addresses,
    /// instruction counts tuned to a specific binary, or any other non-structural
    /// binary-specific data. Passes that do so are overfitting and will be rejected
    /// at review.
    ///
    /// See [`InvariantBasis`] for the allowed variants.
    fn invariant_basis(&self) -> InvariantBasis;

    /// Cross-pass structural contract enforced when `PassManager` runs in
    /// `ValidationMode::VerifyEach` (or at stage boundaries).
    fn contract(&self) -> PassContract {
        PassContract::default_cfg_mutating()
    }

    fn run(
        &mut self,
        ir: &mut NirFunc<'_, '_>,
        store: &mut AnalysisStore,
    ) -> Result<PassResult, String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PassRunError {
    PassFailed(String),
    InvariantViolation(StructuralValidationReport),
}

impl fmt::Display for PassRunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PassFailed(msg) => f.write_str(msg),
            Self::InvariantViolation(report) => write!(f, "IR invariant violation: {report}"),
        }
    }
}

pub(crate) fn validate_nir_func(
    ir: &NirFunc<'_, '_>,
    invariants: IrInvariantSet,
    pass: &str,
    revision: u64,
) -> Result<(), StructuralValidationReport> {
    let mut violations = Vec::new();
    let total_blocks = ir.block_count();

    if invariants.contains(IrInvariant::CfgEdgeSymmetry) {
        violations.extend(validate_cfg_edges(
            total_blocks,
            ir.successors(),
            ir.predecessors(),
            IrStage::CfgStructuring,
            pass,
            revision,
        ));
        let orig_count = ir.builder.pcode.blocks.len();
        for (virt_idx, &orig) in ir.virtual_block_map().iter().enumerate() {
            if orig >= orig_count {
                let block_idx = (orig_count + virt_idx) as u32;
                violations.push(StructuralViolation::new(
                    IrStage::CfgStructuring,
                    pass,
                    revision,
                    IrInvariant::CfgEdgeSymmetry,
                    ValidationSite::Block { block: block_idx },
                    format!(
                        "virtual block {block_idx} maps to out-of-bounds pcode block {orig} (count={orig_count})"
                    ),
                    &format!("virtual_block_map[{virt_idx}]={orig} >= {orig_count}"),
                ));
            }
        }
    }

    let body_invariants = invariants.without(IrInvariantSet::STRUCTURING_CFG);
    if !body_invariants.is_empty()
        && let Some(body) = ir.structured_body()
        && let Err(mut body_report) = validate_prehir_stmts(
            body,
            body_invariants,
            None,
            IrStage::CfgStructuring,
            pass,
            revision,
        )
    {
        violations.append(&mut body_report.violations);
    }

    StructuralValidationReport::new(
        IrStage::CfgStructuring,
        pass,
        revision,
        invariants,
        violations,
    )
    .into_result()
}

pub(crate) struct PassManager {
    passes: Vec<Box<dyn NirPass>>,
    repeat_mode: RepeatMode,
    max_rounds: usize,
    validation_mode: ValidationMode,
}

impl PassManager {
    pub(crate) fn new(repeat_mode: RepeatMode, max_rounds: usize) -> Self {
        Self {
            passes: Vec::new(),
            repeat_mode,
            max_rounds,
            validation_mode: ValidationMode::from_env(),
        }
    }

    pub(crate) fn with_validation_mode(mut self, mode: ValidationMode) -> Self {
        self.validation_mode = mode;
        self
    }

    pub(crate) fn add_pass(&mut self, pass: Box<dyn NirPass>) {
        debug_assert!(
            self.passes.len() < MAX_STRUCTURING_PASSES,
            "Pass count cap ({MAX_STRUCTURING_PASSES}) exceeded when adding '{}'. \
             Either retire/generalize an existing pass, or file an ADR explaining \
             the new structural gap before raising the cap.",
            pass.name()
        );
        self.passes.push(pass);
    }

    pub(crate) fn run(
        &mut self,
        ir: &mut NirFunc<'_, '_>,
        store: &mut AnalysisStore,
    ) -> Result<PassResult, String> {
        self.run_checked(ir, store).map_err(|e| e.to_string())
    }

    pub(crate) fn run_checked(
        &mut self,
        ir: &mut NirFunc<'_, '_>,
        store: &mut AnalysisStore,
    ) -> Result<PassResult, PassRunError> {
        let mut overall_changed = PassResult::NoChange;
        let mut round = 0;
        let mut revision = 0u64;

        if self.validation_mode != ValidationMode::Off {
            validate_nir_func(
                ir,
                IrInvariantSet::STRUCTURING_CFG,
                "stage:structuring:entry",
                revision,
            )
            .map_err(PassRunError::InvariantViolation)?;
        }

        loop {
            let mut round_changed = false;
            for pass in &mut self.passes {
                let contract = pass.contract();
                let cfg_version_before = ir.cfg_version();

                if self.validation_mode == ValidationMode::VerifyEach
                    && !contract.required.is_empty()
                {
                    let pre_label = format!("pre:{}", pass.name());
                    validate_nir_func(ir, contract.required, &pre_label, revision)
                        .map_err(PassRunError::InvariantViolation)?;
                }

                match pass.run(ir, store).map_err(PassRunError::PassFailed)? {
                    PassResult::Changed => {
                        round_changed = true;
                        overall_changed = PassResult::Changed;
                    }
                    PassResult::NoChange => {}
                }
                revision += 1;

                if self.validation_mode == ValidationMode::VerifyEach {
                    if contract.preserves_cfg_analyses && ir.cfg_version() != cfg_version_before {
                        return Err(PassRunError::InvariantViolation(
                            StructuralValidationReport::new(
                                IrStage::CfgStructuring,
                                pass.name(),
                                revision,
                                IrInvariantSet::single(IrInvariant::CfgEdgeSymmetry),
                                vec![StructuralViolation::new(
                                    IrStage::CfgStructuring,
                                    pass.name(),
                                    revision,
                                    IrInvariant::CfgEdgeSymmetry,
                                    ValidationSite::Function {
                                        name: ir.builder.pcode.blocks.first().map_or_else(
                                            || "cfg".to_string(),
                                            |b| format!("0x{:x}", b.start_address),
                                        ),
                                    },
                                    format!(
                                        "pass '{}' declared preserves_cfg_analyses=true but mutated CFG version ({cfg_version_before} -> {})",
                                        pass.name(),
                                        ir.cfg_version()
                                    ),
                                    &format!(
                                        "cfg_version {cfg_version_before} -> {}",
                                        ir.cfg_version()
                                    ),
                                )],
                            ),
                        ));
                    }
                    if !contract.guaranteed.is_empty() {
                        validate_nir_func(ir, contract.guaranteed, pass.name(), revision)
                            .map_err(PassRunError::InvariantViolation)?;
                    }
                }
            }

            round += 1;

            if self.repeat_mode == RepeatMode::Once || !round_changed || round >= self.max_rounds {
                break;
            }
        }

        if self.validation_mode != ValidationMode::Off {
            let exit_invariants = if ir.structured_body().is_some() {
                IrInvariantSet::STRUCTURING_OUTPUT
            } else {
                IrInvariantSet::STRUCTURING_CFG
            };
            validate_nir_func(ir, exit_invariants, "stage:structuring:exit", revision)
                .map_err(PassRunError::InvariantViolation)?;
        }

        Ok(overall_changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midend::builder::PreviewBuilder;
    use crate::midend::ir::MlilPreviewOptions;
    use crate::pcode::{PcodeBasicBlock, PcodeFunction};
    use std::cell::RefCell;
    use std::rc::Rc;

    fn make_dummy_builder() -> PreviewBuilder<'static> {
        let pcode = Box::leak(Box::new(PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x1000,
                    successors: vec![1],
                    ops: Vec::new(),
                },
                PcodeBasicBlock {
                    index: 1,
                    start_address: 0x2000,
                    successors: Vec::new(),
                    ops: Vec::new(),
                },
            ],
        }));
        let options = Box::leak(Box::new(MlilPreviewOptions {
            is_64bit: true,
            pointer_size: 8,
            ..Default::default()
        }));
        PreviewBuilder::new(pcode, options, None)
    }

    struct MockPass {
        runs: Rc<RefCell<usize>>,
        limit: usize,
    }

    impl NirPass for MockPass {
        fn name(&self) -> &str {
            "MockPass"
        }

        fn invariant_basis(&self) -> InvariantBasis {
            // Test-only stub; no real structural invariant.
            InvariantBasis::EdgeClassification
        }

        fn run(
            &mut self,
            ir: &mut NirFunc<'_, '_>,
            _store: &mut AnalysisStore,
        ) -> Result<PassResult, String> {
            let mut runs = self.runs.borrow_mut();
            *runs += 1;
            if *runs < self.limit {
                // Mutate the successors to force version bump
                ir.successors_mut();
                Ok(PassResult::Changed)
            } else {
                Ok(PassResult::NoChange)
            }
        }
    }

    #[test]
    fn test_cache_invalidation_on_mutation() {
        let mut builder = make_dummy_builder();
        let mut ir = NirFunc::new(&mut builder);
        let mut store = AnalysisStore::new();

        // 1. Initial access caches analyses
        assert_eq!(ir.cfg_version(), 0);
        let _facts1 = store.cfg_facts(&ir);
        assert_eq!(store.cfg_version_for_test(), Some(0));

        // 2. Querying again with no mutation hits the cache (version remains 0)
        let _facts2 = store.cfg_facts(&ir);
        assert_eq!(store.cfg_version_for_test(), Some(0));

        // 3. Mutating ir increments version
        ir.successors_mut();
        assert_eq!(ir.cfg_version(), 1);

        // 4. Querying again triggers invalidation and re-computation
        let _facts3 = store.cfg_facts(&ir);
        assert_eq!(store.cfg_version_for_test(), Some(1));
    }

    #[test]
    fn test_pass_manager_until_stable() {
        let mut builder = make_dummy_builder();
        let mut ir = NirFunc::new(&mut builder);
        let mut store = AnalysisStore::new();

        let runs = Rc::new(RefCell::new(0));
        let mut pm = PassManager::new(RepeatMode::UntilStable, 10);
        pm.add_pass(Box::new(MockPass {
            runs: runs.clone(),
            limit: 3,
        }));

        let res = pm.run(&mut ir, &mut store).unwrap();
        assert_eq!(res, PassResult::Changed);
        assert_eq!(*runs.borrow(), 3);
    }

    #[test]
    fn test_pass_manager_once() {
        let mut builder = make_dummy_builder();
        let mut ir = NirFunc::new(&mut builder);
        let mut store = AnalysisStore::new();

        let runs = Rc::new(RefCell::new(0));
        let mut pm = PassManager::new(RepeatMode::Once, 10);
        pm.add_pass(Box::new(MockPass {
            runs: runs.clone(),
            limit: 3,
        }));

        let res = pm.run(&mut ir, &mut store).unwrap();
        assert_eq!(res, PassResult::Changed);
        assert_eq!(*runs.borrow(), 1);
    }

    struct CfgCorruptingPass {
        ran_third: Rc<RefCell<bool>>,
    }

    impl NirPass for CfgCorruptingPass {
        fn name(&self) -> &str {
            "CfgCorruptingPass"
        }

        fn invariant_basis(&self) -> InvariantBasis {
            InvariantBasis::EdgeClassification
        }

        fn contract(&self) -> PassContract {
            PassContract::default_cfg_mutating()
        }

        fn run(
            &mut self,
            ir: &mut NirFunc<'_, '_>,
            _store: &mut AnalysisStore,
        ) -> Result<PassResult, String> {
            let _ = &self.ran_third;
            // Remove predecessor symmetry while adding a successor.
            ir.successors_mut()[0] = vec![1, 99];
            Ok(PassResult::Changed)
        }
    }

    struct UnannouncedCfgMutationPass;

    impl NirPass for UnannouncedCfgMutationPass {
        fn name(&self) -> &str {
            "UnannouncedCfgMutationPass"
        }

        fn invariant_basis(&self) -> InvariantBasis {
            InvariantBasis::EdgeClassification
        }

        fn contract(&self) -> PassContract {
            PassContract::default_cfg_preserving()
        }

        fn run(
            &mut self,
            ir: &mut NirFunc<'_, '_>,
            _store: &mut AnalysisStore,
        ) -> Result<PassResult, String> {
            ir.successors_mut();
            Ok(PassResult::Changed)
        }
    }

    struct StructuredBodyPass {
        emit_stray_break: bool,
    }

    impl NirPass for StructuredBodyPass {
        fn name(&self) -> &str {
            "StructuredBodyPass"
        }

        fn invariant_basis(&self) -> InvariantBasis {
            InvariantBasis::PostStructuringCleanup
        }

        fn contract(&self) -> PassContract {
            PassContract::new(
                IrInvariantSet::STRUCTURING_CFG,
                IrInvariantSet::STRUCTURING_OUTPUT,
                true,
            )
        }

        fn run(
            &mut self,
            ir: &mut NirFunc<'_, '_>,
            _store: &mut AnalysisStore,
        ) -> Result<PassResult, String> {
            let body = if self.emit_stray_break {
                vec![fission_midend_prehir::PreHirStmt::Break]
            } else {
                vec![fission_midend_prehir::PreHirStmt::Goto(
                    "missing_lbl".to_string(),
                )]
            };
            ir.set_structured_body(body);
            Ok(PassResult::Changed)
        }
    }

    struct RecordRunPass {
        ran: Rc<RefCell<bool>>,
    }

    impl NirPass for RecordRunPass {
        fn name(&self) -> &str {
            "RecordRunPass"
        }

        fn invariant_basis(&self) -> InvariantBasis {
            InvariantBasis::PostStructuringCleanup
        }

        fn contract(&self) -> PassContract {
            PassContract::default_cfg_preserving()
        }

        fn run(
            &mut self,
            _ir: &mut NirFunc<'_, '_>,
            _store: &mut AnalysisStore,
        ) -> Result<PassResult, String> {
            *self.ran.borrow_mut() = true;
            Ok(PassResult::NoChange)
        }
    }

    #[test]
    fn verify_each_localizes_first_cfg_or_structured_violation_deterministically() {
        // 1. Out-of-bounds/asymmetric CFG edge caught immediately at the offending pass.
        let mut builder = make_dummy_builder();
        let mut ir = NirFunc::new(&mut builder);
        let mut store = AnalysisStore::new();
        let ran_third = Rc::new(RefCell::new(false));
        let mut pm =
            PassManager::new(RepeatMode::Once, 1).with_validation_mode(ValidationMode::VerifyEach);
        pm.add_pass(Box::new(MockPass {
            runs: Rc::new(RefCell::new(1)),
            limit: 1,
        }));
        pm.add_pass(Box::new(CfgCorruptingPass {
            ran_third: ran_third.clone(),
        }));
        pm.add_pass(Box::new(RecordRunPass {
            ran: ran_third.clone(),
        }));
        let err = pm.run_checked(&mut ir, &mut store).unwrap_err();
        let PassRunError::InvariantViolation(report) = err else {
            panic!("expected invariant violation, got {err:?}");
        };
        assert_eq!(report.pass, "CfgCorruptingPass");
        assert_eq!(report.revision, 2);
        assert_eq!(report.violations[0].invariant, IrInvariant::CfgEdgeSymmetry);
        assert!(!*ran_third.borrow(), "later pass must not execute");

        // Deterministic formatting across runs.
        let formatted_once = report.to_string();
        let mut builder2 = make_dummy_builder();
        let mut ir2 = NirFunc::new(&mut builder2);
        let mut store2 = AnalysisStore::new();
        let mut pm2 =
            PassManager::new(RepeatMode::Once, 1).with_validation_mode(ValidationMode::VerifyEach);
        pm2.add_pass(Box::new(MockPass {
            runs: Rc::new(RefCell::new(1)),
            limit: 1,
        }));
        pm2.add_pass(Box::new(CfgCorruptingPass {
            ran_third: Rc::new(RefCell::new(false)),
        }));
        let PassRunError::InvariantViolation(report2) =
            pm2.run_checked(&mut ir2, &mut store2).unwrap_err()
        else {
            panic!("expected deterministic invariant violation");
        };
        assert_eq!(formatted_once, report2.to_string());

        // 2. Unannounced CFG mutation violates `preserves_cfg_analyses = true`.
        let mut builder3 = make_dummy_builder();
        let mut ir3 = NirFunc::new(&mut builder3);
        let mut store3 = AnalysisStore::new();
        let mut pm3 =
            PassManager::new(RepeatMode::Once, 1).with_validation_mode(ValidationMode::VerifyEach);
        pm3.add_pass(Box::new(UnannouncedCfgMutationPass));
        let PassRunError::InvariantViolation(report3) =
            pm3.run_checked(&mut ir3, &mut store3).unwrap_err()
        else {
            panic!("expected preserves_cfg_analyses violation");
        };
        assert_eq!(report3.pass, "UnannouncedCfgMutationPass");
        assert!(
            report3.violations[0]
                .message
                .contains("preserves_cfg_analyses")
        );

        // 3. Stray Break and unresolved Goto caught by `VerifyEach`.
        for emit_stray_break in [true, false] {
            let mut b = make_dummy_builder();
            let mut i = NirFunc::new(&mut b);
            let mut s = AnalysisStore::new();
            let mut p = PassManager::new(RepeatMode::Once, 1)
                .with_validation_mode(ValidationMode::VerifyEach);
            p.add_pass(Box::new(StructuredBodyPass { emit_stray_break }));
            let PassRunError::InvariantViolation(rep) = p.run_checked(&mut i, &mut s).unwrap_err()
            else {
                panic!("expected structured violation");
            };
            assert_eq!(rep.pass, "StructuredBodyPass");
            let expected_inv = if emit_stray_break {
                IrInvariant::StructuredControlExits
            } else {
                IrInvariant::LabelGotoResolution
            };
            assert_eq!(rep.violations[0].invariant, expected_inv);
            assert!(!rep.proves_semantic_equivalence());
        }
    }
}
