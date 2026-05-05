//! Binary identity analysis — loader provenance / detection evidence (not a decompiler repair layer).
//!
//! Phase 1 attaches a structured [`BinaryIdentityReport`] to [`LoadedBinary`] without changing parse results.

mod entropy;
mod evidence;
mod model;
mod overlay;
mod pe;

pub use model::{
    BinaryIdentityReport, BinaryIdentitySummary, EvidenceLocation, IDENTITY_SCHEMA_VERSION,
    IdentityDetection, IdentityEvidence, IdentityKind, IdentityScanLimits, IdentitySource,
    OverlayInfo, PeRichHeaderSummary, SectionEntropy,
};

use crate::detector::Confidence;
use crate::loader::LoadedBinary;

use entropy::{classify_executable_entropy, shannon_entropy};
use evidence::EvidenceBudget;
use model::{
    BinaryIdentityReport as Report, BinaryIdentitySummary as Summary, IdentityDetection as Det,
    SectionEntropy as SecEnt,
};

#[must_use]
pub fn analyze(binary: &LoadedBinary, limits: IdentityScanLimits) -> Report {
    let _string_scan_budget = limits.max_string_scan_bytes;
    let section_entropy = scan_executable_entropy(binary, &limits);

    let overlay_info = overlay::detect_overlay(binary);

    let mut evidence_budget = EvidenceBudget::new(limits.max_evidence_items);
    let mut detections = Vec::new();

    pe::collect_pe_identity(binary, &mut detections, &mut evidence_budget);

    if let Some(ref pdb) = binary.pdb_debug_info {
        let mut ev = Vec::new();
        let matched = pdb
            .path_hint
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "CodeView/PDB directory present".to_string());
        evidence_budget.push_simple(
            &mut ev,
            IdentitySource::DebugInfo,
            EvidenceLocation::None,
            "PE debug directory references PDB metadata",
            matched.clone(),
        );
        detections.push(Det {
            kind: IdentityKind::DebugInfo,
            name: "PDB".to_string(),
            version: pdb.age.map(|a| a.to_string()),
            confidence: Confidence::Medium,
            source: IdentitySource::DebugInfo,
            evidence: ev,
        });
    }

    if let Some(ref o) = overlay_info {
        let mut ev = Vec::new();
        evidence_budget.push_simple(
            &mut ev,
            IdentitySource::Overlay,
            EvidenceLocation::FileOffset(o.file_offset_start),
            "Trailing bytes after last mapped file section",
            format!("{} bytes", o.size),
        );
        detections.push(Det {
            kind: IdentityKind::Overlay,
            name: "TrailingOverlay".to_string(),
            version: None,
            confidence: Confidence::Low,
            source: IdentitySource::Overlay,
            evidence: ev,
        });
    }

    let high_entropy_executable_sections = section_entropy
        .iter()
        .filter(|s| s.classification == "high_entropy")
        .count();

    if high_entropy_executable_sections > 0 {
        let mut ev = Vec::new();
        for sec in section_entropy
            .iter()
            .filter(|s| s.classification == "high_entropy")
            .take(8)
        {
            evidence_budget.push_simple(
                &mut ev,
                IdentitySource::Entropy,
                EvidenceLocation::Section(sec.section.clone()),
                "Executable section Shannon entropy above heuristic packing threshold",
                format!("entropy={:.2}", sec.entropy),
            );
        }
        detections.push(Det {
            kind: IdentityKind::Unknown,
            name: "HighEntropyExecutableSections".to_string(),
            version: None,
            confidence: Confidence::Low,
            source: IdentitySource::Entropy,
            evidence: ev,
        });
    }

    detections.truncate(limits.max_detections);

    let summary = build_summary(
        &detections,
        overlay_info.is_some(),
        high_entropy_executable_sections,
    );

    Report {
        schema_version: IDENTITY_SCHEMA_VERSION,
        detections,
        section_entropy,
        overlay: overlay_info,
        rich_header: None,
        summary,
    }
}

fn scan_executable_entropy(binary: &LoadedBinary, limits: &IdentityScanLimits) -> Vec<SecEnt> {
    let data = binary.data.as_slice();
    let mut budget = limits.max_scan_bytes;
    let mut out = Vec::new();
    for sec in &binary.sections {
        if !sec.is_executable {
            continue;
        }
        let size_u64 = sec.file_size.min(sec.virtual_size);
        let size = usize::try_from(size_u64).unwrap_or(usize::MAX);
        let start = usize::try_from(sec.file_offset).unwrap_or(usize::MAX);
        if size == 0 || start >= data.len() {
            continue;
        }
        let avail = data.len().saturating_sub(start);
        let take = size.min(avail).min(budget);
        if take == 0 {
            continue;
        }
        let slice = &data[start..start + take];
        budget = budget.saturating_sub(take);
        let e = shannon_entropy(slice);
        let class = classify_executable_entropy(e);
        out.push(SecEnt {
            section: sec.name.clone(),
            entropy: e,
            classification: class.to_string(),
        });
        if budget == 0 {
            break;
        }
    }
    out
}

fn build_summary(
    detections: &[Det],
    has_overlay: bool,
    high_entropy_executable_sections: usize,
) -> Summary {
    let mut packed_score = 0.0_f32;
    if high_entropy_executable_sections > 0 {
        packed_score += 0.15 * (high_entropy_executable_sections.min(3) as f32);
    }
    if detections.iter().any(|d| d.kind == IdentityKind::Packer) {
        packed_score += 0.35;
    }
    if has_overlay {
        packed_score += 0.1;
    }
    packed_score = packed_score.min(1.0);

    fn best_by_kind<'a>(detections: &'a [Det], kind: IdentityKind) -> Option<&'a Det> {
        detections
            .iter()
            .filter(|d| d.kind == kind)
            .max_by_key(|d| d.confidence)
    }

    let likely_compiler = best_by_kind(detections, IdentityKind::Compiler).map(|d| d.name.clone());
    let likely_language = best_by_kind(detections, IdentityKind::Language).map(|d| d.name.clone());
    let likely_linker = best_by_kind(detections, IdentityKind::Linker).map(|d| d.name.clone());
    let likely_packer = best_by_kind(detections, IdentityKind::Packer).map(|d| d.name.clone());

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

    Summary {
        likely_language,
        likely_compiler,
        likely_linker,
        likely_packer,
        packed_score,
        has_overlay,
        high_entropy_executable_sections,
        confidence: summary_confidence,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::{DataBuffer, FunctionInfo, LoadedBinaryBuilder, SectionInfo};

    #[test]
    fn entropy_uniform_high() {
        let mut buf = Vec::with_capacity(4096);
        for i in 0_u32..1024 {
            buf.push(((i % 251) as u8).wrapping_mul(97));
        }
        let e = shannon_entropy(&buf);
        assert!(e >= 7.5, "expected high entropy, got {e}");
    }

    #[test]
    fn entropy_repeated_low() {
        let buf = vec![0xff_u8; 4096];
        let e = shannon_entropy(&buf);
        assert!(e < 0.1, "expected ~0 entropy, got {e}");
    }

    #[test]
    fn overlay_detects_tail() {
        let mut data = vec![0_u8; 256];
        data.extend_from_slice(&[0x41_u8; 64]);
        let binary = LoadedBinaryBuilder::new("tail.bin".to_string(), DataBuffer::Heap(data))
            .format("RAW")
            .entry_point(0)
            .image_base(0)
            .is_64bit(false)
            .add_section(SectionInfo {
                name: ".text".to_string(),
                virtual_address: 0,
                virtual_size: 256,
                file_offset: 0,
                file_size: 256,
                is_executable: true,
                is_readable: true,
                is_writable: false,
            })
            .build()
            .expect("build");
        let ov = overlay::detect_overlay(&binary).expect("overlay");
        assert_eq!(ov.file_offset_start, 256);
        assert_eq!(ov.size, 64);
    }

    #[test]
    fn pe_upx_section_and_msvc_import() {
        let binary =
            LoadedBinaryBuilder::new("stub.exe".to_string(), DataBuffer::Heap(vec![0u8; 512]))
                .format("PE64")
                .entry_point(0x1000)
                .image_base(0x140000000)
                .is_64bit(true)
                .add_section(SectionInfo {
                    name: "UPX0".to_string(),
                    virtual_address: 0x1000,
                    virtual_size: 256,
                    file_offset: 0,
                    file_size: 256,
                    is_executable: true,
                    is_readable: true,
                    is_writable: false,
                })
                .add_iat_symbol(0x2000, "__CxxFrameHandler3".to_string())
                .build()
                .expect("build");

        let report = analyze(&binary, IdentityScanLimits::default());
        assert!(
            report
                .detections
                .iter()
                .any(|d| d.kind == IdentityKind::Packer && d.name == "UPX")
        );
        assert!(
            report
                .detections
                .iter()
                .any(|d| d.kind == IdentityKind::Compiler && d.name == "MSVC")
        );
        assert!(
            report.summary.likely_compiler.as_deref() == Some("MSVC")
                || report.summary.likely_packer.as_deref() == Some("UPX")
        );
    }

    #[test]
    fn identity_report_roundtrip_loaded_via_builder() {
        let binary =
            LoadedBinaryBuilder::new("x.bin".to_string(), DataBuffer::Heap(vec![0x90; 128]))
                .format("ELF64")
                .entry_point(0)
                .image_base(0)
                .is_64bit(true)
                .add_section(SectionInfo {
                    name: ".text".to_string(),
                    virtual_address: 0,
                    virtual_size: 128,
                    file_offset: 0,
                    file_size: 128,
                    is_executable: true,
                    is_readable: true,
                    is_writable: false,
                })
                .add_function(FunctionInfo {
                    name: "_start".into(),
                    address: 0,
                    size: 4,
                    is_export: false,
                    is_import: false,
                    ..Default::default()
                })
                .build()
                .expect("build");
        let j =
            serde_json::to_string(&analyze(&binary, IdentityScanLimits::default())).expect("json");
        assert!(j.contains("schema_version"));
    }
}
