//! PE-oriented identity hints (imports, section names).

use crate::detector::Confidence;
use crate::loader::LoadedBinary;

use super::evidence::EvidenceBudget;
use super::model::{EvidenceLocation, IdentityDetection, IdentityKind, IdentitySource};

#[must_use]
pub fn is_pe_format(fmt: &str) -> bool {
    fmt.to_ascii_uppercase().starts_with("PE")
}

pub fn collect_pe_identity(
    binary: &LoadedBinary,
    detections: &mut Vec<IdentityDetection>,
    evidence: &mut EvidenceBudget,
) {
    if !is_pe_format(&binary.format) {
        return;
    }

    section_name_signals(binary, detections, evidence);
    import_signals(binary, detections, evidence);
}

fn section_name_signals(
    binary: &LoadedBinary,
    detections: &mut Vec<IdentityDetection>,
    evidence: &mut EvidenceBudget,
) {
    let mut upx_evidence = Vec::new();
    for sec in &binary.sections {
        let name = sec.name.as_str();
        let looks_upx = name.contains("UPX")
            || matches!(name, "UPX0" | "UPX1" | ".UPX0" | ".UPX1" | "UPX2" | ".UPX2");
        if looks_upx {
            evidence.push_simple(
                &mut upx_evidence,
                IdentitySource::SectionName,
                EvidenceLocation::Section(sec.name.clone()),
                "Section name consistent with UPX-style layout",
                sec.name.clone(),
            );
        }
    }

    if !upx_evidence.is_empty() {
        detections.push(IdentityDetection {
            kind: IdentityKind::Packer,
            name: "UPX".to_string(),
            version: None,
            confidence: Confidence::Medium,
            source: IdentitySource::SectionName,
            evidence: upx_evidence,
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
        detections.push(IdentityDetection {
            kind: IdentityKind::DebugInfo,
            name: "EmbeddedDebugSections".to_string(),
            version: None,
            confidence: Confidence::Low,
            source: IdentitySource::SectionName,
            evidence: dbg_evidence,
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
        detections.push(IdentityDetection {
            kind: IdentityKind::Compiler,
            name: "MSVC".to_string(),
            version: None,
            confidence: Confidence::Medium,
            source: IdentitySource::ImportTable,
            evidence: msvc_evidence,
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
        detections.push(IdentityDetection {
            kind: IdentityKind::Compiler,
            name: "MinGW/GCC".to_string(),
            version: None,
            confidence: Confidence::Low,
            source: IdentitySource::ImportTable,
            evidence: mingw_evidence,
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
        detections.push(IdentityDetection {
            kind: IdentityKind::Language,
            name: "Go".to_string(),
            version: None,
            confidence: Confidence::Medium,
            source: IdentitySource::ImportTable,
            evidence: go_evidence,
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
        detections.push(IdentityDetection {
            kind: IdentityKind::Language,
            name: "Rust".to_string(),
            version: None,
            confidence: Confidence::Medium,
            source: IdentitySource::ImportTable,
            evidence: rust_evidence,
        });
    }
}
