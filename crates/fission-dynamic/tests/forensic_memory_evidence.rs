#![cfg(feature = "interactive_runtime")]

use fission_dynamic::debug::forensics::{
    AcquisitionStatus, ByteSourceKind, ImportCandidate, ModuleAcquisitionOptions,
    ModuleMemoryAcquisition, PeRebuildOptions, PeReconstructionMode, acquire_module_memory,
    default_acquisition_path, default_manifest_path, rebuild_imports_derived,
    reconstruct_pe_from_acquisition,
};
use fission_dynamic::debug::memory::{
    MemoryError, MemoryGapReason, MemoryManager, MemoryProtection, MemoryRegion, SparseByte,
};
use fission_dynamic::debug::platform::PlatformMemory;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct MockSegment {
    base: u64,
    bytes: Vec<u8>,
    prot: MemoryProtection,
    name: String,
    fault: Option<MemoryError>,
}

struct MockProcessMemory {
    open: bool,
    segments: Vec<MockSegment>,
}

impl MockProcessMemory {
    fn new(segments: Vec<MockSegment>) -> Self {
        Self {
            open: true,
            segments,
        }
    }
}

impl PlatformMemory for MockProcessMemory {
    fn open_process(&mut self, _pid: u32) -> Result<(), MemoryError> {
        self.open = true;
        Ok(())
    }

    fn read_into(&self, address: u64, buffer: &mut [u8]) -> Result<usize, MemoryError> {
        if !self.open {
            return Err(MemoryError::NoProcess);
        }
        let Some(seg) = self
            .segments
            .iter()
            .find(|s| address >= s.base && address < s.base + s.bytes.len() as u64)
        else {
            return Err(MemoryError::Unmapped {
                address,
                size: buffer.len(),
            });
        };
        if let Some(err) = &seg.fault {
            return Err(err.clone());
        }
        let off = (address - seg.base) as usize;
        let avail = seg.bytes.len() - off;
        let copy_len = buffer.len().min(avail);
        buffer[..copy_len].copy_from_slice(&seg.bytes[off..off + copy_len]);
        Ok(copy_len)
    }

    fn write(&self, _address: u64, _data: &[u8]) -> Result<usize, MemoryError> {
        Err(MemoryError::WriteFailed {
            address: 0,
            reason: "read-only mock".to_string(),
        })
    }

    fn query_regions(&self) -> Result<Vec<MemoryRegion>, MemoryError> {
        Ok(self
            .segments
            .iter()
            .map(|s| MemoryRegion {
                base_address: s.base,
                size: s.bytes.len(),
                protection: s.prot,
                name: Some(s.name.clone()),
            })
            .collect())
    }

    fn is_open(&self) -> bool {
        self.open
    }
}

fn temp_dir(label: &str) -> PathBuf {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let id = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "fission_forensics_test_{label}_{}_{}",
        std::process::id(),
        id
    ));
    fs::create_dir_all(&dir).expect("create temp test dir");
    dir
}

fn write_pe_section_header(
    headers: &mut [u8],
    index: usize,
    section_table_offset: usize,
    name: &[u8; 8],
    vsize: u32,
    rva: u32,
    rsize: u32,
    rptr: u32,
    chars: u32,
) {
    let at = section_table_offset + index * 40;
    headers[at..at + 8].copy_from_slice(name);
    headers[at + 8..at + 12].copy_from_slice(&vsize.to_le_bytes());
    headers[at + 12..at + 16].copy_from_slice(&rva.to_le_bytes());
    headers[at + 16..at + 20].copy_from_slice(&rsize.to_le_bytes());
    headers[at + 20..at + 24].copy_from_slice(&rptr.to_le_bytes());
    headers[at + 36..at + 40].copy_from_slice(&chars.to_le_bytes());
}

fn build_fixture_process_image(base: u64) -> (MockProcessMemory, u64) {
    // 4 sections: .text, .data (all zeros observed), .bss (virtual-only, no raw bytes), .secret (AccessDenied)
    let mut headers = vec![0u8; 0x400];
    headers[0..2].copy_from_slice(b"MZ");
    // Modified runtime marker inside DOS stub to verify live header mutations survive acquisition verbatim.
    headers[0x40..0x51].copy_from_slice(b"MODIFIED_LIVE_HDR");
    headers[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
    headers[0x80..0x84].copy_from_slice(b"PE\0\0");
    let file_header = 0x84;
    headers[file_header..file_header + 2].copy_from_slice(&0x8664u16.to_le_bytes());
    headers[file_header + 2..file_header + 4].copy_from_slice(&4u16.to_le_bytes());
    headers[file_header + 16..file_header + 18].copy_from_slice(&0xF0u16.to_le_bytes());
    let optional = file_header + 20;
    headers[optional..optional + 2].copy_from_slice(&0x20bu16.to_le_bytes());
    headers[optional + 32..optional + 36].copy_from_slice(&0x1000u32.to_le_bytes()); // SectionAlignment
    headers[optional + 36..optional + 40].copy_from_slice(&0x200u32.to_le_bytes()); // FileAlignment
    headers[optional + 56..optional + 60].copy_from_slice(&0x5000u32.to_le_bytes()); // SizeOfImage
    headers[optional + 60..optional + 64].copy_from_slice(&0x400u32.to_le_bytes()); // SizeOfHeaders
    let rva_count = optional + 108;
    headers[rva_count..rva_count + 4].copy_from_slice(&16u32.to_le_bytes());

    let sec_table = optional + 0xF0;
    write_pe_section_header(
        &mut headers,
        0,
        sec_table,
        b".text\0\0\0",
        0x180,
        0x1000,
        0x200,
        0x400,
        0x6000_0020,
    );
    write_pe_section_header(
        &mut headers,
        1,
        sec_table,
        b".data\0\0\0",
        0x100,
        0x2000,
        0x200,
        0x600,
        0xC000_0040,
    );
    write_pe_section_header(
        &mut headers,
        2,
        sec_table,
        b".bss\0\0\0\0",
        0x400,
        0x3000,
        0,
        0,
        0xC000_0080,
    );
    write_pe_section_header(
        &mut headers,
        3,
        sec_table,
        b".secret\0",
        0x1000,
        0x4000,
        0x1000,
        0x800,
        0x4000_0040,
    );

    let mut text = vec![0x90u8; 0x1000];
    text[0..4].copy_from_slice(&[0x48, 0x31, 0xC0, 0xC3]);

    // Legitimate observed zeros in .data.
    let data = vec![0x00u8; 0x1000];

    // Live runtime bytes populated into .bss (which has SizeOfRawData = 0 on disk).
    let mut bss = vec![0u8; 0x1000];
    bss[0..8].copy_from_slice(b"BSS_LIVE");

    let secret = vec![0xAAu8; 0x1000];

    // Extra out-of-file-layout anonymous allocation outside module span.
    let extra_va = 0x7FFE_0000u64;
    let extra_data = b"OUT_OF_FILE_LAYOUT_REGION_BYTES!".to_vec();

    // Also include 0x400..0x1000 unmapped/gap padding in headers page? Let's make headers cover full 0x1000
    // with header bytes in 0..0x400 and observed out-of-file-layout tail in 0x400..0x1000.
    headers.resize(0x1000, 0xCC);

    let mock = MockProcessMemory::new(vec![
        MockSegment {
            base,
            bytes: headers,
            prot: MemoryProtection::RW,
            name: "sample.exe!headers".into(),
            fault: None,
        },
        MockSegment {
            base: base + 0x1000,
            bytes: text,
            prot: MemoryProtection::RX,
            name: "sample.exe!.text".into(),
            fault: None,
        },
        MockSegment {
            base: base + 0x2000,
            bytes: data,
            prot: MemoryProtection::RW,
            name: "sample.exe!.data".into(),
            fault: None,
        },
        MockSegment {
            base: base + 0x3000,
            bytes: bss,
            prot: MemoryProtection::RW,
            name: "sample.exe!.bss".into(),
            fault: None,
        },
        MockSegment {
            base: base + 0x4000,
            bytes: secret,
            prot: MemoryProtection::NONE,
            name: "sample.exe!.secret".into(),
            fault: Some(MemoryError::AccessDenied {
                address: base + 0x4000,
            }),
        },
        MockSegment {
            base: extra_va,
            bytes: extra_data,
            prot: MemoryProtection::RW,
            name: "[anon_heap]".into(),
            fault: None,
        },
    ]);
    (mock, extra_va)
}

#[test]
fn partial_capture_separates_raw_evidence_from_reconstructed_pe_and_iat_derivatives() {
    let base = 0x1_4000_0000u64;
    let (mock, extra_va) = build_fixture_process_image(base);
    let mut mm = MemoryManager::with_backend(mock);

    let acq_opts = ModuleAcquisitionOptions {
        module_name: Some("sample.exe".to_string()),
        process_id: Some(4242),
        extra_va_ranges: vec![(extra_va, 32)],
        ..Default::default()
    };
    let acquisition = acquire_module_memory(&mut mm, base, &acq_opts).expect("acquire module");

    assert_eq!(acquisition.status, AcquisitionStatus::Partial);
    assert!(acquisition.verify_digest());
    assert_eq!(acquisition.total_missing_bytes, 0x1000);
    assert_eq!(acquisition.total_observed_bytes, 0x4000 + 32);
    assert_eq!(acquisition.mappings.len(), 6);

    // Changed header bytes, legitimate zero data, BSS bytes, and extra VA region are all preserved verbatim in raw acquisition.
    assert_eq!(
        acquisition
            .primary_capture
            .read_exact_slice(base + 0x40, 17)
            .unwrap(),
        b"MODIFIED_LIVE_HDR"
    );
    assert_eq!(
        acquisition.lookup_byte(base + 0x2005),
        Some(SparseByte::Observed(0))
    );
    assert_eq!(
        acquisition
            .primary_capture
            .read_exact_slice(base + 0x3000, 8)
            .unwrap(),
        b"BSS_LIVE"
    );
    assert_eq!(
        acquisition.lookup_byte(base + 0x4000),
        Some(SparseByte::Missing(&MemoryGapReason::Denied))
    );
    assert_eq!(
        acquisition.extra_captures[0]
            .read_exact_slice(extra_va, 32)
            .unwrap(),
        b"OUT_OF_FILE_LAYOUT_REGION_BYTES!"
    );

    // Persist immutable raw acquisition to disk (no-clobber).
    let dir = temp_dir("lineage");
    let raw_pe_path = dir.join("sample_rebuilt.exe");
    let acq_path = default_acquisition_path(&raw_pe_path);
    acquisition
        .save_no_clobber(&acq_path)
        .expect("save acquisition");
    let original_acq_disk_bytes = fs::read(&acq_path).unwrap();
    // No-clobber refuses overwriting the preserved acquisition.
    assert!(acquisition.save_no_clobber(&acq_path).is_err());

    // Reload verified acquisition and derive PE image + provenance manifest.
    let reloaded = ModuleMemoryAcquisition::load_verified(&acq_path).expect("reload verified");
    assert_eq!(reloaded.artifact_digest, acquisition.artifact_digest);

    let derived = reconstruct_pe_from_acquisition(&reloaded, &PeRebuildOptions::default())
        .expect("reconstruct PE");
    let manifest_path = default_manifest_path(&raw_pe_path);
    derived
        .save_no_clobber(&raw_pe_path, &manifest_path)
        .expect("save derived PE");
    // No-clobber refuses overwriting derived PE/manifest too.
    assert!(
        derived
            .save_no_clobber(&raw_pe_path, &manifest_path)
            .is_err()
    );

    // Derived PE never hides partial raw capture behind a valid PE structure.
    assert_eq!(
        derived.manifest.parent_capture_status,
        AcquisitionStatus::Partial
    );
    assert_eq!(derived.manifest.parent_missing_bytes, 0x1000);
    assert_eq!(
        derived.manifest.root_acquisition_digest,
        acquisition.artifact_digest
    );
    assert_eq!(
        derived.manifest.parent_artifact_digest,
        acquisition.artifact_digest
    );
    derived
        .manifest
        .validate_provenance(derived.bytes.len() as u64)
        .expect("full provenance partition");

    // Modified header byte is traced to ObservedMemory at base + 0x40.
    let hdr_source = derived.manifest.resolve_file_offset(0x40).unwrap();
    assert_eq!(
        hdr_source,
        ByteSourceKind::ObservedMemory {
            source_va: base + 0x40
        }
    );

    // .text code [0x400..0x580) is ObservedMemory; trailing file alignment [0x580..0x600) is SynthesizedAlignmentPadding.
    assert!(matches!(
        derived.manifest.resolve_file_offset(0x400).unwrap(),
        ByteSourceKind::ObservedMemory { source_va } if source_va == base + 0x1000
    ));
    assert!(matches!(
        derived.manifest.resolve_file_offset(0x590).unwrap(),
        ByteSourceKind::SynthesizedAlignmentPadding { .. }
    ));

    // .data genuine 0x00 byte at file offset 0x605 is ObservedMemory (NEVER confused with synthesized gap zeros).
    assert_eq!(derived.bytes[0x605], 0x00);
    assert_eq!(
        derived.manifest.resolve_file_offset(0x605).unwrap(),
        ByteSourceKind::ObservedMemory {
            source_va: base + 0x2005
        }
    );

    // .secret unavailable bytes [0x800..0x1800) are zero-filled in the PE, explicitly marked SynthesizedGapFill(Denied).
    assert!(derived.bytes[0x800..0x1800].iter().all(|&b| b == 0));
    let secret_source = derived.manifest.resolve_file_offset(0x800).unwrap();
    assert!(matches!(
        secret_source,
        ByteSourceKind::SynthesizedGapFill {
            source_va,
            reason: MemoryGapReason::Denied,
        } if source_va == base + 0x4000
    ));

    // .bss live bytes and out-of-file-layout extra capture are documented in omitted_ranges!
    assert!(
        derived
            .manifest
            .omitted_ranges
            .iter()
            .any(|o| o.va_start == base + 0x3000 && o.len == 0x400 && o.observed_bytes == 0x400)
    );
    assert!(
        derived
            .manifest
            .omitted_ranges
            .iter()
            .any(|o| o.va_start == extra_va && o.len == 32 && o.observed_bytes == 32)
    );

    // Rebuild imports into a SECOND derivative artifact without touching original PE or acquisition!
    let candidates = vec![
        ImportCandidate {
            rva: 0x1010,
            target_address: 0x7FFF_0010,
            module_name: Some("kernel32.dll".to_string()),
            function_name: Some("ExitProcess".to_string()),
            ordinal: Some(0),
            unresolved_reason: None,
        },
        ImportCandidate {
            rva: 0x1018,
            target_address: 0xDEAD_BEEF,
            module_name: None,
            function_name: None,
            ordinal: None,
            unresolved_reason: Some("call target landed in unmapped DLL".to_string()),
        },
        ImportCandidate {
            rva: 0x1020,
            target_address: 0xCAFE_BABE,
            module_name: Some("user32.dll".to_string()),
            function_name: None,
            ordinal: Some(0x1_0001),
            unresolved_reason: None,
        },
    ];
    let rebuilt = rebuild_imports_derived(&derived, &candidates).expect("rebuild imports");
    let rebuilt_path = dir.join("sample_rebuilt_iat.exe");
    let rebuilt_manifest_path = default_manifest_path(&rebuilt_path);
    rebuilt
        .save_no_clobber(&rebuilt_path, &rebuilt_manifest_path)
        .expect("save rebuilt derivative");

    // Lineage chain links back to both immediate parent and root raw acquisition; original file & digest are intact!
    assert_eq!(
        rebuilt.manifest.root_acquisition_digest,
        acquisition.artifact_digest
    );
    assert_eq!(
        rebuilt.manifest.parent_artifact_digest,
        derived.manifest.derived_bytes_digest
    );
    assert_eq!(rebuilt.manifest.transformations.len(), 2);
    assert_eq!(rebuilt.manifest.unresolved_imports.len(), 2);
    rebuilt
        .manifest
        .validate_provenance(rebuilt.bytes.len() as u64)
        .expect("iat provenance valid");

    // New header patch & appended .fission section are distinguishable from observed memory.
    let patched_section_hdr = rebuilt
        .manifest
        .trace_file_offset(0x84 + 20 + 0xF0 + 4 * 40)
        .unwrap();
    assert!(matches!(
        patched_section_hdr.source,
        ByteSourceKind::SynthesizedHeaderPatch { .. }
    ));
    assert!(matches!(
        rebuilt
            .manifest
            .trace_file_offset((rebuilt.bytes.len() - 4) as u64)
            .unwrap()
            .source,
        ByteSourceKind::SynthesizedImportSection { .. }
    ));

    // Original acquisition artifact on disk is completely unchanged.
    assert_eq!(fs::read(&acq_path).unwrap(), original_acq_disk_bytes);

    // Deterministic reproducibility: re-running both stages from the saved acquisition produces identical outputs.
    let derived_2 =
        reconstruct_pe_from_acquisition(&reloaded, &PeRebuildOptions::default()).unwrap();
    let rebuilt_2 = rebuild_imports_derived(&derived_2, &candidates).unwrap();
    assert_eq!(derived_2.bytes, derived.bytes);
    assert_eq!(derived_2.manifest, derived.manifest);
    assert_eq!(rebuilt_2.bytes, rebuilt.bytes);
    assert_eq!(rebuilt_2.manifest, rebuilt.manifest);

    // PreserveVirtualTails option records mode deterministically in parameters.
    let vtail_derived = reconstruct_pe_from_acquisition(
        &reloaded,
        &PeRebuildOptions {
            mode: PeReconstructionMode::PreserveVirtualTails,
            ..Default::default()
        },
    )
    .unwrap();
    assert_ne!(
        vtail_derived.manifest.transformations[0],
        derived.manifest.transformations[0]
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn failed_acquisition_refuses_pe_fabrication_and_short_read_does_not_zero_pad() {
    let base = 0x1_0000_0000u64;
    let mock = MockProcessMemory::new(vec![MockSegment {
        base,
        bytes: vec![0xAA; 0x100], // Only 0x100 readable, rest unmapped
        prot: MemoryProtection::NONE,
        name: "bad.exe".into(),
        fault: Some(MemoryError::AccessDenied { address: base }),
    }]);
    let mut mm = MemoryManager::with_backend(mock);

    let failed = acquire_module_memory(&mut mm, base, &ModuleAcquisitionOptions::default())
        .expect("returns structured Failed acquisition");
    assert_eq!(failed.status, AcquisitionStatus::Failed);
    assert_eq!(failed.total_observed_bytes, 0);
    assert!(failed.total_missing_bytes > 0);
    assert!(reconstruct_pe_from_acquisition(&failed, &PeRebuildOptions::default()).is_err());

    // Tampered acquisition digest is rejected.
    let mut partial_mm = MemoryManager::with_backend(MockProcessMemory::new(vec![MockSegment {
        base,
        bytes: vec![0x4Du8; 0x100],
        prot: MemoryProtection::RW,
        name: "partial.bin".into(),
        fault: None,
    }]));
    let mut partial = acquire_module_memory(
        &mut partial_mm,
        base,
        &ModuleAcquisitionOptions {
            explicit_span_bytes: Some(0x200),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(partial.status, AcquisitionStatus::Partial);
    assert_eq!(partial.total_observed_bytes, 0x100);
    assert_eq!(partial.total_missing_bytes, 0x100);
    partial.base_address += 1;
    assert!(!partial.verify_digest());
    assert!(reconstruct_pe_from_acquisition(&partial, &PeRebuildOptions::default()).is_err());

    // Sparse capture deserialization validates exact coverage invariant (holes/overlaps rejected).
    let tampered_json = serde_json::json!({
        "start": 4096,
        "requested_len": 4096,
        "segments": []
    });
    assert!(
        serde_json::from_value::<fission_dynamic::debug::memory::SparseMemoryCapture>(
            tampered_json
        )
        .is_err()
    );
}
