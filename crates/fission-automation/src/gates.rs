//! Go/stop and performance regression gates for automation lanes.

use anyhow::{Result, bail};
use metrics::counter;

use crate::report::{AutomationDecisionInsights, AutomationSummary};

pub fn enforce_fail_on_stop(
    insights: &AutomationDecisionInsights,
    fail_on_stop: bool,
) -> Result<()> {
    if fail_on_stop && !insights.go_stop_gate.decision.starts_with("go_") {
        counter!(
            "fission.automation.nir_check.gate_stop_total",
            "decision" => insights.go_stop_gate.decision.clone()
        )
        .increment(1);
        bail!(
            "go/stop gate is `{}` (rationale: {}); --fail-on-stop requested",
            insights.go_stop_gate.decision,
            insights.go_stop_gate.rationale
        );
    }
    Ok(())
}

/// Fail when any pass whose baseline wall time exceeds 10ms regresses by more than 1.25×.
pub fn enforce_perf_regression(
    current: &AutomationSummary,
    baseline: &AutomationSummary,
) -> Result<()> {
    for (pass_name, current_agg) in &current.aggregate.nir_build_stats_totals.pass_metrics {
        if let Some(base_agg) = baseline
            .aggregate
            .nir_build_stats_totals
            .pass_metrics
            .get(pass_name)
        {
            if base_agg.total_time_ms > 10.0 {
                let ratio = current_agg.total_time_ms / base_agg.total_time_ms;
                if ratio > 1.25 {
                    counter!(
                        "fission.automation.nir_check.pass_regression_total",
                        "pass" => pass_name.clone()
                    )
                    .increment(1);
                    bail!(
                        "performance regression detected in pass '{}': {:.1}ms -> {:.1}ms ({:.1}x increase)",
                        pass_name,
                        base_agg.total_time_ms,
                        current_agg.total_time_ms,
                        ratio
                    );
                }
            }
        }
    }
    Ok(())
}
