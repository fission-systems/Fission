//! Markdown and terminal rendering for automation reports.

use crate::corpus::CorpusArtifacts;
use crate::diagnosis::DiagnosisReport;
use crate::report::delta::SummaryDelta;
use crate::report::insights::AutomationDecisionInsights;
use crate::report::quality::{
    build_quality_measurement, structuring_fallback_reasons, top_build_stats,
};
use crate::report::snapshot::AutomationSummary;

pub fn render_markdown(
    summary: &AutomationSummary,
    diagnosis: &DiagnosisReport,
    corpus: &CorpusArtifacts,
    delta: Option<&SummaryDelta>,
    insights: Option<&AutomationDecisionInsights>,
) -> String {
    let mut out = String::new();
    out.push_str("# Fission NIR Automation Summary\n\n");
    out.push_str(&format!("- Lane: `{}`\n", summary.lane));
    out.push_str(&format!("- Run profile: `{}`\n", summary.run_profile));
    out.push_str(&format!("- Target count: `{}`\n", summary.target_count));
    out.push_str(&format!("- Run: `{}`\n", summary.run_id));
    out.push_str(&format!("- Generated at: `{}`\n", summary.generated_at));
    out.push_str(&format!(
        "- Timings(ms): inventory=`{}`, diagnosis=`{}`, write_outputs=`{}`, total=`{}`\n",
        summary.inventory_elapsed_ms,
        summary.diagnosis_elapsed_ms,
        summary.write_outputs_elapsed_ms,
        summary.total_elapsed_ms,
    ));
    out.push_str(&format!(
        "- Recommended next patch: `{}`\n\n",
        summary
            .aggregate
            .recommended_next_patch
            .as_deref()
            .unwrap_or("none")
    ));

    out.push_str("## Aggregate Counts\n\n");
    out.push_str(&format!(
        "- direct_success_count: `{}`\n- nir_failure_count: `{}`\n- explicit_fact_nonzero_count: `{}`\n- strict_explicit_candidate_count: `{}`\n- inventory_surface_gap_count: `{}`\n",
        summary.aggregate.direct_success_count,
        summary.aggregate.nir_failure_count,
        summary.aggregate.explicit_fact_nonzero_count,
        summary.aggregate.strict_explicit_candidate_count,
        summary.aggregate.inventory_surface_gap_count,
    ));
    out.push_str(&format!(
        "- source_presence_counts: `{:?}`\n- provenance_surface_totals: `{:?}`\n",
        summary.aggregate.source_presence_counts, summary.aggregate.provenance_surface_totals
    ));
    out.push_str(&format!(
        "- diagnosis_bucket_counts: `{:?}`\n- nir_block_signature_counts: `{:?}`\n- recovery_attempted_counts: `{:?}`\n- recovery_outcome_counts: `{:?}`\n- recovery_quality_flag_counts: `{:?}`\n- recovery_structuring_mode_counts: `{:?}`\n\n",
        summary.aggregate.diagnosis_bucket_counts,
        summary.aggregate.nir_block_signature_counts,
        summary.aggregate.recovery_strategy_attempted_counts,
        summary.aggregate.recovery_outcome_counts,
        summary.aggregate.recovery_quality_flag_counts,
        summary.aggregate.recovery_structuring_mode_counts,
    ));
    let quality = build_quality_measurement(summary);
    out.push_str("## Output Quality\n\n");
    out.push_str(&format!(
        "- nir_output_class_counts: `{:?}`\n- structured_ratio_all_rows: `{:.2}%`\n- structured_ratio_success_rows: `{:.2}%`\n- linear_fallback_ratio_all_rows: `{:.2}%`\n- linear_fallback_ratio_success_rows: `{:.2}%`\n- top_build_stats: `{:?}`\n- structuring_fallbacks: `{:?}`\n\n",
        quality.nir_output_class_counts,
        quality.structured_ratio_all_rows * 100.0,
        quality.structured_ratio_success_rows * 100.0,
        quality.linear_fallback_ratio_all_rows * 100.0,
        quality.linear_fallback_ratio_success_rows * 100.0,
        quality.top_build_stats,
        quality.structuring_fallback_reasons,
    ));

    if let Some(delta) = delta {
        out.push_str("## Baseline Delta\n\n");
        out.push_str(&format!(
            "- direct_success_count: `{:+}`\n- nir_failure_count: `{:+}`\n- explicit_fact_nonzero_count: `{:+}`\n- strict_explicit_candidate_count: `{:+}`\n- inventory_surface_gap_count: `{:+}`\n- pdb_nonzero_rows: `{:+}`\n- region_linearized_count: `{:+}`\n- forced_linear_count: `{:+}`\n- conditional_tail_exit_mismatch_count: `{:+}`\n- body_lowering_failed_count: `{:+}`\n- successor_inline_rejected_count: `{:+}`\n- revisit_cycle_count: `{:+}`\n- unsupported_terminator_count: `{:+}`\n- rejected_irreducible_cfg_count: `{:+}`\n- structuring_scc_component_count: `{:+}`\n- structuring_irreducible_scc_count: `{:+}`\n- structuring_irreducible_header_count: `{:+}`\n- loop_control_explicit_reducer_count: `{:+}`\n- loop_control_rewrite_break_count: `{:+}`\n- loop_control_rewrite_continue_count: `{:+}`\n- loop_control_rewrite_skipped_nested_scope_count: `{:+}`\n\n",
            delta.direct_success_count,
            delta.nir_failure_count,
            delta.explicit_fact_nonzero_count,
            delta.strict_explicit_candidate_count,
            delta.inventory_surface_gap_count,
            delta.pdb_nonzero_rows,
            delta.region_linearized_count,
            delta.forced_linear_count,
            delta.conditional_tail_exit_mismatch_count,
            delta.body_lowering_failed_count,
            delta.successor_inline_rejected_count,
            delta.revisit_cycle_count,
            delta.unsupported_terminator_count,
            delta.rejected_irreducible_cfg_count,
            delta.structuring_scc_component_count,
            delta.structuring_irreducible_scc_count,
            delta.structuring_irreducible_header_count,
            delta.loop_control_explicit_reducer_count,
            delta.loop_control_rewrite_break_count,
            delta.loop_control_rewrite_continue_count,
            delta.loop_control_rewrite_skipped_nested_scope_count,
        ));
    }

    if let Some(insights) = insights {
        out.push_str("## Conditional-Tail Decision Insights\n\n");
        out.push_str(&format!(
            "- changed_row_count: `{}`\n- go_stop_gate: `{}`\n- rationale: {}\n\n",
            insights.changed_row_count,
            insights.go_stop_gate.decision,
            insights.go_stop_gate.rationale,
        ));
        out.push_str("### mismatch_subtype_ranking\n\n");
        for (name, count) in &insights.mismatch_subtype_ranking {
            out.push_str(&format!("- `{}`: `{}`\n", name, count));
        }
        out.push_str("\n### top_mismatch_rows\n\n");
        for row in insights.top_mismatch_rows.iter().take(8) {
            out.push_str(&format!(
                "- `{}` `{}` mismatch={} failed={} mode={:?} class={:?} subtype={:?}\n",
                row.binary,
                row.address,
                row.mismatch_count,
                row.body_lowering_failed_count,
                row.recovery_structuring_mode,
                row.nir_output_class,
                row.subtype_counts,
            ));
        }
        out.push_str("\n### mismatch_row_deltas\n\n");
        for row in insights.mismatch_row_deltas.iter().take(12) {
            out.push_str(&format!(
                "- `{}` `{}` `{}` baseline={} current={} delta={:+}\n",
                row.binary,
                row.address,
                row.name,
                row.baseline_mismatch_count,
                row.current_mismatch_count,
                row.mismatch_delta,
            ));
        }
        out.push('\n');
    }

    out.push_str("## Per-Binary Highlights\n\n");
    for entry in &diagnosis.binaries {
        out.push_str(&format!("### {}\n\n", entry.binary));
        out.push_str(&format!(
            "- diagnosis: `{}`\n- next_action: `{}`\n- explicit_nonzero_rows: `{}`\n- strict_explicit_candidate_count: `{}`\n- nir_block_signatures: `{:?}`\n- nir_output_class_counts: `{:?}`\n- top_build_stats: `{:?}`\n- structuring_fallbacks: `{:?}`\n- recovery_attempted_counts: `{:?}`\n- recovery_outcome_counts: `{:?}`\n- recovery_structuring_mode_counts: `{:?}`\n- recovery_quality_flag_counts: `{:?}`\n\n",
            entry.diagnosis_bucket,
            entry.next_action,
            entry.derived_metrics.explicit_nonzero_rows,
            entry.inventory_summary.strict_explicit_candidate_count,
            entry.derived_metrics.blocked_nir_block_signature_counts,
            entry.inventory_summary.nir_output_class_counts,
            top_build_stats(&entry.inventory_summary.nir_build_stats_totals, 6),
            structuring_fallback_reasons(&entry.inventory_summary.nir_build_stats_totals),
            entry.inventory_summary.recovery_strategy_attempted_counts,
            entry.inventory_summary.recovery_outcome_counts,
            entry.inventory_summary.recovery_structuring_mode_counts,
            entry.inventory_summary.recovery_quality_flag_counts,
        ));
    }

    out.push_str("## Suggested Changelog Bullets\n\n");
    out.push_str(&format!(
        "- `fission-automation` lane `{}` aggregated `{}` binaries into a canonical local quality run.\n",
        summary.lane,
        summary.binaries.len()
    ));
    out.push_str(&format!(
        "- aggregate explicit surfacing reached `explicit_fact_nonzero_count = {}` with `strict_explicit_candidate_count = {}`.\n",
        summary.aggregate.explicit_fact_nonzero_count,
        summary.aggregate.strict_explicit_candidate_count
    ));
    out.push_str(&format!(
        "- dominant diagnosis is `{:?}` and the current recommended next patch is `{:?}`.\n",
        diagnosis.aggregate.dominant_diagnosis, diagnosis.aggregate.recommended_next_patch
    ));
    out.push_str(&format!(
        "- corpus outputs now include `{}` explicit seeds, `{}` heuristic seeds, and `{}` blocked explicit candidates.\n",
        corpus.quality_explicit_facts.len(),
        corpus.quality_heuristic_surface.len(),
        corpus.blocked_explicit_candidates.len()
    ));
    out
}

pub fn print_terminal_summary(summary: &AutomationSummary, diagnosis: &DiagnosisReport) {
    let quality = build_quality_measurement(summary);
    println!("[fission-automation] lane={}", summary.lane);
    println!(
        "  direct_success={} nir_failure={} explicit_nonzero={} strict_explicit={}",
        summary.aggregate.direct_success_count,
        summary.aggregate.nir_failure_count,
        summary.aggregate.explicit_fact_nonzero_count,
        summary.aggregate.strict_explicit_candidate_count
    );
    println!(
        "  inventory_surface_gap={} pdb_nonzero_rows={}",
        summary.aggregate.inventory_surface_gap_count,
        summary.aggregate.provenance_surface_totals.pdb_nonzero_rows
    );
    println!(
        "  structured_ratio={:.1}% linear_fallback_ratio={:.1}% output_classes={:?}",
        quality.structured_ratio_all_rows * 100.0,
        quality.linear_fallback_ratio_all_rows * 100.0,
        quality.nir_output_class_counts
    );
    println!(
        "  dominant_diagnosis={:?} next_patch={:?}",
        diagnosis.aggregate.dominant_diagnosis, diagnosis.aggregate.recommended_next_patch
    );
    println!(
        "  nir_block_signatures={:?}",
        diagnosis.aggregate.nir_block_signature_counts
    );
    println!(
        "  recovery_attempted={:?} recovery_outcome={:?} recovery_quality_flags={:?}",
        summary.aggregate.recovery_strategy_attempted_counts,
        summary.aggregate.recovery_outcome_counts,
        summary.aggregate.recovery_quality_flag_counts
    );
    println!("  top_build_stats={:?}", quality.top_build_stats);
    println!(
        "  structuring_fallbacks={:?}",
        quality.structuring_fallback_reasons
    );

    let mut pass_metrics: Vec<_> = summary
        .aggregate
        .nir_build_stats_totals
        .pass_metrics
        .iter()
        .collect();
    pass_metrics.sort_by(|a, b| {
        b.1.total_time_ms
            .partial_cmp(&a.1.total_time_ms)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let slowest_passes: Vec<_> = pass_metrics
        .iter()
        .take(5)
        .map(|(n, a)| format!("{} ({:.1}ms)", n, a.total_time_ms))
        .collect();

    pass_metrics.sort_by_key(|a| std::cmp::Reverse(a.1.stmts_reduced));
    let impactful_passes: Vec<_> = pass_metrics
        .iter()
        .take(5)
        .map(|(n, a)| format!("{} ({} stmts)", n, a.stmts_reduced))
        .collect();

    if !slowest_passes.is_empty() {
        println!("  slowest_passes={:?}", slowest_passes);
        println!("  impactful_passes={:?}", impactful_passes);
    }
}
