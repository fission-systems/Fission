use crate::report::insights::MismatchRowDelta;
use crate::report::snapshot::AutomationSummary;
use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoStopDecisionGate {
    pub decision: String,
    pub rationale: String,
}

pub fn evaluate_quality_gate(
    summary: &AutomationSummary,
    baseline_summary: Option<&AutomationSummary>,
    row_deltas: &[MismatchRowDelta],
    current_families: &BTreeMap<String, usize>,
    baseline_families: Option<&BTreeMap<String, usize>>,
    structuring_family_counts: &BTreeMap<String, usize>,
) -> GoStopDecisionGate {
    if let (Some(baseline), Some(base_fams)) = (baseline_summary, baseline_families) {
        let structuring_delta = current_families.get("structuring").copied().unwrap_or(0) as isize
            - base_fams.get("structuring").copied().unwrap_or(0) as isize;
        let abi_delta = current_families.get("abi").copied().unwrap_or(0) as isize
            - base_fams.get("abi").copied().unwrap_or(0) as isize;
        let variadic_delta = current_families.get("variadic").copied().unwrap_or(0) as isize
            - base_fams.get("variadic").copied().unwrap_or(0) as isize;
        let call_signature_delta = current_families.get("call_signature").copied().unwrap_or(0)
            as isize
            - base_fams.get("call_signature").copied().unwrap_or(0) as isize;
        let security_delta = current_families.get("security").copied().unwrap_or(0) as isize
            - base_fams.get("security").copied().unwrap_or(0) as isize;

        let has_material_improvement = structuring_delta < 0
            || abi_delta > 0
            || variadic_delta > 0
            || call_signature_delta > 0
            || security_delta > 0;

        let family_ranking = structuring_family_counts
            .iter()
            .filter(|(_, count)| **count > 0)
            .map(|(k, v)| (k.clone(), *v))
            .collect::<Vec<_>>();
            
        let dominant_family = family_ranking
            .iter()
            .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
            .map(|(name, _)| name.as_str());

        let safe_dominant = matches!(
            dominant_family,
            Some("follow_failure") | Some("loop_exit") | Some("region_legality")
        );

        let irreducible_scc_delta = summary
            .aggregate
            .nir_build_stats_totals
            .structuring_irreducible_scc_count as isize
            - baseline
                .aggregate
                .nir_build_stats_totals
                .structuring_irreducible_scc_count as isize;

        let irreducible_header_delta = summary
            .aggregate
            .nir_build_stats_totals
            .structuring_irreducible_header_count as isize
            - baseline
                .aggregate
                .nir_build_stats_totals
                .structuring_irreducible_header_count as isize;

        let has_any_row_regression = row_deltas.iter().any(|d| d.mismatch_delta > 0);

        if has_material_improvement
            && structuring_delta <= 0
            && safe_dominant
            && irreducible_scc_delta <= 0
            && irreducible_header_delta <= 0
            && abi_delta >= 0
            && variadic_delta >= 0
            && call_signature_delta >= 0
            && security_delta >= 0
            && !has_any_row_regression
        {
            GoStopDecisionGate {
                decision: "go_p5h3g_candidate".to_string(),
                rationale: "semantic family deltas are non-regressive and no row-level regressions detected"
                    .to_string(),
            }
        } else if has_any_row_regression {
            GoStopDecisionGate {
                decision: "stop_row_level_regression".to_string(),
                rationale: "row-level regression detected (zero-tolerance policy)".to_string(),
            }
        } else {
            GoStopDecisionGate {
                decision: "stop_hold_p5h3f".to_string(),
                rationale: "semantic family delta vector is regressive or structuring safety signals are insufficient"
                    .to_string(),
            }
        }
    } else {
        GoStopDecisionGate {
            decision: "stop_no_baseline".to_string(),
            rationale: "baseline summary/candidates unavailable; cannot compute stable go/stop gate".to_string(),
        }
    }
}
