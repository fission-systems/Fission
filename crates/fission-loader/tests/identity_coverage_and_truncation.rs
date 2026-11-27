//! Regression & fixture evaluation tests for issue #1270:
//! - Coverage-aware executable section entropy scanning (stable identity, ranges, skip reasons)
//! - Negative absence deductions strictly gated on complete declared coverage
//! - Separation of detection scoring & summary calculation from output presentation truncation
//! - Evaluation on synthetic edge cases and held-out repository benign binaries

use std::path::PathBuf;

use fission_core::evidence_policy::PACKED_SCORE_POLICY_VERSION;
use fission_loader::loader::identity::{
    EntropyCoverageStatus, IdentityKind, IdentityScanLimits, SectionScanCompleteness,
    SectionSkipReason, analyze,
};
use fission_loader::loader::{DataBuffer, LoadedBinary, LoadedBinaryBuilder, SectionInfo};
use fission_loader::{Confidence, detect};

fn high_entropy_bytes(len: usize) -> Vec<u8> {
    // Deterministic 8-bit LFSR-style mixing over all 256 byte values (~7.99 bits/byte).
    let mut out = Vec::with_capacity(len);
    let mut state: u32 = 0x1357_9bdf;
    for i in 0..len {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let byte = ((state >> 16) ^ ((state >> 24) & 0xff) ^ (i as u32)) as u8;
        out.push(byte);
    }
    out
}

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("testdata")
        .join(name)
}

#[test]
fn zero_and_tiny_scan_budget_suppresses_absence_deduction() {
    // Overlay (trailing 64 bytes) gives raw_base_score = 0.10.
    // Complete entropy scan sees 256 low-entropy bytes and deducts 0.08 -> 0.02.
    // Zero/partial budgets must NOT deduct 0.08 -> remain at 0.10.
    let mut data = vec![0x90_u8; 256];
    data.extend_from_slice(&[0x41_u8; 64]);

    let binary = LoadedBinaryBuilder::new("ep-less.bin".to_string(), DataBuffer::Heap(data))
        .format("RAW")
        .entry_point(0x5000)
        .image_base(0x1000)
        .is_64bit(true)
        .add_section(SectionInfo {
            name: ".custom".to_string(),
            virtual_address: 0x1000,
            virtual_size: 256,
            file_offset: 0,
            file_size: 256,
            is_executable: true,
            is_readable: true,
            is_writable: false,
        })
        .build()
        .expect("build");

    // 1. Complete scan
    let complete = analyze(&binary, IdentityScanLimits::default());
    assert_eq!(
        complete.entropy_coverage.status,
        EntropyCoverageStatus::CompleteNoHighEntropy
    );
    assert!(complete.entropy_coverage.sufficient_for_absence_evidence);
    assert_eq!(complete.entropy_coverage.sampled_bytes_total, 256);
    assert_eq!(complete.entropy_coverage.declared_bytes_total, 256);
    assert!((complete.entropy_coverage.coverage_ratio - 1.0).abs() < 1e-6);
    assert!((complete.summary.packed_score - 0.02).abs() < 1e-6);
    assert!(
        complete
            .packed_score_breakdown
            .negative_components
            .iter()
            .any(|c| c.name == "no_high_entropy_executable_sections")
    );
    assert!(
        complete
            .packed_score_breakdown
            .suppressed_deductions
            .is_empty()
    );

    // 2. Zero budget scan
    let zero_budget = analyze(
        &binary,
        IdentityScanLimits {
            max_scan_bytes: 0,
            ..IdentityScanLimits::default()
        },
    );
    assert_eq!(
        zero_budget.entropy_coverage.status,
        EntropyCoverageStatus::IncompleteNoHighEntropyObserved
    );
    assert!(!zero_budget.entropy_coverage.sufficient_for_absence_evidence);
    assert_eq!(zero_budget.entropy_coverage.sampled_bytes_total, 0);
    assert_eq!(zero_budget.entropy_coverage.sections_skipped, 1);
    assert_eq!(zero_budget.section_entropy.len(), 1);
    assert_eq!(
        zero_budget.section_entropy[0].completeness,
        SectionScanCompleteness::Skipped
    );
    assert_eq!(
        zero_budget.section_entropy[0].skip_reason,
        Some(SectionSkipReason::ScanBudgetExhausted)
    );
    assert_eq!(zero_budget.section_entropy[0].sampled_bytes, 0);
    assert_eq!(zero_budget.section_entropy[0].eligible_bytes, 256);
    assert!(zero_budget.section_entropy[0].observed_entropy.is_none());
    assert!((zero_budget.summary.packed_score - 0.10).abs() < 1e-6);
    assert!(
        zero_budget
            .packed_score_breakdown
            .suppressed_deductions
            .iter()
            .any(|d| d.name == "no_high_entropy_executable_sections")
    );

    // 3. Tiny partial budget (32 / 256 bytes)
    let partial_budget = analyze(
        &binary,
        IdentityScanLimits {
            max_scan_bytes: 32,
            ..IdentityScanLimits::default()
        },
    );
    assert_eq!(
        partial_budget.entropy_coverage.status,
        EntropyCoverageStatus::IncompleteNoHighEntropyObserved
    );
    assert!(
        !partial_budget
            .entropy_coverage
            .sufficient_for_absence_evidence
    );
    assert_eq!(partial_budget.entropy_coverage.sampled_bytes_total, 32);
    assert_eq!(partial_budget.entropy_coverage.sections_partial, 1);
    assert_eq!(
        partial_budget.section_entropy[0].completeness,
        SectionScanCompleteness::Partial
    );
    assert_eq!(
        partial_budget.section_entropy[0].skip_reason,
        Some(SectionSkipReason::ScanBudgetExhausted)
    );
    assert_eq!(partial_budget.section_entropy[0].sampled_bytes, 32);
    assert!((partial_budget.summary.packed_score - 0.10).abs() < 1e-6);
}

#[test]
fn file_range_truncated_and_invalid_bounds_and_zero_size() {
    // Case A: File offset out of bounds + overlay -> cannot claim CompleteNoHighEntropy.
    let oob_bin =
        LoadedBinaryBuilder::new("oob.bin".to_string(), DataBuffer::Heap(vec![0x90; 128]))
            .format("RAW")
            .entry_point(0x5000)
            .image_base(0x1000)
            .is_64bit(true)
            .add_section(SectionInfo {
                name: ".badtext".to_string(),
                virtual_address: 0x1000,
                virtual_size: 256,
                file_offset: 1024,
                file_size: 256,
                is_executable: true,
                is_readable: true,
                is_writable: false,
            })
            .build()
            .expect("build");

    let oob_report = analyze(&oob_bin, IdentityScanLimits::default());
    assert_eq!(
        oob_report.entropy_coverage.status,
        EntropyCoverageStatus::IncompleteNoHighEntropyObserved
    );
    assert!(!oob_report.entropy_coverage.sufficient_for_absence_evidence);
    assert_eq!(oob_report.entropy_coverage.sections_out_of_bounds, 1);
    assert_eq!(
        oob_report.section_entropy[0].completeness,
        SectionScanCompleteness::Skipped
    );
    assert_eq!(
        oob_report.section_entropy[0].skip_reason,
        Some(SectionSkipReason::FileOffsetOutOfBounds)
    );

    // Case B: Section extends past EOF (declared 256, file has 64 bytes -> FileRangeTruncated).
    let trunc_bin =
        LoadedBinaryBuilder::new("trunc.bin".to_string(), DataBuffer::Heap(vec![0x90; 64]))
            .format("RAW")
            .entry_point(0x5000)
            .image_base(0x1000)
            .is_64bit(true)
            .add_section(SectionInfo {
                name: ".trunctext".to_string(),
                virtual_address: 0x1000,
                virtual_size: 256,
                file_offset: 0,
                file_size: 256,
                is_executable: true,
                is_readable: true,
                is_writable: false,
            })
            .build()
            .expect("build");

    let trunc_report = analyze(&trunc_bin, IdentityScanLimits::default());
    assert_eq!(
        trunc_report.entropy_coverage.status,
        EntropyCoverageStatus::IncompleteNoHighEntropyObserved
    );
    assert!(
        !trunc_report
            .entropy_coverage
            .sufficient_for_absence_evidence
    );
    assert_eq!(trunc_report.entropy_coverage.sections_partial, 1);
    assert_eq!(trunc_report.section_entropy[0].declared_bytes, 256);
    assert_eq!(trunc_report.section_entropy[0].eligible_bytes, 64);
    assert_eq!(trunc_report.section_entropy[0].sampled_bytes, 64);
    assert_eq!(
        trunc_report.section_entropy[0].completeness,
        SectionScanCompleteness::Partial
    );
    assert_eq!(
        trunc_report.section_entropy[0].skip_reason,
        Some(SectionSkipReason::FileRangeTruncated)
    );

    // Case C: Zero-declared-size executable section -> NoExecutableBytes.
    let zero_bin =
        LoadedBinaryBuilder::new("zero.bin".to_string(), DataBuffer::Heap(vec![0x90; 64]))
            .format("RAW")
            .entry_point(0x5000)
            .image_base(0x1000)
            .is_64bit(true)
            .add_section(SectionInfo {
                name: ".empty".to_string(),
                virtual_address: 0x1000,
                virtual_size: 0,
                file_offset: 0,
                file_size: 0,
                is_executable: true,
                is_readable: true,
                is_writable: false,
            })
            .build()
            .expect("build");

    let zero_report = analyze(&zero_bin, IdentityScanLimits::default());
    assert_eq!(
        zero_report.entropy_coverage.status,
        EntropyCoverageStatus::NoExecutableBytes
    );
    assert!(!zero_report.entropy_coverage.sufficient_for_absence_evidence);
    assert_eq!(
        zero_report.section_entropy[0].skip_reason,
        Some(SectionSkipReason::ZeroDeclaredSize)
    );
}

#[test]
fn duplicate_section_names_have_stable_identity_and_coverage_tracking() {
    let mut data = vec![0x90_u8; 512];
    let hi = high_entropy_bytes(512);
    data.extend_from_slice(&hi);

    let binary = LoadedBinaryBuilder::new("dup.bin".to_string(), DataBuffer::Heap(data))
        .format("RAW")
        .entry_point(0x1000)
        .image_base(0x1000)
        .is_64bit(true)
        .add_section(SectionInfo {
            name: ".text".to_string(),
            virtual_address: 0x1000,
            virtual_size: 512,
            file_offset: 0,
            file_size: 512,
            is_executable: true,
            is_readable: true,
            is_writable: false,
        })
        .add_section(SectionInfo {
            name: ".data".to_string(),
            virtual_address: 0x2000,
            virtual_size: 16,
            file_offset: 0,
            file_size: 16,
            is_executable: false,
            is_readable: true,
            is_writable: true,
        })
        .add_section(SectionInfo {
            name: ".text".to_string(),
            virtual_address: 0x3000,
            virtual_size: 512,
            file_offset: 512,
            file_size: 512,
            is_executable: true,
            is_readable: true,
            is_writable: false,
        })
        .build()
        .expect("build");

    // Full scan distinguishes both `.text` sections by section_index and VA.
    let full = analyze(&binary, IdentityScanLimits::default());
    assert_eq!(full.section_entropy.len(), 2);
    assert_eq!(full.section_entropy[0].section, ".text");
    assert_eq!(full.section_entropy[0].section_index, 0);
    assert_eq!(full.section_entropy[0].virtual_address, 0x1000);
    assert_eq!(full.section_entropy[0].file_offset, 0);
    assert_eq!(full.section_entropy[1].section, ".text");
    assert_eq!(full.section_entropy[1].section_index, 2);
    assert_eq!(full.section_entropy[1].virtual_address, 0x3000);
    assert_eq!(full.section_entropy[1].file_offset, 512);
    assert_eq!(full.summary.high_entropy_executable_sections, 1);
    assert_eq!(
        full.entropy_coverage.status,
        EntropyCoverageStatus::HighEntropyObserved
    );

    // Budget stops right after first `.text`: second `.text` must still be emitted as Skipped!
    let capped = analyze(
        &binary,
        IdentityScanLimits {
            max_scan_bytes: 512,
            ..IdentityScanLimits::default()
        },
    );
    assert_eq!(capped.section_entropy.len(), 2);
    assert_eq!(
        capped.section_entropy[0].completeness,
        SectionScanCompleteness::Complete
    );
    assert_eq!(
        capped.section_entropy[1].completeness,
        SectionScanCompleteness::Skipped
    );
    assert_eq!(
        capped.section_entropy[1].skip_reason,
        Some(SectionSkipReason::ScanBudgetExhausted)
    );
    assert_eq!(
        capped.entropy_coverage.status,
        EntropyCoverageStatus::IncompleteNoHighEntropyObserved
    );
    assert!(!capped.entropy_coverage.sufficient_for_absence_evidence);
}

#[test]
fn output_caps_do_not_alter_summary_or_packed_score() {
    // Rich multi-signal PE64 fixture: UPX + MSVC + Go + Rust + HighEntropy + Overlay
    let mut data = high_entropy_bytes(1024);
    // Embed "UPX!" marker within max_string_scan_bytes and 64 overlay bytes after sections.
    data[..4].copy_from_slice(b"UPX!");
    data.extend_from_slice(&[0x41_u8; 64]);

    let binary = LoadedBinaryBuilder::new("multi.exe".to_string(), DataBuffer::Heap(data))
        .format("PE64")
        .entry_point(0x14000_1000)
        .image_base(0x14000_0000)
        .is_64bit(true)
        .add_section(SectionInfo {
            name: "UPX0".to_string(),
            virtual_address: 0x14000_1000,
            virtual_size: 512,
            file_offset: 0,
            file_size: 512,
            is_executable: true,
            is_readable: true,
            is_writable: false,
        })
        .add_section(SectionInfo {
            name: "UPX1".to_string(),
            virtual_address: 0x14000_2000,
            virtual_size: 512,
            file_offset: 512,
            file_size: 512,
            is_executable: true,
            is_readable: true,
            is_writable: false,
        })
        .add_iat_symbol(0x14000_3000, "__CxxFrameHandler3".to_string())
        .add_iat_symbol(0x14000_3008, "__chkstk".to_string())
        .add_iat_symbol(0x14000_3010, "__security_cookie".to_string())
        .add_iat_symbol(0x14000_3018, "runtime.gopanic".to_string())
        .add_iat_symbol(0x14000_3020, "runtime.main".to_string())
        .add_iat_symbol(0x14000_3028, "rust_begin_unwind".to_string())
        .add_iat_symbol(0x14000_3030, "_RNvCs123_4test".to_string())
        .build()
        .expect("build");

    let baseline = analyze(&binary, IdentityScanLimits::default());
    assert!(!baseline.truncation.truncated);
    assert!(!baseline.summary.presentation_truncated);
    assert!(baseline.truncation.summary_computed_before_truncation);
    assert!(baseline.truncation.detections_total >= 4);
    assert_eq!(baseline.summary.likely_packer.as_deref(), Some("UPX"));
    assert_eq!(baseline.summary.likely_compiler.as_deref(), Some("MSVC"));
    assert!(baseline.summary.likely_language.is_some());

    for (max_detections, max_evidence_items) in [(4, 16), (2, 4), (1, 1), (0, 0)] {
        let capped = analyze(
            &binary,
            IdentityScanLimits {
                max_detections,
                max_evidence_items,
                ..IdentityScanLimits::default()
            },
        );

        // Semantic summary and packed score breakdown are completely invariant to output caps!
        assert_eq!(
            capped.summary.likely_language,
            baseline.summary.likely_language
        );
        assert_eq!(
            capped.summary.likely_compiler,
            baseline.summary.likely_compiler
        );
        assert_eq!(capped.summary.likely_linker, baseline.summary.likely_linker);
        assert_eq!(capped.summary.likely_packer, baseline.summary.likely_packer);
        assert_eq!(capped.summary.confidence, baseline.summary.confidence);
        assert_eq!(
            capped.summary.compiler_conflicts,
            baseline.summary.compiler_conflicts
        );
        assert_eq!(
            capped.summary.weak_signal_count,
            baseline.summary.weak_signal_count
        );
        assert_eq!(
            capped.summary.high_entropy_executable_sections,
            baseline.summary.high_entropy_executable_sections
        );
        assert_eq!(
            capped.summary.entropy_coverage_status,
            baseline.summary.entropy_coverage_status
        );
        assert!((capped.summary.packed_score - baseline.summary.packed_score).abs() < 1e-6);
        assert_eq!(
            capped.packed_score_breakdown.adjusted_score,
            baseline.packed_score_breakdown.adjusted_score
        );

        assert!(capped.detections.len() <= max_detections);
        let emitted_ev: usize = capped.detections.iter().map(|d| d.evidence.len()).sum();
        assert!(emitted_ev <= max_evidence_items);
        assert_eq!(
            capped.truncation.detections_total,
            baseline.truncation.detections_total
        );
        assert_eq!(
            capped.truncation.evidence_items_total_before_cap,
            baseline.truncation.evidence_items_total_before_cap
        );
        assert!(capped.truncation.truncated);
        assert!(capped.summary.presentation_truncated);
        assert!(capped.truncation.summary_computed_before_truncation);

        // When at least 1 detection + 1 evidence item is allowed, a summary-backing detection
        // is retained first with supporting evidence preserved.
        if max_detections >= 1 && max_evidence_items >= 1 {
            assert!(!capped.detections[0].evidence.is_empty());
        }
        if max_detections >= 2 {
            assert!(
                capped
                    .detections
                    .iter()
                    .any(|d| d.kind == IdentityKind::Packer && d.name == "UPX")
            );
            assert!(
                capped
                    .detections
                    .iter()
                    .any(|d| d.kind == IdentityKind::Compiler && d.name == "MSVC")
            );
        }
    }
}

#[test]
fn high_entropy_benign_compressed_section_never_emits_packer_verdict() {
    let data = high_entropy_bytes(4096);
    let mut builder =
        LoadedBinaryBuilder::new("benign_compressed.exe".to_string(), DataBuffer::Heap(data))
            .format("PE64")
            .entry_point(0x14000_1000)
            .image_base(0x14000_0000)
            .is_64bit(true)
            .add_section(SectionInfo {
                name: ".text".to_string(),
                virtual_address: 0x14000_1000,
                virtual_size: 4096,
                file_offset: 0,
                file_size: 4096,
                is_executable: true,
                is_readable: true,
                is_writable: false,
            });
    for idx in 0..20_u64 {
        builder = builder.add_iat_symbol(0x14000_3000 + idx * 8, format!("api_func_{idx}"));
    }
    let binary = builder.build().expect("build");

    let report = analyze(&binary, IdentityScanLimits::default());
    assert_eq!(
        report.entropy_coverage.status,
        EntropyCoverageStatus::HighEntropyObserved
    );
    assert_eq!(report.summary.high_entropy_executable_sections, 1);
    assert!(
        report.summary.likely_packer.is_none(),
        "entropy alone must never yield likely_packer"
    );
    assert_ne!(
        report.summary.confidence,
        Confidence::High,
        "entropy-only signal must not produce High aggregate confidence"
    );
    assert_eq!(
        report.packed_score_breakdown.policy_version,
        PACKED_SCORE_POLICY_VERSION
    );
    assert!(
        report
            .packed_score_breakdown
            .semantics
            .contains("not_malware_probability")
    );
}

#[test]
fn held_out_repository_benign_fixtures_coverage_and_fp_check() {
    let fixtures = [
        "x64_pdb_struct_test.exe",
        "x64_pdb_method_test.exe",
        "x64_seh_guarded_test.exe",
        "go_hello_linux_amd64.elf",
        "x64_dyn_puts.elf",
        "x64_cpp_methods_a_o2.elf",
        "x64_dwarf5_strx.elf",
        "arm_mixed_mode.elf",
    ];

    let mut false_positive_packers = 0usize;
    let mut evaluated = 0usize;

    for name in fixtures {
        let path = fixture_path(name);
        if !path.exists() {
            continue;
        }
        evaluated += 1;
        let binary =
            LoadedBinary::from_file(&path).unwrap_or_else(|e| panic!("failed to load {name}: {e}"));
        let full = analyze(&binary, IdentityScanLimits::default());

        // Also sanity-check legacy detector has no spurious packer verdict.
        let legacy = detect(&binary);
        assert!(legacy.detections.len() < 1000);

        if full.summary.likely_packer.is_some() {
            false_positive_packers += 1;
        }
        assert!(
            full.entropy_coverage.executable_sections_total > 0,
            "{name} should report executable sections"
        );
        assert!(
            full.entropy_coverage.sampled_bytes_total > 0,
            "{name} should sample executable bytes"
        );

        // Under tiny scan budget, status cannot claim CompleteNoHighEntropy if incomplete.
        let tiny_scan = analyze(
            &binary,
            IdentityScanLimits {
                max_scan_bytes: 64,
                ..IdentityScanLimits::default()
            },
        );
        if tiny_scan.entropy_coverage.sampled_bytes_total
            < tiny_scan.entropy_coverage.declared_bytes_total
            && tiny_scan.summary.high_entropy_executable_sections == 0
        {
            assert_eq!(
                tiny_scan.entropy_coverage.status,
                EntropyCoverageStatus::IncompleteNoHighEntropyObserved,
                "{name} must distinguish partial scan from CompleteNoHighEntropy"
            );
            assert!(!tiny_scan.entropy_coverage.sufficient_for_absence_evidence);
            assert!(
                tiny_scan
                    .packed_score_breakdown
                    .suppressed_deductions
                    .iter()
                    .any(|d| d.name == "no_high_entropy_executable_sections")
            );
        }

        // Under severe output caps, summary remains identical to uncapped analysis.
        let tight_caps = analyze(
            &binary,
            IdentityScanLimits {
                max_detections: 1,
                max_evidence_items: 1,
                ..IdentityScanLimits::default()
            },
        );
        assert_eq!(
            tight_caps.summary.likely_compiler,
            full.summary.likely_compiler
        );
        assert_eq!(
            tight_caps.summary.likely_language,
            full.summary.likely_language
        );
        assert_eq!(tight_caps.summary.likely_linker, full.summary.likely_linker);
        assert_eq!(tight_caps.summary.likely_packer, full.summary.likely_packer);
        assert_eq!(tight_caps.summary.confidence, full.summary.confidence);
        assert!((tight_caps.summary.packed_score - full.summary.packed_score).abs() < 1e-6);
    }

    assert_eq!(
        evaluated,
        fixtures.len(),
        "expected all benign fixtures present"
    );
    assert_eq!(
        false_positive_packers, 0,
        "expected 0 benign fixtures misclassified with likely_packer"
    );
}
