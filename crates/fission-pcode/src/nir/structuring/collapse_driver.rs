//! Explicit CollapseDriver rule engine (Ghidra ActionStructureTransform analog).

use super::driver::collapse::{CollapseRule, ACTIVE_COLLAPSE_RULES};
use super::*;

/// Ghidra-style collapse rule driver: dispatches [`CollapseRule`] reducers on a block index.
pub(crate) struct CollapseDriver;

impl CollapseDriver {
    pub(crate) const IDEAL_RULES: &'static [CollapseRule] = &[
        CollapseRule::Switch,
        CollapseRule::ForLoop,
        CollapseRule::DoWhile,
        CollapseRule::WhileDo,
        CollapseRule::InfLoopBreak,
        CollapseRule::InfLoop,
        CollapseRule::Conditional,
    ];

    pub(crate) fn active_rules() -> &'static [CollapseRule; 9] {
        &ACTIVE_COLLAPSE_RULES
    }

    pub(crate) fn apply_rule<'a>(
        builder: &mut PreviewBuilder<'a>,
        rule: CollapseRule,
        idx: usize,
        follow: Option<usize>,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        match rule {
            CollapseRule::Switch => builder.try_lower_switch(idx),
            CollapseRule::ForLoop => builder.try_lower_for(idx),
            CollapseRule::DoWhile => {
                let mut dw = builder.try_lower_dowhile(idx)?;
                if dw.is_none() {
                    dw = builder.try_lower_multiblock_dowhile(idx)?;
                }
                Ok(dw)
            }
            CollapseRule::WhileDo => builder.try_lower_while(idx),
            CollapseRule::InfLoopBreak => builder.try_lower_infloop_with_break(idx),
            CollapseRule::InfLoop => {
                let mut inf = builder.try_lower_infloop(idx);
                if inf.is_err() || matches!(inf, Ok(None)) {
                    inf = builder.try_lower_multiblock_infloop(idx);
                }
                inf
            }
            CollapseRule::Conditional => {
                let mut cond = builder.try_lower_short_circuit_if(idx);
                if cond.is_err() || matches!(cond, Ok(None)) {
                    cond = builder.try_reduce_if_else_with_follow(idx, follow);
                }
                if cond.is_err() || matches!(cond, Ok(None)) {
                    cond = builder.try_lower_if_else(idx);
                }
                if cond.is_err() || matches!(cond, Ok(None)) {
                    cond = builder.try_lower_if(idx);
                }
                cond
            }
            CollapseRule::Sequence | CollapseRule::Unstructured => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapse_driver_exposes_ghidra_rule_order() {
        let names: Vec<_> = CollapseDriver::active_rules()
            .iter()
            .map(|rule| rule.name())
            .collect();
        assert_eq!(
            names,
            vec![
                "switch", "for", "dowhile", "while", "loop_control", "infloop", "conditional",
                "sequence", "unstructured",
            ]
        );
    }
}
