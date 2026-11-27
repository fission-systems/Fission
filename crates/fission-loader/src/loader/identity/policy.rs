//! Negative evidence, DIE promotion gates, summary thresholds, compiler conflicts.

use fission_core::PAGE_SIZE;
use fission_core::evidence_policy::EvidencePolicy;

use crate::detector::Confidence;
use crate::loader::LoadedBinary;

use super::model::{
    BinaryIdentitySummary, DieRawMatch, EntropyCoverageStatus, EntropyScanCoverage,
    EvidenceLocation, IdentityDetection, IdentityEvidence, IdentityKind, IdentitySource,
    NegativeIdentityEvidence, PackedScoreBreakdown, PackedScoreComponent, SuppressedDetection,
    SuppressedScoreDeduction,
};
use super::scoring::{distinct_evidence_sources, gate_high_for_kind};

const EVIDENCE_POLICY: EvidencePolicy = EvidencePolicy::DEFAULT;

#[must_use]
pub(super) fn pe_has_rich_header_probe(binary: &LoadedBinary) -> bool {
    let data = binary.data.as_slice();
    let n = data.len().min(PAGE_SIZE);
    data[..n].windows(4).any(|w| w == b"Rich")
}

#[must_use]
pub(super) fn entry_section_name(binary: &LoadedBinary) -> Option<String> {
    let ep = binary.entry_point;
    binary
        .sections
        .iter()
        .find(|s| ep >= s.virtual_address && ep < s.virtual_address.saturating_add(s.virtual_size))
        .map(|s| s.name.clone())
}

#[must_use]
fn entry_in_named_text(binary: &LoadedBinary) -> bool {
    entry_section_name(binary).map_or(false, |n| {
        let lower = n.to_ascii_lowercase();
        lower == ".text"
    })
}

#[must_use]
fn section_name_hints_upx(name: &str) -> bool {
    name.contains("UPX") || matches!(name, "UPX0" | "UPX1" | ".UPX0" | ".UPX1" | "UPX2" | ".UPX2")
}

#[must_use]
pub(super) fn entry_in_upx_named_section(binary: &LoadedBinary) -> bool {
    entry_section_name(binary)
        .map(|n| section_name_hints_upx(&n))
        .unwrap_or(false)
}

pub(super) fn attach_negative_evidence_for_binary(
    binary: &LoadedBinary,
    pdb_present: bool,
    pe_debug_dirs_present: bool,
    entropy_coverage: &EntropyScanCoverage,
    detections: &mut [IdentityDetection],
) {
    let rich_import_table =
        binary.iat_symbols.len() >= EVIDENCE_POLICY.auxiliary.import_table_rich_symbol_count;
    let normal_ep_text = entry_in_named_text(binary);
    let rich_present = pe_has_rich_header_probe(binary);
    let confirmed_no_high_entropy = entropy_coverage.status
        == EntropyCoverageStatus::CompleteNoHighEntropy
        && entropy_coverage.sufficient_for_absence_evidence;

    for det in detections.iter_mut() {
        let mut neg = Vec::new();
        if normal_ep_text && matches!(det.kind, IdentityKind::Packer | IdentityKind::Unknown) {
            neg.push(NegativeIdentityEvidence {
                source: IdentitySource::LoaderHeader,
                description: "Entry point maps into a conventional executable (.text) section"
                    .to_string(),
            });
        }
        if rich_import_table {
            neg.push(NegativeIdentityEvidence {
                source: IdentitySource::ImportTable,
                description: format!(
                    "Import table has {} symbols (non-trivial IAT)",
                    binary.iat_symbols.len()
                ),
            });
        }
        if pdb_present || pe_debug_dirs_present {
            neg.push(NegativeIdentityEvidence {
                source: IdentitySource::DebugInfo,
                description: "Debug/CodeView metadata present on binary".to_string(),
            });
        }
        if rich_present && matches!(det.kind, IdentityKind::Compiler | IdentityKind::Runtime) {
            neg.push(NegativeIdentityEvidence {
                source: IdentitySource::RichHeader,
                description: "Rich stub marker present near DOS header".to_string(),
            });
        }
        if confirmed_no_high_entropy
            && matches!(det.kind, IdentityKind::Unknown)
            && det.name == "HighEntropyExecutableSections"
        {
            neg.push(NegativeIdentityEvidence {
                source: IdentitySource::Entropy,
                description: "Complete executable entropy scan found no sections above threshold"
                    .to_string(),
            });
        }
        det.negative_evidence = neg;
    }
}

pub(super) fn merge_crt_into_msvc(detections: &mut Vec<IdentityDetection>) {
    let crt_hits: Vec<String> = detections
        .iter()
        .filter(|d| d.kind == IdentityKind::Runtime && d.source == IdentitySource::SignaturePattern)
        .map(|d| d.name.clone())
        .collect();
    if crt_hits.is_empty() {
        return;
    }
    let Some(msvc) = detections
        .iter_mut()
        .find(|d| d.kind == IdentityKind::Compiler && d.name == "MSVC")
    else {
        return;
    };
    for n in crt_hits {
        msvc.evidence.push(IdentityEvidence {
            source: IdentitySource::SignaturePattern,
            location: EvidenceLocation::None,
            description: "CRT bytecode pattern near entry (corroborates MSVC runtime)".into(),
            matched: n,
        });
    }
}

pub(super) fn refresh_msvc_detection(
    binary: &LoadedBinary,
    pdb_present: bool,
    det: &mut IdentityDetection,
) {
    if det.kind != IdentityKind::Compiler || det.name != "MSVC" {
        return;
    }
    if pe_has_rich_header_probe(binary)
        && !det
            .evidence
            .iter()
            .any(|e| e.source == IdentitySource::RichHeader && e.matched == "Rich")
    {
        det.evidence.push(IdentityEvidence {
            source: IdentitySource::RichHeader,
            location: EvidenceLocation::None,
            description: "Rich marker near DOS header (MSVC toolchain corroboration)".into(),
            matched: "Rich".into(),
        });
    }
    if pdb_present
        && !det
            .evidence
            .iter()
            .any(|e| e.source == IdentitySource::DebugInfo && e.matched == "PDB")
    {
        det.evidence.push(IdentityEvidence {
            source: IdentitySource::DebugInfo,
            location: EvidenceLocation::None,
            description: "CodeView/PDB metadata attached to loader".into(),
            matched: "PDB".into(),
        });
    }
    let distinct = distinct_evidence_sources(&det.evidence);
    let score = compute_msvc_score(det);
    det.score = score;
    det.confidence = gate_high_for_kind(IdentityKind::Compiler, score, distinct);
}

fn compute_msvc_score(det: &IdentityDetection) -> u32 {
    let imports = det
        .evidence
        .iter()
        .filter(|e| e.source == IdentitySource::ImportTable)
        .count();
    let mut s = imports.min(3) as u32;
    if imports >= 2 {
        s = s.saturating_add(2);
    }
    if det
        .evidence
        .iter()
        .any(|e| e.source == IdentitySource::RichHeader)
    {
        s = s.saturating_add(2);
    }
    if det
        .evidence
        .iter()
        .any(|e| e.source == IdentitySource::DebugInfo)
    {
        s = s.saturating_add(2);
    }
    if det
        .evidence
        .iter()
        .any(|e| e.source == IdentitySource::SignaturePattern)
    {
        s = s.saturating_add(3);
    }
    s.max(2).min(12)
}

pub(super) fn promote_die_raw_matches(
    raw_matches: &[DieRawMatch],
    existing_before_die: &[IdentityDetection],
) -> (Vec<IdentityDetection>, Vec<SuppressedDetection>) {
    let mut out = Vec::new();
    let mut suppressed = Vec::new();

    let non_die_sources = existing_before_die
        .iter()
        .filter(|d| d.source != IdentitySource::DieRule)
        .count();

    for raw in raw_matches {
        let total = raw.total_primitive_slots.max(1);
        let coverage = raw.matched_primitive_count as f32 / total as f32;
        let corroborated = non_die_sources > 0;

        let weak_die = raw.matched_primitive_count == 1
            && raw.total_primitive_slots >= 4
            && raw.unsupported_primitives_ignored >= 2
            && !corroborated;
        if weak_die {
            suppressed.push(SuppressedDetection {
                name: raw.rule_name.clone(),
                kind: die_category_to_kind(&raw.category),
                reason:
                    "Single Phase-2-evaluated primitive on a rule body that is mostly unsupported"
                        .into(),
                raw_score: raw.matched_primitive_count as u32,
            });
            continue;
        }

        let kind = die_category_to_kind(&raw.category);
        let mut score = (raw.matched_primitive_count as u32)
            .saturating_mul(2)
            .saturating_add(raw.raw_score_bonus);
        if corroborated {
            score = score.saturating_add(3);
        }

        let mut evidence = vec![IdentityEvidence {
            source: IdentitySource::DieRule,
            location: EvidenceLocation::None,
            description: "DIE JSON rule matched (subset evaluation)".into(),
            matched: raw.details.clone(),
        }];
        if corroborated {
            evidence.push(IdentityEvidence {
                source: IdentitySource::ImportTable,
                location: EvidenceLocation::None,
                description: "Other loader identity hints present (corroboration)".into(),
                matched: format!("{non_die_sources} non-DIE detection(s)"),
            });
        }

        let distinct = distinct_evidence_sources(&evidence);
        let mut confidence = gate_high_for_kind(kind, score, distinct);

        if raw.unsupported_primitives_ignored > 0 || coverage < 0.5 {
            if confidence == Confidence::High {
                confidence = Confidence::Medium;
            }
        }
        if !corroborated && confidence == Confidence::High {
            confidence = Confidence::Medium;
        }

        out.push(IdentityDetection {
            kind,
            name: raw.rule_name.clone(),
            version: None,
            confidence,
            source: IdentitySource::DieRule,
            score,
            evidence,
            negative_evidence: Vec::new(),
        });
    }

    (out, suppressed)
}

fn die_category_to_kind(cat: &str) -> IdentityKind {
    match cat.to_ascii_lowercase().as_str() {
        "packer" => IdentityKind::Packer,
        "protector" => IdentityKind::Protector,
        "compiler" => IdentityKind::Compiler,
        "language" => IdentityKind::Language,
        "installer" => IdentityKind::Installer,
        _ => IdentityKind::Signature,
    }
}

pub(super) fn compute_packed_score(
    binary: &LoadedBinary,
    pdb_present: bool,
    pe_debug_dirs_present: bool,
    has_overlay: bool,
    high_entropy_exec_sections: usize,
    entropy_coverage: &EntropyScanCoverage,
    detections: &[IdentityDetection],
) -> PackedScoreBreakdown {
    let p = EVIDENCE_POLICY.packed;
    let mut positive_components = Vec::new();
    let mut raw_base_score = 0.0_f32;

    if high_entropy_exec_sections > 0 {
        let counted = high_entropy_exec_sections.min(p.entropy_section_cap);
        let delta = p.entropy_weight_per_section * (counted as f32);
        raw_base_score += delta;
        positive_components.push(PackedScoreComponent {
            name: "high_entropy_executable_sections".to_string(),
            delta,
            reason: format!(
                "{high_entropy_exec_sections} executable section(s) above high-entropy threshold (capped at {counted})"
            ),
        });
    }
    if detections.iter().any(|d| d.kind == IdentityKind::Packer) {
        raw_base_score += p.packer_present_bump;
        positive_components.push(PackedScoreComponent {
            name: "packer_detection_present".to_string(),
            delta: p.packer_present_bump,
            reason: "Static packer signature or section-layout hint present".to_string(),
        });
    }
    if has_overlay {
        raw_base_score += p.overlay_bump;
        positive_components.push(PackedScoreComponent {
            name: "trailing_overlay_present".to_string(),
            delta: p.overlay_bump,
            reason: "Unmapped trailing bytes present after last mapped section".to_string(),
        });
    }

    let clamped_base_score = raw_base_score.min(1.0);
    let mut score = clamped_base_score;
    let mut negative_components = Vec::new();
    let mut suppressed_deductions = Vec::new();

    if entry_in_named_text(binary) {
        score -= p.deduction_entry_in_text_section;
        negative_components.push(PackedScoreComponent {
            name: "entry_in_text_section".to_string(),
            delta: -p.deduction_entry_in_text_section,
            reason: "Entry point resides in conventional .text section".to_string(),
        });
    }
    if binary.iat_symbols.len() >= EVIDENCE_POLICY.auxiliary.import_table_rich_symbol_count {
        score -= p.deduction_rich_import_table;
        negative_components.push(PackedScoreComponent {
            name: "rich_import_table".to_string(),
            delta: -p.deduction_rich_import_table,
            reason: format!(
                "Import table has {} symbols (>= {})",
                binary.iat_symbols.len(),
                EVIDENCE_POLICY.auxiliary.import_table_rich_symbol_count
            ),
        });
    }
    if pdb_present || pe_debug_dirs_present {
        score -= p.deduction_debug_present;
        negative_components.push(PackedScoreComponent {
            name: "debug_metadata_present".to_string(),
            delta: -p.deduction_debug_present,
            reason: "PDB/CodeView or PE debug directory metadata present".to_string(),
        });
    }
    if high_entropy_exec_sections == 0 {
        if entropy_coverage.status == EntropyCoverageStatus::CompleteNoHighEntropy
            && entropy_coverage.sufficient_for_absence_evidence
        {
            score -= p.deduction_no_high_entropy_executable;
            negative_components.push(PackedScoreComponent {
                name: "no_high_entropy_executable_sections".to_string(),
                delta: -p.deduction_no_high_entropy_executable,
                reason: format!(
                    "Complete executable entropy scan ({}/{} bytes across {} section(s)) observed no high-entropy code",
                    entropy_coverage.sampled_bytes_total,
                    entropy_coverage.declared_bytes_total,
                    entropy_coverage.executable_sections_total
                ),
            });
        } else {
            suppressed_deductions.push(SuppressedScoreDeduction {
                name: "no_high_entropy_executable_sections".to_string(),
                skipped_delta: -p.deduction_no_high_entropy_executable,
                reason: format!(
                    "Suppressed absence deduction: coverage status {:?} ({}/{} declared bytes sampled, complete={}/{}, oob={})",
                    entropy_coverage.status,
                    entropy_coverage.sampled_bytes_total,
                    entropy_coverage.declared_bytes_total,
                    entropy_coverage.sections_complete,
                    entropy_coverage.executable_sections_total,
                    entropy_coverage.sections_out_of_bounds
                ),
            });
        }
    }
    if detections.iter().any(|d| {
        d.kind == IdentityKind::Compiler
            && matches!(d.confidence, Confidence::Medium | Confidence::High)
    }) {
        score -= p.deduction_compiler_medium_or_high;
        negative_components.push(PackedScoreComponent {
            name: "compiler_medium_or_high".to_string(),
            delta: -p.deduction_compiler_medium_or_high,
            reason: "Corroborated Medium/High confidence compiler signal present".to_string(),
        });
    }

    let adjusted_score = score.clamp(0.0, 1.0);

    PackedScoreBreakdown {
        policy_version: p.policy_version.to_string(),
        semantics: "heuristic_layout_packing_indicator_not_malware_probability".to_string(),
        raw_base_score,
        clamped_base_score,
        adjusted_score,
        positive_components,
        negative_components,
        suppressed_deductions,
    }
}

pub(super) fn build_summary_policy(
    detections: &[IdentityDetection],
    has_overlay: bool,
    high_entropy_executable_sections: usize,
    entropy_coverage: &EntropyScanCoverage,
    packed_score_breakdown: &PackedScoreBreakdown,
    compiler_conflicts: Vec<String>,
    weak_signal_count: usize,
    presentation_truncated: bool,
) -> BinaryIdentitySummary {
    fn best_promoted_compiler(
        detections: &[IdentityDetection],
        min_conf: Confidence,
    ) -> Option<&IdentityDetection> {
        detections
            .iter()
            .filter(|d| d.kind == IdentityKind::Compiler && d.confidence >= min_conf)
            .max_by_key(|d| (d.confidence, d.score))
    }

    fn best_promoted_language(
        detections: &[IdentityDetection],
        min_conf: Confidence,
    ) -> Option<&IdentityDetection> {
        detections
            .iter()
            .filter(|d| d.kind == IdentityKind::Language && d.confidence >= min_conf)
            .max_by_key(|d| (d.confidence, d.score))
    }

    fn best_promoted_linker(
        detections: &[IdentityDetection],
        min_conf: Confidence,
    ) -> Option<&IdentityDetection> {
        detections
            .iter()
            .filter(|d| d.kind == IdentityKind::Linker && d.confidence >= min_conf)
            .max_by_key(|d| (d.confidence, d.score))
    }

    fn best_promoted_packer(
        detections: &[IdentityDetection],
        min_conf: Confidence,
    ) -> Option<&IdentityDetection> {
        detections
            .iter()
            .filter(|d| d.kind == IdentityKind::Packer && d.confidence >= min_conf)
            .max_by_key(|d| (d.confidence, d.score))
    }

    let likely_compiler =
        best_promoted_compiler(detections, Confidence::Medium).map(|d| d.name.clone());
    let likely_language =
        best_promoted_language(detections, Confidence::Medium).map(|d| d.name.clone());
    let likely_linker =
        best_promoted_linker(detections, Confidence::Medium).map(|d| d.name.clone());
    let likely_packer = best_promoted_packer(detections, Confidence::High).map(|d| d.name.clone());

    let summary_confidence = detections
        .iter()
        .filter(|d| {
            !matches!(
                d.kind,
                IdentityKind::Overlay | IdentityKind::Unknown // entropy bucket
            )
        })
        .map(|d| d.confidence)
        .max()
        .unwrap_or(Confidence::Low);

    BinaryIdentitySummary {
        likely_language,
        likely_compiler,
        likely_linker,
        likely_packer,
        packed_score: packed_score_breakdown.adjusted_score,
        has_overlay,
        high_entropy_executable_sections,
        confidence: summary_confidence,
        compiler_conflicts,
        weak_signal_count,
        entropy_coverage_status: entropy_coverage.status,
        packed_score_policy_version: packed_score_breakdown.policy_version.clone(),
        presentation_truncated,
    }
}

pub(super) fn compute_compiler_conflicts(detections: &[IdentityDetection]) -> Vec<String> {
    let compilers: Vec<&str> = detections
        .iter()
        .filter(|d| {
            d.kind == IdentityKind::Compiler
                && matches!(d.confidence, Confidence::Medium | Confidence::High)
        })
        .map(|d| d.name.as_str())
        .collect();
    if compilers.len() < 2 {
        return Vec::new();
    }
    let mut uniq: Vec<String> = compilers.iter().map(|s| (*s).to_string()).collect();
    uniq.sort();
    uniq.dedup();
    if uniq.len() < 2 {
        return Vec::new();
    }
    vec![format!("compiler_hint_conflict: {}", uniq.join(" vs "))]
}

pub(super) fn weak_signal_count_for_summary(detections: &[IdentityDetection]) -> usize {
    let cap = EVIDENCE_POLICY.auxiliary.weak_signal_score_cap;
    detections
        .iter()
        .filter(|d| matches!(d.confidence, Confidence::Low) && d.score <= cap)
        .count()
}
