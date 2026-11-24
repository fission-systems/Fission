//! Memory - Process memory operations
//!
//! Provides unified memory read/write/mapping operations across platforms.
//! Uses platform-specific implementations via the [`PlatformMemory`] trait.
//!
//! # Exact vs. Partial vs. Sparse Contracts
//!
//! - **Exact reads ([`MemoryManager::read`], [`MemoryManager::read_exact`], scalar
//!   readers `read_u8`/`read_u16`/`read_u32`/`read_u64`)**: Intended for headers,
//!   pointer tables, inline operands, breakpoints, and register-sized scalars where
//!   every requested byte must be observed. Any short transfer fails with
//!   [`MemoryError::ShortRead`] rather than returning trailing zero padding.
//! - **Explicit partial reads ([`MemoryManager::read_partial`], [`MemoryManager::read_into`])**:
//!   Intended for probing near page/segment boundaries where a prefix transfer may
//!   stop early. Returns only the bytes actually observed (`0..=requested`) paired
//!   with an explicit [`PartialRead::trailing_gap`] describing the unread tail.
//! - **Sparse acquisitions ([`MemoryManager::read_sparse`])**: Intended for dumps,
//!   scans, and forensic snapshots spanning multiple pages/regions. Preserves
//!   genuinely observed `0x00` bytes inside [`ObservedMemoryRun`] while recording
//!   every unread interval as an explicit [`MemoryGap`] tagged with a
//!   [`MemoryGapReason`] (`unmapped`, `denied`, `partial`, `process_exited`,
//!   `budget`, or `read_error`) and summarized in [`SparseCoverageSummary`]
//!   (`observed_bytes + missing_bytes == requested_bytes`).

use super::platform::{PlatformMemory, PlatformMemoryImpl};
use serde::Serialize;
use thiserror::Error;

/// Default hard upper bound for a single exact or partial read request (64 MiB).
pub const DEFAULT_MAX_READ_BYTES: usize = 64 * 1024 * 1024;

/// Default page step used by sparse memory acquisitions.
pub const DEFAULT_SPARSE_PAGE_SIZE: usize = 4096;

/// Memory operation errors.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum MemoryError {
    #[error("Failed to read memory at {address:#x}: {reason}")]
    ReadFailed { address: u64, reason: String },

    #[error("Short memory read at {address:#x}: expected {expected} bytes, got {actual}")]
    ShortRead {
        address: u64,
        expected: usize,
        actual: usize,
    },

    #[error(
        "Backend reported oversized transfer at {address:#x}: requested {requested} bytes, backend claimed {reported}"
    )]
    OversizedTransfer {
        address: u64,
        requested: usize,
        reported: usize,
    },

    #[error("Failed to write memory at {address:#x}: {reason}")]
    WriteFailed { address: u64, reason: String },

    #[error("Invalid memory region: {address:#x} ({size:#x} bytes)")]
    InvalidRegion { address: u64, size: usize },

    #[error("Unmapped memory at {address:#x} ({size:#x} bytes)")]
    Unmapped { address: u64, size: usize },

    #[error("Access denied at {address:#x}")]
    AccessDenied { address: u64 },

    #[error("No process attached")]
    NoProcess,

    #[error("Target process exited before reading {address:#x}")]
    ProcessExited { address: u64 },

    #[error("Memory read of {requested} bytes exceeds budget limit of {limit} bytes")]
    BudgetExceeded { requested: usize, limit: usize },
}

/// Memory protection flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryProtection {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}

impl MemoryProtection {
    pub const RX: Self = Self {
        read: true,
        write: false,
        execute: true,
    };
    pub const RW: Self = Self {
        read: true,
        write: true,
        execute: false,
    };
    pub const RWX: Self = Self {
        read: true,
        write: true,
        execute: true,
    };
    pub const NONE: Self = Self {
        read: false,
        write: false,
        execute: false,
    };
}

/// Represents a memory region in the target process.
#[derive(Debug, Clone)]
pub struct MemoryRegion {
    /// Start address of the region.
    pub base_address: u64,

    /// Size of the region in bytes.
    pub size: usize,

    /// Memory protection flags.
    pub protection: MemoryProtection,

    /// Optional name (e.g., module name, "[stack]", "[heap]").
    pub name: Option<String>,
}

/// Reason why an address range inside a partial or sparse acquisition could not
/// be observed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MemoryGapReason {
    /// No memory mapping covers the address range.
    Unmapped,
    /// Mapping exists, or target policy denied read access.
    Denied,
    /// Backend completed only a shorter prefix or returned 0 bytes cleanly.
    Partial,
    /// Target process exited or is no longer attached.
    ProcessExited,
    /// Acquisition stopped due to configured byte/span budget.
    Budget,
    /// Other explicit read failure reported by the backend.
    ReadError { detail: String },
}

impl MemoryGapReason {
    /// Classify a [`MemoryError`] into an explicit sparse gap reason.
    pub fn from_memory_error(error: &MemoryError) -> Self {
        match error {
            MemoryError::Unmapped { .. } => Self::Unmapped,
            MemoryError::AccessDenied { .. } => Self::Denied,
            MemoryError::ShortRead { .. } => Self::Partial,
            MemoryError::ProcessExited { .. } | MemoryError::NoProcess => Self::ProcessExited,
            MemoryError::BudgetExceeded { .. } => Self::Budget,
            other => Self::ReadError {
                detail: other.to_string(),
            },
        }
    }

    /// Convert a gap reason back into a typed [`MemoryError`] for exact slice queries.
    pub fn into_memory_error(&self, address: u64, size: usize) -> MemoryError {
        match self {
            Self::Unmapped => MemoryError::Unmapped { address, size },
            Self::Denied => MemoryError::AccessDenied { address },
            Self::Partial => MemoryError::ShortRead {
                address,
                expected: size,
                actual: 0,
            },
            Self::ProcessExited => MemoryError::ProcessExited { address },
            Self::Budget => MemoryError::BudgetExceeded {
                requested: size,
                limit: 0,
            },
            Self::ReadError { detail } => MemoryError::ReadFailed {
                address,
                reason: detail.clone(),
            },
        }
    }
}

/// Contiguous unread interval `[start, start + len)` with its failure reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MemoryGap {
    pub start: u64,
    pub len: u64,
    pub reason: MemoryGapReason,
}

impl MemoryGap {
    pub fn end(&self) -> u64 {
        self.start.saturating_add(self.len)
    }
}

/// Contiguous observed bytes read from `[start, start + bytes.len())`.
///
/// Any `0x00` byte present inside `bytes` was genuinely observed from target memory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ObservedMemoryRun {
    pub start: u64,
    pub bytes: Vec<u8>,
}

impl ObservedMemoryRun {
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub fn end(&self) -> u64 {
        self.start.saturating_add(self.bytes.len() as u64)
    }
}

/// Ordered element inside a [`SparseMemoryCapture`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum MemorySegment {
    Observed(ObservedMemoryRun),
    Gap(MemoryGap),
}

/// Explicit partial read result containing only genuinely observed prefix bytes
/// and an optional [`MemoryGap`] for any unread tail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PartialRead {
    pub address: u64,
    pub requested: usize,
    pub bytes: Vec<u8>,
    pub trailing_gap: Option<MemoryGap>,
}

impl PartialRead {
    pub fn observed_len(&self) -> usize {
        self.bytes.len()
    }

    pub fn missing_len(&self) -> usize {
        self.requested.saturating_sub(self.bytes.len())
    }

    pub fn is_complete(&self) -> bool {
        self.bytes.len() == self.requested && self.trailing_gap.is_none()
    }
}

/// Result of querying a single byte inside a [`SparseMemoryCapture`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SparseByte<'a> {
    Observed(u8),
    Missing(&'a MemoryGapReason),
}

/// Configuration and resource bounds for [`MemoryManager::read_sparse`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SparseReadOptions {
    /// Maximum number of bytes allowed to be observed before stopping with [`MemoryGapReason::Budget`].
    pub max_bytes: usize,
    /// Chunk / page size used when advancing past boundaries and holes.
    pub page_size: usize,
    /// Maximum virtual address span to probe across holes before recording a trailing [`MemoryGapReason::Budget`].
    pub max_span_bytes: usize,
}

impl Default for SparseReadOptions {
    fn default() -> Self {
        Self {
            max_bytes: DEFAULT_MAX_READ_BYTES,
            page_size: DEFAULT_SPARSE_PAGE_SIZE,
            max_span_bytes: DEFAULT_MAX_READ_BYTES,
        }
    }
}

impl SparseReadOptions {
    pub fn with_max_bytes(max_bytes: usize) -> Self {
        Self {
            max_bytes,
            max_span_bytes: DEFAULT_MAX_READ_BYTES.max(max_bytes),
            ..Self::default()
        }
    }
}

/// Accounting summary for a sparse memory acquisition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SparseCoverageSummary {
    pub requested_bytes: u64,
    pub observed_bytes: u64,
    pub missing_bytes: u64,
    pub observed_runs: usize,
    pub gap_count: usize,
    pub complete: bool,
}

/// Ordered sparse memory acquisition preserving observed runs and unread gaps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SparseMemoryCapture {
    pub start: u64,
    pub requested_len: u64,
    pub segments: Vec<MemorySegment>,
    pub coverage: SparseCoverageSummary,
}

impl SparseMemoryCapture {
    /// Build a canonical [`SparseMemoryCapture`], coalescing contiguous observed runs
    /// and adjacent gaps with matching reasons.
    pub fn from_segments(start: u64, requested_len: u64, raw_segments: Vec<MemorySegment>) -> Self {
        let mut segments: Vec<MemorySegment> = Vec::with_capacity(raw_segments.len());
        for segment in raw_segments {
            match segment {
                MemorySegment::Observed(run) if run.bytes.is_empty() => {}
                MemorySegment::Gap(gap) if gap.len == 0 => {}
                MemorySegment::Observed(run) => {
                    if let Some(MemorySegment::Observed(prev)) = segments.last_mut() {
                        if prev.end() == run.start {
                            prev.bytes.extend_from_slice(&run.bytes);
                            continue;
                        }
                    }
                    segments.push(MemorySegment::Observed(run));
                }
                MemorySegment::Gap(gap) => {
                    if let Some(MemorySegment::Gap(prev)) = segments.last_mut() {
                        if prev.end() == gap.start && prev.reason == gap.reason {
                            prev.len = prev.len.saturating_add(gap.len);
                            continue;
                        }
                    }
                    segments.push(MemorySegment::Gap(gap));
                }
            }
        }

        let mut observed_bytes = 0u64;
        let mut missing_bytes = 0u64;
        let mut observed_runs = 0usize;
        let mut gap_count = 0usize;

        for segment in &segments {
            match segment {
                MemorySegment::Observed(run) => {
                    observed_bytes = observed_bytes.saturating_add(run.len() as u64);
                    observed_runs += 1;
                }
                MemorySegment::Gap(gap) => {
                    missing_bytes = missing_bytes.saturating_add(gap.len);
                    gap_count += 1;
                }
            }
        }

        debug_assert_eq!(
            observed_bytes.saturating_add(missing_bytes),
            requested_len,
            "sparse capture coverage mismatch: observed={observed_bytes} missing={missing_bytes} requested={requested_len}"
        );

        let coverage = SparseCoverageSummary {
            requested_bytes: requested_len,
            observed_bytes,
            missing_bytes,
            observed_runs,
            gap_count,
            complete: missing_bytes == 0 && observed_bytes == requested_len,
        };

        Self {
            start,
            requested_len,
            segments,
            coverage,
        }
    }

    pub fn is_complete(&self) -> bool {
        self.coverage.complete
    }

    pub fn observed_runs(&self) -> impl Iterator<Item = &ObservedMemoryRun> {
        self.segments.iter().filter_map(|seg| match seg {
            MemorySegment::Observed(run) => Some(run),
            MemorySegment::Gap(_) => None,
        })
    }

    pub fn gaps(&self) -> impl Iterator<Item = &MemoryGap> {
        self.segments.iter().filter_map(|seg| match seg {
            MemorySegment::Gap(gap) => Some(gap),
            MemorySegment::Observed(_) => None,
        })
    }

    /// Query one byte at `address` inside this capture, distinguishing a genuinely
    /// observed `0x00` (`Some(SparseByte::Observed(0))`) from an unread gap
    /// (`Some(SparseByte::Missing(reason))`), or `None` if `address` is outside `[start, start + requested_len)`.
    pub fn byte_at(&self, address: u64) -> Option<SparseByte<'_>> {
        let end = self.start.checked_add(self.requested_len)?;
        if address < self.start || address >= end {
            return None;
        }
        for segment in &self.segments {
            match segment {
                MemorySegment::Observed(run) => {
                    if address >= run.start && address < run.end() {
                        let offset = (address - run.start) as usize;
                        return Some(SparseByte::Observed(run.bytes[offset]));
                    }
                }
                MemorySegment::Gap(gap) => {
                    if address >= gap.start && address < gap.end() {
                        return Some(SparseByte::Missing(&gap.reason));
                    }
                }
            }
        }
        None
    }

    /// Extract an exact byte slice `[address, address + len)` only if every byte
    /// in that range was genuinely observed. Fails on any intersecting gap instead
    /// of padding zeros.
    pub fn read_exact_slice(&self, address: u64, len: usize) -> Result<Vec<u8>, MemoryError> {
        let Some(slice_end) = address.checked_add(len as u64) else {
            return Err(MemoryError::InvalidRegion { address, size: len });
        };
        let Some(cap_end) = self.start.checked_add(self.requested_len) else {
            return Err(MemoryError::InvalidRegion { address, size: len });
        };
        if address < self.start || slice_end > cap_end {
            return Err(MemoryError::Unmapped { address, size: len });
        }
        if len == 0 {
            return Ok(Vec::new());
        }

        let mut out = Vec::with_capacity(len);
        let mut cursor = address;
        for segment in &self.segments {
            if cursor >= slice_end {
                break;
            }
            match segment {
                MemorySegment::Observed(run) => {
                    let seg_start = run.start;
                    let seg_end = run.end();
                    if seg_end <= cursor || seg_start >= slice_end {
                        continue;
                    }
                    if seg_start > cursor {
                        return Err(MemoryError::Unmapped {
                            address: cursor,
                            size: (seg_start - cursor) as usize,
                        });
                    }
                    let take_end = seg_end.min(slice_end);
                    let start_off = (cursor - seg_start) as usize;
                    let end_off = (take_end - seg_start) as usize;
                    out.extend_from_slice(&run.bytes[start_off..end_off]);
                    cursor = take_end;
                }
                MemorySegment::Gap(gap) => {
                    let seg_start = gap.start;
                    let seg_end = gap.end();
                    if seg_end <= cursor || seg_start >= slice_end {
                        continue;
                    }
                    let err_addr = cursor.max(seg_start);
                    let err_len = (slice_end.min(seg_end) - err_addr) as usize;
                    return Err(gap.reason.into_memory_error(err_addr, err_len.max(1)));
                }
            }
        }
        if out.len() != len {
            return Err(MemoryError::ShortRead {
                address,
                expected: len,
                actual: out.len(),
            });
        }
        Ok(out)
    }

    /// Export this sparse capture with hex-formatted addresses and per-segment metadata.
    pub fn to_json_value(&self) -> serde_json::Value {
        let segments: Vec<serde_json::Value> = self
            .segments
            .iter()
            .map(|seg| match seg {
                MemorySegment::Observed(run) => serde_json::json!({
                    "state": "observed",
                    "start": format!("0x{:x}", run.start),
                    "end": format!("0x{:x}", run.end()),
                    "len": run.len(),
                    "bytes_hex": run.bytes.iter().map(|b| format!("{b:02x}")).collect::<String>(),
                }),
                MemorySegment::Gap(gap) => serde_json::json!({
                    "state": "gap",
                    "start": format!("0x{:x}", gap.start),
                    "end": format!("0x{:x}", gap.end()),
                    "len": gap.len,
                    "reason": gap.reason,
                }),
            })
            .collect();

        serde_json::json!({
            "start": format!("0x{:x}", self.start),
            "requested_len": self.requested_len,
            "coverage": self.coverage,
            "segments": segments,
        })
    }
}

pub(crate) fn validate_read_range(
    address: u64,
    size: usize,
    max_bytes: usize,
) -> Result<(), MemoryError> {
    if size > max_bytes {
        return Err(MemoryError::BudgetExceeded {
            requested: size,
            limit: max_bytes,
        });
    }
    let size_u64 = u64::try_from(size).map_err(|_| MemoryError::InvalidRegion { address, size })?;
    if address.checked_add(size_u64).is_none() {
        return Err(MemoryError::InvalidRegion { address, size });
    }
    Ok(())
}

pub(crate) fn checked_transfer_len(
    address: u64,
    requested: usize,
    reported: usize,
) -> Result<usize, MemoryError> {
    if reported > requested {
        return Err(MemoryError::OversizedTransfer {
            address,
            requested,
            reported,
        });
    }
    Ok(reported)
}

/// Memory manager for reading/writing process memory.
///
/// Wraps a [`PlatformMemory`] backend (defaulting to [`PlatformMemoryImpl`]) and
/// enforces exact-read, explicit-partial, and bounded sparse acquisition contracts.
pub struct MemoryManager<M: PlatformMemory = PlatformMemoryImpl> {
    /// Platform-specific (or mock) memory implementation.
    platform: M,
    /// Cached memory regions.
    regions: Vec<MemoryRegion>,
    /// Hard limit on single exact or partial read size.
    max_read_bytes: usize,
}

impl MemoryManager<PlatformMemoryImpl> {
    /// Create a new memory manager using the host OS backend.
    pub fn new() -> Self {
        Self::with_backend(PlatformMemoryImpl::new())
    }
}

impl<M: PlatformMemory> MemoryManager<M> {
    /// Create a memory manager over an arbitrary [`PlatformMemory`] backend.
    pub fn with_backend(platform: M) -> Self {
        Self {
            platform,
            regions: Vec::new(),
            max_read_bytes: DEFAULT_MAX_READ_BYTES,
        }
    }

    /// Override the maximum single-read byte budget.
    pub fn with_max_read_bytes(mut self, max_read_bytes: usize) -> Self {
        self.max_read_bytes = max_read_bytes;
        self
    }

    /// Return the configured maximum single-read byte budget.
    pub fn max_read_bytes(&self) -> usize {
        self.max_read_bytes
    }

    /// Access the underlying backend.
    pub fn backend(&self) -> &M {
        &self.platform
    }

    /// Mutably access the underlying backend.
    pub fn backend_mut(&mut self) -> &mut M {
        &mut self.platform
    }

    /// Open a process for memory operations.
    pub fn open_process(&mut self, pid: u32) -> Result<(), MemoryError> {
        self.platform.open_process(pid)
    }

    /// Check if a process is currently open.
    pub fn is_open(&self) -> bool {
        self.platform.is_open()
    }

    /// Exact memory read: returns `Ok(bytes)` of length `size` only if every
    /// requested byte was transferred. Fails with [`MemoryError::ShortRead`] on
    /// short or zero-byte transfers instead of returning trailing zeros.
    pub fn read(&self, address: u64, size: usize) -> Result<Vec<u8>, MemoryError> {
        self.read_exact(address, size)
    }

    /// Exact memory read requiring all `size` bytes to be observed.
    pub fn read_exact(&self, address: u64, size: usize) -> Result<Vec<u8>, MemoryError> {
        validate_read_range(address, size, self.max_read_bytes)?;
        if !self.platform.is_open() {
            return Err(MemoryError::NoProcess);
        }
        if size == 0 {
            return Ok(Vec::new());
        }
        let mut buffer = vec![0u8; size];
        let actual = self.read_into(address, &mut buffer)?;
        if actual < size {
            return Err(MemoryError::ShortRead {
                address,
                expected: size,
                actual,
            });
        }
        Ok(buffer)
    }

    /// Explicit partial memory read: returns only the bytes actually transferred
    /// without padding missing tail bytes with zeros, alongside an optional
    /// [`MemoryGap`] explaining any unread tail.
    pub fn read_partial(&self, address: u64, size: usize) -> Result<PartialRead, MemoryError> {
        validate_read_range(address, size, self.max_read_bytes)?;
        if !self.platform.is_open() {
            return Err(MemoryError::NoProcess);
        }
        if size == 0 {
            return Ok(PartialRead {
                address,
                requested: 0,
                bytes: Vec::new(),
                trailing_gap: None,
            });
        }

        let mut buffer = vec![0u8; size];
        let actual = self.read_into(address, &mut buffer)?;
        buffer.truncate(actual);
        let trailing_gap = (actual < size).then(|| MemoryGap {
            start: address.saturating_add(actual as u64),
            len: (size - actual) as u64,
            reason: MemoryGapReason::Partial,
        });
        Ok(PartialRead {
            address,
            requested: size,
            bytes: buffer,
            trailing_gap,
        })
    }

    /// Bounded sparse acquisition over `[address, address + size)`.
    ///
    /// Walks page-aligned chunks across the requested range, preserving genuinely
    /// observed `0x00` bytes inside [`ObservedMemoryRun`] while recording every
    /// unmapped/denied/partial/exited/budget gap as an explicit [`MemoryGap`].
    pub fn read_sparse(
        &self,
        address: u64,
        size: usize,
        options: SparseReadOptions,
    ) -> Result<SparseMemoryCapture, MemoryError> {
        let size_u64 =
            u64::try_from(size).map_err(|_| MemoryError::InvalidRegion { address, size })?;
        let Some(end) = address.checked_add(size_u64) else {
            return Err(MemoryError::InvalidRegion { address, size });
        };
        if options.page_size == 0 {
            return Err(MemoryError::InvalidRegion { address, size: 0 });
        }
        if !self.platform.is_open() {
            return Err(MemoryError::NoProcess);
        }
        if size == 0 {
            return Ok(SparseMemoryCapture::from_segments(address, 0, Vec::new()));
        }

        let page_size_u64 = options.page_size as u64;
        let mut cursor = address;
        let mut observed_total = 0usize;
        let mut probed_span = 0usize;
        let mut segments = Vec::new();

        while cursor < end {
            if observed_total >= options.max_bytes || probed_span >= options.max_span_bytes {
                segments.push(MemorySegment::Gap(MemoryGap {
                    start: cursor,
                    len: end - cursor,
                    reason: MemoryGapReason::Budget,
                }));
                break;
            }

            let offset_in_page = (cursor % page_size_u64) as usize;
            let to_page_end = options.page_size - offset_in_page;
            let remaining_req = usize::try_from(end - cursor).unwrap_or(usize::MAX);
            let remaining_obs_budget = options.max_bytes.saturating_sub(observed_total);
            let remaining_span_budget = options.max_span_bytes.saturating_sub(probed_span);
            let chunk_len = to_page_end
                .min(remaining_req)
                .min(remaining_obs_budget)
                .min(remaining_span_budget)
                .min(self.max_read_bytes);

            if chunk_len == 0 {
                segments.push(MemorySegment::Gap(MemoryGap {
                    start: cursor,
                    len: end - cursor,
                    reason: MemoryGapReason::Budget,
                }));
                break;
            }

            let mut buf = vec![0u8; chunk_len];
            match self.read_into(cursor, &mut buf) {
                Ok(0) => {
                    segments.push(MemorySegment::Gap(MemoryGap {
                        start: cursor,
                        len: chunk_len as u64,
                        reason: MemoryGapReason::Partial,
                    }));
                    cursor = cursor.saturating_add(chunk_len as u64);
                    probed_span = probed_span.saturating_add(chunk_len);
                }
                Ok(n) => {
                    buf.truncate(n);
                    observed_total = observed_total.saturating_add(n);
                    segments.push(MemorySegment::Observed(ObservedMemoryRun {
                        start: cursor,
                        bytes: buf,
                    }));
                    if n < chunk_len {
                        segments.push(MemorySegment::Gap(MemoryGap {
                            start: cursor.saturating_add(n as u64),
                            len: (chunk_len - n) as u64,
                            reason: MemoryGapReason::Partial,
                        }));
                    }
                    cursor = cursor.saturating_add(chunk_len as u64);
                    probed_span = probed_span.saturating_add(chunk_len);
                }
                Err(err @ MemoryError::OversizedTransfer { .. }) => return Err(err),
                Err(MemoryError::ProcessExited { .. } | MemoryError::NoProcess) => {
                    segments.push(MemorySegment::Gap(MemoryGap {
                        start: cursor,
                        len: end - cursor,
                        reason: MemoryGapReason::ProcessExited,
                    }));
                    break;
                }
                Err(err) => {
                    segments.push(MemorySegment::Gap(MemoryGap {
                        start: cursor,
                        len: chunk_len as u64,
                        reason: MemoryGapReason::from_memory_error(&err),
                    }));
                    cursor = cursor.saturating_add(chunk_len as u64);
                    probed_span = probed_span.saturating_add(chunk_len);
                }
            }
        }

        Ok(SparseMemoryCapture::from_segments(
            address, size_u64, segments,
        ))
    }

    /// Read memory into an existing buffer, validating address bounds, budget,
    /// open state, and ensuring the backend never reports more bytes than `buffer.len()`.
    pub fn read_into(&self, address: u64, buffer: &mut [u8]) -> Result<usize, MemoryError> {
        validate_read_range(address, buffer.len(), self.max_read_bytes)?;
        if !self.platform.is_open() {
            return Err(MemoryError::NoProcess);
        }
        if buffer.is_empty() {
            return Ok(0);
        }
        let reported = self.platform.read_into(address, buffer)?;
        checked_transfer_len(address, buffer.len(), reported)
    }

    /// Write memory to the target process.
    pub fn write(&self, address: u64, data: &[u8]) -> Result<usize, MemoryError> {
        self.platform.write(address, data)
    }

    /// Get memory regions of the target process.
    pub fn query_regions(&mut self) -> Result<&[MemoryRegion], MemoryError> {
        self.regions = self.platform.query_regions()?;
        Ok(&self.regions)
    }

    /// Read a null-terminated string from memory.
    pub fn read_string(&self, address: u64, max_len: usize) -> Result<String, MemoryError> {
        validate_read_range(address, max_len, self.max_read_bytes)?;
        read_string_chunked(address, max_len, |addr, buf| self.read_into(addr, buf))
    }

    /// Read a u64 value from memory (little-endian). Fails on short transfers.
    pub fn read_u64(&self, address: u64) -> Result<u64, MemoryError> {
        let data = self.read_exact(address, 8)?;
        let bytes: [u8; 8] = data.try_into().map_err(|_| MemoryError::ReadFailed {
            address,
            reason: "Invalid data length for u64".to_string(),
        })?;
        Ok(u64::from_le_bytes(bytes))
    }

    /// Read a u32 value from memory (little-endian). Fails on short transfers.
    pub fn read_u32(&self, address: u64) -> Result<u32, MemoryError> {
        let data = self.read_exact(address, 4)?;
        let bytes: [u8; 4] = data.try_into().map_err(|_| MemoryError::ReadFailed {
            address,
            reason: "Invalid data length for u32".to_string(),
        })?;
        Ok(u32::from_le_bytes(bytes))
    }

    /// Read a u16 value from memory (little-endian). Fails on short transfers.
    pub fn read_u16(&self, address: u64) -> Result<u16, MemoryError> {
        let data = self.read_exact(address, 2)?;
        let bytes: [u8; 2] = data.try_into().map_err(|_| MemoryError::ReadFailed {
            address,
            reason: "Invalid data length for u16".to_string(),
        })?;
        Ok(u16::from_le_bytes(bytes))
    }

    /// Read a u8 value from memory. Fails on short transfers.
    pub fn read_u8(&self, address: u64) -> Result<u8, MemoryError> {
        let data = self.read_exact(address, 1)?;
        Ok(data[0])
    }
}

impl<M: PlatformMemory + Default> Default for MemoryManager<M> {
    fn default() -> Self {
        Self::with_backend(M::default())
    }
}

pub(crate) fn read_string_chunked<F>(
    address: u64,
    max_len: usize,
    mut read_into: F,
) -> Result<String, MemoryError>
where
    F: FnMut(u64, &mut [u8]) -> Result<usize, MemoryError>,
{
    if max_len == 0 {
        return Ok(String::new());
    }

    let mut current_addr = address;
    let mut bytes = Vec::new();

    while bytes.len() < max_len {
        let offset_in_page = (current_addr % 4096) as usize;
        let bytes_to_page_end = 4096 - offset_in_page;
        let remaining = max_len - bytes.len();
        let chunk_size = bytes_to_page_end.min(remaining);

        let mut chunk = vec![0u8; chunk_size];
        let reported = read_into(current_addr, &mut chunk)?;
        let bytes_read = checked_transfer_len(current_addr, chunk_size, reported)?;
        if bytes_read == 0 {
            return Err(MemoryError::ShortRead {
                address: current_addr,
                expected: 1,
                actual: 0,
            });
        }

        let valid = &chunk[..bytes_read];
        if let Some(null_pos) = valid.iter().position(|&b| b == 0) {
            bytes.extend_from_slice(&valid[..null_pos]);
            return Ok(String::from_utf8_lossy(&bytes).into_owned());
        }

        bytes.extend_from_slice(valid);
        if bytes_read < chunk_size {
            return Err(MemoryError::ShortRead {
                address: current_addr.saturating_add(bytes_read as u64),
                expected: chunk_size - bytes_read,
                actual: 0,
            });
        }

        match current_addr.checked_add(bytes_read as u64) {
            Some(next) => current_addr = next,
            None => break,
        }
    }

    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn test_read_string_stops_before_page_boundary() {
        // String located near end of page: 0x1ff8..0x1ffe (6 bytes: "hello\0")
        // Page boundary is 0x2000. Reading past 0x2000 would fault.
        let mut read_calls = Vec::new();
        let result = read_string_chunked(0x1ff8, 256, |addr, buf| {
            read_calls.push((addr, buf.len()));
            assert!(
                addr + buf.len() as u64 <= 0x2000,
                "Read request crossed page boundary!"
            );
            let s = b"hello\0";
            let copy_len = s.len().min(buf.len());
            buf[..copy_len].copy_from_slice(&s[..copy_len]);
            Ok(copy_len)
        });

        assert_eq!(result.unwrap(), "hello");
        assert_eq!(read_calls.len(), 1);
        assert_eq!(read_calls[0], (0x1ff8, 8)); // 4096 - (0x1ff8 % 4096) = 8
    }

    #[test]
    fn test_read_string_crosses_page_boundaries_in_chunks() {
        // String starts at 0x1ffe, spans 4 bytes: "ab" on page 1, "cd\0" on page 2.
        let mut read_calls = Vec::new();
        let result = read_string_chunked(0x1ffe, 256, |addr, buf| {
            read_calls.push((addr, buf.len()));
            if addr == 0x1ffe {
                assert_eq!(buf.len(), 2);
                buf[0] = b'a';
                buf[1] = b'b';
                Ok(2)
            } else if addr == 0x2000 {
                buf[0] = b'c';
                buf[1] = b'd';
                buf[2] = 0;
                Ok(3)
            } else {
                panic!("Unexpected read address: {:#x}", addr);
            }
        });

        assert_eq!(result.unwrap(), "abcd");
        assert_eq!(read_calls.len(), 2);
        assert_eq!(read_calls[0], (0x1ffe, 2));
        assert_eq!(read_calls[1], (0x2000, 254));
    }

    #[test]
    fn test_read_string_lossy_utf8() {
        let result = read_string_chunked(0x1000, 16, |_addr, buf| {
            buf[0] = 0xff; // Invalid UTF-8
            buf[1] = 0xfe;
            buf[2] = 0;
            Ok(3)
        });
        assert!(result.is_ok());
    }

    #[test]
    fn test_read_string_rejects_short_unterminated_read() {
        let err = read_string_chunked(0x1000, 16, |_addr, buf| {
            buf[0] = b'a';
            buf[1] = b'b';
            Ok(2)
        })
        .expect_err("short unterminated string read must fail instead of returning partial");
        assert!(matches!(err, MemoryError::ShortRead { .. }), "{err:?}");
    }

    #[derive(Clone)]
    enum MockAction {
        Bytes(Vec<u8>),
        Short { prefix: Vec<u8>, reported: usize },
        OversizedCount(usize),
        Error(MemoryError),
    }

    struct MockPlatformMemory {
        open: bool,
        responses: BTreeMap<(u64, usize), MockAction>,
        prefix_fallback: bool,
        page_map: BTreeMap<u64, Result<Vec<u8>, MemoryError>>,
    }

    impl MockPlatformMemory {
        fn new() -> Self {
            Self {
                open: true,
                responses: BTreeMap::new(),
                prefix_fallback: false,
                page_map: BTreeMap::new(),
            }
        }
    }

    impl PlatformMemory for MockPlatformMemory {
        fn open_process(&mut self, _pid: u32) -> Result<(), MemoryError> {
            self.open = true;
            Ok(())
        }

        fn read_into(&self, address: u64, buffer: &mut [u8]) -> Result<usize, MemoryError> {
            if !self.open {
                return Err(MemoryError::NoProcess);
            }
            if let Some(action) = self.responses.get(&(address, buffer.len())) {
                return match action {
                    MockAction::Bytes(data) => {
                        let copy_len = data.len().min(buffer.len());
                        buffer[..copy_len].copy_from_slice(&data[..copy_len]);
                        Ok(data.len())
                    }
                    MockAction::Short { prefix, reported } => {
                        let copy_len = prefix.len().min(buffer.len());
                        buffer[..copy_len].copy_from_slice(&prefix[..copy_len]);
                        Ok(*reported)
                    }
                    MockAction::OversizedCount(n) => Ok(*n),
                    MockAction::Error(err) => Err(err.clone()),
                };
            }
            if let Some(entry) = self.page_map.get(&(address & !0xfff)) {
                return match entry {
                    Ok(page) => {
                        let off = (address & 0xfff) as usize;
                        if off >= page.len() {
                            return Ok(0);
                        }
                        let available = (page.len() - off).min(buffer.len());
                        buffer[..available].copy_from_slice(&page[off..off + available]);
                        Ok(available)
                    }
                    Err(err) => Err(err.clone()),
                };
            }
            if self.prefix_fallback {
                return Ok(0);
            }
            Err(MemoryError::Unmapped {
                address,
                size: buffer.len(),
            })
        }

        fn write(&self, _address: u64, data: &[u8]) -> Result<usize, MemoryError> {
            Ok(data.len())
        }

        fn query_regions(&self) -> Result<Vec<MemoryRegion>, MemoryError> {
            Ok(Vec::new())
        }

        fn is_open(&self) -> bool {
            self.open
        }
    }

    #[test]
    fn exact_read_and_scalars_reject_zero_and_short_transfers() {
        let mut mock = MockPlatformMemory::new();
        mock.responses.insert(
            (0x1000, 1),
            MockAction::Short {
                prefix: vec![],
                reported: 0,
            },
        );
        mock.responses.insert(
            (0x1000, 2),
            MockAction::Short {
                prefix: vec![0xaa],
                reported: 1,
            },
        );
        mock.responses.insert(
            (0x1000, 4),
            MockAction::Short {
                prefix: vec![0x11, 0x22],
                reported: 2,
            },
        );
        mock.responses.insert(
            (0x1000, 8),
            MockAction::Short {
                prefix: vec![0x01, 0x02, 0x03],
                reported: 3,
            },
        );
        mock.responses.insert(
            (0x2000, 8),
            MockAction::Short {
                prefix: vec![],
                reported: 0,
            },
        );

        let mem = MemoryManager::with_backend(mock);

        assert_eq!(
            mem.read(0x1000, 1).unwrap_err(),
            MemoryError::ShortRead {
                address: 0x1000,
                expected: 1,
                actual: 0,
            }
        );
        assert!(matches!(
            mem.read_u8(0x1000).unwrap_err(),
            MemoryError::ShortRead {
                expected: 1,
                actual: 0,
                ..
            }
        ));
        assert!(matches!(
            mem.read_u16(0x1000).unwrap_err(),
            MemoryError::ShortRead {
                expected: 2,
                actual: 1,
                ..
            }
        ));
        assert!(matches!(
            mem.read_u32(0x1000).unwrap_err(),
            MemoryError::ShortRead {
                expected: 4,
                actual: 2,
                ..
            }
        ));
        assert!(matches!(
            mem.read_u64(0x1000).unwrap_err(),
            MemoryError::ShortRead {
                expected: 8,
                actual: 3,
                ..
            }
        ));
        assert!(matches!(
            mem.read_u64(0x2000).unwrap_err(),
            MemoryError::ShortRead {
                expected: 8,
                actual: 0,
                ..
            }
        ));
    }

    #[test]
    fn exact_read_preserves_genuinely_observed_zeros_and_nonzeros() {
        let mut mock = MockPlatformMemory::new();
        mock.responses
            .insert((0x3000, 4), MockAction::Bytes(vec![0x00, 0x00, 0x00, 0x00]));
        mock.responses
            .insert((0x3004, 4), MockAction::Bytes(vec![0x00, 0x11, 0x00, 0x22]));
        mock.responses
            .insert((0x3008, 8), MockAction::Bytes(vec![0x00; 8]));

        let mem = MemoryManager::with_backend(mock);

        assert_eq!(mem.read(0x3000, 4).unwrap(), [0, 0, 0, 0]);
        assert_eq!(mem.read_u32(0x3000).unwrap(), 0);
        assert_eq!(mem.read_u32(0x3004).unwrap(), 0x2200_1100);
        assert_eq!(mem.read_u64(0x3008).unwrap(), 0);
    }

    #[test]
    fn oversized_transfer_count_and_oversized_or_wrapping_requests_are_rejected() {
        let mut mock = MockPlatformMemory::new();
        mock.responses
            .insert((0x4000, 4), MockAction::OversizedCount(8));
        mock.responses
            .insert((0x4000, 1024), MockAction::OversizedCount(2048));

        let mem = MemoryManager::with_backend(mock).with_max_read_bytes(1024);

        let mut buf = [0u8; 4];
        assert_eq!(
            mem.read_into(0x4000, &mut buf).unwrap_err(),
            MemoryError::OversizedTransfer {
                address: 0x4000,
                requested: 4,
                reported: 8,
            }
        );
        assert!(matches!(
            mem.read_exact(0x4000, 4).unwrap_err(),
            MemoryError::OversizedTransfer { .. }
        ));
        assert!(matches!(
            mem.read_partial(0x4000, 4).unwrap_err(),
            MemoryError::OversizedTransfer { .. }
        ));
        assert!(matches!(
            mem.read_sparse(0x4000, 4096, SparseReadOptions::default())
                .unwrap_err(),
            MemoryError::OversizedTransfer {
                address: 0x4000,
                requested: 1024,
                reported: 2048,
            }
        ));

        assert_eq!(
            mem.read_exact(0x4000, 1025).unwrap_err(),
            MemoryError::BudgetExceeded {
                requested: 1025,
                limit: 1024,
            }
        );
        assert!(matches!(
            mem.read_exact(u64::MAX - 1, 4).unwrap_err(),
            MemoryError::InvalidRegion { size: 4, .. }
        ));
        assert!(matches!(
            mem.read_sparse(u64::MAX - 1, 4, SparseReadOptions::default())
                .unwrap_err(),
            MemoryError::InvalidRegion { size: 4, .. }
        ));
    }

    #[test]
    fn partial_read_returns_only_observed_bytes_with_explicit_trailing_gap() {
        let mut mock = MockPlatformMemory::new();
        mock.responses.insert(
            (0x5000, 8),
            MockAction::Short {
                prefix: vec![0xaa, 0x00, 0xbb],
                reported: 3,
            },
        );
        mock.responses.insert(
            (0x5010, 4),
            MockAction::Short {
                prefix: vec![],
                reported: 0,
            },
        );
        mock.responses.insert(
            (0x5020, 4),
            MockAction::Error(MemoryError::AccessDenied { address: 0x5020 }),
        );

        let mem = MemoryManager::with_backend(mock);
        assert!(matches!(
            mem.read_partial(0x5020, 4).unwrap_err(),
            MemoryError::AccessDenied { address: 0x5020 }
        ));
        let partial = mem.read_partial(0x5000, 8).unwrap();
        assert_eq!(partial.bytes, vec![0xaa, 0x00, 0xbb]);
        assert_eq!(partial.observed_len(), 3);
        assert_eq!(partial.missing_len(), 5);
        assert!(!partial.is_complete());
        assert_eq!(
            partial.trailing_gap,
            Some(MemoryGap {
                start: 0x5003,
                len: 5,
                reason: MemoryGapReason::Partial,
            })
        );

        let empty_partial = mem.read_partial(0x5010, 4).unwrap();
        assert!(empty_partial.bytes.is_empty());
        assert_eq!(
            empty_partial.trailing_gap,
            Some(MemoryGap {
                start: 0x5010,
                len: 4,
                reason: MemoryGapReason::Partial,
            })
        );
    }

    #[test]
    fn sparse_capture_distinguishes_observed_zeros_from_all_gap_reasons() {
        let mut mock = MockPlatformMemory::new();
        // Page 0 (0x1000..0x2000): genuine zero bytes followed by nonzero marker.
        let mut page0 = vec![0u8; 4096];
        page0[4] = 0xde;
        page0[5] = 0xad;
        mock.page_map.insert(0x1000, Ok(page0));
        // Page 1 (0x2000..0x3000): unmapped hole.
        mock.page_map.insert(
            0x2000,
            Err(MemoryError::Unmapped {
                address: 0x2000,
                size: 4096,
            }),
        );
        // Page 2 (0x3000..0x4000): access denied.
        mock.page_map
            .insert(0x3000, Err(MemoryError::AccessDenied { address: 0x3000 }));
        // Page 3 (0x4000..0x5000): partial transfer (only 128 bytes).
        mock.page_map.insert(0x4000, Ok(vec![0x42u8; 128]));
        // Page 4 (0x5000..0x6000): full page after gaps, starting with 0x00.
        let mut page4 = vec![0x77u8; 4096];
        page4[0] = 0x00;
        mock.page_map.insert(0x5000, Ok(page4));

        let mem = MemoryManager::with_backend(mock);
        let capture = mem
            .read_sparse(0x1000, 5 * 4096, SparseReadOptions::default())
            .unwrap();

        assert!(!capture.is_complete());
        assert_eq!(capture.coverage.requested_bytes, 5 * 4096);
        assert_eq!(capture.coverage.observed_bytes, 4096 + 128 + 4096);
        assert_eq!(capture.coverage.missing_bytes, 4096 + 4096 + (4096 - 128));
        assert_eq!(
            capture.coverage.observed_bytes + capture.coverage.missing_bytes,
            capture.coverage.requested_bytes
        );

        // Genuinely observed 0x00 bytes are indistinguishable from ordinary data.
        assert_eq!(capture.byte_at(0x1000), Some(SparseByte::Observed(0x00)));
        assert_eq!(capture.byte_at(0x1004), Some(SparseByte::Observed(0xde)));
        assert_eq!(capture.byte_at(0x5000), Some(SparseByte::Observed(0x00)));

        // Unread bytes record exact reasons rather than zeros.
        assert_eq!(
            capture.byte_at(0x2000),
            Some(SparseByte::Missing(&MemoryGapReason::Unmapped))
        );
        assert_eq!(
            capture.byte_at(0x3000),
            Some(SparseByte::Missing(&MemoryGapReason::Denied))
        );
        assert_eq!(
            capture.byte_at(0x4080),
            Some(SparseByte::Missing(&MemoryGapReason::Partial))
        );

        assert_eq!(
            capture.read_exact_slice(0x1000, 6).unwrap(),
            [0x00, 0x00, 0x00, 0x00, 0xde, 0xad]
        );
        assert!(matches!(
            capture.read_exact_slice(0x2000, 4).unwrap_err(),
            MemoryError::Unmapped { .. }
        ));
        assert!(matches!(
            capture.read_exact_slice(0x1ffe, 8).unwrap_err(),
            MemoryError::Unmapped { .. }
        ));
        assert!(matches!(
            capture.read_exact_slice(0x3000, 4).unwrap_err(),
            MemoryError::AccessDenied { .. }
        ));

        let exported = capture.to_json_value();
        assert_eq!(exported["coverage"]["observed_bytes"], 4096 + 128 + 4096);
        assert_eq!(exported["segments"][1]["reason"]["kind"], "unmapped");
        assert_eq!(exported["segments"][2]["reason"]["kind"], "denied");
    }

    #[test]
    fn sparse_capture_records_budget_and_process_exit_gaps() {
        let mut mock = MockPlatformMemory::new();
        mock.page_map.insert(0x1000, Ok(vec![0x11u8; 4096]));
        mock.page_map.insert(0x2000, Ok(vec![0x22u8; 4096]));
        mock.page_map
            .insert(0x3000, Err(MemoryError::ProcessExited { address: 0x3000 }));
        mock.page_map.insert(0x4000, Ok(vec![0x44u8; 4096]));

        let mem = MemoryManager::with_backend(mock);

        // Tight budget stops cleanly and tags trailing tail as Budget.
        let budgeted = mem
            .read_sparse(0x1000, 3 * 4096, SparseReadOptions::with_max_bytes(4096))
            .unwrap();
        assert_eq!(budgeted.coverage.observed_bytes, 4096);
        assert_eq!(budgeted.coverage.missing_bytes, 2 * 4096);
        assert_eq!(
            budgeted.byte_at(0x2000),
            Some(SparseByte::Missing(&MemoryGapReason::Budget))
        );
        assert!(matches!(
            budgeted.read_exact_slice(0x2000, 4).unwrap_err(),
            MemoryError::BudgetExceeded { .. }
        ));

        // Process exit marks the remainder of the capture as ProcessExited.
        let exited = mem
            .read_sparse(0x2000, 3 * 4096, SparseReadOptions::default())
            .unwrap();
        assert_eq!(exited.coverage.observed_bytes, 4096);
        assert_eq!(exited.coverage.missing_bytes, 2 * 4096);
        assert_eq!(
            exited.byte_at(0x3000),
            Some(SparseByte::Missing(&MemoryGapReason::ProcessExited))
        );
        assert_eq!(
            exited.byte_at(0x4500),
            Some(SparseByte::Missing(&MemoryGapReason::ProcessExited))
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn native_self_process_read_and_sparse_capture_distinguish_zeros_holes_and_denied_pages() {
        unsafe extern "C" {
            fn mmap(
                addr: *mut std::ffi::c_void,
                len: usize,
                prot: i32,
                flags: i32,
                fd: i32,
                offset: i64,
            ) -> *mut std::ffi::c_void;
            fn munmap(addr: *mut std::ffi::c_void, len: usize) -> i32;
            fn mprotect(addr: *mut std::ffi::c_void, len: usize, prot: i32) -> i32;
            fn getpagesize() -> i32;
        }
        const PROT_NONE: i32 = 0;
        const PROT_READ: i32 = 1;
        const PROT_WRITE: i32 = 2;
        const MAP_PRIVATE: i32 = 0x0002;
        const MAP_ANON: i32 = 0x1000;
        const MAP_FAILED: *mut std::ffi::c_void = !0usize as *mut std::ffi::c_void;

        let page = unsafe { getpagesize() } as usize;
        assert!(page >= 4096);

        // Allocate 4 contiguous RW pages:
        // page 0: genuine zeros + sentinel bytes at tail
        // page 1: munmap -> unmapped hole
        // page 2: mprotect(PROT_NONE) -> access denied
        // page 3: nonzero pattern readable after gaps
        let base_ptr = unsafe {
            mmap(
                std::ptr::null_mut(),
                page * 4,
                PROT_READ | PROT_WRITE,
                MAP_PRIVATE | MAP_ANON,
                -1,
                0,
            )
        };
        assert_ne!(base_ptr, MAP_FAILED, "mmap failed");

        struct MmapGuard {
            base: usize,
            page: usize,
            page1_unmapped: bool,
        }
        impl Drop for MmapGuard {
            fn drop(&mut self) {
                unsafe {
                    munmap(self.base as *mut _, self.page);
                    if !self.page1_unmapped {
                        munmap((self.base + self.page) as *mut _, self.page);
                    }
                    munmap((self.base + self.page * 2) as *mut _, self.page);
                    munmap((self.base + self.page * 3) as *mut _, self.page);
                }
            }
        }

        let base = base_ptr as u64;
        let mut guard = MmapGuard {
            base: base_ptr as usize,
            page,
            page1_unmapped: false,
        };

        unsafe {
            let p0 = std::slice::from_raw_parts_mut(base_ptr as *mut u8, page);
            p0.fill(0x00);
            p0[page - 4..].copy_from_slice(&[0xde, 0xad, 0xbe, 0xef]);

            let p3_ptr = (base_ptr as usize + page * 3) as *mut u8;
            let p3 = std::slice::from_raw_parts_mut(p3_ptr, page);
            p3.fill(0x5a);
            p3[0] = 0x00;

            assert_eq!(munmap((base_ptr as usize + page) as *mut _, page), 0);
            guard.page1_unmapped = true;

            assert_eq!(
                mprotect((base_ptr as usize + page * 2) as *mut _, page, PROT_NONE),
                0
            );
        }

        let mut mem = MemoryManager::new();
        mem.open_process(std::process::id())
            .expect("open self task port");

        // Exact read on genuinely zeroed memory succeeds and preserves zeros.
        let zeros = mem.read(base, 16).expect("read genuine zeros on page 0");
        assert_eq!(zeros, vec![0u8; 16]);
        assert_eq!(mem.read_u32(base).unwrap(), 0);

        // Exact read crossing page 0 -> unmapped page 1 must fail cleanly (no zero padding).
        let boundary_err = mem
            .read(base + page as u64 - 4, 8)
            .expect_err("crossing into unmapped page must fail");
        assert!(
            matches!(
                boundary_err,
                MemoryError::Unmapped { .. }
                    | MemoryError::ShortRead { .. }
                    | MemoryError::ReadFailed { .. }
            ),
            "{boundary_err:?}"
        );

        // Sparse acquisition captures page 0 + page 3 and tags page 1 & page 2 gaps accurately.
        let capture = mem
            .read_sparse(
                base,
                page * 4,
                SparseReadOptions {
                    page_size: page,
                    ..SparseReadOptions::default()
                },
            )
            .expect("sparse read self memory");

        assert_eq!(capture.coverage.requested_bytes, (page * 4) as u64);
        assert_eq!(capture.coverage.observed_bytes, (page * 2) as u64);
        assert_eq!(capture.coverage.missing_bytes, (page * 2) as u64);
        assert_eq!(capture.byte_at(base), Some(SparseByte::Observed(0x00)));
        assert_eq!(
            capture.byte_at(base + page as u64 - 1),
            Some(SparseByte::Observed(0xef))
        );
        assert_eq!(
            capture.byte_at(base + page as u64),
            Some(SparseByte::Missing(&MemoryGapReason::Unmapped))
        );
        assert_eq!(
            capture.byte_at(base + (page * 2) as u64),
            Some(SparseByte::Missing(&MemoryGapReason::Denied))
        );
        assert_eq!(
            capture.byte_at(base + (page * 3) as u64),
            Some(SparseByte::Observed(0x00))
        );
        assert_eq!(
            capture.byte_at(base + (page * 3) as u64 + 1),
            Some(SparseByte::Observed(0x5a))
        );
    }
}

#[cfg(test)]
mod error_message_tests {
    use super::MemoryError;

    #[test]
    fn an_invalid_region_names_its_size_not_its_address_twice() {
        let message = MemoryError::InvalidRegion {
            address: 0x1000,
            size: 0x20,
        }
        .to_string();
        assert!(message.contains("0x1000"), "{message}");
        assert!(message.contains("0x20"), "the size is missing: {message}");
        assert_ne!(
            message.matches("0x1000").count(),
            2,
            "the address is printed twice: {message}"
        );
    }
}
