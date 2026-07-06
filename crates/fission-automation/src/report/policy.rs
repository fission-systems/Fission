use crate::report::insights::MismatchRowDelta;
use crate::report::snapshot::AutomationSummary;
use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoStopDecisionGate {
    pub decision: String,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatePolicyConfig {
    pub gate: GateConfig,
    pub thresholds: ThresholdsConfig,
    pub safety: SafetyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateConfig {
    pub allow_row_regression: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdsConfig {
    pub max_structuring_delta: isize,
    pub max_irreducible_scc_delta: isize,
    pub max_irreducible_header_delta: isize,
    pub min_abi_delta: isize,
    pub min_variadic_delta: isize,
    pub min_call_signature_delta: isize,
    pub min_security_delta: isize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyConfig {
    pub allowed_dominant_families: Vec<String>,
}

pub fn evaluate_quality_gate(
    summary: &AutomationSummary,
    baseline_summary: Option<&AutomationSummary>,
    row_deltas: &[MismatchRowDelta],
    current_families: &BTreeMap<String, usize>,
    baseline_families: Option<&BTreeMap<String, usize>>,
    structuring_family_counts: &BTreeMap<String, usize>,
) -> GoStopDecisionGate {
    let policy_path = fission_core::core::path_config::PATHS
        .get_gate_policy_path()
        .expect("FATAL: gate_policy.toml not found. Automation cannot run safely.");
    let policy_str = std::fs::read_to_string(&policy_path)
        .unwrap_or_else(|e| panic!("FATAL: Failed to read gate_policy.toml: {}", e));
    let policy: GatePolicyConfig = toml::from_str(&policy_str)
        .unwrap_or_else(|e| panic!("FATAL: Failed to parse gate_policy.toml: {}", e));

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

        let has_material_improvement = structuring_delta < policy.thresholds.max_structuring_delta
            || abi_delta > policy.thresholds.min_abi_delta
            || variadic_delta > policy.thresholds.min_variadic_delta
            || call_signature_delta > policy.thresholds.min_call_signature_delta
            || security_delta > policy.thresholds.min_security_delta;

        let family_ranking = structuring_family_counts
            .iter()
            .filter(|(_, count)| **count > 0)
            .map(|(k, v)| (k.clone(), *v))
            .collect::<Vec<_>>();
            
        let dominant_family = family_ranking
            .iter()
            .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
            .map(|(name, _)| name.as_str());

        let safe_dominant = dominant_family.map_or(false, |f| {
            policy.safety.allowed_dominant_families.iter().any(|a| a == f)
        });

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
        let violates_row_regression = !policy.gate.allow_row_regression && has_any_row_regression;

        if has_material_improvement
            && structuring_delta <= policy.thresholds.max_structuring_delta
            && safe_dominant
            && irreducible_scc_delta <= policy.thresholds.max_irreducible_scc_delta
            && irreducible_header_delta <= policy.thresholds.max_irreducible_header_delta
            && abi_delta >= policy.thresholds.min_abi_delta
            && variadic_delta >= policy.thresholds.min_variadic_delta
            && call_signature_delta >= policy.thresholds.min_call_signature_delta
            && security_delta >= policy.thresholds.min_security_delta
            && !violates_row_regression
        {
            GoStopDecisionGate {
                decision: "go_p5h3g_candidate".to_string(),
                rationale: "semantic family deltas are non-regressive and no row-level regressions detected"
                    .to_string(),
            }
        } else if violates_row_regression {
            GoStopDecisionGate {
                decision: "stop_row_level_regression".to_string(),
                rationale: "row-level regression detected (violates zero-tolerance policy)".to_string(),
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
