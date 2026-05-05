//! Binary identity analysis — loader provenance / detection evidence (not a decompiler repair layer).
//!
//! Phase 1 attaches a structured [`BinaryIdentityReport`] to [`LoadedBinary`] without changing parse results.

mod die_compat;
mod entropy;
mod evidence;
mod model;
mod overlay;
mod pe;
mod pe_identity;
mod policy;
mod resources;
mod scoring;
mod signatures_bridge;

pub use model::{
    BinaryIdentityReport, BinaryIdentitySummary, DieCompatSummary, DieRawMatch, EvidenceLocation,
    IDENTITY_SCHEMA_VERSION, IdentityDetection, IdentityEvidence, IdentityKind,
    IdentityResourceSummary, IdentityScanLimits, IdentitySource, NegativeIdentityEvidence,
    OverlayInfo, PeIdentitySummary, PeRichHeaderSummary, SectionEntropy, SuppressedDetection,
    WinApiCatalogSummary,
};

use crate::loader::LoadedBinary;

use entropy::{classify_executable_entropy, shannon_entropy};
use evidence::EvidenceBudget;
use model::{
    BinaryIdentityReport as Report, IdentityDetection as Det, SectionEntropy as SecEnt,
};
use scoring::{distinct_evidence_sources, gate_high_for_kind};

#[must_use]
pub fn analyze(binary: &LoadedBinary, limits: IdentityScanLimits) -> Report {
    let section_entropy = scan_executable_entropy(binary, &limits);

    let overlay_info = overlay::detect_overlay(binary);

    let resources = Some(resources::summarize_identity_resources());

    let mut evidence_budget = EvidenceBudget::new(limits.max_evidence_items);
    let mut detections = Vec::new();

    pe::collect_pe_identity(binary, &limits, &mut detections, &mut evidence_budget);

    let pe_extra = pe_identity::summarize_pe_identity(binary);

    detections.extend(signatures_bridge::signature_pattern_identity(
        binary,
        &mut evidence_budget,
    ));

    let snapshot_before_die = detections.clone();

    let (die_compat, die_raw_matches) = die_compat::die_compat_identity(binary, &limits);

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
        let distinct = distinct_evidence_sources(&ev);
        let score = 4_u32;
        let confidence = gate_high_for_kind(IdentityKind::DebugInfo, score, distinct);
        detections.push(Det {
            kind: IdentityKind::DebugInfo,
            name: "PDB".to_string(),
            version: pdb.age.map(|a| a.to_string()),
            confidence,
            source: IdentitySource::DebugInfo,
            score,
            evidence: ev,
            negative_evidence: Vec::new(),
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
        let distinct = distinct_evidence_sources(&ev);
        let score = 2_u32;
        let confidence = gate_high_for_kind(IdentityKind::Overlay, score, distinct);
        detections.push(Det {
            kind: IdentityKind::Overlay,
            name: "TrailingOverlay".to_string(),
            version: None,
            confidence,
            source: IdentitySource::Overlay,
            score,
            evidence: ev,
            negative_evidence: Vec::new(),
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
        let distinct = distinct_evidence_sources(&ev);
        let score = 3_u32;
        let confidence = gate_high_for_kind(IdentityKind::Unknown, score, distinct);
        detections.push(Det {
            kind: IdentityKind::Unknown,
            name: "HighEntropyExecutableSections".to_string(),
            version: None,
            confidence,
            source: IdentitySource::Entropy,
            score,
            evidence: ev,
            negative_evidence: Vec::new(),
        });
    }

    let winapi_catalog = signatures_bridge::winapi_catalog_summary(binary);

    policy::merge_crt_into_msvc(&mut detections);

    let pdb_present = binary.pdb_debug_info.is_some();
    for det in detections.iter_mut() {
        policy::refresh_msvc_detection(binary, pdb_present, det);
    }

    let (die_promoted, suppressed_detections) =
        policy::promote_die_raw_matches(&die_raw_matches, &snapshot_before_die);
    detections.extend(die_promoted);

    let pe_debug_dirs_present = pe_extra
        .as_ref()
        .map(|p| !p.debug_directory_kinds.is_empty())
        .unwrap_or(false);

    policy::attach_negative_evidence_for_binary(
        binary,
        pdb_present,
        pe_debug_dirs_present,
        high_entropy_executable_sections,
        &mut detections,
    );

    let mut packed_score_base = 0.0_f32;
    if high_entropy_executable_sections > 0 {
        packed_score_base += 0.15 * (high_entropy_executable_sections.min(3) as f32);
    }
    if detections.iter().any(|d| d.kind == IdentityKind::Packer) {
        packed_score_base += 0.35;
    }
    if overlay_info.is_some() {
        packed_score_base += 0.1;
    }
    packed_score_base = packed_score_base.min(1.0);

    let packed_score = policy::packed_score_adjusted(
        packed_score_base,
        binary,
        pdb_present,
        pe_debug_dirs_present,
        high_entropy_executable_sections,
        &detections,
    );

    let compiler_conflicts = policy::compute_compiler_conflicts(&detections);
    let weak_signal_count = policy::weak_signal_count_for_summary(&detections);

    detections.truncate(limits.max_detections);

    let summary = policy::build_summary_policy(
        &detections,
        overlay_info.is_some(),
        high_entropy_executable_sections,
        packed_score,
        compiler_conflicts,
        weak_signal_count,
    );

    Report {
        schema_version: IDENTITY_SCHEMA_VERSION,
        detections,
        die_raw_matches,
        suppressed_detections,
        section_entropy,
        overlay: overlay_info,
        rich_header: Some(PeRichHeaderSummary {
            present: policy::pe_has_rich_header_probe(binary),
        }),
        resources,
        die_compat,
        pe: pe_extra,
        winapi_catalog,
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
                .add_section(SectionInfo {
                    name: "UPX1".to_string(),
                    virtual_address: 0x1100,
                    virtual_size: 256,
                    file_offset: 256,
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
        // Summary packer lane is High-only; UPX0+UPX1 + EP-in-UPX reaches High (independent sources).
        assert_eq!(report.summary.likely_packer.as_deref(), Some("UPX"));
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

    #[test]
    fn identity_resources_synthetic_paths() {
        use std::fs;

        let tmp = tempfile::tempdir().expect("tempdir");
        let sig = tmp.path().join("signatures");
        fs::create_dir_all(sig.join("die").join("detect-it-easy").join("db")).expect("mkdir");
        fs::write(sig.join("die").join("pe_signatures.json"), b"{}\n").expect("write");
        fs::create_dir_all(sig.join("patterns")).expect("mkdir");
        fs::write(sig.join("patterns").join("a.json"), b"{}\n").expect("write");
        fs::create_dir_all(sig.join("fid")).expect("mkdir");
        fs::write(sig.join("fid").join("x.fidbf"), []).expect("write");
        fs::create_dir_all(sig.join("typeinfo").join("win32")).expect("mkdir");
        fs::write(
            sig.join("typeinfo").join("win32").join("win_api_signatures.txt"),
            b"a|void|void\n",
        )
        .expect("write");
        fs::write(
            sig.join("die").join("detect-it-easy").join("db").join("z.sg"),
            b"x",
        )
        .expect("sg");

        let cfg = fission_core::path_config::PathConfig {
            workspace_root: Some(tmp.path().to_path_buf()),
            signatures_base: Some(sig.clone()),
            fid_dir: Some(sig.join("fid")),
            gdt_dir: Some(sig.join("typeinfo").join("win32")),
            die_dir: Some(sig.join("die")),
            patterns_dir: Some(sig.join("patterns")),
        };
        let r = resources::summarize_identity_resources_for(&cfg);
        assert!(r.die_pe_json_present);
        assert!(r.patterns_present);
        assert_eq!(r.pattern_json_count, Some(1));
        assert!(r.win_typeinfo_present);
        assert!(r.fid_present);
        assert_eq!(r.fid_bf_count, Some(1));
        assert!(r.die_sg_file_count.unwrap_or(0) >= 1);
    }

    #[test]
    fn die_compat_indexes_skipped_primitives() {
        use crate::detector::die_engine::SignatureDatabase;
        use std::fs;

        let tmp = tempfile::tempdir().expect("tempdir");
        let json = r#"{
            "format_version": "t",
            "description": "t",
            "source": "t",
            "signatures": [{
                "name": "Mixed",
                "type": "compiler",
                "rules": [
                    {"type":"section_name","name":".nope"},
                    {"type":"ep_pattern","pattern":"9090"}
                ]
            }]
        }"#;
        let path = tmp.path().join("pe_signatures.json");
        fs::write(&path, json).expect("write");
        let db = SignatureDatabase::load(&path).expect("load");

        let binary =
            LoadedBinaryBuilder::new("stub.exe".to_string(), DataBuffer::Heap(vec![0u8; 512]))
                .format("PE64")
                .entry_point(0x140001000)
                .image_base(0x140000000)
                .is_64bit(true)
                .add_section(SectionInfo {
                    name: ".text".to_string(),
                    virtual_address: 0x140001000,
                    virtual_size: 256,
                    file_offset: 0,
                    file_size: 256,
                    is_executable: true,
                    is_readable: true,
                    is_writable: false,
                })
                .build()
                .expect("build");

        let (sum, _) =
            die_compat::die_compat_identity_with_db(&binary, &IdentityScanLimits::default(), db);
        assert_eq!(sum.rules_seen, 2);
        assert_eq!(sum.rules_supported, 1);
        assert_eq!(sum.rules_skipped, 1);
        assert!(sum.unsupported_primitives.contains_key("ep_pattern"));
    }
}
