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
