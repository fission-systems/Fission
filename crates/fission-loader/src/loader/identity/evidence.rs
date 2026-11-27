//! Analysis-time evidence collection and presentation-time output truncation.
//!
//! Presentation limits (`max_detections`, `max_evidence_items`) must never alter
//! detection scoring or [`BinaryIdentitySummary`] computation. Analysis collects
//! up to [`MAX_ANALYSIS_EVIDENCE_PER_DETECTION`] items per detection, summary is
//! computed over the full detection set, and presentation caps are applied only
//! afterwards with explicit truncation diagnostics.

use super::model::{
    BinaryIdentitySummary, EvidenceLocation, IdentityDetection, IdentityEvidence, IdentityKind,
    IdentityScanLimits, IdentitySource, PresentationTruncation,
};

/// Fixed scoring-independent safety bound per detection bucket.
///
/// Strictly above every scoring threshold in `policy.rs` / `pe.rs` (currently <= 3)
/// and completely decoupled from user presentation limits.
pub const MAX_ANALYSIS_EVIDENCE_PER_DETECTION: usize = 64;

/// Scoring-independent evidence accumulator used during identity analysis.
#[derive(Debug, Default)]
pub struct AnalysisEvidenceCollector {
    internal_overflow: usize,
}

impl AnalysisEvidenceCollector {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn internal_overflow(&self) -> usize {
        self.internal_overflow
    }

    pub fn push(&mut self, bucket: &mut Vec<IdentityEvidence>, ev: IdentityEvidence) {
        if bucket.len() >= MAX_ANALYSIS_EVIDENCE_PER_DETECTION {
            self.internal_overflow = self.internal_overflow.saturating_add(1);
            return;
        }
        bucket.push(ev);
    }

    pub fn push_simple(
        &mut self,
        bucket: &mut Vec<IdentityEvidence>,
        source: IdentitySource,
        location: EvidenceLocation,
        description: impl Into<String>,
        matched: impl Into<String>,
    ) {
        self.push(
            bucket,
            IdentityEvidence {
                source,
                location,
                description: description.into(),
                matched: matched.into(),
            },
        );
    }
}

fn mark_summary_backing_detections(
    detections: &[IdentityDetection],
    summary: &BinaryIdentitySummary,
) -> Vec<bool> {
    let mut is_backing = vec![false; detections.len()];
    let targets = [
        (IdentityKind::Packer, &summary.likely_packer),
        (IdentityKind::Compiler, &summary.likely_compiler),
        (IdentityKind::Language, &summary.likely_language),
        (IdentityKind::Linker, &summary.likely_linker),
    ];

    for (kind, maybe_name) in targets {
        let Some(target_name) = maybe_name else {
            continue;
        };
        if let Some((idx, _)) = detections
            .iter()
            .enumerate()
            .filter(|(_, d)| d.kind == kind && d.name == *target_name)
            .max_by_key(|(_, d)| (d.confidence, d.score))
        {
            is_backing[idx] = true;
        }
    }

    is_backing
}

/// Apply output presentation caps after summary & scoring have completed.
///
/// Prioritizes summary-backing detections and preserves at least one supporting
/// evidence item for kept detections before allocating remaining evidence budget.
pub fn apply_presentation_caps(
    detections: &mut Vec<IdentityDetection>,
    summary: &BinaryIdentitySummary,
    limits: &IdentityScanLimits,
    internal_overflow: usize,
) -> PresentationTruncation {
    let detections_total = detections.len();
    let evidence_items_total_before_cap = detections
        .iter()
        .map(|d| d.evidence.len())
        .sum::<usize>()
        .saturating_add(internal_overflow);

    let mut is_backing = mark_summary_backing_detections(detections, summary);
    let summary_backing_total = is_backing.iter().filter(|&&b| b).count();

    if detections.len() > limits.max_detections {
        let mut backing_rows = Vec::with_capacity(summary_backing_total);
        let mut other_rows = Vec::with_capacity(detections.len() - summary_backing_total);
        for (det, backing) in detections.drain(..).zip(is_backing.iter().copied()) {
            if backing {
                backing_rows.push(det);
            } else {
                other_rows.push(det);
            }
        }
        let mut new_backing = Vec::with_capacity(limits.max_detections);
        for det in backing_rows {
            if detections.len() >= limits.max_detections {
                break;
            }
            detections.push(det);
            new_backing.push(true);
        }
        for det in other_rows {
            if detections.len() >= limits.max_detections {
                break;
            }
            detections.push(det);
            new_backing.push(false);
        }
        is_backing = new_backing;
    }

    let kept_summary_backing = is_backing.iter().filter(|&&b| b).count();
    let summary_backing_detections_omitted =
        summary_backing_total.saturating_sub(kept_summary_backing);
    let detections_emitted = detections.len();
    let detections_omitted = detections_total.saturating_sub(detections_emitted);

    // Allocate evidence budget across kept detections without affecting scores.
    let mut quotas = vec![0usize; detections.len()];
    let mut remaining = limits.max_evidence_items;

    // Pass 1a: reserve 1 item for each kept summary-backing detection that has evidence.
    for (idx, det) in detections.iter().enumerate() {
        if remaining == 0 {
            break;
        }
        if is_backing[idx] && !det.evidence.is_empty() && quotas[idx] < det.evidence.len() {
            quotas[idx] += 1;
            remaining -= 1;
        }
    }

    // Pass 1b: reserve 1 item for each other kept detection that has evidence.
    for (idx, det) in detections.iter().enumerate() {
        if remaining == 0 {
            break;
        }
        if !is_backing[idx] && !det.evidence.is_empty() && quotas[idx] < det.evidence.len() {
            quotas[idx] += 1;
            remaining -= 1;
        }
    }

    // Pass 2: allocate remaining budget in detection order up to each detection's length.
    for (idx, det) in detections.iter().enumerate() {
        if remaining == 0 {
            break;
        }
        let room = det.evidence.len().saturating_sub(quotas[idx]);
        let take = room.min(remaining);
        quotas[idx] += take;
        remaining -= take;
    }

    for (det, quota) in detections.iter_mut().zip(quotas.into_iter()) {
        det.evidence.truncate(quota);
    }

    let evidence_items_emitted: usize = detections.iter().map(|d| d.evidence.len()).sum();
    let evidence_items_omitted =
        evidence_items_total_before_cap.saturating_sub(evidence_items_emitted);
    let truncated = detections_omitted > 0 || evidence_items_omitted > 0;

    PresentationTruncation {
        max_detections: limits.max_detections,
        detections_total,
        detections_emitted,
        detections_omitted,
        summary_backing_detections_omitted,
        max_evidence_items: limits.max_evidence_items,
        evidence_items_total_before_cap,
        evidence_items_emitted,
        evidence_items_omitted,
        truncated,
        summary_computed_before_truncation: true,
    }
}
