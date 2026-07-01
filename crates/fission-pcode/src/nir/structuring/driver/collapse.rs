use super::StructureNode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CollapseRule {
    Switch,
    ForLoop,
    DoWhile,
    WhileDo,
    InfLoopBreak,
    InfLoop,
    Conditional,
    Sequence,
    Unstructured,
}

impl CollapseRule {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Switch => "switch",
            Self::ForLoop => "for",
            Self::DoWhile => "dowhile",
            Self::WhileDo => "while",
            Self::InfLoopBreak => "loop_control",
            Self::InfLoop => "infloop",
            Self::Conditional => "conditional",
            Self::Sequence => "sequence",
            Self::Unstructured => "unstructured",
        }
    }
}

/// Active Tier-1 collapse rules, applied in sweep order per block.
///
/// # Pass-gate policy — anti-overfitting rules
///
/// **Adding a new rule requires ALL of the following:**
/// 1. A structural invariant in the table below (dom/SCC/loop/edge — no binary-specific data).
/// 2. A synthetic unit test constructing the CFG from first principles (no real-binary fixtures).
/// 3. A positive benchmark delta: `improvement_count ≥ 3` variants, `regression_count = 0`,
///    `Δtotal_goto ≤ 0`. Run `python benchmark/anti_overfit/checker.py`.
/// 4. A `Sequence` or `Unstructured` placeholder replaced (keeping array length = 9),
///    **or** a filed ADR explaining why the count must grow.
///
/// **The following are NOT acceptable justifications for a new rule:**
/// - "This specific function produces a goto" (function-specific overfitting).
/// - "I saw this 3-block pattern in benchmark binary X" (sample-specific overfitting).
/// - "The similarity score for `bubble_sort` is 0.008" (single-function targeting).
///
/// # Structural contract table
///
/// | Rule          | Invariant basis                                               |
/// |---------------|---------------------------------------------------------------|
/// | Switch        | EdgeClassification: computed-goto / jump-table target edges   |
/// | ForLoop       | LoopBody: single back-edge + tail-update idiom in loop body   |
/// | DoWhile       | LoopBody: back-edge originates at bottom of loop body         |
/// | WhileDo       | LoopBody + DominatorTree: header dom-tree node dominates body |
/// | InfLoopBreak  | LoopBody: loop has no natural exits from its header block     |
/// | InfLoop       | LoopBody: no exits of any kind (pure infinite loop)           |
/// | Conditional   | DominatorTree + PostDominatorTree: if/else convergence follow |
/// | Sequence      | *placeholder* — merge into Conditional or Linear when ready  |
/// | Unstructured  | *placeholder* — fallback; must never add new pattern logic    |
pub(crate) const ACTIVE_COLLAPSE_RULES: [CollapseRule; 9] = [
    CollapseRule::Switch,
    CollapseRule::ForLoop,
    CollapseRule::DoWhile,
    CollapseRule::WhileDo,
    CollapseRule::InfLoopBreak,
    CollapseRule::InfLoop,
    CollapseRule::Conditional,
    CollapseRule::Sequence,
    CollapseRule::Unstructured,
];

#[derive(Debug, Clone)]
pub(crate) struct CollapseCandidate {
    pub(crate) rule: CollapseRule,
    pub(crate) node: StructureNode,
}
