//! Forensic separation between raw process-memory evidence and reconstructed PE derivatives.
//!
//! Raw process-memory acquisitions ([`ModuleMemoryAcquisition`]) record the
//! exact observed byte runs alongside explicit [`MemoryGap`] intervals and
//! virtual-memory mapping metadata. Reconstructed PE images and rebuilt import
//! tables ([`DerivedPeArtifact`]) are explicit derived artifacts whose
//! [`DerivedPeManifest`] tracks content digests, transformation parameters,
//! VA-to-file mappings, omitted virtual ranges, unresolved imports, and
//! complete byte-level provenance across every output byte.

use super::memory::{
    DEFAULT_MAX_READ_BYTES, DEFAULT_SPARSE_PAGE_SIZE, MemoryError, MemoryGap, MemoryGapReason,
    MemoryManager, MemorySegment, SparseByte, SparseMemoryCapture, SparseReadOptions,
};
use super::platform::PlatformMemory;
use super::platform::windows_layout::{
    ImportSpec, PeHeaders, PeSectionHeaderInfo, append_import_section_with_layout,
};
use fission_evidence::ContentDigest;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

pub const MODULE_ACQUISITION_SCHEMA_VERSION: &str = "fission.module_memory_acquisition.v1";
pub const DERIVED_PE_MANIFEST_SCHEMA_VERSION: &str = "fission.derived_pe_manifest.v1";
pub const PE_RECONSTRUCT_TRANSFORM_VERSION: &str = "pe_reconstruct/1.0";
pub const IAT_REBUILD_TRANSFORM_VERSION: &str = "iat_rebuild/1.0";
pub const DEFAULT_MAX_DERIVED_PE_BYTES: usize = 256 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcquisitionStatus {
    Complete,
    Partial,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcquiredMappingMetadata {
    pub base_address: u64,
    pub size: usize,
    pub read: bool,
    pub write: bool,
    pub execute: bool,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleAcquisitionOptions {
    pub module_name: Option<String>,
    pub process_id: Option<u32>,
    pub max_bytes: usize,
    pub max_span_bytes: usize,
    pub page_size: usize,
    pub explicit_span_bytes: Option<u64>,
    pub include_out_of_file_layout: bool,
    pub extra_va_ranges: Vec<(u64, u64)>,
    pub captured_at_unix_ms: Option<u64>,
}

impl Default for ModuleAcquisitionOptions {
    fn default() -> Self {
        Self {
            module_name: None,
            process_id: None,
            max_bytes: DEFAULT_MAX_READ_BYTES,
            max_span_bytes: DEFAULT_MAX_READ_BYTES,
            page_size: DEFAULT_SPARSE_PAGE_SIZE,
            explicit_span_bytes: None,
            include_out_of_file_layout: true,
            extra_va_ranges: Vec::new(),
            captured_at_unix_ms: None,
        }
    }
}

#[derive(Serialize)]
struct CanonicalAcquisitionDigestInput<'a> {
    schema_version: &'a str,
    status: AcquisitionStatus,
    base_address: u64,
    module_name: &'a Option<String>,
    process_id: Option<u32>,
    max_bytes_budget: usize,
    max_span_budget: usize,
    mappings: &'a [AcquiredMappingMetadata],
    primary_capture: &'a SparseMemoryCapture,
    extra_captures: &'a [SparseMemoryCapture],
}

/// Immutable raw forensic capture of a target module's virtual memory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleMemoryAcquisition {
    pub schema_version: String,
    pub status: AcquisitionStatus,
    pub base_address: u64,
    pub module_name: Option<String>,
    pub process_id: Option<u32>,
    pub captured_at_unix_ms: Option<u64>,
    pub max_bytes_budget: usize,
    pub max_span_budget: usize,
    pub mappings: Vec<AcquiredMappingMetadata>,
    pub primary_capture: SparseMemoryCapture,
    pub extra_captures: Vec<SparseMemoryCapture>,
    pub total_requested_bytes: u64,
    pub total_observed_bytes: u64,
    pub total_missing_bytes: u64,
    pub failure_reasons: Vec<String>,
    pub artifact_digest: ContentDigest,
}

impl ModuleMemoryAcquisition {
    pub fn compute_digest(&self) -> ContentDigest {
        let canonical = CanonicalAcquisitionDigestInput {
            schema_version: &self.schema_version,
            status: self.status,
            base_address: self.base_address,
            module_name: &self.module_name,
            process_id: self.process_id,
            max_bytes_budget: self.max_bytes_budget,
            max_span_budget: self.max_span_budget,
            mappings: &self.mappings,
            primary_capture: &self.primary_capture,
            extra_captures: &self.extra_captures,
        };
        let bytes = serde_json::to_vec(&canonical).expect("canonical acquisition JSON serializes");
        ContentDigest::of(&bytes)
    }

    pub fn verify_digest(&self) -> bool {
        self.artifact_digest == self.compute_digest()
    }

    pub fn lookup_byte(&self, va: u64) -> Option<SparseByte<'_>> {
        if let Some(byte) = self.primary_capture.byte_at(va) {
            return Some(byte);
        }
        for extra in &self.extra_captures {
            if let Some(byte) = extra.byte_at(va) {
                return Some(byte);
            }
        }
        None
    }

    /// Persist this acquisition strictly without overwriting an existing file.
    pub fn save_no_clobber(&self, path: &Path) -> Result<(), String> {
        if !self.verify_digest() {
            return Err(
                "refusing to persist acquisition with mismatched artifact_digest".to_string(),
            );
        }
        let encoded = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        write_file_no_clobber(path, &encoded)
    }

    /// Load an acquisition from disk and verify its content digest.
    pub fn load_verified(path: &Path) -> Result<Self, String> {
        let bytes =
            fs::read(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        let parsed: Self = serde_json::from_slice(&bytes)
            .map_err(|e| format!("failed to parse {}: {e}", path.display()))?;
        if !parsed.verify_digest() {
            return Err(format!(
                "acquisition digest verification failed for {}",
                path.display()
            ));
        }
        Ok(parsed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeReconstructionMode {
    FileLayout,
    PreserveVirtualTails,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeRebuildOptions {
    pub mode: PeReconstructionMode,
    pub max_output_bytes: usize,
}

impl Default for PeRebuildOptions {
    fn default() -> Self {
        Self {
            mode: PeReconstructionMode::FileLayout,
            max_output_bytes: DEFAULT_MAX_DERIVED_PE_BYTES,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ByteSourceKind {
    ObservedMemory {
        source_va: u64,
    },
    SynthesizedGapFill {
        source_va: u64,
        reason: MemoryGapReason,
    },
    SynthesizedAlignmentPadding {
        rule: String,
    },
    SynthesizedHeaderPatch {
        rule: String,
        field: String,
    },
    SynthesizedImportSection {
        rule: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ByteProvenanceSpan {
    pub file_offset: u64,
    pub len: u64,
    pub source: ByteSourceKind,
}

impl ByteProvenanceSpan {
    pub fn end(&self) -> u64 {
        self.file_offset.saturating_add(self.len)
    }

    pub fn source_va_at(&self, file_offset: u64) -> Option<u64> {
        if file_offset < self.file_offset || file_offset >= self.end() {
            return None;
        }
        let delta = file_offset - self.file_offset;
        match &self.source {
            ByteSourceKind::ObservedMemory { source_va }
            | ByteSourceKind::SynthesizedGapFill { source_va, .. } => Some(source_va + delta),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaFileMapping {
    pub region_name: String,
    pub va_start: u64,
    pub va_len: u64,
    pub file_offset: u64,
    pub file_len: u64,
    pub observed_bytes: u64,
    pub missing_bytes: u64,
    pub status: AcquisitionStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OmittedMemoryRange {
    pub va_start: u64,
    pub len: u64,
    pub observed_bytes: u64,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnresolvedImportRecord {
    pub rva: u64,
    pub target_address: u64,
    pub module_name: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportCandidate {
    pub rva: u64,
    pub target_address: u64,
    pub module_name: Option<String>,
    pub function_name: Option<String>,
    pub ordinal: Option<u32>,
    pub unresolved_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransformationRecord {
    pub transform_version: String,
    pub parent_digest: ContentDigest,
    pub output_digest: ContentDigest,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DerivedPeManifest {
    pub schema_version: String,
    pub root_acquisition_digest: ContentDigest,
    pub parent_artifact_digest: ContentDigest,
    pub derived_bytes_digest: ContentDigest,
    pub parent_capture_status: AcquisitionStatus,
    pub parent_observed_bytes: u64,
    pub parent_missing_bytes: u64,
    pub derived_observed_bytes: u64,
    pub derived_synthesized_bytes: u64,
    pub va_to_file_mappings: Vec<VaFileMapping>,
    pub byte_provenance: Vec<ByteProvenanceSpan>,
    pub omitted_ranges: Vec<OmittedMemoryRange>,
    pub unresolved_imports: Vec<UnresolvedImportRecord>,
    pub transformations: Vec<TransformationRecord>,
}

impl DerivedPeManifest {
    pub fn trace_file_offset(&self, offset: u64) -> Option<&ByteProvenanceSpan> {
        let idx = self
            .byte_provenance
            .partition_point(|span| span.end() <= offset);
        self.byte_provenance
            .get(idx)
            .filter(|span| offset >= span.file_offset && offset < span.end())
    }

    /// Resolve the exact per-byte provenance source (with offset-adjusted `source_va`)
    /// at a specific derived-file byte offset.
    pub fn resolve_file_offset(&self, offset: u64) -> Option<ByteSourceKind> {
        let span = self.trace_file_offset(offset)?;
        let delta = offset - span.file_offset;
        Some(match &span.source {
            ByteSourceKind::ObservedMemory { source_va } => ByteSourceKind::ObservedMemory {
                source_va: source_va + delta,
            },
            ByteSourceKind::SynthesizedGapFill { source_va, reason } => {
                ByteSourceKind::SynthesizedGapFill {
                    source_va: source_va + delta,
                    reason: reason.clone(),
                }
            }
            other => other.clone(),
        })
    }

    pub fn validate_provenance(&self, file_len: u64) -> Result<(), String> {
        let mut cursor = 0u64;
        let mut observed = 0u64;
        let mut synthesized = 0u64;
        for (idx, span) in self.byte_provenance.iter().enumerate() {
            if span.len == 0 {
                return Err(format!("provenance span {idx} is empty"));
            }
            if span.file_offset != cursor {
                return Err(format!(
                    "provenance span {idx} starts at {:#x}, expected {:#x}",
                    span.file_offset, cursor
                ));
            }
            let next = span
                .file_offset
                .checked_add(span.len)
                .ok_or_else(|| format!("provenance span {idx} overflows"))?;
            match &span.source {
                ByteSourceKind::ObservedMemory { .. } => observed += span.len,
                _ => synthesized += span.len,
            }
            cursor = next;
        }
        if cursor != file_len {
            return Err(format!(
                "provenance ends at {cursor:#x}, expected file_len {file_len:#x}"
            ));
        }
        if observed != self.derived_observed_bytes || synthesized != self.derived_synthesized_bytes
        {
            return Err(format!(
                "provenance accounting mismatch: observed={observed} (manifest {}), synthesized={synthesized} (manifest {})",
                self.derived_observed_bytes, self.derived_synthesized_bytes
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivedPeArtifact {
    pub bytes: Vec<u8>,
    pub manifest: DerivedPeManifest,
}

impl DerivedPeArtifact {
    /// Save derived PE bytes and manifest to separate paths without clobbering existing files.
    pub fn save_no_clobber(&self, pe_path: &Path, manifest_path: &Path) -> Result<(), String> {
        if pe_path == manifest_path {
            return Err("derived PE path and manifest path must be distinct".to_string());
        }
        self.manifest.validate_provenance(self.bytes.len() as u64)?;
        if ContentDigest::of(&self.bytes) != self.manifest.derived_bytes_digest {
            return Err("derived PE digest mismatch before write".to_string());
        }
        let manifest_bytes =
            serde_json::to_vec_pretty(&self.manifest).map_err(|e| e.to_string())?;
        write_file_no_clobber(pe_path, &self.bytes)?;
        if let Err(err) = write_file_no_clobber(manifest_path, &manifest_bytes) {
            let _ = fs::remove_file(pe_path);
            return Err(err);
        }
        Ok(())
    }
}

/// Companion path `<artifact>.acquisition.json` for raw memory acquisitions.
pub fn default_acquisition_path(derived_pe_path: &Path) -> std::path::PathBuf {
    append_suffix(derived_pe_path, ".acquisition.json")
}

/// Companion path `<artifact>.manifest.json` for derived PE manifests.
pub fn default_manifest_path(derived_pe_path: &Path) -> std::path::PathBuf {
    append_suffix(derived_pe_path, ".manifest.json")
}

fn append_suffix(base: &Path, suffix: &str) -> std::path::PathBuf {
    let mut os = base.as_os_str().to_os_string();
    os.push(suffix);
    std::path::PathBuf::from(os)
}

/// Atomic no-clobber file creation (`create_new(true)` + owner-only permissions on Unix).
pub fn write_file_no_clobber(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if path.exists() {
        return Err(format!(
            "refusing to overwrite existing forensic artifact {}",
            path.display()
        ));
    }
    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts
        .open(path)
        .map_err(|e| format!("failed to create {}: {e}", path.display()))?;
    file.write_all(bytes)
        .map_err(|e| format!("failed to write {}: {e}", path.display()))?;
    file.sync_all()
        .map_err(|e| format!("failed to sync {}: {e}", path.display()))?;
    Ok(())
}

fn build_budget_capped_capture(
    start: u64,
    total_requested: u64,
    inner: SparseMemoryCapture,
) -> Result<SparseMemoryCapture, MemoryError> {
    if total_requested <= inner.requested_len() {
        return Ok(inner);
    }
    let mut segs = inner.segments().to_vec();
    let tail_start = inner.end();
    let tail_len = total_requested - inner.requested_len();
    segs.push(MemorySegment::Gap(MemoryGap {
        start: tail_start,
        len: tail_len,
        reason: MemoryGapReason::Budget,
    }));
    Ok(SparseMemoryCapture::from_segments(
        start,
        total_requested,
        segs,
    )?)
}

fn leading_observed_bytes(capture: &SparseMemoryCapture) -> &[u8] {
    match capture.segments().first() {
        Some(MemorySegment::Observed(run)) => &run.bytes,
        _ => &[],
    }
}

fn determine_pe_module_span(
    headers: &PeHeaders,
    sections: &[PeSectionHeaderInfo],
    include_out_of_file_layout: bool,
) -> u64 {
    let mut span = u64::from(headers.size_of_headers);
    if include_out_of_file_layout {
        span = span.max(u64::from(headers.size_of_image));
    }
    for sec in sections {
        let len = if include_out_of_file_layout {
            u64::from(sec.virtual_size.max(sec.raw_size))
        } else {
            u64::from(sec.raw_size)
        };
        if let Some(end) = u64::from(sec.virtual_address).checked_add(len) {
            span = span.max(end);
        }
    }
    span.max(DEFAULT_SPARSE_PAGE_SIZE as u64)
}

/// Acquire bounded, gap-preserving raw memory evidence for a loaded module.
pub fn acquire_module_memory<M: PlatformMemory>(
    memory: &mut MemoryManager<M>,
    base_address: u64,
    options: &ModuleAcquisitionOptions,
) -> Result<ModuleMemoryAcquisition, MemoryError> {
    if options.page_size == 0 || options.max_bytes == 0 || options.max_span_bytes == 0 {
        return Err(MemoryError::InvalidRegion {
            address: base_address,
            size: 0,
        });
    }
    if !memory.is_open() {
        return Err(MemoryError::NoProcess);
    }

    let queried_regions = memory.query_regions().unwrap_or(&[]).to_vec();
    let probe_len = options
        .page_size
        .min(options.max_bytes)
        .min(options.max_span_bytes);
    let probe_opts = SparseReadOptions {
        max_bytes: probe_len,
        page_size: options.page_size,
        max_span_bytes: probe_len,
    };
    let header_probe = memory.read_sparse(base_address, probe_len, probe_opts)?;

    let desired_span: u64 = if let Some(explicit) = options.explicit_span_bytes {
        explicit.max(1)
    } else {
        let prefix = leading_observed_bytes(&header_probe);
        if let Ok(initial_headers) = PeHeaders::parse(prefix) {
            let need_headers = usize::try_from(initial_headers.size_of_headers)
                .unwrap_or(prefix.len())
                .min(options.max_bytes)
                .min(options.max_span_bytes)
                .max(probe_len);
            let header_capture = if need_headers > prefix.len() {
                let opts = SparseReadOptions {
                    max_bytes: need_headers,
                    page_size: options.page_size,
                    max_span_bytes: need_headers,
                };
                memory.read_sparse(base_address, need_headers, opts)?
            } else {
                header_probe.clone()
            };
            let full_prefix = leading_observed_bytes(&header_capture);
            if let Ok(headers) = PeHeaders::parse(full_prefix)
                && let Ok(sections) = headers.parse_sections(full_prefix)
            {
                determine_pe_module_span(&headers, &sections, options.include_out_of_file_layout)
            } else {
                determine_pe_module_span(&initial_headers, &[], options.include_out_of_file_layout)
            }
        } else if let Some(region) = queried_regions.iter().find(|r| {
            let end = r.base_address.saturating_add(r.size as u64);
            base_address >= r.base_address && base_address < end
        }) {
            region
                .base_address
                .saturating_add(region.size as u64)
                .saturating_sub(base_address)
                .max(1)
        } else {
            probe_len as u64
        }
    };

    if base_address.checked_add(desired_span).is_none() {
        return Err(MemoryError::InvalidRegion {
            address: base_address,
            size: usize::try_from(desired_span).unwrap_or(usize::MAX),
        });
    }

    let capped_primary = usize::try_from(desired_span)
        .unwrap_or(usize::MAX)
        .min(options.max_span_bytes);
    let primary_opts = SparseReadOptions {
        max_bytes: options.max_bytes,
        page_size: options.page_size,
        max_span_bytes: options.max_span_bytes,
    };
    let primary_raw = memory.read_sparse(base_address, capped_primary, primary_opts)?;
    let primary_capture = build_budget_capped_capture(base_address, desired_span, primary_raw)?;

    let mut remaining_obs_budget = options
        .max_bytes
        .saturating_sub(primary_capture.coverage().observed_bytes as usize);
    let mut remaining_span_budget = options.max_span_bytes.saturating_sub(capped_primary);

    let mut extra_captures = Vec::with_capacity(options.extra_va_ranges.len());
    for &(va, len) in &options.extra_va_ranges {
        if va.checked_add(len).is_none() {
            return Err(MemoryError::InvalidRegion {
                address: va,
                size: usize::try_from(len).unwrap_or(usize::MAX),
            });
        }
        if len == 0 {
            continue;
        }
        if remaining_obs_budget == 0 || remaining_span_budget == 0 {
            extra_captures.push(SparseMemoryCapture::from_segments(
                va,
                len,
                vec![MemorySegment::Gap(MemoryGap {
                    start: va,
                    len,
                    reason: MemoryGapReason::Budget,
                })],
            )?);
            continue;
        }
        let probe_extra = usize::try_from(len)
            .unwrap_or(usize::MAX)
            .min(remaining_span_budget);
        let extra_raw = memory.read_sparse(
            va,
            probe_extra,
            SparseReadOptions {
                max_bytes: remaining_obs_budget,
                page_size: options.page_size,
                max_span_bytes: remaining_span_budget,
            },
        )?;
        let capped = build_budget_capped_capture(va, len, extra_raw)?;
        remaining_obs_budget =
            remaining_obs_budget.saturating_sub(capped.coverage().observed_bytes as usize);
        remaining_span_budget = remaining_span_budget.saturating_sub(probe_extra);
        extra_captures.push(capped);
    }

    let primary_end = primary_capture.end();
    let mappings: Vec<AcquiredMappingMetadata> = queried_regions
        .into_iter()
        .filter(|r| {
            let r_end = r.base_address.saturating_add(r.size as u64);
            let overlaps_primary = r.base_address < primary_end && r_end > base_address;
            let overlaps_extra = extra_captures
                .iter()
                .any(|c| r.base_address < c.end() && r_end > c.start());
            overlaps_primary || overlaps_extra
        })
        .map(|r| AcquiredMappingMetadata {
            base_address: r.base_address,
            size: r.size,
            read: r.protection.read,
            write: r.protection.write,
            execute: r.protection.execute,
            name: r.name,
        })
        .collect();

    let mut total_requested_bytes = primary_capture.requested_len();
    let mut total_observed_bytes = primary_capture.coverage().observed_bytes;
    let mut total_missing_bytes = primary_capture.coverage().missing_bytes;
    let mut failure_reasons = Vec::new();
    collect_gap_reasons(&primary_capture, &mut failure_reasons);
    for extra in &extra_captures {
        total_requested_bytes += extra.requested_len();
        total_observed_bytes += extra.coverage().observed_bytes;
        total_missing_bytes += extra.coverage().missing_bytes;
        collect_gap_reasons(extra, &mut failure_reasons);
    }

    let status = if total_observed_bytes == 0 {
        AcquisitionStatus::Failed
    } else if total_missing_bytes == 0 {
        AcquisitionStatus::Complete
    } else {
        AcquisitionStatus::Partial
    };

    let mut acq = ModuleMemoryAcquisition {
        schema_version: MODULE_ACQUISITION_SCHEMA_VERSION.to_string(),
        status,
        base_address,
        module_name: options.module_name.clone(),
        process_id: options.process_id,
        captured_at_unix_ms: options.captured_at_unix_ms,
        max_bytes_budget: options.max_bytes,
        max_span_budget: options.max_span_bytes,
        mappings,
        primary_capture,
        extra_captures,
        total_requested_bytes,
        total_observed_bytes,
        total_missing_bytes,
        failure_reasons,
        artifact_digest: ContentDigest::of(b"pending"),
    };
    acq.artifact_digest = acq.compute_digest();
    Ok(acq)
}

fn collect_gap_reasons(capture: &SparseMemoryCapture, out: &mut Vec<String>) {
    for gap in capture.gaps() {
        out.push(format!(
            "unread gap [{:#x}, {:#x}) ({} bytes): {:?}",
            gap.start,
            gap.end(),
            gap.len,
            gap.reason
        ));
    }
}

fn copy_capture_into_file(
    capture: &SparseMemoryCapture,
    va_start: u64,
    copy_len: u64,
    file_offset: usize,
    out_bytes: &mut [u8],
    provenance: &mut [ByteSourceKind],
) -> (u64, u64) {
    if copy_len == 0 {
        return (0, 0);
    }
    let va_end = va_start + copy_len;
    let mut observed = 0u64;
    let mut missing = 0u64;

    for seg in capture.segments() {
        let s_start = seg.start();
        let s_end = seg.end();
        if s_end <= va_start {
            continue;
        }
        if s_start >= va_end {
            break;
        }
        let overlap_start = s_start.max(va_start);
        let overlap_end = s_end.min(va_end);
        let overlap_len = (overlap_end - overlap_start) as usize;
        let dst_off = file_offset + (overlap_start - va_start) as usize;

        match seg {
            MemorySegment::Observed(run) => {
                let src_off = (overlap_start - run.start) as usize;
                out_bytes[dst_off..dst_off + overlap_len]
                    .copy_from_slice(&run.bytes[src_off..src_off + overlap_len]);
                for (i, slot) in provenance[dst_off..dst_off + overlap_len]
                    .iter_mut()
                    .enumerate()
                {
                    *slot = ByteSourceKind::ObservedMemory {
                        source_va: overlap_start + i as u64,
                    };
                }
                observed += overlap_len as u64;
            }
            MemorySegment::Gap(gap) => {
                out_bytes[dst_off..dst_off + overlap_len].fill(0);
                for (i, slot) in provenance[dst_off..dst_off + overlap_len]
                    .iter_mut()
                    .enumerate()
                {
                    *slot = ByteSourceKind::SynthesizedGapFill {
                        source_va: overlap_start + i as u64,
                        reason: gap.reason.clone(),
                    };
                }
                missing += overlap_len as u64;
            }
        }
    }

    (observed, missing)
}

fn count_observed_in_range(capture: &SparseMemoryCapture, start: u64, len: u64) -> u64 {
    if len == 0 {
        return 0;
    }
    let end = start.saturating_add(len);
    let mut observed = 0u64;
    for run in capture.observed_runs() {
        let r_end = run.end();
        if r_end <= start || run.start >= end {
            continue;
        }
        observed += r_end.min(end) - run.start.max(start);
    }
    observed
}

fn compress_provenance(per_byte: &[ByteSourceKind]) -> (Vec<ByteProvenanceSpan>, u64, u64) {
    let mut spans: Vec<ByteProvenanceSpan> = Vec::new();
    let mut observed_bytes = 0u64;
    let mut synthesized_bytes = 0u64;

    for (idx, kind) in per_byte.iter().enumerate() {
        let off = idx as u64;
        match kind {
            ByteSourceKind::ObservedMemory { .. } => observed_bytes += 1,
            _ => synthesized_bytes += 1,
        }
        let merged = if let Some(last) = spans.last_mut() {
            match (&last.source, kind) {
                (
                    ByteSourceKind::ObservedMemory { source_va: prev_va },
                    ByteSourceKind::ObservedMemory { source_va: cur_va },
                ) if *cur_va == prev_va + last.len => {
                    last.len += 1;
                    true
                }
                (
                    ByteSourceKind::SynthesizedGapFill {
                        source_va: prev_va,
                        reason: prev_reason,
                    },
                    ByteSourceKind::SynthesizedGapFill {
                        source_va: cur_va,
                        reason: cur_reason,
                    },
                ) if *cur_va == prev_va + last.len && prev_reason == cur_reason => {
                    last.len += 1;
                    true
                }
                (
                    ByteSourceKind::SynthesizedAlignmentPadding { rule: r1 },
                    ByteSourceKind::SynthesizedAlignmentPadding { rule: r2 },
                )
                | (
                    ByteSourceKind::SynthesizedImportSection { rule: r1 },
                    ByteSourceKind::SynthesizedImportSection { rule: r2 },
                ) if r1 == r2 => {
                    last.len += 1;
                    true
                }
                (
                    ByteSourceKind::SynthesizedHeaderPatch {
                        rule: r1,
                        field: f1,
                    },
                    ByteSourceKind::SynthesizedHeaderPatch {
                        rule: r2,
                        field: f2,
                    },
                ) if r1 == r2 && f1 == f2 => {
                    last.len += 1;
                    true
                }
                _ => false,
            }
        } else {
            false
        };
        if !merged {
            spans.push(ByteProvenanceSpan {
                file_offset: off,
                len: 1,
                source: kind.clone(),
            });
        }
    }

    (spans, observed_bytes, synthesized_bytes)
}

fn expand_provenance(
    spans: &[ByteProvenanceSpan],
    file_len: usize,
) -> Result<Vec<ByteSourceKind>, String> {
    let mut out = Vec::with_capacity(file_len);
    for span in spans {
        for i in 0..span.len {
            let expanded = match &span.source {
                ByteSourceKind::ObservedMemory { source_va } => ByteSourceKind::ObservedMemory {
                    source_va: source_va + i,
                },
                ByteSourceKind::SynthesizedGapFill { source_va, reason } => {
                    ByteSourceKind::SynthesizedGapFill {
                        source_va: source_va + i,
                        reason: reason.clone(),
                    }
                }
                other => other.clone(),
            };
            out.push(expanded);
        }
    }
    if out.len() != file_len {
        return Err(format!(
            "expanded provenance length {} does not match image length {}",
            out.len(),
            file_len
        ));
    }
    Ok(out)
}

fn record_uncovered_primary_ranges(
    acquisition: &ModuleMemoryAcquisition,
    covered: &[(u64, u64)],
    omitted: &mut Vec<OmittedMemoryRange>,
) {
    let mut intervals = covered.to_vec();
    intervals.sort_unstable();
    let mut cursor = acquisition.primary_capture.start();
    let end = acquisition.primary_capture.end();
    for (start, iv_end) in intervals {
        let clamped_start = start.max(cursor).min(end);
        let clamped_end = iv_end.max(clamped_start).min(end);
        if clamped_start > cursor {
            let len = clamped_start - cursor;
            omitted.push(OmittedMemoryRange {
                va_start: cursor,
                len,
                observed_bytes: count_observed_in_range(&acquisition.primary_capture, cursor, len),
                reason: "out_of_file_layout_primary_region".to_string(),
            });
        }
        cursor = cursor.max(clamped_end);
    }
    if cursor < end {
        let len = end - cursor;
        omitted.push(OmittedMemoryRange {
            va_start: cursor,
            len,
            observed_bytes: count_observed_in_range(&acquisition.primary_capture, cursor, len),
            reason: "out_of_file_layout_primary_tail".to_string(),
        });
    }
}

/// Reconstruct a derived PE file explicitly from an immutable [`ModuleMemoryAcquisition`].
pub fn reconstruct_pe_from_acquisition(
    acquisition: &ModuleMemoryAcquisition,
    options: &PeRebuildOptions,
) -> Result<DerivedPeArtifact, String> {
    if !acquisition.verify_digest() {
        return Err("raw acquisition failed digest verification".to_string());
    }
    if acquisition.status == AcquisitionStatus::Failed {
        return Err(format!(
            "cannot reconstruct PE from failed acquisition: {}",
            acquisition.failure_reasons.join("; ")
        ));
    }

    let prefix = leading_observed_bytes(&acquisition.primary_capture);
    let headers = PeHeaders::parse(prefix)
        .map_err(|e| format!("unable to parse PE headers from acquired memory: {e}"))?;
    let header_slice = acquisition
        .primary_capture
        .read_exact_slice(acquisition.base_address, headers.size_of_headers as usize)
        .map_err(|e| format!("PE headers incomplete in acquisition: {e}"))?;
    let sections = headers.parse_sections(&header_slice)?;

    let mut next_raw_pointers: Vec<Option<u64>> = vec![None; sections.len()];
    let mut sorted_ptrs: Vec<(usize, u64)> = sections
        .iter()
        .enumerate()
        .map(|(i, s)| (i, u64::from(s.raw_pointer)))
        .collect();
    sorted_ptrs.sort_by_key(|&(_, ptr)| ptr);
    for w in sorted_ptrs.windows(2) {
        if let [(idx, cur_ptr), (_, next_ptr)] = *w
            && next_ptr > cur_ptr
        {
            next_raw_pointers[idx] = Some(next_ptr);
        }
    }

    let mut file_len = u64::from(headers.size_of_headers);
    let mut copy_plans = Vec::with_capacity(sections.len());
    for (idx, sec) in sections.iter().enumerate() {
        let raw_ptr = u64::from(sec.raw_pointer);
        let raw_size = u64::from(sec.raw_size);
        let vlen = if sec.virtual_size > 0 {
            u64::from(sec.virtual_size)
        } else {
            raw_size
        };
        let target_raw_len = match options.mode {
            PeReconstructionMode::FileLayout => raw_size,
            PeReconstructionMode::PreserveVirtualTails => {
                if raw_ptr < u64::from(headers.size_of_headers) {
                    raw_size
                } else {
                    let desired = raw_size.max(vlen);
                    match next_raw_pointers[idx] {
                        Some(next_ptr) if next_ptr > raw_ptr => desired.min(next_ptr - raw_ptr),
                        _ => desired,
                    }
                }
            }
        };
        let copy_len = vlen.min(target_raw_len);
        let sec_end = raw_ptr
            .checked_add(target_raw_len)
            .ok_or("section raw layout overflows u64")?;
        file_len = file_len.max(sec_end);
        copy_plans.push((idx, sec, raw_ptr, target_raw_len, copy_len, vlen));
    }

    let file_len_usize = usize::try_from(file_len)
        .map_err(|_| "reconstructed PE file exceeds host pointer width".to_string())?;
    if file_len_usize > options.max_output_bytes {
        return Err(format!(
            "reconstructed PE size {file_len_usize} exceeds limit {}",
            options.max_output_bytes
        ));
    }

    let mut out_bytes = vec![0u8; file_len_usize];
    let mut provenance = vec![
        ByteSourceKind::SynthesizedAlignmentPadding {
            rule: "pe_file_layout_zero_padding".to_string(),
        };
        file_len_usize
    ];

    let header_len = u64::from(headers.size_of_headers);
    let (hdr_obs, hdr_miss) = copy_capture_into_file(
        &acquisition.primary_capture,
        acquisition.base_address,
        header_len,
        0,
        &mut out_bytes,
        &mut provenance,
    );
    let mut va_to_file_mappings = vec![VaFileMapping {
        region_name: "PE_HEADERS".to_string(),
        va_start: acquisition.base_address,
        va_len: header_len,
        file_offset: 0,
        file_len: header_len,
        observed_bytes: hdr_obs,
        missing_bytes: hdr_miss,
        status: if hdr_obs == 0 {
            AcquisitionStatus::Failed
        } else if hdr_miss == 0 {
            AcquisitionStatus::Complete
        } else {
            AcquisitionStatus::Partial
        },
    }];

    let mut covered_primary_intervals = vec![(
        acquisition.base_address,
        acquisition.base_address + header_len,
    )];
    let mut omitted_ranges = Vec::new();

    for (_, sec, raw_ptr, target_raw_len, copy_len, vlen) in copy_plans {
        let va = acquisition
            .base_address
            .checked_add(u64::from(sec.virtual_address))
            .ok_or("section VA overflows u64")?;
        let raw_ptr_usize = usize::try_from(raw_ptr).map_err(|_| "section offset overflow")?;
        let (sec_obs, sec_miss) = copy_capture_into_file(
            &acquisition.primary_capture,
            va,
            copy_len,
            raw_ptr_usize,
            &mut out_bytes,
            &mut provenance,
        );
        covered_primary_intervals.push((va, va.saturating_add(copy_len)));

        if target_raw_len > copy_len {
            let pad_start = raw_ptr_usize + copy_len as usize;
            let pad_end = raw_ptr_usize + target_raw_len as usize;
            for slot in &mut provenance[pad_start..pad_end] {
                *slot = ByteSourceKind::SynthesizedAlignmentPadding {
                    rule: "pe_section_raw_alignment_padding".to_string(),
                };
            }
        }

        let sec_status = if sec_obs == 0 {
            AcquisitionStatus::Failed
        } else if sec_miss == 0 && vlen <= copy_len {
            AcquisitionStatus::Complete
        } else {
            AcquisitionStatus::Partial
        };
        va_to_file_mappings.push(VaFileMapping {
            region_name: if sec.name.is_empty() {
                format!("section_{:#x}", sec.virtual_address)
            } else {
                sec.name.clone()
            },
            va_start: va,
            va_len: vlen,
            file_offset: raw_ptr,
            file_len: target_raw_len,
            observed_bytes: sec_obs,
            missing_bytes: sec_miss,
            status: sec_status,
        });

        if vlen > copy_len {
            let omitted_start = va.saturating_add(copy_len);
            let omitted_len = vlen - copy_len;
            omitted_ranges.push(OmittedMemoryRange {
                va_start: omitted_start,
                len: omitted_len,
                observed_bytes: count_observed_in_range(
                    &acquisition.primary_capture,
                    omitted_start,
                    omitted_len,
                ),
                reason: "virtual_tail_exceeds_raw_section_size".to_string(),
            });
        }
    }

    record_uncovered_primary_ranges(acquisition, &covered_primary_intervals, &mut omitted_ranges);
    for extra in &acquisition.extra_captures {
        omitted_ranges.push(OmittedMemoryRange {
            va_start: extra.start(),
            len: extra.requested_len(),
            observed_bytes: extra.coverage().observed_bytes,
            reason: "extra_va_region_excluded_from_pe_file_layout".to_string(),
        });
    }

    let (byte_provenance, derived_observed_bytes, derived_synthesized_bytes) =
        compress_provenance(&provenance);
    let derived_bytes_digest = ContentDigest::of(&out_bytes);
    let manifest = DerivedPeManifest {
        schema_version: DERIVED_PE_MANIFEST_SCHEMA_VERSION.to_string(),
        root_acquisition_digest: acquisition.artifact_digest.clone(),
        parent_artifact_digest: acquisition.artifact_digest.clone(),
        derived_bytes_digest: derived_bytes_digest.clone(),
        parent_capture_status: acquisition.status,
        parent_observed_bytes: acquisition.total_observed_bytes,
        parent_missing_bytes: acquisition.total_missing_bytes,
        derived_observed_bytes,
        derived_synthesized_bytes,
        va_to_file_mappings,
        byte_provenance,
        omitted_ranges,
        unresolved_imports: Vec::new(),
        transformations: vec![TransformationRecord {
            transform_version: PE_RECONSTRUCT_TRANSFORM_VERSION.to_string(),
            parent_digest: acquisition.artifact_digest.clone(),
            output_digest: derived_bytes_digest,
            parameters: serde_json::to_value(options).map_err(|e| e.to_string())?,
        }],
    };
    manifest.validate_provenance(out_bytes.len() as u64)?;
    Ok(DerivedPeArtifact {
        bytes: out_bytes,
        manifest,
    })
}

/// Derive a new PE artifact with a rebuilt import section from `parent` without
/// mutating `parent` in place, recording all header patches, appended section
/// provenance, and unresolved import candidates.
pub fn rebuild_imports_derived(
    parent: &DerivedPeArtifact,
    candidates: &[ImportCandidate],
) -> Result<DerivedPeArtifact, String> {
    if ContentDigest::of(&parent.bytes) != parent.manifest.derived_bytes_digest {
        return Err(
            "parent derived PE failed digest verification before import rebuild".to_string(),
        );
    }
    parent
        .manifest
        .validate_provenance(parent.bytes.len() as u64)?;

    let mut specs = Vec::new();
    let mut unresolved = parent.manifest.unresolved_imports.clone();
    let mut new_unresolved_count = 0usize;

    for candidate in candidates {
        if let Some(reason) = &candidate.unresolved_reason {
            unresolved.push(UnresolvedImportRecord {
                rva: candidate.rva,
                target_address: candidate.target_address,
                module_name: candidate.module_name.clone(),
                reason: reason.clone(),
            });
            new_unresolved_count += 1;
            continue;
        }
        let Some(module) = candidate.module_name.as_ref().filter(|m| !m.is_empty()) else {
            unresolved.push(UnresolvedImportRecord {
                rva: candidate.rva,
                target_address: candidate.target_address,
                module_name: candidate.module_name.clone(),
                reason: "missing module name".to_string(),
            });
            new_unresolved_count += 1;
            continue;
        };
        let ordinal_u32 = candidate.ordinal.unwrap_or(0);
        let Ok(ordinal) = u16::try_from(ordinal_u32) else {
            unresolved.push(UnresolvedImportRecord {
                rva: candidate.rva,
                target_address: candidate.target_address,
                module_name: Some(module.clone()),
                reason: format!("ordinal {ordinal_u32} exceeds u16"),
            });
            new_unresolved_count += 1;
            continue;
        };
        if candidate.function_name.is_none() && ordinal == 0 {
            unresolved.push(UnresolvedImportRecord {
                rva: candidate.rva,
                target_address: candidate.target_address,
                module_name: Some(module.clone()),
                reason: "missing both function name and non-zero ordinal".to_string(),
            });
            new_unresolved_count += 1;
            continue;
        }
        specs.push(ImportSpec {
            module: module.clone(),
            function: candidate.function_name.clone(),
            ordinal,
        });
    }

    let mut new_bytes = parent.bytes.clone();
    let layout = append_import_section_with_layout(&mut new_bytes, &specs)?;

    let mut provenance = expand_provenance(&parent.manifest.byte_provenance, parent.bytes.len())?;
    let mut patch = |offset: usize, len: usize, field: &str| {
        for slot in &mut provenance[offset..offset + len] {
            *slot = ByteSourceKind::SynthesizedHeaderPatch {
                rule: IAT_REBUILD_TRANSFORM_VERSION.to_string(),
                field: field.to_string(),
            };
        }
    };
    patch(layout.new_section_header_offset, 40, "new_section_header");
    patch(layout.number_of_sections_offset, 2, "number_of_sections");
    patch(layout.size_of_image_offset, 4, "size_of_image");
    patch(layout.import_dir_offset, 8, "import_directory");
    if let Some(iat_off) = layout.iat_dir_offset {
        patch(iat_off, 8, "iat_directory");
    }

    provenance.extend(std::iter::repeat_n(
        ByteSourceKind::SynthesizedAlignmentPadding {
            rule: "iat_section_file_alignment".to_string(),
        },
        layout.alignment_padding_len,
    ));
    provenance.extend(std::iter::repeat_n(
        ByteSourceKind::SynthesizedImportSection {
            rule: IAT_REBUILD_TRANSFORM_VERSION.to_string(),
        },
        layout.section_raw_size,
    ));

    let (byte_provenance, derived_observed_bytes, derived_synthesized_bytes) =
        compress_provenance(&provenance);
    let derived_bytes_digest = ContentDigest::of(&new_bytes);
    let mut va_to_file_mappings = parent.manifest.va_to_file_mappings.clone();
    let base_va = parent
        .manifest
        .va_to_file_mappings
        .first()
        .map_or(0, |m| m.va_start);
    va_to_file_mappings.push(VaFileMapping {
        region_name: ".fission (synthesized_iat)".to_string(),
        va_start: base_va.saturating_add(u64::from(layout.section_rva)),
        va_len: u64::from(layout.section_virtual_size),
        file_offset: layout.section_file_offset as u64,
        file_len: layout.section_raw_size as u64,
        observed_bytes: 0,
        missing_bytes: 0,
        status: AcquisitionStatus::Complete,
    });

    let mut transformations = parent.manifest.transformations.clone();
    transformations.push(TransformationRecord {
        transform_version: IAT_REBUILD_TRANSFORM_VERSION.to_string(),
        parent_digest: parent.manifest.derived_bytes_digest.clone(),
        output_digest: derived_bytes_digest.clone(),
        parameters: serde_json::json!({
            "resolved_import_count": specs.len(),
            "unresolved_import_count": new_unresolved_count,
        }),
    });

    let manifest = DerivedPeManifest {
        schema_version: DERIVED_PE_MANIFEST_SCHEMA_VERSION.to_string(),
        root_acquisition_digest: parent.manifest.root_acquisition_digest.clone(),
        parent_artifact_digest: parent.manifest.derived_bytes_digest.clone(),
        derived_bytes_digest,
        parent_capture_status: parent.manifest.parent_capture_status,
        parent_observed_bytes: parent.manifest.parent_observed_bytes,
        parent_missing_bytes: parent.manifest.parent_missing_bytes,
        derived_observed_bytes,
        derived_synthesized_bytes,
        va_to_file_mappings,
        byte_provenance,
        omitted_ranges: parent.manifest.omitted_ranges.clone(),
        unresolved_imports: unresolved,
        transformations,
    };
    manifest.validate_provenance(new_bytes.len() as u64)?;
    Ok(DerivedPeArtifact {
        bytes: new_bytes,
        manifest,
    })
}

/// Build an explicit standalone [`DerivedPeArtifact`] from raw on-disk PE bytes when no prior
/// manifest is present, keeping all bytes tagged cleanly as synthesized external input rather
/// than claiming direct live-memory observation.
pub fn derived_artifact_from_unmanifested_pe(
    bytes: Vec<u8>,
    origin_label: &str,
) -> DerivedPeArtifact {
    let digest = ContentDigest::of(&bytes);
    let len = bytes.len() as u64;
    let byte_provenance = if len == 0 {
        Vec::new()
    } else {
        vec![ByteProvenanceSpan {
            file_offset: 0,
            len,
            source: ByteSourceKind::SynthesizedAlignmentPadding {
                rule: format!("external_unmanifested_input:{origin_label}"),
            },
        }]
    };
    DerivedPeArtifact {
        bytes,
        manifest: DerivedPeManifest {
            schema_version: DERIVED_PE_MANIFEST_SCHEMA_VERSION.to_string(),
            root_acquisition_digest: digest.clone(),
            parent_artifact_digest: digest.clone(),
            derived_bytes_digest: digest,
            parent_capture_status: AcquisitionStatus::Partial,
            parent_observed_bytes: 0,
            parent_missing_bytes: 0,
            derived_observed_bytes: 0,
            derived_synthesized_bytes: len,
            va_to_file_mappings: Vec::new(),
            byte_provenance,
            omitted_ranges: Vec::new(),
            unresolved_imports: Vec::new(),
            transformations: Vec::new(),
        },
    }
}
