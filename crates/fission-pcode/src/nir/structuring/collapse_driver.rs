//! Explicit CollapseDriver rule engine (Ghidra ActionStructureTransform analog).

use super::driver::collapse::{ACTIVE_COLLAPSE_RULES, CollapseRule};
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

    pub(crate) fn run(builder: &mut PreviewBuilder<'_>) -> Result<Vec<HirStmt>, MlilPreviewError> {
        use crate::nir::pass::{
            AnalysisStore, EarlyReturnPass, IrreducibleReductionPass, NirFunc,
            OrphanGotoRepairPass, PassManager, RepeatMode, SeseStructuringPass,
        };

        builder.structuring_start = Some(std::time::Instant::now());

        let mut ir = NirFunc::new(builder);
        let mut store = AnalysisStore::new();

        let mut pm = PassManager::new(RepeatMode::Once, 1);
        pm.add_pass(Box::new(EarlyReturnPass));
        pm.add_pass(Box::new(IrreducibleReductionPass));
        pm.add_pass(Box::new(SeseStructuringPass));
        pm.add_pass(Box::new(OrphanGotoRepairPass));

        match pm.run(&mut ir, &mut store) {
            Ok(_) => {
                if let Some(body) = ir.structured_body() {
                    Ok(body.to_vec())
                } else {
                    Err(MlilPreviewError::UnsupportedCfgRegionShape)
                }
            }
            Err(err_str) => Err(parse_preview_error(&err_str)),
        }
    }
}

fn parse_preview_error(s: &str) -> MlilPreviewError {
    if s.contains("supports PE x64 only") {
        MlilPreviewError::UnsupportedArchitecture
    } else if s.contains("unsupported architecture") {
        MlilPreviewError::UnsupportedArchitectureDetailed
    } else if s.contains("unsupported control flow") {
        MlilPreviewError::UnsupportedControlFlow
    } else if s.contains("unsupported branch target") {
        MlilPreviewError::UnsupportedCfgBranchTarget
    } else if s.contains("unsupported region shape") {
        MlilPreviewError::UnsupportedCfgRegionShape
    } else if s.contains("unsupported phi join") {
        MlilPreviewError::UnsupportedCfgPhiJoin
    } else if s.contains("unsupported indirect call region") {
        MlilPreviewError::UnsupportedCfgIndirectCallRegion
    } else if s.contains("value lowering failed") {
        MlilPreviewError::LoweringFailed
    } else if s.contains("not a function") {
        MlilPreviewError::NotAFunctionOrphanBlock
    } else if s.starts_with("unsupported pcode pattern:") {
        let pat = s.trim_start_matches("unsupported pcode pattern:").trim();
        MlilPreviewError::UnsupportedPattern(Box::leak(pat.to_string().into_boxed_str()))
    } else {
        MlilPreviewError::UnsupportedCfgRegionShape
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
                "switch",
                "for",
                "dowhile",
                "while",
                "loop_control",
                "infloop",
                "conditional",
                "sequence",
                "unstructured",
            ]
        );
    }
}
