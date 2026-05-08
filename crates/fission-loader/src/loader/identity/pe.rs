//! PE-oriented identity hints (imports, section names) with conservative scoring.

use crate::loader::LoadedBinary;

use super::evidence::EvidenceBudget;
use super::model::{
    EvidenceLocation, IdentityDetection, IdentityKind, IdentityScanLimits, IdentitySource,
};
use super::policy::{entry_in_upx_named_section, entry_section_name};
use super::scoring::{distinct_evidence_sources, gate_high_for_kind};

#[must_use]
pub fn is_pe_format(fmt: &str) -> bool {
    fmt.to_ascii_uppercase().starts_with("PE")
}

#[must_use]
fn section_name_hints_upx(name: &str) -> bool {
    name.contains("UPX") || matches!(name, "UPX0" | "UPX1" | ".UPX0" | ".UPX1" | "UPX2" | ".UPX2")
}

fn prefix_contains_upx_mark(data: &[u8], scan_cap: usize) -> bool {
    let end = data.len().min(scan_cap);
    data[..end].windows(5).any(|w| w == b"UPX!\x01") || data[..end].windows(4).any(|w| w == b"UPX!")
}

pub fn collect_pe_identity(
    binary: &LoadedBinary,
    limits: &IdentityScanLimits,
    detections: &mut Vec<IdentityDetection>,
    evidence: &mut EvidenceBudget,
) {
    if !is_pe_format(&binary.format) {
        return;
    }

    section_name_signals(binary, limits, detections, evidence);
    import_signals(binary, detections, evidence);
}

fn section_name_signals(
    binary: &LoadedBinary,
    limits: &IdentityScanLimits,
    detections: &mut Vec<IdentityDetection>,
    evidence: &mut EvidenceBudget,
) {
    let mut upx_evidence = Vec::new();
    let mut has_upx0 = false;
    let mut has_upx1 = false;
    for sec in &binary.sections {
        let name = sec.name.as_str();
        let looks_upx = section_name_hints_upx(name);
        if looks_upx {
            evidence.push_simple(
                &mut upx_evidence,
                IdentitySource::SectionName,
                EvidenceLocation::Section(sec.name.clone()),
                "Section name consistent with UPX-style layout",
                sec.name.clone(),
            );
            let upper = name.to_ascii_uppercase();
            if upper.contains("UPX0") {
                has_upx0 = true;
            }
            if upper.contains("UPX1") {
                has_upx1 = true;
            }
        }
    }

    if has_upx0 && has_upx1 {
        evidence.push_simple(
            &mut upx_evidence,
            IdentitySource::SectionName,
            EvidenceLocation::None,
            "UPX-style paired executable sections (UPX0 + UPX1)",
            "UPX0+UPX1",
        );
    }

    if prefix_contains_upx_mark(binary.data.as_slice(), limits.max_string_scan_bytes) {
        evidence.push_simple(
            &mut upx_evidence,
            IdentitySource::StringPattern,
            EvidenceLocation::None,
            "Embedded UPX marker string in bounded prefix scan",
            "UPX!",
        );
    }

    if entry_in_upx_named_section(binary) {
        let matched = entry_section_name(binary).unwrap_or_default();
        evidence.push_simple(
            &mut upx_evidence,
            IdentitySource::LoaderHeader,
            EvidenceLocation::VirtualAddress(binary.entry_point),
            "Entry point resides in UPX-named section",
            matched,
        );
    }

    if !upx_evidence.is_empty() {
        let distinct = distinct_evidence_sources(&upx_evidence);
        let mut score = upx_evidence
            .iter()
            .filter(|e| e.source == IdentitySource::SectionName && e.matched != "UPX0+UPX1")
            .count()
            .min(3) as u32;
        if has_upx0 && has_upx1 {
            score = score.saturating_add(3);
        }
        if upx_evidence
            .iter()
            .any(|e| e.source == IdentitySource::StringPattern)
        {
            score = score.saturating_add(2);
        }
        if upx_evidence
            .iter()
            .any(|e| e.source == IdentitySource::LoaderHeader)
        {
            score = score.saturating_add(2);
        }
        score = score.max(1).min(12);
        let confidence = gate_high_for_kind(IdentityKind::Packer, score, distinct);
        detections.push(IdentityDetection {
            kind: IdentityKind::Packer,
            name: "UPX".to_string(),
            version: None,
            confidence,
            source: IdentitySource::SectionName,
            score,
            evidence: upx_evidence,
            negative_evidence: Vec::new(),
        });
    }

    let mut dbg_evidence = Vec::new();
    for sec in &binary.sections {
        let lower = sec.name.to_ascii_lowercase();
        if lower.contains(".debug") || lower.starts_with(".zdebug") {
            evidence.push_simple(
                &mut dbg_evidence,
                IdentitySource::SectionName,
                EvidenceLocation::Section(sec.name.clone()),
                "Section name suggests embedded debug metadata",
                sec.name.clone(),
            );
        }
    }
    if !dbg_evidence.is_empty() {
        let score = dbg_evidence.len().min(2) as u32;
        let distinct = distinct_evidence_sources(&dbg_evidence);
        let confidence = gate_high_for_kind(IdentityKind::DebugInfo, score.max(1), distinct);
        detections.push(IdentityDetection {
            kind: IdentityKind::DebugInfo,
            name: "EmbeddedDebugSections".to_string(),
            version: None,
            confidence,
            source: IdentitySource::SectionName,
            score,
            evidence: dbg_evidence,
            negative_evidence: Vec::new(),
        });
    }
}

fn import_signals(
    binary: &LoadedBinary,
    detections: &mut Vec<IdentityDetection>,
    evidence: &mut EvidenceBudget,
) {
    let mut msvc_evidence = Vec::new();
    let needles = [
        ("__CxxFrameHandler3", "MSVC C++ exception framing import"),
        ("__chkstk", "MSVC stack probe"),
        ("__security_cookie", "MSVC security cookie"),
    ];

    for sym in binary.iat_symbols.values() {
        for (needle, desc) in needles {
            if sym.contains(needle) {
                evidence.push_simple(
                    &mut msvc_evidence,
                    IdentitySource::ImportTable,
                    EvidenceLocation::Import(sym.clone()),
                    desc,
                    needle.to_string(),
                );
                break;
            }
        }
    }

    if !msvc_evidence.is_empty() {
        let imports = msvc_evidence.len();
        let mut score = imports.min(3) as u32;
        if imports >= 2 {
            score = score.saturating_add(2);
        }
        score = score.max(2).min(12);
        let distinct = distinct_evidence_sources(&msvc_evidence);
        let confidence = gate_high_for_kind(IdentityKind::Compiler, score, distinct);
        detections.push(IdentityDetection {
            kind: IdentityKind::Compiler,
            name: "MSVC".to_string(),
            version: None,
            confidence,
            source: IdentitySource::ImportTable,
            score,
            evidence: msvc_evidence,
            negative_evidence: Vec::new(),
        });
    }

    let mut mingw_evidence = Vec::new();
    if binary
        .iat_symbols
        .values()
        .any(|s| s.contains("__CTOR_LIST__"))
    {
        evidence.push_simple(
            &mut mingw_evidence,
            IdentitySource::ImportTable,
            EvidenceLocation::Import(String::from("__CTOR_LIST__")),
            "GCC CRT ctor list symbol",
            "__CTOR_LIST__",
        );
    }
    if !mingw_evidence.is_empty() {
        let distinct = distinct_evidence_sources(&mingw_evidence);
        let score = 2_u32;
        let confidence = gate_high_for_kind(IdentityKind::Compiler, score, distinct);
        detections.push(IdentityDetection {
            kind: IdentityKind::Compiler,
            name: "MinGW/GCC".to_string(),
            version: None,
            confidence,
            source: IdentitySource::ImportTable,
            score,
            evidence: mingw_evidence,
            negative_evidence: Vec::new(),
        });
    }

    let mut go_evidence = Vec::new();
    for sym in binary.iat_symbols.values() {
        if sym.starts_with("runtime.") || sym.contains("__go_buildinfo") {
            evidence.push_simple(
                &mut go_evidence,
                IdentitySource::ImportTable,
                EvidenceLocation::Import(sym.clone()),
                "Go runtime symbol shape",
                sym.clone(),
            );
        }
    }
    if !go_evidence.is_empty() {
        let distinct = distinct_evidence_sources(&go_evidence);
        let score = if go_evidence.len() >= 2 { 4_u32 } else { 3_u32 };
        let confidence = gate_high_for_kind(IdentityKind::Language, score, distinct);
        detections.push(IdentityDetection {
            kind: IdentityKind::Language,
            name: "Go".to_string(),
            version: None,
            confidence,
            source: IdentitySource::ImportTable,
            score,
            evidence: go_evidence,
            negative_evidence: Vec::new(),
        });
    }

    let mut rust_evidence = Vec::new();
    for sym in binary.iat_symbols.values() {
        if sym.contains("rust_begin_unwind") || sym.contains("_RNv") {
            evidence.push_simple(
                &mut rust_evidence,
                IdentitySource::ImportTable,
                EvidenceLocation::Import(sym.clone()),
                "Rust runtime / mangling hint",
                sym.clone(),
            );
        }
    }
    if !rust_evidence.is_empty() {
        let distinct = distinct_evidence_sources(&rust_evidence);
        let score = if rust_evidence.len() >= 2 {
            4_u32
        } else {
            3_u32
        };
        let confidence = gate_high_for_kind(IdentityKind::Language, score, distinct);
        detections.push(IdentityDetection {
            kind: IdentityKind::Language,
            name: "Rust".to_string(),
            version: None,
            confidence,
            source: IdentitySource::ImportTable,
            score,
            evidence: rust_evidence,
            negative_evidence: Vec::new(),
        });
    }
}
